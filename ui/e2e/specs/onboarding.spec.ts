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

const QUESTION = "which device's traffic do you want to see";
// The host platform's option is qualified by the OS the app read, and this
// suite's target is macOS — so the label is exact, not guessed.
const THIS_MACHINE = "This machine \u2014 macOS";
const picker = () => $(`[aria-label="${QUESTION}"][aria-haspopup="listbox"]`);

// The platform axis is a Select, so an answer is two clicks: open the trigger,
// then the option. Both are matched by aria-label — the component's own
// contract — never by partial text, which the embedded WebKit provider silently
// fails to match.
async function choosePlatform(label: string) {
  await picker().click();
  await $(`[role="option"][aria-label="${label}"]`).click();
  await browser.waitUntil(async () => !(await $('[role="listbox"]').isExisting()), {
    timeout: 5_000,
    timeoutMsg: `the picker stayed open after choosing ${label}`,
  });
}

// The window is already bound to this run's core: the launcher handed it the
// address in the environment and the shell injected it before the first script
// ran, so a spec only waits for the screen it painted. Nothing here navigates —
// a reload stops the WKWebView scheme tasks the Tauri IPC rides on, and wry
// answers a stopped one with an Objective-C exception that aborts the app.
async function shellIsUp() {
  await $("header").waitForExist({ timeout: 15_000 });
}

