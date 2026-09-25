import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  proxy: string;
  origin: string;
  control: string;
  closed: string;
};

const ECHO_URL = `${ports.origin}echo`;

const rowCount = () => $$('[data-testid="flow-row"]').length;
const hitEcho = (header: string) => fetch(`${ports.control}/hit?p=echo&h=${encodeURIComponent(header)}`);

const composer = () => $('[data-testid="composer"]');
const urlField = () => composer().$('input[aria-label="url"]');
const sendZone = () => $('[data-testid="send-zone"]');
const sendButton = () => sendZone().$("button=send");
const captureTrigger = () => $('button[aria-label="capture actions"]');
const clearShortcut = () => $('button[aria-label^="clear "]');
const RESPONSE_PANE = '[data-testid="body-viewer"][data-leg="response"]';

// Two rows may share a header name, so a row is found by what its name field
// holds, never by the value field's label.
async function headerRowNamed(name: string) {
  for (const row of await $$('[data-testid="header-row"]')) {
    const [nameField] = await row.$$("input");
    if ((await nameField!.getValue()) === name) return row;
  }
  throw new Error(`no header row named ${name}`);
}

const editedRowNames = async () => {
  const names: string[] = [];
  for (const row of await $$('[data-testid="header-row"][data-mark="edited"]')) {
    const [nameField] = await row.$$("input");
    names.push(await nameField!.getValue());
  }
  return names;
};

// A status pill and a duration each carry an accessible name that starts with
// a number ("200 OK", "2.0 seconds"). A failed send renders neither.
const numberNamedIn = async (zone: ReturnType<typeof sendZone>) => {
  const names: string[] = [];
  for (const el of await zone.$$("[aria-label]")) {
    const name = await el.getAttribute("aria-label");
    if (/^\d/.test(name)) names.push(name);
  }
  return names;
};

async function sendAndWatchItWait() {
  await sendButton().click();
  await browser.waitUntil(
    async () => (await sendZone().getText()).includes("sending…") && !(await sendButton().isEnabled()),
    { timeout: 1_500, timeoutMsg: "send never disabled with sending… beside it" },
  );
}

async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

// UC-8 end to end through the real window: the edit reaches the wire, the answer
// lands as a flow, and a send that got no reply lands nothing. The origin's echo
// route answers with the headers it received, two seconds late, so the spec reads
// the edit off the wire and can watch the composer wait.
//
// It leaves the capture empty, as the capture spec before it does, because the
// flows spec after it starts from its own hits.
describe("UC-8 — send a request again, edited or from scratch", () => {
  before(async () => {
    await shellIsUp();
    await hitEcho("x-probe: original");
    await browser.waitUntil(async () => (await rowCount()) === 1, {
      timeout: 15_000,
      timeoutMsg: "the proxied request to /echo never appeared as a row",
    });
  });

  after(async () => {
    await browser.keys("Escape");
    await clearShortcut().click();
    const ask = $('[data-testid="clear-ask-panel"]');
    if (await ask.isExisting()) await ask.$("button=clear").click();
    await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });
  });

  it("replays a flow with one header edited, and only that row marks itself", async () => {
    await (await $$('[data-testid="flow-row"]'))[0]!.click();
    await $('[data-testid="detail"]').waitForDisplayed({ timeout: 15_000 });
    await $('[data-testid="detail"]').$("button=replay").click();
    await composer().waitForDisplayed({ timeout: 15_000 });

    const [, valueField] = await (await headerRowNamed("x-probe")).$$("input");
    expect(await valueField!.getValue()).toBe("original");
    await valueField!.setValue("edited");

    await browser.waitUntil(async () => (await editedRowNames()).length > 0, {
      timeout: 15_000,
      timeoutMsg: "the edited row never marked itself",
    });
    expect(await editedRowNames()).toEqual(["x-probe"]);
  });

  it("disables send while it waits, then lands the status, the duration and the new flow", async () => {
    await sendAndWatchItWait();

    await sendZone().$('[aria-label="200 OK"]').waitForExist({ timeout: 15_000 });
    expect(await sendButton().isEnabled()).toBe(true);
    expect(await numberNamedIn(sendZone())).toContainEqual(expect.stringMatching(/seconds$/));
    expect(await sendZone().$("button=open the new flow").isExisting()).toBe(true);

    await browser.waitUntil(async () => (await rowCount()) === 2, {
      timeout: 15_000,
      timeoutMsg: "the sent flow never appeared in the list",
    });
    const top = (await $$('[data-testid="flow-row"]'))[0]!;
    expect(await top.$('[aria-label="sent"]').isExisting()).toBe(true);
  });

  it("the new flow's response is what the origin received: the edited header", async () => {
    await sendZone().$("button=open the new flow").click();
    await $('[data-testid="detail"]').waitForDisplayed({ timeout: 15_000 });
    await $('[data-testid="detail"] header [role="radiogroup"]').$("button=response").click();
    await $(`${RESPONSE_PANE} [role="radiogroup"]`).$("button=raw").click();
    const body = $(`${RESPONSE_PANE} [data-testid="body-text"]`);
    await body.waitForExist({ timeout: 15_000 });
    expect(await body.getText()).toContain('"x-probe":"edited"');
  });

  it("a new request cannot be sent without a URL, and sends once it has one", async () => {
    await captureTrigger().click();
    const item = $('[role="menuitem"][aria-label="new request"]');
    await item.waitForDisplayed({ timeout: 15_000 });
    await item.click();
    await composer().waitForDisplayed({ timeout: 15_000 });

    expect(await urlField().getValue()).toBe("");
    expect(await sendButton().isEnabled()).toBe(false);
    expect(await sendZone().getText()).toContain("a request needs a URL");

    await urlField().setValue(ECHO_URL);
    await sendAndWatchItWait();
    await sendZone().$('[aria-label="200 OK"]').waitForExist({ timeout: 15_000 });
    await browser.waitUntil(async () => (await rowCount()) === 3, {
      timeout: 15_000,
      timeoutMsg: "the composed flow never appeared in the list",
    });
  });

  it("the draft survives its own send, and still takes typing", async () => {
    expect(await urlField().getValue()).toBe(ECHO_URL);
    await urlField().addValue("?again=1");
    expect(await urlField().getValue()).toBe(`${ECHO_URL}?again=1`);
  });

  it("a send that gets no reply is ⚠ with the reason, and no flow lands", async () => {
    const before = await rowCount();
    await urlField().setValue(ports.closed);
    await sendButton().click();

    const failed = sendZone().$('[data-testid="send-failed"]');
    await failed.waitForDisplayed({ timeout: 15_000 });
    const reason = await failed.getText();
    expect(reason).toContain("⚠");
    expect(reason).toContain("refused");
    expect(await numberNamedIn(sendZone())).toEqual([]);
    expect(await rowCount()).toBe(before);
  });
});
