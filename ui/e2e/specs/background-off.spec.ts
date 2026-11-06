import { $, expect } from "@wdio/globals";
import { theAppExits } from "./quit.ts";
import {
  appOf,
  askToCloseTheWindow,
  everythingStopped,
  openSettings,
  runningCore,
  storedKeepCapturing,
  switchesRead,
  type Running,
} from "./app.ts";

// The stored answer is off (the previous file answered Quit): nothing is asked and
// ✕ ends the app. The login switch of an unbundled
// build says why it cannot be written, so a run never touches this machine's own.
// General's update switch loads on its own and is not one of the two.
const bothSwitches = async () =>
  (await switchesRead()).filter((box) => !box.words.includes("updates")).map((box) => box.on);

describe("UC-BG — nothing switched, nothing changed", () => {
  let core: Running;
  let appPid: number;

  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
    core = await runningCore();
    appPid = appOf(core.pid);
  });

  it("General shows both switches off", async () => {
    expect(storedKeepCapturing()).toBe(false);
    await openSettings();
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    expect(await bothSwitches()).toEqual([false, false]);
  });

  it("an unbundled build refuses the login item with its reason, and the switch stays off", async () => {
    await $('//label[contains(., "open Trawler at login")]').click();
    await $('//*[contains(text(), "couldn\'t save")]').waitForExist({ timeout: 15_000 });
    expect(await bothSwitches()).toEqual([false, false]);
  });

  it("✕ over the Settings dialog ends the app with no question and its ports freed", async () => {
    await askToCloseTheWindow();
    await theAppExits();
    expect(await everythingStopped(core, appPid)).toBe(true);
  });
});
