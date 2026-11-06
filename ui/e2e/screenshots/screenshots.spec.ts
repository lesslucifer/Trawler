import { browser, $, $$ } from "@wdio/globals";
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { invoke, onWindow, runningCore, sendThroughProxy } from "../specs/app.ts";
import { addChips } from "../specs/query.ts";
import { LAUNCH_VAR, OUT_VAR, PORTS_FILE, type Ports } from "./screenshots.conf.ts";
import { FIXTURE, FIXTURE_ROWS } from "./fixture.ts";

const out = process.env[OUT_VAR] ?? "";
const SETTLE_MS = 700;

async function shoot(name: string) {
  await browser.pause(SETTLE_MS);
  await browser.saveScreenshot(join(out, `${name}.png`));
}

async function ok(response: Promise<Response>, what: string) {
  const answer = await response;
  if (!answer.ok) throw new Error(`${what}: ${answer.status} ${await answer.text()}`);
  return answer;
}

const buttonStarting = (prefix: string) => $(`//button[starts-with(normalize-space(.), "${prefix}")]`);
const rowWith = (...texts: string[]) =>
  $(`//*[@data-testid="flow-row"]${texts.map((text) => `[contains(., "${text}")]`).join("")}`);
const detail = () => $('[data-testid="detail"]');
const leg = (name: "request" | "response") => detail().$('header [role="radiogroup"]').$(`button=${name}`);
const RESPONSE_PANE = '[data-testid="body-viewer"][data-leg="response"]';

async function click(prefix: string) {
  const target = buttonStarting(prefix);
  await target.waitForEnabled({ timeout: 15_000, timeoutMsg: `no enabled button starting "${prefix}"` });
  await target.click();
}

async function choose(label: string) {
  if (!(await $('[role="listbox"]').isExisting()))
    await $(`[aria-label="which device's traffic do you want to see"][aria-haspopup="listbox"]`).click();
  await $(`[role="option"][aria-label="${label}"]`).click();
  await browser.waitUntil(async () => !(await $('[role="listbox"]').isExisting()), {
    timeout: 5_000,
    timeoutMsg: `the picker stayed open after choosing ${label}`,
  });
}

// The window re-reads this machine on focus; the second read cannot have
// started before the request it is meant to count.
async function rereadOnFocus() {
  for (let i = 0; i < 2; i++) {
    await browser.execute(() => window.dispatchEvent(new Event("focus")));
    await browser.pause(400);
  }
}

async function selectRow(...texts: string[]) {
  await rowWith(...texts).click();
  await detail().waitForDisplayed({ timeout: 15_000 });
}

async function headerRowNamed(name: string) {
  for (const row of await $$('[data-testid="header-row"]')) {
    const [nameField] = await row.$$("input");
    if ((await nameField!.getValue()) === name) return row;
  }
  throw new Error(`no header row named ${name}`);
}

const THIS_MACHINE = { darwin: "This machine — macOS", win32: "This machine — Windows", linux: "This machine — Linux" }[
  process.platform as "darwin" | "win32" | "linux"
];

