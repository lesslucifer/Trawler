import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { readFileSync, writeFileSync, rmSync } from "node:fs";
import { findRun, type FrontMode, type Prelaunch } from "./script.ts";

const here = dirname(fileURLToPath(import.meta.url));

// Its own target dir, so no e2e run ever drives a binary built with the
// scenario feature, and no walkthrough drives one built without it.
const APP_BINARY = resolve(here, "../../src-tauri/target/scenario/release/trawler-ui");

export const PORTS_FILE = resolve(here, ".walkthrough-ports.json");

// The app saves its layout and its window geometry into the real user config dir;
// a layout or a window the user sized would film the reels at their sizes instead
// of the configured ones.
const SAVED_SIZES = ["layout.json", "windows.json"].map((name) =>
  resolve(process.env.HOME ?? "", "Library/Application Support/dev.trawler.app", name),
);

export const RUN_VAR = "TRAWLER_WALKTHROUGH_RUN";
export const OUT_VAR = "TRAWLER_WALKTHROUGH_OUT";

export interface Ports {
  api: string;
  proxy: string;
  origin: string;
  control: string;
  front: string;
}

const run = findRun(process.env[RUN_VAR] ?? "");

let harness: ChildProcessWithoutNullStreams | undefined;

function startCore(): Promise<Ports> {
  return new Promise((res, rej) => {
    harness = spawn("cargo", ["run", "--quiet"], {
      cwd: resolve(here, "../harness"),
      stdio: ["ignore", "pipe", "inherit"],
    });
    const rl = createInterface({ input: harness.stdout });
    const timer = setTimeout(() => rej(new Error("core harness did not report ports")), 120_000);
    rl.once("line", (line) => {
      clearTimeout(timer);
      writeFileSync(PORTS_FILE, line);
      rl.close();
      res(JSON.parse(line) as Ports);
    });
    harness.once("error", rej);
  });
}

export function frontQuery(mode: FrontMode): string {
  return typeof mode === "string" ? `mode=${mode}` : `mode=slow&ms=${mode.slow}`;
}

async function ok(response: Promise<Response>, what: string) {
  const answer = await response;
  if (!answer.ok) throw new Error(`${what}: ${answer.status} ${await answer.text()}`);
}

const HAR_ROW = (origin: string) =>
  JSON.stringify({
    log: {
      version: "1.2",
      entries: [
        {
          startedDateTime: new Date().toISOString(),
          time: 9,
          request: { method: "GET", url: `${origin}imported`, httpVersion: "HTTP/1.1", headers: [], bodySize: 0 },
          response: {
            status: 200,
            httpVersion: "HTTP/1.1",
            headers: [],
            content: { size: 0, mimeType: "text/plain" },
          },
        },
      ],
    },
  });

// Everything a launch meets is in place before the app starts: its first read of
// the core is the moment several chapters film.
async function prepare(ports: Ports, prelaunch: Prelaunch) {
  for (let i = 0; i < (prelaunch.hits ?? 0); i++) await ok(fetch(`${ports.control}/hit`), "hit");
  if (prelaunch.seed === "imported")
    await ok(fetch(`${ports.api}/flows/import`, { method: "POST", body: HAR_ROW(ports.origin) }), "import");
  if (prelaunch.seed === "sent")
    await ok(
      fetch(`${ports.api}/flows`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ method: "GET", url: ports.origin }),
      }),
      "send",
    );
  if (prelaunch.interfaces)
    await ok(
      fetch(`${ports.control}/front/reach`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(prelaunch.interfaces),
      }),
      "front reach",
    );
  if (prelaunch.front) await ok(fetch(`${ports.control}/front?${frontQuery(prelaunch.front)}`), "front");
}

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: [resolve(here, "walkthrough.spec.ts")],
  maxInstances: 1,

  // The app's own output is kept, so a reel the app died in names its panic.
  services: [["@wdio/tauri-service", { driverProvider: "embedded", captureBackendLogs: true }]],
  outputDir: join(process.env[OUT_VAR] ?? "", "logs", `${run.reel.id}-${run.launch.id}`),
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
  mochaOpts: { ui: "bdd", timeout: 90_000, bail: true },

  onPrepare: async () => {
    for (const file of SAVED_SIZES) rmSync(file, { force: true });
    const ports = await startCore();
    await prepare(ports, run.launch.prelaunch);
    process.env.TRAWLER_E2E_API = ports.front;
    process.env.TRAWLER_SCENARIO_HOST = run.reel.host;
    process.env.TRAWLER_SCENARIO_SCRIPT = JSON.stringify(run.launch.prelaunch.scenario ?? {});
  },
  onComplete: async () => {
    try {
      const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as Ports;
      await fetch(`${ports.control}/stop`).catch(() => {});
    } catch {
      /* ports file may not exist if startup failed */
    }
    harness?.kill();
    rmSync(PORTS_FILE, { force: true });
    for (const file of SAVED_SIZES) rmSync(file, { force: true });
  },
};
