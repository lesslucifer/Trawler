import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { APP_BINARY } from "./wdio.conf.ts";

const here = dirname(fileURLToPath(import.meta.url));

// The owned suite: the app is launched with no attach address, so it spawns its
// own core, which the e2e build puts on ephemeral ports. Its own spec learns them
// from the app itself, so nothing crosses from this launcher but the environment.
const RUN_TALLY = resolve(here, ".run-tally-one-app.json");
// Each background file ends by quitting the app, and the launcher launches it again
// for the next: the stored answer one leaves is what the next asserts.
const SPECS = [
  "./specs/one-app.spec.ts",
  "./specs/background-asked.spec.ts",
  "./specs/background-off.spec.ts",
  "./specs/background-keep.spec.ts",
  "./specs/background-quit.spec.ts",
];

// The app keeps its CA, markers, layout and window sizes under HOME and finds
// Android through these variables. Pointing them at a dir the run deletes keeps
// the host's Trawler data and emulators out of it, and the run out of them.
let home: string | undefined;

function useTempHome() {
  home = mkdtempSync(resolve(tmpdir(), "trawler-e2e-home-"));
  const sdk = resolve(home, "android-sdk");
  mkdirSync(sdk);
  process.env.HOME = home;
  for (const name of ["ANDROID_HOME", "ANDROID_SDK_ROOT", "ANDROID_AVD_HOME", "ANDROID_USER_HOME"]) {
    process.env[name] = sdk;
  }
  delete process.env.TRAWLER_API;
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

  onPrepare: () => {
    useTempHome();
  },
  onComplete: (_exitCode, _config, _capabilities, results) => {
    writeFileSync(
      RUN_TALLY,
      JSON.stringify({
        specs: SPECS.length,
        finished: results.finished,
        passed: results.passed,
        failed: results.failed,
      }),
    );
    if (home) rmSync(home, { recursive: true, force: true });
  },
};
