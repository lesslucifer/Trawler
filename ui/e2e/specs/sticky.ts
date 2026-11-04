import { browser, $, $$ } from "@wdio/globals";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { PORTS_FILE } from "../wdio.conf.ts";
import { CONFIG_DIR, WINDOWS_FILE } from "./layout.ts";

export { clearTheCapture, quitTheApp, shellIsUp } from "./layout.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { control: string };

export const VIEW_PREFS_FILE = resolve(CONFIG_DIR, "view-prefs.json");
export const SETTINGS = "settings";
export const SETTINGS_RECT = { x: 160, y: 120, width: 600, height: 520 };
export const ANDROID_EMULATOR = "Android emulator";

type Leg = "request" | "response";
type BodyMode = "pretty" | "raw";

const legGroup = () => $('[data-testid="detail"] header [role="radiogroup"]');
const legSegment = (leg: Leg) => legGroup().$(`button=${leg}`);
const responsePane = () => $('[data-testid="body-viewer"][data-leg="response"]');
const modeSegment = (mode: BodyMode) => responsePane().$('[role="radiogroup"]').$(`button=${mode}`);
const checked = async (segment: ReturnType<typeof $>) => (await segment.getAttribute("aria-checked")) === "true";

export const legIs = (leg: Leg) => checked(legSegment(leg));
export const bodyModeIs = (mode: BodyMode) => checked(modeSegment(mode));
export const responseBody = (kind: "json" | "text") => responsePane().$(`[data-testid="body-${kind}"]`);

export async function pickLeg(leg: Leg) {
  await legSegment(leg).click();
  await $(`[data-testid="body-viewer"][data-leg="${leg}"]`).waitForExist({ timeout: 15_000 });
}

export async function pickBodyMode(mode: BodyMode) {
  await modeSegment(mode).click();
  await browser.waitUntil(() => bodyModeIs(mode), { timeout: 5_000, timeoutMsg: `${mode} was never checked` });
}

export async function twoJsonFlows() {
  await fetch(`${ports.control}/hit?p=json`);
  await fetch(`${ports.control}/hit?p=json`);
  await browser.waitUntil(async () => (await $$('[data-testid="flow-row"]').length) >= 2, {
    timeout: 15_000,
    timeoutMsg: "the two proxied requests never appeared as rows",
  });
}

export async function openRow(index: number) {
  await (await $$('[data-testid="flow-row"]'))[index]!.click();
  await $('[data-testid="detail"]').waitForDisplayed({ timeout: 15_000 });
}

export function savedViewPrefs(): Record<string, unknown> | null {
  try {
    return JSON.parse(readFileSync(VIEW_PREFS_FILE, "utf8")) as Record<string, unknown>;
  } catch {
    return null;
  }
}

export async function savedViewPrefsInclude(expected: Record<string, unknown>) {
  await browser.waitUntil(
    async () => Object.entries(expected).every(([field, value]) => savedViewPrefs()?.[field] === value),
    { timeout: 5_000, timeoutMsg: `view-prefs.json never held ${JSON.stringify(expected)}: ${JSON.stringify(savedViewPrefs())}` },
  );
}

export function corruptTheSavedViewPrefs() {
  writeFileSync(VIEW_PREFS_FILE, '{"leg":"resp');
}

const tauriInvoke = (command: string) =>
  browser.executeAsync((name: string, done: () => void) => {
    (window as unknown as { __TAURI__: { core: { invoke: (c: string) => Promise<unknown> } } }).__TAURI__.core
      .invoke(name)
      .then(done, done);
  }, command);

export async function openSettings() {
  await tauriInvoke("open_settings");
  await browser.waitUntil(async () => (await browser.getWindowHandles()).includes(SETTINGS), {
    timeout: 15_000,
    timeoutMsg: "no settings window opened",
  });
  await browser.switchToWindow(SETTINGS);
  await $("h1").waitForDisplayed({ timeout: 15_000 });
}

export async function closeSettings() {
  await browser.execute(() => {
    const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { destroy: () => Promise<void> } } } })
      .__TAURI__;
    setTimeout(() => void tauri.window.getCurrentWindow().destroy(), 50);
  });
  await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes(SETTINGS), {
    timeout: 15_000,
    timeoutMsg: "the settings window never closed",
  });
  await browser.switchToWindow("main");
}

type Rect = typeof SETTINGS_RECT;

// The embedded WebDriver sizes and reads only the main window, so the Settings
// window is placed and read through its own Tauri window, in logical points.
export function placeSettings(rect: Rect): Promise<string | null> {
  return browser.executeAsync((to: Rect, done: (refused: string | null) => void) => {
    const tauri = (window as unknown as { __TAURI__: TauriWindowGlobals }).__TAURI__;
    const current = tauri.window.getCurrentWindow();
    current
      .setSize(new tauri.dpi.LogicalSize(to.width, to.height))
      .then(() => current.setPosition(new tauri.dpi.LogicalPosition(to.x, to.y)))
      .then(
        () => done(null),
        (refusal: unknown) => done(String(refusal)),
      );
  }, rect);
}

export function settingsRect(): Promise<Rect> {
  return browser.executeAsync((done: (rect: Rect) => void) => {
    const current = (window as unknown as { __TAURI__: TauriWindowGlobals }).__TAURI__.window.getCurrentWindow();
    void Promise.all([current.scaleFactor(), current.outerPosition(), current.innerSize()]).then(([scale, at, size]) =>
      done({
        x: Math.round(at.x / scale),
        y: Math.round(at.y / scale),
        width: Math.round(size.width / scale),
        height: Math.round(size.height / scale),
      }),
    );
  });
}

type Physical = { x: number; y: number; width: number; height: number };
type TauriWindowGlobals = {
  dpi: {
    LogicalSize: new (width: number, height: number) => unknown;
    LogicalPosition: new (x: number, y: number) => unknown;
  };
  window: {
    getCurrentWindow: () => {
      setSize: (size: unknown) => Promise<void>;
      setPosition: (position: unknown) => Promise<void>;
      scaleFactor: () => Promise<number>;
      outerPosition: () => Promise<Pick<Physical, "x" | "y">>;
      innerSize: () => Promise<Pick<Physical, "width" | "height">>;
    };
  };
};

export function settingsIsSaved() {
  try {
    return SETTINGS in (JSON.parse(readFileSync(WINDOWS_FILE, "utf8")) as Record<string, unknown>);
  } catch {
    return false;
  }
}

const QUESTION = "which device's traffic do you want to see";
export const platformPicker = () => $(`[aria-label="${QUESTION}"][aria-haspopup="listbox"]`);

export async function openSetupFromTheMenu() {
  await $('button[aria-label="capture actions"]').click();
  const item = $('[role="menuitem"][aria-label="set up another device"]');
  await item.waitForDisplayed({ timeout: 15_000 });
  await item.click();
  await $('[data-testid="lead"]').waitForExist({ timeout: 15_000 });
}

export async function choosePlatform(label: string) {
  await platformPicker().click();
  await $(`[role="option"][aria-label="${label}"]`).click();
  await browser.waitUntil(async () => !(await $('[role="listbox"]').isExisting()), {
    timeout: 5_000,
    timeoutMsg: `the picker stayed open after choosing ${label}`,
  });
}

export async function backToTraffic() {
  await $('//button[starts-with(normalize-space(.), "‹ show traffic")]').click();
  await $('[aria-label="filter"]').waitForExist({ timeout: 15_000 });
}