describe("README and guide screenshots, the window on the harness core", function () {
  if (process.env[LAUNCH_VAR] !== "attached") return;
  let ports: Ports;

  before(async () => {
    mkdirSync(out, { recursive: true });
    ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as Ports;
    await $("header").waitForExist({ timeout: 30_000 });
  });

  const hit = async (query = "") => {
    await rereadOnFocus();
    await ok(fetch(`${ports.control}/hit${query}`), "hit");
    await rereadOnFocus();
  };

  it("S2 · setup, this machine, every task done", async () => {
    await hit();
    await click("set up another device");
    await choose(THIS_MACHINE);
    await hit();
    await $('[data-testid="proven"]').waitForExist({ timeout: 30_000 });
    await shoot("s2-setup-this-machine");
  });

  it("S6 · setup, iPhone, the address allowed", async () => {
    await choose("iPhone or iPad");
    await invoke("scenario_script", { patch: { allow_reach: true } });
    await click("allow devices on");
    await $('[data-testid="device-address"]').waitForExist({ timeout: 15_000 });
    await shoot("s6-setup-iphone");
    await click("stop listening");
    await browser.waitUntil(async () => !(await $("//*[starts-with(normalize-space(.), 'reachable on')]").isExisting()), {
      timeout: 15_000,
      timeoutMsg: "the reachable chip stayed after stop listening",
    });
    await click("‹ show traffic");
  });

  it("S1 · the list with a JSON response open", async () => {
    const harnessRows = (await (await ok(fetch(`${ports.api}/flows?host=localhost`), "list")).json()) as { id: string }[];
    for (const { id } of harnessRows) await ok(fetch(`${ports.api}/flows/${id}`, { method: "DELETE" }), "delete");
    await ok(fetch(`${ports.api}/flows/import`, { method: "POST", body: JSON.stringify(FIXTURE) }), "import");
    await browser.waitUntil(async () => (await $$('[data-testid="flow-row"]').length) === FIXTURE_ROWS, {
      timeout: 15_000,
      timeoutMsg: `the list never held the ${FIXTURE_ROWS} fixture rows`,
    });
    await selectRow("/v1/orders/9f3c");
    await leg("response").click();
    await $(`${RESPONSE_PANE} [role="radiogroup"]`).$("button=pretty").click();
    await shoot("s1-main-window");
  });

  it("S4 · the inspector's headers on an HTTP/2 request", async () => {
    await selectRow("POST", "/v1/session");
    await leg("request").click();
    await shoot("s4-inspector-headers");
  });

  it("S10 · the ⋮ menu over the list", async () => {
    await browser.keys("Escape");
    await $('button[aria-label="capture actions"]').click();
    await $('[role="menuitem"]').waitForDisplayed({ timeout: 15_000 });
    await shoot("s10-capture-menu");
    await browser.keys("Escape");
  });

  it("S3 · the list filtered by two chips", async () => {
    await addChips("status:4xx");
    await addChips("-method:OPTIONS");
    await browser.keys("Escape");
    await shoot("s3-filter-chips");
    for (const chip of await $$('[data-testid="chip"] button')) await chip.click();
  });

  it("S5 · a request sent again with one header edited", async () => {
    await hit(`?p=echo&h=${encodeURIComponent("x-client: trawler-docs")}`);
    await selectRow("/echo");
    await detail().$("button=replay").click();
    await $('[data-testid="composer"]').waitForDisplayed({ timeout: 15_000 });
    const [, value] = await (await headerRowNamed("x-client")).$$("input");
    await value!.setValue("trawler-docs-edited");
    await $('[data-testid="send-zone"]').$("button=send").click();
    await $('[data-testid="send-zone"]').$('[aria-label="200 OK"]').waitForExist({ timeout: 15_000 });
    await shoot("s5-send-again");
  });

  it("S11 · Settings → General", async () => {
    await invoke("open_settings");
    await onWindow("settings");
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    await shoot("s11-settings-general");
  });
});

describe("README and guide screenshots, the window on its own core", function () {
  if (process.env[LAUNCH_VAR] !== "owned") return;

  it("S7 · Settings → Capture service, running, with its log", async () => {
    mkdirSync(out, { recursive: true });
    await $("header").waitForExist({ timeout: 30_000 });
    const core = await runningCore();
    await sendThroughProxy(core.proxy, `http://${core.api.replace(/^http:\/\//, "")}/stats`).catch(() => 0);
    await invoke("open_settings");
    await onWindow("settings");
    await $("button=Capture service").click();
    await $("h1=Capture service").waitForDisplayed({ timeout: 15_000 });
    await browser.waitUntil(async () => (await $("body").getText()).includes("running"), {
      timeout: 30_000,
      timeoutMsg: "the Capture service page never read running",
    });
    await $('[role="log"]').waitForExist({ timeout: 15_000 });
    await shoot("s7-settings-capture-service");
  });
});
