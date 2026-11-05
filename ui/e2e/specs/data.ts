import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

export const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  origin: string;
  control: string;
  beacon: string;
};

export const MAIN = "main";
export const RESPONSE_PANE = '[data-testid="body-viewer"][data-leg="response"]';
export const SURFACE = '[data-testid="data-surface"]';
export const hit = (path: string, header?: string) =>
  fetch(`${ports.control}/hit?${new URLSearchParams({ p: path.slice(1), ...(header ? { h: header } : {}) })}`);

export async function flowIdOf(path: string): Promise<string> {
  const listed = (await (await fetch(`${ports.api}/flows`)).json()) as { id: string; uri: string }[] | { flows: { id: string; uri: string }[] };
  const flows = Array.isArray(listed) ? listed : listed.flows;
  const flow = flows.find((f) => new URL(f.uri).pathname === path);
  if (!flow) throw new Error(`no captured flow for ${path}`);
  return flow.id;
}

export const capturedBody = async (path: string) =>
  (await fetch(`${ports.api}/flows/${encodeURIComponent(await flowIdOf(path))}/response/body?decode=source`)).text();

export async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

export const REQUEST_PANE = '[data-testid="body-viewer"][data-leg="request"]';

export async function showResponseOf(path: string, mode: "raw" | "pretty") {
  await showFlow(path, "response", mode);
}

export async function showFlow(path: string, leg: "request" | "response", mode?: "raw" | "pretty", header?: string) {
  await hit(path, header);
  let row: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      for (const candidate of await $$('[data-testid="flow-row"]')) {
        if ((await candidate.getText()).includes(path.split("?")[0])) {
          row = candidate;
          return true;
        }
      }
      return false;
    },
    { timeout: 15_000, timeoutMsg: `no flow row for ${path}` },
  );
  await row!.click();
  await pickWhenSettled(() => $('[data-testid="detail"] header [role="radiogroup"]').$(`button=${leg}`), `the ${leg} leg`);
  if (mode) {
    const pane = leg === "response" ? RESPONSE_PANE : REQUEST_PANE;
    await pickWhenSettled(() => $(`${pane} [role="radiogroup"]`).$(`button=${mode}`), `the ${mode} mode`);
  }
}

// The detail pane re-renders after a row click, so a lookup chained off it can
// land on a detached parent; picking a radio again is harmless, so retry.
export async function pickWhenSettled(option: () => ReturnType<typeof $>, what: string) {
  await browser.waitUntil(
    async () => {
      try {
        await option().click();
        return true;
      } catch {
        return false;
      }
    },
    { timeout: 15_000, timeoutMsg: `could not pick ${what}` },
  );
}

export const dataWindows = async () => (await browser.getWindowHandles()).filter((handle) => handle.startsWith("data-"));

export async function onNewDataWindow(known: string[]): Promise<string> {
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

export const windowTitle = () =>
  browser.executeAsync((done: (title: string) => void) => {
    const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { title: () => Promise<string> } } } })
      .__TAURI__;
    void tauri.window.getCurrentWindow().title().then(done);
  });

// WebDriver cannot read the OS clipboard, so each window's clipboard is replaced
// with a recorder before a copy and read back after, as the flows spec does.
export const recordClipboard = () =>
  browser.execute(() => {
    const w = window as unknown as { __copied?: string };
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (text: string) => ((w.__copied = text), Promise.resolve()) },
    });
  });

export async function copied(act: () => Promise<void>): Promise<string> {
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

export const copyOnScreen = () => copied(() => $('header button:not([aria-label="more"])').click());

export async function pick(label: string) {
  for (const item of await $$('[role="menu"] [role="menuitem"]')) {
    if ((await item.$("span.flex-1").getText()) === label) {
      await item.click();
      return;
    }
  }
  throw new Error(`no menu row "${label}"`);
}

export async function fromMore(label: string) {
  await $('button[aria-label="more"]').click();
  await $('[role="menu"]').waitForDisplayed({ timeout: 5_000 });
  await pick(label);
}

export const modified = () => $('[data-testid="modified"]');

export async function editCountReads(text: string | null) {
  await browser.waitUntil(
    async () => (text === null ? !(await modified().isExisting()) : (await modified().isExisting()) && (await modified().getText()) === text),
    { timeout: 5_000, timeoutMsg: `the modified marker never read ${text ?? "nothing"}` },
  );
}

// The embedded WebDriver types only into form fields, so text reaches the
// editor the way an input method's does: an insertion at a DOM caret that the
// editor reads back from the document.
export async function typeAtEndOfFirstLine(text: string) {
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
export async function rightClickOn(text: string) {
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

export async function findReads(query: string, count: string) {
  await $('input[aria-label="find"]').setValue(query);
  await browser.waitUntil(async () => (await $('[data-testid="find-count"]').getText()) === count, {
    timeout: 5_000,
    timeoutMsg: `find "${query}" never read ${count}`,
  });
}

export async function formPairKeyed(key: string) {
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

export const requestsMade = () =>
  browser.execute(() =>
    performance
      .getEntriesByType("resource")
      .map((entry) => entry.name)
      .filter((name) => /^https?:/.test(name)),
  );