// UC-ONBOARD end to end, on the axis E-ONBOARD rebuilt the screen around: the
// user names the platform they are setting up, and every instruction below that
// answer belongs to it.
//
// Two journeys, both through the real Tauri window against a real ephemeral
// core. THIS MACHINE is the payoff journey — a real proxied HTTPS request is
// decrypted by the core and that, and nothing else, ends onboarding. ANOTHER
// PLATFORM is the journey W1 exists for, and it runs FIRST: capture latches
// everProven, and after the reveal the app never re-forces setup.
//
// What journey 2 does NOT drive, and why. Arming reach is an authorization act
// (DEC-55) and the shell asks before it binds — confirm_reach opens a native
// message dialog, which WebDriver cannot reach from inside the WKWebView. Nor
// can the arming be staged around it: App reads reach once on mount and re-reads
// it only from the two calls that change it, so a POST /reach from here would
// bind a listener this window never learns of, and the suite runs one core and
// one window, so there is no second launch that could come up already armed.
// Stubbing the confirm under the e2e feature was refused: that feature adds the
// wdio plugins and changes no behaviour, and an e2e that proved arming by
// removing its authorization would be proving the wrong thing. So journey 2
// asserts the unarmed half — the half that has to be honest — and the armed
// rendering stays covered at integration (App.test.tsx) and unit.
describe("UC-ONBOARD — set up capture, on the platform axis", () => {
  before(async () => {
    await shellIsUp();
  });

  it("a fresh launch leads with what this screen is, not with the flow list", async () => {
    await expect($('[data-testid="lead"]')).toBeExisting();
    await expect($('[data-testid="platform"]')).toBeExisting();
    await expect($('[data-testid="steps"]')).toBeExisting();
    await expect($('[aria-label="host"]')).not.toBeExisting();
  });

  it("this machine is already the answer, and its three tasks are numbered and read at once", async () => {
    await expect(picker()).toHaveText(expect.stringContaining(THIS_MACHINE));

    await expect($('[data-testid="task-1"]')).toBeExisting();
    await expect($('[data-testid="task-2"]')).toBeExisting();
    await expect($('[data-testid="task-3"]')).toBeExisting();

    // The successor to "each step carries a verdict". The two probed tasks
    // carry a state the machine was read for; the third's state is the traffic
    // layer's and lives in its body beside the remedy, which is why it is read
    // through the task rather than through a state slot. On a core nothing has
    // reached yet, that reading is deterministic.
    await expect($('[data-testid="task-1-state"]')).toBeExisting();
    await expect($('[data-testid="task-2-state"]')).toBeExisting();
    await expect($('[data-testid="task-3"]')).toHaveText(
      expect.stringContaining("waiting for the first request"),
    );

    // Nothing here is waiting on a precondition, so nothing is dimmed.
    await expect($$('[data-dim="true"]')).toBeElementsArrayOfSize(0);

    // The probes answer for this machine, so the row that dates them is here.
    await expect($('[data-testid="probe"]')).toBeExisting();
  });

  it("there is no wizard: no cursor, no prev/next pair, and no step that hides another", async () => {
    await expect($('[data-testid="onb-next"]')).not.toBeExisting();
    await expect($('[data-testid="onb-back"]')).not.toBeExisting();
    // The accordion retired with the stepper: a task is always open. What
    // collapses now is a guide, whose rule is at most one open — asserted as an
    // invariant over all eight platforms in the unit suite, where every platform
    // can be reached.
    await expect($('[data-testid="step-body"]')).not.toBeExisting();
  });

  // Journey 2. It runs before any flow exists, because the reveal is one way.
  it("an off-machine platform states what is missing once and offers no step that cannot work", async () => {
    await choosePlatform("iPhone or iPad");

    // Reach becomes task 1 and pushes the rest down: the axis changed every
    // instruction below it, which is the whole claim of the redesign.
    const network = $('[data-testid="task-1"]');
    await expect(network).toHaveText(expect.stringContaining("not done"));
    await expect($('[data-testid="task-4"]')).toBeExisting();

    // The three tasks that depend on reach are dimmed rather than given steps
    // that would fail — and they keep their numbers, because a number is a
    // reading order and not a claim about progress.
    for (const n of [2, 3, 4]) {
      await expect($(`[data-testid="task-${n}"]`)).toHaveAttribute("data-dim", "true");
    }

    // Nothing here could read this phone, so nothing pretends to: no probe row,
    // and no address to point a device at until reach is armed.
    await expect($('[data-testid="probe"]')).not.toBeExisting();
    await expect($('[data-testid="device-address"]')).not.toBeExisting();
  });

  it("answering the picker again restores this machine's own tasks", async () => {
    await choosePlatform(THIS_MACHINE);

    await expect($('[data-dim="true"]')).not.toBeExisting();
    await expect($('[data-testid="task-4"]')).not.toBeExisting();
    await expect($('[data-testid="probe"]')).toBeExisting();
  });

  // Journey 1's payoff. A probe-green task and a bare connection both refuse to
  // end onboarding; only a decrypted https flow does (DEC-13).
  it("a decrypted HTTPS flow ends setup, reveals the list and says so", async () => {
    await hit();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "setup never revealed the list after a decrypted flow",
    });

    await expect($('[data-testid="lead"]')).not.toBeExisting();
    await expect($('[data-testid="flow-row"]')).toHaveText(expect.stringContaining("localhost"));

    // The moment setup succeeds is the list's first paint, not the setup
    // screen's last — and it offers the next device rather than only closing.
    const banner = $('[data-testid="set-up-banner"]');
    await expect(banner).toBeExisting();
    await expect(banner).toHaveText(expect.stringContaining("set up another device"));
  });

  // Re-invoke (status dot) + Back-to-flows navigation is covered at the integration
  // layer (App.test.tsx); the e2e proves the capture-driven journey spine, not the
  // 10px dot-click interaction.

  // Lifecycle wiring is exercised here implicitly: this app boots through the Tauri
  // setup hook (launch reconciliation) and quits through the RunEvent::ExitRequested
  // net — a broken hook or capability would fail this launch/quit. The quit-time
  // confirm (App.tsx onCloseRequested), the unset_proxy / proxy_is_set /
  // clear_proxy_marker commands, the marker gate and native off-argvs are unit-tested
  // in src-tauri + closeConfirm.test.tsx; the real proxy-off is elevation-gated and
  // not hermetically runnable, so it is not driven from the UI here.

  // UC-4 list behaviour, now reached through setup: the revealed list filters
  // and appends live exactly as before.
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
