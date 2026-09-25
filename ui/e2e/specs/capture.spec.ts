import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  proxy: string;
  origin: string;
  control: string;
};

const CURL_COMMAND =
  "curl 'http://api.example.com/v1/widgets' -X POST -H 'content-type: application/json' --data-raw '{\"n\":1}'";

const rowCount = () => $$('[data-testid="flow-row"]').length;
const hit = (path?: string) => fetch(`${ports.control}/hit${path ? `?p=${path}` : ""}`);

const captureSlot = () => $('[data-testid="capture-slot"]');
const clearShortcut = () => $('button[aria-label^="clear "]');
const captureTrigger = () => $('button[aria-label="capture actions"]');
const menuItem = (name: string) => $(`[role="menuitem"][aria-label="${name}"]`);
const importAsk = () => $('[data-testid="import-ask-panel"]');
const clearAsk = () => $('[data-testid="clear-ask-panel"]');

async function openAct(name: string) {
  await captureTrigger().click();
  const item = menuItem(name);
  await item.waitForDisplayed({ timeout: 15_000 });
  await item.click();
}

// The window is already bound to this run's core: the launcher handed it the
// address in the environment and the shell injected it before the first script
// ran, so a spec only waits for the screen it painted. Nothing here navigates —
// a reload stops the WKWebView scheme tasks the Tauri IPC rides on, and wry
// answers a stopped one with an Objective-C exception that aborts the app.
async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

// UC-7 and UC-6's paste half, end to end through the real window: the capture is
// emptied from the control that empties it, the ask can be silenced and the next
// clear then skips it, and a pasted curl command brings a flow back marked.
//
// What is NOT here, and why: export-to-file and import-from-file cross the OS
// save and open panels. In binary mode Tauri defines `__TAURI_INTERNALS__.invoke`
// non-writable and non-configurable and does not consult the wdio mock registry,
// so no test can answer those panels and no test can reach past them — pressing
// export in this harness opens a native sheet that never closes. Those two acts
// are proven where they are reachable: the whole HAR round trip over the real
// engine in `tests/e2e_export.rs`, and the controls' own behaviour against a
// stubbed native layer in `CaptureControls.test.tsx`.
describe("UC-6 / UC-7 — empty the capture, and bring a flow back", () => {
  before(async () => {
    await shellIsUp();
    await hit();
    await hit("/json");
    await browser.waitUntil(async () => (await rowCount()) >= 2, {
      timeout: 15_000,
      timeoutMsg: "the list never filled with the traffic to clear",
    });
  });

  it("clears the capture behind an ask, and the list empties", async () => {
    await clearShortcut().click();
    await clearAsk().waitForDisplayed({ timeout: 15_000 });
    await clearAsk().$("button=clear").click();

    await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });
    expect(await rowCount()).toBe(0);
    await expect(captureSlot()).toHaveText("0 flows");
  });

  it("takes a pasted curl command as a flow, marked imported", async () => {
    await openAct("import a curl command or a har file");
    await importAsk().waitForDisplayed({ timeout: 15_000 });
    await importAsk().$('[data-wire="curl-command"] textarea').setValue(CURL_COMMAND);
    await importAsk().$("button=import").click();

    await importAsk().waitForExist({ reverse: true, timeout: 15_000 });
    await browser.waitUntil(async () => (await rowCount()) === 1, {
      timeout: 15_000,
      timeoutMsg: "the pasted command never appeared as a row",
    });

    const row = (await $$('[data-testid="flow-row"]'))[0]!;
    expect(await row.getText()).toContain("/v1/widgets");
    expect(await row.getText()).toContain("POST");
    expect(await row.$('[aria-label="imported"]').isExisting()).toBe(true);
    await expect(captureSlot()).toHaveText("1 flow");
  });

  it("silencing the ask makes the next clear direct", async () => {
    await clearShortcut().click();
    await clearAsk().waitForDisplayed({ timeout: 15_000 });
    await clearAsk().$("input[type='checkbox']").click();
    await clearAsk().$("button=clear").click();
    await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });

    await hit();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "the list never refilled after the silenced clear",
    });

    await clearShortcut().click();
    await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });
    expect(await clearAsk().isExisting()).toBe(false);
    expect(await rowCount()).toBe(0);
  });
});
