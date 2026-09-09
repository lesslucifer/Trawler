import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { readFileSync, writeFileSync, rmSync } from "node:fs";

const here = dirname(fileURLToPath(import.meta.url));

// The macOS-primary target has no WKWebView WebDriver for raw tauri-driver, so
// the e2e drives the real window through @wdio/tauri-service's embedded provider.
// The app is built with the `e2e` cargo feature so the wdio plugins are present.
const APP_BINARY = resolve(here, "../src-tauri/target/release/trawler-ui");

// The launcher (this config) and the test worker are separate processes, so the
// core's ports cross to the spec via this file — never a shared variable.
export const PORTS_FILE = resolve(here, ".core-ports.json");

// The app reads its core's address from this variable at window creation, before
// its first script runs (`e2e_core_address` in the shell). The launcher spawns
// the app, so exporting it here is what puts it in the app's environment — and it
// is why no spec ever navigates the window: a reload mid-run stops the WKWebView
// scheme tasks the Tauri IPC rides on, and answering a stopped one aborts the app.
const CORE_ADDRESS_VAR = "TRAWLER_E2E_API";

// The app writes its dismissed-notice markers into the real user config dir, so
// they outlive a run. A marker left by an earlier one would silence the clear ask
// the capture spec exists to assert, and the spec's own last case leaves one
// behind — so the run starts and ends without them.
const NOTICE_MARKERS = [
  "clear-ask-dismissed",
  "remote-notice-dismissed",
].map((name) => resolve(process.env.HOME ?? "", "Library/Application Support/dev.trawler.app", name));

function clearNoticeMarkers() {
  for (const marker of NOTICE_MARKERS) rmSync(marker, { force: true });
}

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

export const config: WebdriverIO.Config = {
  runner: "local",
  // One core for the whole run, specs sequential (maxInstances 1) — so a captured
  // flow persists across specs. Onboarding's "fresh launch shows onboarding" is only
  // true before any flow exists, so it MUST run first. The flows spec MUST run last:
  // its closing pair kills the core to prove the shell states a silence, and the
  // harness exits the process rather than draining it, so nothing can follow. The
  // capture spec sits between them — it empties the capture, which the flows spec
  // refills from its own hits. Order is explicit, not alphabetical-by-accident.
  specs: ["./specs/onboarding.spec.ts", "./specs/capture.spec.ts", "./specs/flows.spec.ts"],
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
    clearNoticeMarkers();
    await startCore();
  },
  onComplete: async () => {
    try {
      const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8"));
      await fetch(`${ports.control}/stop`).catch(() => {});
    } catch {
      /* ports file may not exist if startup failed */
    }
    harness?.kill();
    rmSync(PORTS_FILE, { force: true });
    clearNoticeMarkers();
  },
};
