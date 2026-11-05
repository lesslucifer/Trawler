import { browser, $, $$, expect } from "@wdio/globals";
import {
  MAIN,
  RESPONSE_PANE,
  SURFACE,
  capturedBody,
  copyOnScreen,
  copied,
  dataWindows,
  editCountReads,
  findReads,
  formPairKeyed,
  fromMore,
  modified,
  onNewDataWindow,
  pick,
  recordClipboard,
  requestsMade,
  rightClickOn,
  shellIsUp,
  showResponseOf,
  typeAtEndOfFirstLine,
  windowTitle,
  ports,
} from "./data.ts";
import { clearTheCapture } from "./layout.ts";

const LONG_JSON = "/long-json";
const LONG_FORM = "/long-form";
const TOKEN_SIGNATURE = "bm90LWEtcmVhbC1zaWduYXR1cmU";
const EXP = "1793851200";

// UC-9 end to end through the real windows (E-INSPWIN): the pane hands its bytes
// to a data window, the window works a copy of them, decodes a value into a
// window of its own, and none of it reaches the pane, the capture or the core.
// It leaves the capture empty and no data window open, as the specs around it do.
describe("UC-9 — work with a piece of data in its own window", () => {
  let served = "";
  let jsonWindow = "";

  before(async () => {
    await shellIsUp();
  });

  after(async () => {
    await browser.switchToWindow(MAIN);
    await clearTheCapture();
  });

  it("J1 · the cut's way out opens the whole body in a window titled by its opener", async () => {
    await showResponseOf(LONG_JSON, "raw");
    const pane = $(`${RESPONSE_PANE} [data-testid="body-text"]`);
    await pane.waitForExist({ timeout: 15_000 });
    expect(await pane.getText()).toMatch(/⚠ showing .+ of .+/);
    served = await capturedBody(LONG_JSON);

    await pane.$("button=open in data window").click();
    jsonWindow = await onNewDataWindow([]);

    const title = `response body · raw · GET ${new URL(ports.origin).host}${LONG_JSON}`;
    expect(await windowTitle()).toBe(title);
    expect(await $("h1").getAttribute("aria-label")).toBe(title);
    expect((await $("//h1/preceding-sibling::p").getText()).toLowerCase()).toBe("response body · raw");
    expect(await $("h1").$("span=GET").isExisting()).toBe(true);
    await recordClipboard();
    expect(await copyOnScreen()).toBe(served);
  });

  it("J2 · find counts and steps through the matches", async () => {
    await findReads("needle", "1 of 25");
    await $('button[aria-label="next match"]').click();
    await browser.waitUntil(async () => (await $('[data-testid="find-count"]').getText()) === "2 of 25", {
      timeout: 5_000,
      timeoutMsg: "next match never stepped to 2 of 25",
    });
    await $('input[aria-label="find"]').click();
    await browser.keys("Enter");
    await browser.waitUntil(async () => (await $('[data-testid="find-count"]').getText()) === "3 of 25", {
      timeout: 5_000,
      timeoutMsg: "Enter in find never stepped to 3 of 25",
    });
  });

  it("J2 · a sort and a hand edit change the copy on screen, never the original", async () => {
    await fromMore("sort keys");
    await editCountReads("modified · 1 edit");
    await typeAtEndOfFirstLine(" ");
    await editCountReads("modified · 2 edits");

    const edited = await copyOnScreen();
    expect(edited.startsWith("{ \n")).toBe(true);
    expect(edited.indexOf('"count"')).toBeLessThan(edited.indexOf('"items"'));
    expect(await copied(() => fromMore("copy original"))).toBe(served);
  });

  it("J2 · undo steps back to the original, and restore original returns it in one act", async () => {
    await fromMore("undo");
    await editCountReads("modified · 1 edit");
    await fromMore("undo");
    await editCountReads(null);

    await fromMore("sort keys");
    await typeAtEndOfFirstLine(" ");
    await editCountReads("modified · 2 edits");
    await fromMore("restore original");
    await editCountReads(null);
    expect(await copyOnScreen()).toBe(served);
  });

  it("J1 · opening the same body again focuses its window, the edit still there", async () => {
    await typeAtEndOfFirstLine(" ");
    await modified().waitForExist({ timeout: 5_000 });
    const marker = await modified().getText();

    await browser.switchToWindow(MAIN);
    await $(RESPONSE_PANE).$('button[aria-label="open in data window"]').click();
    await browser.pause(1_500);
    expect(await dataWindows()).toEqual([jsonWindow]);

    await browser.switchToWindow(jsonWindow);
    await editCountReads(marker);
  });

  it("J3 · a long form shows its last pair whole, and a token decodes to the instant it expires", async () => {
    await browser.switchToWindow(MAIN);
    await showResponseOf(LONG_FORM, "pretty");
    await $(`${RESPONSE_PANE} [data-testid="body-form"]`).waitForExist({ timeout: 15_000 });
    await $(RESPONSE_PANE).$('button[aria-label="open in data window"]').click();
    const formWindow = await onNewDataWindow([jsonWindow]);

    await findReads("last", "1 of 1");
    const last = await formPairKeyed("last");
    expect(await last.$('[data-wire="value"]').getProperty("textContent")).toBe("whole-".repeat(300));

    await findReads("token", "1 of 1");
    await browser.execute((pair: HTMLElement) => {
      const box = pair.getBoundingClientRect();
      pair.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: box.left + 8, clientY: box.top + 8 }),
      );
    }, (await formPairKeyed("token")) as unknown as HTMLElement);
    await $('[role="menu"]').waitForDisplayed({ timeout: 5_000 });
    await pick("decode the value");

    const peek = $('[data-testid="decode-peek"]');
    await peek.waitForDisplayed({ timeout: 5_000 });
    expect(await peek.getText()).toContain("JWT");
    expect(await peek.$('[data-testid="signature"]').getText()).toBe(TOKEN_SIGNATURE);
    expect(await peek.getText()).toContain(EXP);

    await peek.$('button[aria-label="open in data window"]').click();
    await onNewDataWindow([jsonWindow, formWindow]);
    await rightClickOn(EXP);
    await pick("decode");
    const timePeek = $('[data-testid="decode-peek"]');
    await timePeek.waitForDisplayed({ timeout: 5_000 });
    expect(await timePeek.getText()).toContain("time");
    expect(await timePeek.$('[data-testid="utc-time"]').getText()).toBe("2026-11-05 04:00:00Z");
  });

  it("J4 · nothing a data window does reaches the pane, the capture or the network", async () => {
    await browser.switchToWindow(MAIN);
    expect(await requestsMade()).not.toEqual([]);
    expect(await capturedBody(LONG_JSON)).toBe(served);

    for (const handle of await dataWindows()) {
      await browser.switchToWindow(handle);
      expect(await requestsMade()).toEqual([]);
    }

    await browser.switchToWindow(MAIN);
    await showResponseOf(LONG_JSON, "pretty");
    const pane = $(`${RESPONSE_PANE} [data-testid="body-text"]`);
    await pane.waitForExist({ timeout: 15_000 });
    expect(await pane.getText()).toContain('"items": [');
  });

  it("J4 · a data window closes without asking, and the main window stays", async () => {
    for (const handle of await dataWindows()) {
      await browser.switchToWindow(handle);
      await browser.execute(() => {
        const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { close: () => Promise<void> } } } })
          .__TAURI__;
        setTimeout(() => void tauri.window.getCurrentWindow().close(), 50);
      });
      await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes(handle), {
        timeout: 15_000,
        timeoutMsg: `${handle} never closed`,
      });
    }
    expect(await browser.getWindowHandles()).toEqual([MAIN]);
    await browser.switchToWindow(MAIN);
    await shellIsUp();
  });
});
