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

// UC-5 (E-UISETUP) end to end: the real Tauri window against a real ephemeral core.
// A fresh launch with no decrypted flow shows onboarding; a real proxied HTTPS
// request (the harness /hit) is decrypted by the core, and the Verify check flips
// to proven and reveals the flow list. This spec MUST run before flows.spec.ts —
// once capture is proven the app latches everProven and never re-forces onboarding.
async function pointAppAtCore() {
  // sessionStorage survives the reload; the fresh page reads it before React
  // mounts, so both the launch /flows read and the SSE stream bind the port.
  await browser.execute((api: string) => {
    window.sessionStorage.setItem("__TRAWLER_API__", api);
    location.reload();
  }, ports.api);
  await $("header").waitForExist({ timeout: 15_000 });
}

describe("UC-5 — onboard to the first captured flow", () => {
  before(async () => {
    await pointAppAtCore();
  });

  it("a fresh launch shows onboarding, not the flow list", async () => {
    await expect($('[data-testid="step-trust-ca"]')).toBeExisting();
    await expect($('[data-testid="step-verify"]')).toBeExisting();
    await expect($('[aria-label="host"]')).not.toBeExisting();
  });

  it("a decrypted HTTPS flow flips Verify green and reveals the list", async () => {
    await hit();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "onboarding never revealed the list after a decrypted flow",
    });
    await expect($('[data-testid="step-trust-ca"]')).not.toBeExisting();
    await expect($('[data-testid="flow-row"]')).toHaveText(expect.stringContaining("localhost"));
  });

  // Re-invoke (status dot) + Back-to-flows navigation is covered at the integration
  // layer (App.test.tsx); the e2e proves the capture-driven journey spine, not the
  // 10px dot-click interaction.

  // UC-4 list behaviour, now reached through onboarding: the revealed list filters
  // and appends live exactly as before W4.
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
