import { $, expect } from "@wdio/globals";
import { theAppExits } from "./quit.ts";
import { appOf, askToQuitTheApp, everythingStopped, runningCore, storedKeepCapturing, type Running } from "./background.ts";

// W3/W9: with keep capturing on, a request to quit the app (⌘Q, the menu-bar Quit
// reaching an open window) runs DEC-64's check and ends the app instead of only
// hiding the window. Nothing here routes through Trawler, so nothing is asked.
describe("UC-BG — a quit request quits even where ✕ would keep capturing", () => {
  let core: Running;
  let appPid: number;

  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
    core = await runningCore();
    appPid = appOf(core.pid);
  });

  it("ends the app with its ports freed", async () => {
    expect(storedKeepCapturing()).toBe(true);
    await askToQuitTheApp();
    await theAppExits();
    expect(await everythingStopped(core, appPid)).toBe(true);
  });
});
