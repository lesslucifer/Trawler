import { browser, $, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { api: string };

// J4's new half: this suite's app drives the harness core through the attach
// address, so the Capture service page must say the app does not own it.
describe("UC-SVC — an attached core is shown as attached", () => {
  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
  });

  after(async () => {
    await browser.execute(() => {
      const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { destroy: () => Promise<void> } } } })
        .__TAURI__;
      setTimeout(() => void tauri.window.getCurrentWindow().destroy(), 50);
    });
    await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes("settings"), {
      timeout: 15_000,
      timeoutMsg: "the settings window never closed",
    });
    await browser.switchToWindow("main");
  });

  it("names the harness's address, offers no log, and no restart", async () => {
    await browser.executeAsync((done: () => void) => {
      (window as unknown as { __TAURI__: { core: { invoke: (c: string) => Promise<unknown> } } }).__TAURI__.core
        .invoke("open_settings")
        .then(done, done);
    });
    await browser.waitUntil(async () => (await browser.getWindowHandles()).includes("settings"), {
      timeout: 15_000,
      timeoutMsg: "no settings window opened",
    });
    await browser.switchToWindow("settings");
    await $("button=Capture service").click();
    await $("h1=Capture service").waitForDisplayed({ timeout: 15_000 });

    expect(await $("body").getText()).toContain("attached — started outside this app");
    expect(await $('[data-wire="control API address"]').getText()).toContain(ports.api);
    expect(await $("body").getText()).toContain("can't restart it or show its log");
    expect(await $('[role="log"]').isExisting()).toBe(false);
    expect(await $("button=restart").isExisting()).toBe(false);
  });
});
