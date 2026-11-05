import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const here = dirname(fileURLToPath(import.meta.url));

// The macOS-primary target has no WKWebView WebDriver for raw tauri-driver, so
// the e2e drives the real window through @wdio/tauri-service's embedded provider.
// The app is built with the `e2e` cargo feature so the wdio plugins are present.
export const APP_BINARY = resolve(here, "../../target/release/trawler-ui");

// The launcher (this config) and the test worker are separate processes, so the
// core's ports cross to the spec via this file — never a shared variable.
export const PORTS_FILE = resolve(here, ".core-ports.json");

// `cargo xtask ui e2e` reads this after wdio exits and is red unless every spec
// file passed: a service that fails before any spec starts leaves wdio green.
const RUN_TALLY = resolve(here, ".run-tally.json");

// This variable attaches the app to the run's core (attach mode): it spawns none and
// injects the address before the first script runs (`core_address`). The launcher spawns
// the app, so exporting it here is what puts it in the app's environment — and it
// is why no spec ever navigates the window: a reload mid-run stops the WKWebView
// scheme tasks the Tauri IPC rides on, and answering a stopped one aborts the app.
const CORE_ADDRESS_VAR = "TRAWLER_API";

// The app writes its dismissed-notice markers, its saved layout, its saved window
// geometry and its view choices into the real user config dir, so they outlive a run. A marker
// left by an earlier one would silence the clear ask the capture spec exists to
// assert, and the spec's own last case leaves one behind; a layout or a window the
// user sized would open the run at their sizes, and a leg or body mode chosen
// would open every flow on it; the layout and sticky specs leave all of these
// behind — so the run starts and ends without them.
const SAVED_STATE = [
  "clear-ask-dismissed",
  "remote-notice-dismissed",
  "layout.json",
  "windows.json",
  "view-prefs.json",
].map((name) => resolve(process.env.HOME ?? "", "Library/Application Support/dev.trawler.app", name));

function clearSavedState() {
  for (const file of SAVED_STATE) rmSync(file, { force: true });
}

// The shell reads the Android emulator through the first SDK that holds adb,
// ANDROID_HOME before Android Studio's, so a run reads this SDK and never the
// host's: one booted emulator, its user store readable and empty, no proxy set.
// Whether this host runs an emulator no longer decides the onboarding journey.
const FAKE_ADB = `#!/bin/sh
[ "$1" = "-s" ] && shift 2
case "$*" in
  devices) printf 'List of devices attached\\nemulator-5554\\tdevice\\n\\n' ;;
  "shell getprop sys.boot_completed") echo 1 ;;
  "shell getprop ro.boot.qemu.avd_name") echo Trawler_E2E ;;
  "shell settings get global http_proxy") echo null ;;
  "shell dumpsys connectivity") printf 'Active default network: 100\\n  NetworkAgentInfo{network{100} handle{1} ni{WIFI CONNECTED}}\\n' ;;
  "shell su 0 sh -c"*) echo trawler-su ;;
  *) exit 1 ;;
esac
`;

let fakeSdk: string | undefined;

function useFakeSdk() {
  fakeSdk = mkdtempSync(resolve(tmpdir(), "trawler-e2e-sdk-"));
  const tools = resolve(fakeSdk, "platform-tools");
  mkdirSync(tools);
  mkdirSync(resolve(fakeSdk, "avd"));
  writeFileSync(resolve(tools, "adb"), FAKE_ADB);
  chmodSync(resolve(tools, "adb"), 0o755);
  process.env.ANDROID_HOME = fakeSdk;
  process.env.ANDROID_AVD_HOME = resolve(fakeSdk, "avd");
}

// One core for the whole run, specs sequential (maxInstances 1) — so a captured
// flow persists across specs. Onboarding's "fresh launch shows onboarding" is only
// true before any flow exists, so it MUST run first. The flows spec MUST run last:
// its closing pair kills the core to prove the shell states a silence, and the
// harness exits the process rather than draining it, so nothing can follow. The
// capture and replay specs sit between them — each leaves the capture empty, which
// the flows spec refills from its own hits. Order is explicit, not
// alphabetical-by-accident. The three layout specs run in this order because each
// of the first two ends by quitting the app, and the launcher launches it again
// for the next file: that relaunch is what the second and third assert. The three
// sticky specs are the same shape for the user's view choices, and the two
// query-bar specs for the chips.
const SPECS = [
  "./specs/onboarding.spec.ts",
  "./specs/capture.spec.ts",
  "./specs/replay.spec.ts",
  "./specs/service-attached.spec.ts",
  "./specs/sticky.spec.ts",
  "./specs/sticky-relaunched.spec.ts",
  "./specs/sticky-broken.spec.ts",
  "./specs/layout.spec.ts",
  "./specs/layout-relaunched.spec.ts",
  "./specs/layout-broken.spec.ts",
  "./specs/data-window.spec.ts",
  "./specs/inspect-window-v2.spec.ts",
  "./specs/query-bar.spec.ts",
  "./specs/query-bar-relaunched.spec.ts",
  "./specs/flows.spec.ts",
];

let harness: ChildProcessWithoutNullStreams | undefined;

function startCore(): Promise<void> {
  return new Promise((res, rej) => {
    harness = spawn("cargo", ["run", "--quiet"], {
      cwd: resolve(here, "harness"),
      stdio: ["ignore", "pipe", "inherit"],
    });
    const rl = createInterface({ input: harness.stdout });
    const timer = setTimeout(() => rej(new Error("core harness did not report ports")), 120_000);
    rl.once("line", (line) => {
      clearTimeout(timer);
      writeFileSync(PORTS_FILE, line);
      process.env[CORE_ADDRESS_VAR] = JSON.parse(line).api;
      rl.close();
      res();
    });
    harness.once("error", rej);
  });
}

// UC-ONBOARD's proof is a captured row, and a row the core sent itself is not
// one (R3). The launch read is what that rule governs, so the window must come
// up over a core that already holds a sent https row — which only the launcher
// can arrange, since the window launches once, after onPrepare.
async function seedSentFlow(): Promise<void> {
  const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8"));
  const sent = await fetch(`${ports.api}/flows`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ method: "GET", url: ports.origin }),
  });
  if (sent.status !== 201) throw new Error(`the core refused the seed send: ${sent.status}`);
}

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: SPECS,
  maxInstances: 1,

  services: [["@wdio/tauri-service", { driverProvider: "embedded" }]],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: APP_BINARY },
    } as WebdriverIO.Capabilities,
  ],

  logLevel: "warn",
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 2,

  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 60_000 },

  onPrepare: async () => {
    clearSavedState();
    useFakeSdk();
    await startCore();
    await seedSentFlow();
  },
  onComplete: async (_exitCode, _config, _capabilities, results) => {
    writeFileSync(
      RUN_TALLY,
      JSON.stringify({
        specs: SPECS.length,
        finished: results.finished,
        passed: results.passed,
        failed: results.failed,
      }),
    );
    try {
      const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8"));
      await fetch(`${ports.control}/stop`).catch(() => {});
    } catch {
      /* ports file may not exist if startup failed */
    }
    harness?.kill();
    rmSync(PORTS_FILE, { force: true });
    if (fakeSdk) rmSync(fakeSdk, { recursive: true, force: true });
    clearSavedState();
  },
};
