import { browser, expect } from "@wdio/globals";
import {
  ANDROID_EMULATOR,
  SETTINGS_RECT,
  backToTraffic,
  bodyModeIs,
  choosePlatform,
  closeSettings,
  legIs,
  openRow,
  openSettings,
  openSetupFromTheMenu,
  pickBodyMode,
  pickLeg,
  placeSettings,
  quitTheApp,
  responseBody,
  savedViewPrefsInclude,
  settingsIsSaved,
  settingsRect,
  shellIsUp,
  twoJsonFlows,
} from "./sticky.ts";

// A choice belongs to the user, not the flow. Ends in a real quit: the next spec
// file is a new app process that must find every choice again.
describe("UC-5 / UC-ONBOARD / UC-SVC — what the user chose stays chosen", () => {
  before(async () => {
    await shellIsUp();
    await twoJsonFlows();
  });

  it("opens the next flow on the leg and body mode picked on the last one", async () => {
    await openRow(1);
    await pickLeg("response");
    await pickBodyMode("raw");

    await openRow(0);
    expect(await legIs("response")).toBe(true);
    expect(await bodyModeIs("raw")).toBe(true);
    await responseBody("text").waitForExist({ timeout: 15_000 });
    expect(await responseBody("json").isExisting()).toBe(false);
  });

  it("remembers the platform the user names in setup", async () => {
    await openSetupFromTheMenu();
    await choosePlatform(ANDROID_EMULATOR);
    await backToTraffic();
    await savedViewPrefsInclude({ leg: "response", bodyMode: "raw", lastPlatform: "android-sim" });
  });

  it("records where the Settings window was left, then quits", async () => {
    await openSettings();
    expect(await placeSettings(SETTINGS_RECT)).toBeNull();
    expect(await settingsRect()).toEqual(SETTINGS_RECT);
    await closeSettings();
    await browser.waitUntil(async () => settingsIsSaved(), {
      timeout: 5_000,
      timeoutMsg: "windows.json never recorded the settings window",
    });

    await quitTheApp();
  });
});
