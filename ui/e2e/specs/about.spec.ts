import { browser, $, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { invoke, onWindow } from "./app.ts";

const here = dirname(fileURLToPath(import.meta.url));
const RUNNING_VERSION = (JSON.parse(readFileSync(resolve(here, "../../package.json"), "utf8")) as { version: string })
  .version;

// UC-ABOUT: the version the user reads is the one this build runs.
describe("UC-ABOUT — the running version in Settings", () => {
  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
  });

  it("Settings → General reads the version this app runs", async () => {
    await invoke("open_settings");
    await onWindow("settings");
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    const version = await $('[aria-label="copy version"]');
    await version.waitForDisplayed({ timeout: 15_000 });
    expect(await version.getText()).toBe(RUNNING_VERSION);
    await browser.closeWindow();
    await browser.switchToWindow("main");
  });
});
