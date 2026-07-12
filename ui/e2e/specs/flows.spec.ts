import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  proxy: string;
  origin: string;
  control: string;
};

const rowCount = () => $$('[data-testid="flow-row"]').length;
const hit = () => fetch(`${ports.control}/hit`);

// UC-4 end to end: the real Tauri window against a real ephemeral core. Point the
// app at the core's API port (runtime config, no rebuild) with a plain WebDriver
// eval — no Tauri invoke bridge — then prove the live flow list, filtering, and
// SSE-driven append.
async function pointAppAtCore() {
  // sessionStorage survives the reload; the fresh page reads it before React
  // mounts, so both the fetch and the SSE stream bind the ephemeral port.
  await browser.execute((api: string) => {
    window.sessionStorage.setItem("__TRAWLER_API__", api);
    location.reload();
  }, ports.api);
  await $("header").waitForExist({ timeout: 15_000 });
}

describe("UC-4 — watch live traffic in the native app", () => {
  before(async () => {
    await pointAppAtCore();
  });

  it("renders the base layout with an empty flow list", async () => {
    await expect($("header")).toBeExisting();
    await expect($('[aria-label="host"]')).toBeExisting();
    await expect($('[data-testid="empty"]')).toBeExisting();
  });

  it("shows a proxied request as a live flow row", async () => {
    await hit();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "flow row never appeared after a proxied request",
    });
    await expect($('[data-testid="flow-row"]')).toHaveText(expect.stringContaining("127.0.0.1"));
  });

  it("filters the row out on a non-matching host and back in when cleared", async () => {
    await $('[aria-label="host"]').setValue("no-such-host.test");
    await browser.waitUntil(async () => (await rowCount()) === 0, {
      timeout: 15_000,
      timeoutMsg: "non-matching filter did not empty the list",
    });
    await $('[aria-label="host"]').setValue("");
    await browser.waitUntil(async () => (await rowCount()) >= 1, { timeout: 15_000 });
  });

  it("appends a second row on a new request without a manual refresh", async () => {
    const before = await rowCount();
    await hit();
    await browser.waitUntil(async () => (await rowCount()) > before, {
      timeout: 15_000,
      timeoutMsg: "second proxied request did not append a row live",
    });
  });
});
