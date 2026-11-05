import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";
import { clearTheCapture } from "./layout.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  origin: string;
  control: string;
};

const MAIN = "main";
const RESPONSE_PANE = '[data-testid="body-viewer"][data-leg="response"]';
const SURFACE = '[data-testid="data-surface"]';
const LONG_JSON = "/long-json";
const LONG_FORM = "/long-form";
const TOKEN_SIGNATURE = "bm90LWEtcmVhbC1zaWduYXR1cmU";
const EXP = "1793851200";

const hit = (path: string) => fetch(`${ports.control}/hit?p=${path.slice(1)}`);

async function flowIdOf(path: string): Promise<string> {
  const listed = (await (await fetch(`${ports.api}/flows`)).json()) as { id: string; uri: string }[] | { flows: { id: string; uri: string }[] };
  const flows = Array.isArray(listed) ? listed : listed.flows;
  const flow = flows.find((f) => new URL(f.uri).pathname === path);
  if (!flow) throw new Error(`no captured flow for ${path}`);
  return flow.id;
}

const capturedBody = async (path: string) =>
  (await fetch(`${ports.api}/flows/${encodeURIComponent(await flowIdOf(path))}/response/body?decode=source`)).text();

async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

async function showResponseOf(path: string, mode: "raw" | "pretty") {
  await hit(path);
  let row: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      for (const candidate of await $$('[data-testid="flow-row"]')) {
        if ((await candidate.getText()).includes(path)) {
          row = candidate;
          return true;
        }
      }
      return false;
    },
    { timeout: 15_000, timeoutMsg: `no flow row for ${path}` },
  );
  await row!.click();
  await $('[data-testid="detail"] header [role="radiogroup"]').$("button=response").click();
  const toggle = $(`${RESPONSE_PANE} [role="radiogroup"]`).$(`button=${mode}`);
  await toggle.waitForExist({ timeout: 15_000 });
  await toggle.click();
}

const dataWindows = async () => (await browser.getWindowHandles()).filter((handle) => handle.startsWith("data-"));

async function onNewDataWindow(known: string[]): Promise<string> {
  let opened: string | undefined;
  await browser.waitUntil(
    async () => {
      opened = (await dataWindows()).find((handle) => !known.includes(handle));
      return opened !== undefined;
    },
    { timeout: 15_000, timeoutMsg: "no new data window opened" },
  );
  await browser.switchToWindow(opened!);
  await $(SURFACE).waitForExist({ timeout: 15_000 });
  return opened!;
}

const windowTitle = () =>
  browser.executeAsync((done: (title: string) => void) => {
    const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { title: () => Promise<string> } } } })
      .__TAURI__;
    void tauri.window.getCurrentWindow().title().then(done);
  });

// WebDriver cannot read the OS clipboard, so each window's clipboard is replaced
// with a recorder before a copy and read back after, as the flows spec does.
const recordClipboard = () =>
  browser.execute(() => {
    const w = window as unknown as { __copied?: string };
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (text: string) => ((w.__copied = text), Promise.resolve()) },
    });
  });

async function copied(act: () => Promise<void>): Promise<string> {
  await browser.execute(() => delete (window as unknown as { __copied?: string }).__copied);
  await act();
  let text: string | undefined;
  await browser.waitUntil(
    async () => {
      text = await browser.execute(() => (window as unknown as { __copied?: string }).__copied);
      return text !== undefined && text !== null;
    },
    { timeout: 5_000, timeoutMsg: "nothing reached the clipboard" },
  );
  return text!;
}

const copyOnScreen = () => copied(() => $('header button:not([aria-label="more"])').click());

async function pick(label: string) {
  for (const item of await $$('[role="menu"] [role="menuitem"]')) {
    if ((await item.$("span.flex-1").getText()) === label) {
      await item.click();
      return;
    }
  }
  throw new Error(`no menu row "${label}"`);
}

async function fromMore(label: string) {
  await $('button[aria-label="more"]').click();
  await $('[role="menu"]').waitForDisplayed({ timeout: 5_000 });
  await pick(label);
}

const modified = () => $('[data-testid="modified"]');

async function editCountReads(text: string | null) {
  await browser.waitUntil(
    async () => (text === null ? !(await modified().isExisting()) : (await modified().isExisting()) && (await modified().getText()) === text),
    { timeout: 5_000, timeoutMsg: `the modified marker never read ${text ?? "nothing"}` },
  );
}

// The embedded WebDriver types only into form fields, so text reaches the
// editor the way an input method's does: an insertion at a DOM caret that the
// editor reads back from the document.
async function typeAtEndOfFirstLine(text: string) {
  await browser.execute(
    (surface: string, typed: string) => {
      const line = document.querySelector(`${surface} .cm-line`)!;
      (line.closest(".cm-content") as HTMLElement).focus();
      const caret = document.createRange();
      caret.selectNodeContents(line);
      caret.collapse(false);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(caret);
      document.execCommand("insertText", false, typed);
    },
    SURFACE,
    text,
  );
}

// The embedded WebDriver dispatches only primary clicks, so a right-click is
// the contextmenu event the pointer would raise at the middle of that text.
async function rightClickOn(text: string) {
  const found = await browser.execute(
    (surface: string, wanted: string) => {
      const walker = document.createTreeWalker(document.querySelector(`${surface} .cm-content`)!, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const at = node.textContent!.indexOf(wanted);
        if (at < 0) continue;
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + wanted.length);
        const box = range.getBoundingClientRect();
        node.parentElement!.dispatchEvent(
          new MouseEvent("contextmenu", {
            bubbles: true,
            cancelable: true,
            button: 2,
            clientX: box.left + box.width / 2,
            clientY: box.top + box.height / 2,
          }),
        );
        return true;
      }
      return false;
    },
    SURFACE,
    text,
  );
  expect(found).toBe(true);
  await $('[role="menu"]').waitForDisplayed({ timeout: 5_000 });
}

async function findReads(query: string, count: string) {
  await $('input[aria-label="find"]').setValue(query);
  await browser.waitUntil(async () => (await $('[data-testid="find-count"]').getText()) === count, {
    timeout: 5_000,
    timeoutMsg: `find "${query}" never read ${count}`,
  });
}

async function formPairKeyed(key: string) {
  let pair: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      for (const candidate of await $$('[data-testid="form-pair"]')) {
        if ((await candidate.$('[data-wire="key"]').getProperty("textContent")) === key) {
          pair = candidate;
          return true;
        }
      }
      return false;
    },
    { timeout: 5_000, timeoutMsg: `no form pair keyed ${key} on screen` },
  );
  return pair!;
}

const requestsMade = () =>
  browser.execute(() =>
    performance
      .getEntriesByType("resource")
      .map((entry) => entry.name)
      .filter((name) => /^https?:/.test(name)),
  );

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
