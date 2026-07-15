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
  // true before any flow exists, so it MUST run first; the inspector spec then reuses
  // the revealed list. Order is explicit, not alphabetical-by-accident.
  specs: ["./specs/onboarding.spec.ts", "./specs/flows.spec.ts"],
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
  },
};
