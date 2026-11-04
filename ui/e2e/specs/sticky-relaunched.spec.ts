import { $, expect } from "@wdio/globals";
import {
  ANDROID_EMULATOR,
  SETTINGS_RECT,
  backToTraffic,
  bodyModeIs,
  closeSettings,
  corruptTheSavedViewPrefs,
  legIs,
  openRow,
  openSettings,
  openSetupFromTheMenu,
  platformPicker,
  quitTheApp,
  responseBody,
  settingsRect,
  shellIsUp,
} from "./sticky.ts";

// A new app process: the sticky spec quit the last one after choosing everything.
describe("UC-5 / UC-ONBOARD / UC-SVC — the choices survive a relaunch", () => {
  before(async () => {
    await shellIsUp();
  });

  it("opens on the list, not on the remembered platform's setup", async () => {
    await $('[data-testid="flow-row"]').waitForExist({ timeout: 15_000 });
    expect(await $('[data-testid="lead"]').isExisting()).toBe(false);
  });

  it("opens a flow on the leg and body mode chosen before the quit", async () => {
    await openRow(0);
    expect(await legIs("response")).toBe(true);
    expect(await bodyModeIs("raw")).toBe(true);
    await responseBody("text").waitForExist({ timeout: 15_000 });
  });

  it("opens setup the user asks for on the platform last named", async () => {
    await openSetupFromTheMenu();
    await expect(platformPicker()).toHaveText(expect.stringContaining(ANDROID_EMULATOR));
    await backToTraffic();
  });

  it("reopens the Settings window where it was left", async () => {
    await openSettings();
    expect(await settingsRect()).toEqual(SETTINGS_RECT);
    await closeSettings();
  });

  it("quits with a corrupt view-prefs.json left for the next launch", async () => {
    corruptTheSavedViewPrefs();
    await quitTheApp();
  });
});
