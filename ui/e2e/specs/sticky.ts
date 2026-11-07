import { browser, $, $$ } from "@wdio/globals";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { PORTS_FILE } from "../wdio.conf.ts";
import { CONFIG_DIR } from "./layout.ts";

export { clearTheCapture, quitTheApp, shellIsUp } from "./layout.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { control: string };

export const VIEW_PREFS_FILE = resolve(CONFIG_DIR, "view-prefs.json");
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
