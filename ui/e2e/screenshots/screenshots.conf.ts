import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { readFileSync, writeFileSync, rmSync } from "node:fs";

const here = dirname(fileURLToPath(import.meta.url));

// The walkthrough's scenario build: the setup screen reads this machine's
// certificate and proxy from a script, so a shot shows a state without changing
// the host.
const APP_BINARY = resolve(here, "../../src-tauri/target/scenario/release/trawler-ui");

export const PORTS_FILE = resolve(here, ".screenshots-ports.json");
export const LAUNCH_VAR = "TRAWLER_SCREENSHOTS_LAUNCH";
export const OUT_VAR = "TRAWLER_SCREENSHOTS_OUT";

export interface Ports {
  api: string;
  proxy: string;
  origin: string;
  control: string;
  front: string;
}

export const ETHERNET = { name: "en0", address: "192.168.1.23" };
export const OURS = { set: { host: "127.0.0.1", port: 7420 } };

const attached = process.env[LAUNCH_VAR] === "attached";

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

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: [resolve(here, "screenshots.spec.ts")],
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
  mochaOpts: { ui: "bdd", timeout: 90_000, bail: true },

  onPrepare: async () => {
    delete process.env.TRAWLER_API;
    process.env.TRAWLER_SCENARIO_SCRIPT = JSON.stringify({
      interface_names: { en0: "Ethernet" },
      trust: "trusted",
      proxy: OURS,
    });
    if (!attached) return;
    const ports = await startCore();
    const reach = await fetch(`${ports.control}/front/reach`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify([ETHERNET]),
    });
    if (!reach.ok) throw new Error(`harness /front/reach: ${reach.status}`);
    process.env.TRAWLER_API = ports.front;
  },
  onComplete: async () => {
    if (!attached) return;
    try {
      const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as Ports;
      await fetch(`${ports.control}/stop`).catch(() => {});
    } catch {
      /* ports file may not exist if startup failed */
    }
    harness?.kill();
    rmSync(PORTS_FILE, { force: true });
  },
};
