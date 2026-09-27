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
const dot = () => $('[data-testid="status-dot"]');
const successLine = () => $('[data-testid="proven"]');
const tryTask = () => $('[data-testid="task-3"]');
const button = (prefix: string) => $(`//button[starts-with(normalize-space(.), "${prefix}")]`);

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
// Through the real Tauri window against a real ephemeral core, whose launcher
// sent one https request before the window came up, so the launch read finds a
// row the core sent itself. THIS MACHINE is the payoff journey — a real proxied
// HTTPS request is decrypted by the core and that, and nothing else, ends
// onboarding. ANOTHER PLATFORM runs before it, because capture latches
// everProven and after the reveal the app never re-forces setup. The two
// re-entry journeys (E-ONBREV) run after it for the same reason: each is a new
// setup session on a capture already proven.
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

  it("a launch over a core holding only a sent row leads with setup, not with the flow list", async () => {
    await expect($('[data-testid="lead"]')).toBeExisting();
    await expect($('[data-testid="flow-row"]')).not.toBeExisting();
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
    // The seeded row was in the core all along: setup stood over it.
    await expect($('[data-testid="flow-row"] [aria-label="sent"]')).toBeExisting();

    // The moment setup succeeds is the list's first paint, not the setup
    // screen's last — and it offers the next device rather than only closing.
    const banner = $('[data-testid="set-up-banner"]');
    await expect(banner).toBeExisting();
    await expect(banner).toHaveText(expect.stringContaining("set up another device"));
  });

  // A second device is its own setup session: the first device's success is
  // not this one's, so nothing reads as set up until a new request arrives.
  it("setting up another device keeps capture connected and answers no device", async () => {
    await button("set up another device").click();

    await expect($('[data-testid="lead"]')).toBeExisting();
    await expect(dot()).toHaveAttribute("data-status", "connected");
    await expect($('[data-testid="task-1"]')).not.toBeExisting();
    await expect(successLine()).not.toBeExisting();
  });

  it("the device chosen opens its first guide, and only its own request says set up", async () => {
    await choosePlatform("Android emulator");

    await expect($('[data-testid="task-1"] details[open]')).toBeExisting();
    await expect(tryTask()).toHaveText(expect.stringContaining("waiting for the first request"));
    await expect(successLine()).not.toBeExisting();

    await hit();
    await successLine().waitForExist({ timeout: 15_000, timeoutMsg: "no success line after the new request" });
    await expect(tryTask()).toHaveText(expect.stringContaining("capturing traffic"));
  });

  // Setup reads its own session unfiltered: a list filtered to nothing is the
  // list's view, never the setup screen's evidence.
  it("re-entry with the list filtered to nothing still reads the new request", async () => {
    await button("‹ back to flows").click();
    await $('[aria-label="filter"]').setValue("no-such-path.test");
    await browser.waitUntil(async () => (await rowCount()) === 0, {
      timeout: 15_000,
      timeoutMsg: "non-matching filter did not empty the list",
    });

    await dot().click();
    await expect(picker()).toHaveText(expect.stringContaining("Android emulator"));
    await expect(tryTask()).toHaveText(expect.stringContaining("waiting for the first request"));
    await expect(successLine()).not.toBeExisting();

    await hit();
    await successLine().waitForExist({ timeout: 15_000, timeoutMsg: "the filtered re-entry never read the request" });
    await expect(tryTask()).toHaveText(expect.stringContaining("capturing traffic"));
  });

  it("back from setup keeps the list's filter, and clearing it brings the rows back", async () => {
    await button("‹ back to flows").click();
    await $('[aria-label="filter"]').waitForExist({ timeout: 15_000 });
    expect(await rowCount()).toBe(0);

    await $('[aria-label="filter"]').clearValue();
    await browser.waitUntil(async () => (await rowCount()) >= 1, { timeout: 15_000 });
  });

  // Lifecycle wiring is exercised here implicitly: this app boots through the Tauri
  // setup hook (launch reconciliation) and quits through the RunEvent::ExitRequested
  // net — a broken hook or capability would fail this launch/quit. The quit-time
  // confirm (App.tsx onCloseRequested), the unset_proxy / proxy_is_set /
  // clear_proxy_marker commands, the marker gate and native off-argvs are unit-tested
  // in src-tauri + closeConfirm.test.tsx; the real proxy-off is elevation-gated and
  // not hermetically runnable, so it is not driven from the UI here.

  it("appends a second row on a new request without a manual refresh", async () => {
    const before = await rowCount();
    await hit();
    await browser.waitUntil(async () => (await rowCount()) > before, {
      timeout: 15_000,
      timeoutMsg: "second proxied request did not append a row live",
    });
  });
});
