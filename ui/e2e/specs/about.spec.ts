import { browser, $, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { closeTheDialog, dialogShown, invoke, openSettings } from "./app.ts";

const here = dirname(fileURLToPath(import.meta.url));
const RUNNING_VERSION = (JSON.parse(readFileSync(resolve(here, "../../package.json"), "utf8")) as { version: string })
  .version;

// UC-ABOUT: the version the user reads is the one this build runs, and About is
// a dialog of the main window on the page its menu item names (W19).
describe("UC-ABOUT — the running version, Settings and About as dialogs", () => {
  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
  });

  it("Settings → General reads the version this app runs, in a dialog of the main window", async () => {
    await openSettings();
    expect(await browser.getWindowHandles()).toEqual(["main"]);
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    const version = await $('[aria-label="copy version"]');
    await version.waitForDisplayed({ timeout: 15_000 });
    expect(await version.getText()).toBe(RUNNING_VERSION);
    await closeTheDialog("settings");
  });

  it("Help → Third-Party Licences opens About on its licences page, and Esc closes it", async () => {
    await invoke("e2e_menu", { id: "trawler-licences" });
    await dialogShown("about");
    expect(await browser.getWindowHandles()).toEqual(["main"]);
    expect(await $('[data-testid="about-panel"] [role="tab"][aria-selected="true"]').getText()).toBe(
      "Third-party licences",
    );
    await browser.keys("Escape");
    await $('[data-testid="about-dialog"]').waitForExist({ reverse: true, timeout: 15_000 });
  });
});
