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

// The window is already bound to this run's core: the launcher handed it the
// address in the environment and the shell injected it before the first script
// ran, so a spec only waits for the screen it painted. Nothing here navigates —
// a reload stops the WKWebView scheme tasks the Tauri IPC rides on, and wry
// answers a stopped one with an Objective-C exception that aborts the app.
async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

// UC-ONBOARD (E-UISETUP, rebuilt by E-REDESIGN.W10) end to end: the real Tauri
// window against a real ephemeral core.
// A fresh launch with no decrypted flow shows onboarding; a real proxied HTTPS
// request (the harness /hit) is decrypted by the core, and the Verify check flips
// to proven and reveals the flow list. The onboarding assertions must come before
// the list ones (the tests below are ordered), since capture latches everProven and
// the app then never re-forces onboarding — so the UC-4 list checks are folded in
// here after the reveal rather than living in a separate spec.
describe("UC-ONBOARD — onboard to the first captured flow", () => {
  before(async () => {
    await shellIsUp();
  });

  it("a fresh launch shows onboarding, not the flow list", async () => {
    await expect($('[data-testid="step-trust-ca"]')).toBeExisting();
    await expect($('[data-testid="step-verify"]')).toBeExisting();
    await expect($('[aria-label="host"]')).not.toBeExisting();
  });

  // The redesigned surface: a checklist of probed verdicts, not a wizard. The
  // navigation it deliberately does not ship is as much the contract as the
  // verdicts it does.
  it("each step carries a verdict, and there is no prev/next pair", async () => {
    await expect($('[data-testid="step-trust-ca-status"]')).toBeExisting();
    await expect($('[data-testid="step-set-proxy-status"]')).toBeExisting();
    await expect($('[data-testid="step-verify-status"]')).toBeExisting();

    await expect($('[data-testid="onb-next"]')).not.toBeExisting();
    await expect($('[data-testid="onb-back"]')).not.toBeExisting();
  });

  it("a step opens from its own header, one body at a time", async () => {
    await $('[data-testid="step-set-proxy"] button').click();
    await browser.waitUntil(
      async () => (await $('[data-testid="step-set-proxy"]').getAttribute("data-status")) === "active",
      { timeout: 5_000, timeoutMsg: "clicking a step header did not open it" },
    );
    await expect($$('[data-testid="step-body"]')).toBeElementsArrayOfSize(1);
  });

  it("a decrypted HTTPS flow flips the last step green and reveals the list", async () => {
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

  // W5 (proxy auto-revert) lifecycle wiring is exercised here implicitly: this app
  // boots through the Tauri setup hook (launch reconciliation) and quits through the
  // RunEvent::ExitRequested net — a broken hook or capability would fail this
  // launch/quit. The quit-time confirm (App.tsx onCloseRequested), the unset_proxy /
  // proxy_is_set / clear_proxy_marker commands, the marker gate and native off-argvs
  // are unit-tested in src-tauri + closeConfirm.test.tsx; the real proxy-off is
  // elevation-gated and not hermetically runnable (epic waiver W5 inherits), so it is
  // not driven from the UI here.

  // UC-4 list behaviour, now reached through onboarding: the revealed list filters
  // and appends live exactly as before W4.
  it("filters the row out on a non-matching query and back in when cleared", async () => {
    // Host is a dropdown of what capture has seen, so an unmatchable value is no
    // longer expressible there; the free-text key is where an arbitrary query lives.
    await $('[aria-label="filter"]').setValue("no-such-path.test");
    await browser.waitUntil(async () => (await rowCount()) === 0, {
      timeout: 15_000,
      timeoutMsg: "non-matching filter did not empty the list",
    });
    await $('[aria-label="filter"]').clearValue();
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
