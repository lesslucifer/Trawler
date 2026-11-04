import { browser, $, $$ } from "@wdio/globals";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { PORTS_FILE } from "../wdio.conf.ts";

export { quitTheApp } from "./quit.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { control: string };

export const CONFIG_DIR = resolve(process.env.HOME ?? "", "Library/Application Support/dev.trawler.app");
export const LAYOUT_FILE = resolve(CONFIG_DIR, "layout.json");
export const WINDOWS_FILE = resolve(CONFIG_DIR, "windows.json");

export type SavedLayout = {
  pane: number;
  headersShare: number;
  columns: { time: number; method: number; host: number; status: number; duration: number };
};

export const DEFAULTS: SavedLayout = {
  pane: 520,
  headersShare: 0.35,
  columns: { time: 160, method: 70, host: 180, status: 70, duration: 90 },
};

export const SET_BEFORE_THE_QUIT = {
  window: { width: 1400, height: 780 },
  pane: 576,
  headersShare: 0.6,
  host: 196,
};

export const paneSplitter = () => $('[role="separator"][aria-label="detail pane width"]');
export const headersSplitter = () => $('[role="separator"][aria-label="headers height"]');
export const hostHandle = () => $('[role="separator"][aria-label="host column width"]');

type Splitter = ReturnType<typeof paneSplitter>;

export const valueOf = async (splitter: Splitter) => ({
  now: Number(await splitter.getAttribute("aria-valuenow")),
  min: Number(await splitter.getAttribute("aria-valuemin")),
  max: Number(await splitter.getAttribute("aria-valuemax")),
});

export const paneWidth = async () => (await $('[data-testid="detail-pane"]').getSize()).width;

export const headersCeiling = () =>
  browser.execute(() => (document.querySelector('[data-testid="headers-zone"]') as HTMLElement).style.maxHeight);

export function savedLayout(): SavedLayout | null {
  try {
    return JSON.parse(readFileSync(LAYOUT_FILE, "utf8")) as SavedLayout;
  } catch {
    return null;
  }
}

export function corruptTheSavedLayout() {
  writeFileSync(LAYOUT_FILE, '{"pane":6');
}

export async function savedLayoutBecomes(expected: SavedLayout) {
  await browser.waitUntil(async () => JSON.stringify(savedLayout()) === JSON.stringify(expected), {
    timeout: 5_000,
    timeoutMsg: `layout.json never became ${JSON.stringify(expected)}: ${JSON.stringify(savedLayout())}`,
  });
}

export async function shellIsUp() {
  await $("header").waitForExist({ timeout: 30_000 });
}

export async function aFlowIsOpen() {
  await fetch(`${ports.control}/hit`);
  await browser.waitUntil(async () => (await $$('[data-testid="flow-row"]').length) > 0, {
    timeout: 15_000,
    timeoutMsg: "the proxied request never appeared as a row",
  });
  await (await $$('[data-testid="flow-row"]'))[0]!.click();
  await $('[data-testid="detail"]').waitForDisplayed({ timeout: 15_000 });
}

// The embedded WebDriver dispatches its pointer actions as synthetic mouse events
// at the point, never as pointer events, so a WebDriver drag never reaches a
// splitter. The drag is dispatched at the same layer, as the pointer events a
// captured pointer delivers to the separator it was pressed on.
export async function dragTo(splitter: Splitter, x: number, y: number) {
  await browser.execute(
    (el: HTMLElement, toX: number, toY: number) => {
      const box = el.getBoundingClientRect();
      const at = (type: string, clientX: number, clientY: number, buttons: number) =>
        el.dispatchEvent(
          new PointerEvent(type, { bubbles: true, pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, buttons, clientX, clientY }),
        );
      at("pointerdown", box.left + box.width / 2, box.top + box.height / 2, 1);
      at("pointermove", toX, toY, 1);
      at("pointerup", toX, toY, 0);
    },
    (await splitter) as unknown as HTMLElement,
    Math.round(x),
    Math.round(y),
  );
}

export async function doubleClick(splitter: Splitter) {
  await browser.execute(
    (el: HTMLElement) => el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 })),
    (await splitter) as unknown as HTMLElement,
  );
}

// The embedded WebDriver's key table has no Home or End, so a splitter's keys are
// dispatched on it the way its own focus would receive them.
export async function press(splitter: Splitter, key: "ArrowLeft" | "ArrowRight" | "Home" | "End", times = 1) {
  const target = (await splitter) as unknown as HTMLElement;
  for (let i = 0; i < times; i++) {
    await browser.execute(
      (el: HTMLElement, name: string) => {
        el.focus();
        el.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
      },
      target,
      key,
    );
  }
}

export const viewport = () => browser.execute(() => ({ width: window.innerWidth, height: window.innerHeight }));

export async function clearTheCapture() {
  await browser.keys("Escape");
  await $('button[aria-label^="clear "]').click();
  const ask = $('[data-testid="clear-ask-panel"]');
  if (await ask.isExisting()) await ask.$("button=clear").click();
  await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });
}
