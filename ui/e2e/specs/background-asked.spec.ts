import { browser, $, expect } from "@wdio/globals";
import { theAppExits } from "./quit.ts";
import {
  appOf,
  askToCloseTheWindow,
  everythingStopped,
  forgetTheAnswer,
  pressButton,
  runningCore,
  storedKeepCapturing,
  type Running,
} from "./background.ts";

// UC-BG's once-question: this file starts with no stored answer and ends in a real
// exit, so the next file is a new app process that must find "off" stored.
describe("UC-BG — the window's ✕ asks once whether to keep capturing", () => {
  let core: Running;
  let appPid: number;

  before(async () => {
    forgetTheAnswer();
    await $("header").waitForExist({ timeout: 30_000 });
    core = await runningCore();
    appPid = appOf(core.pid);
  });

  it("asks, and Esc cancels: the window stays, nothing is stored", async () => {
    await askToCloseTheWindow();
    await $('[data-testid="keep-capturing-dialog"]').waitForDisplayed({ timeout: 15_000 });
    await browser.keys("Escape");
    await $('[data-testid="keep-capturing-dialog"]').waitForDisplayed({ reverse: true });
    expect(await $("header").isExisting()).toBe(true);
    expect(storedKeepCapturing()).toBeNull();
  });

  it("asks again at the next ✕; Quit stores off and the app ends with its ports freed", async () => {
    await askToCloseTheWindow();
    await $('[data-testid="keep-capturing-dialog"]').waitForDisplayed({ timeout: 15_000 });
    expect(await pressButton("keep-capturing", "quit")).toBe(true);
    await theAppExits();
    expect(storedKeepCapturing()).toBe(false);
    expect(await everythingStopped(core, appPid)).toBe(true);
  });
});
