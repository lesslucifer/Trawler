import { browser, $, $$, expect } from "@wdio/globals";
import { execFileSync } from "node:child_process";
import { createServer, request, type Server } from "node:http";
import { createServer as createListener } from "node:net";
import type { AddressInfo } from "node:net";
import { quitTheApp } from "./quit.ts";

type Running = { state: "running"; pid: number; proxy: string; api: string };
type CoreState = Running | { state: "starting" | "exited" | "attached" };

const MAIN = "main";
const SETTINGS = "settings";
const ORIGIN_PATH = "/one-app";

async function invoke<T>(command: string): Promise<T> {
  const answer = await browser.executeAsync(
    (name: string, done: (answer: { ok?: unknown; refused?: string }) => void) => {
      (window as unknown as { __TAURI__: { core: { invoke: (c: string) => Promise<unknown> } } }).__TAURI__.core
        .invoke(name)
        .then(
          (ok) => done({ ok }),
          (refused: unknown) => done({ refused: String(refused) }),
        );
    },
    command,
  );
  if (answer.refused !== undefined) throw new Error(`${command} refused: ${answer.refused}`);
  return answer.ok as T;
}

async function runningOtherThan(pid: number | null): Promise<Running> {
  let state: CoreState | undefined;
  await browser.waitUntil(
    async () => {
      state = await invoke<CoreState>("core_status");
      return state.state === "running" && state.pid !== pid;
    },
    { timeout: 30_000, timeoutMsg: `the core never ran (last read ${JSON.stringify(state)})` },
  );
  return state as Running;
}

const portOf = (address: string) => Number(address.split(":").pop());

const isAlive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return (err as NodeJS.ErrnoException).code === "EPERM";
  }
};

const parentOf = (pid: number) => Number(execFileSync("ps", ["-o", "ppid=", "-p", String(pid)]).toString().trim());
const commandOf = (pid: number) => execFileSync("ps", ["-o", "comm=", "-p", String(pid)]).toString().trim();

const bindable = (port: number) =>
  new Promise<boolean>((done) => {
    const listener = createListener();
    listener.once("error", () => done(false));
    listener.listen(port, "127.0.0.1", () => listener.close(() => done(true)));
  });

function startOrigin(): Promise<Server> {
  return new Promise((ready) => {
    const origin = createServer((_, res) => res.end("hello from the one-app origin"));
    origin.listen(0, "127.0.0.1", () => ready(origin));
  });
}

function sendThroughProxy(proxy: string, url: string): Promise<number> {
  const [host, port] = proxy.split(":");
  return new Promise((answered, failed) => {
    const sent = request({ host, port: Number(port), method: "GET", path: url }, (res) => {
      res.resume();
      res.on("end", () => answered(res.statusCode ?? 0));
    });
    sent.once("error", failed);
    sent.end();
  });
}

async function onWindow(label: string) {
  await browser.waitUntil(async () => (await browser.getWindowHandles()).includes(label), {
    timeout: 15_000,
    timeoutMsg: `no ${label} window opened`,
  });
  await browser.switchToWindow(label);
}

const statusReads = (words: string) =>
  browser.waitUntil(async () => (await $("body").getText()).includes(words), {
    timeout: 30_000,
    timeoutMsg: `the Capture service page never read "${words}"`,
  });

// The epic's journeys J1–J3 over the app binary alone: no harness core, no attach
// address. The app spawns its own core; the e2e build lets the OS choose its
// ports, and this spec learns them from the app the way its own window does.
describe("E-ONEAPP — one launch is the whole app", () => {
  let origin: Server;
  let first: Running;
  let second: Running;
  let appPid: number;

  before(async () => {
    await $("header").waitForExist({ timeout: 30_000 });
    origin = await startOrigin();
    first = await runningOtherThan(null);
    appPid = parentOf(first.pid);
  });

  after(() => {
    origin?.close();
  });

  it("J1 · the app starts its own core and hands its window the core's address", async () => {
    expect(commandOf(appPid)).toMatch(/trawler-ui$/);
    expect(portOf(first.proxy)).not.toBe(7420);
    expect(portOf(first.api)).not.toBe(7421);
    const injected = await browser.execute(() => (window as unknown as { __TRAWLER_API__?: string }).__TRAWLER_API__);
    expect(injected).toBe(`http://${first.api}`);
  });

  it("J1 · a request through its proxy lands in the window's list and in GET /flows", async () => {
    const { port } = origin.address() as AddressInfo;
    const url = `http://127.0.0.1:${port}${ORIGIN_PATH}`;
    expect(await sendThroughProxy(first.proxy, url)).toBe(200);

    const wayBack = $('[data-testid="back"] button');
    if (await wayBack.isExisting()) await wayBack.click();
    await browser.waitUntil(
      async () => {
        for (const row of await $$('[data-testid="flow-row"]')) {
          if ((await row.getText()).includes(ORIGIN_PATH)) return true;
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `no row for ${ORIGIN_PATH}` },
    );

    const flows = (await (await fetch(`http://${first.api}/flows`)).json()) as { uri: string }[];
    expect(flows.some((flow) => flow.uri === url)).toBe(true);
  });

  it("J2 · the Capture service page shows the running core", async () => {
    await invoke("open_settings");
    await onWindow(SETTINGS);
    await $("h1=Capture service").waitForDisplayed({ timeout: 15_000 });
    await statusReads("running");
    expect(await $('[data-wire="process id"]').getText()).toBe(String(first.pid));
    expect(await $('[data-wire="proxy address"]').getText()).toContain(first.proxy);
    expect(await $('[data-wire="control API address"]').getText()).toContain(first.api);
    expect(await $('[data-testid="log-start"]').getText()).toContain(`process ${first.pid}`);
  });

  it("J2 · a killed core reads as killed, and the window survives it", async () => {
    process.kill(first.pid, "SIGKILL");
    await statusReads("killed (signal 9)");
    await browser.switchToWindow(MAIN);
    expect(await $("header").isExisting()).toBe(true);
    await browser.switchToWindow(SETTINGS);
  });

  it("J2 · restart brings the API back on the same address", async () => {
    await $("button=restart").click();
    second = await runningOtherThan(first.pid);
    expect(second.api).toBe(first.api);
    expect(second.proxy).toBe(first.proxy);
    expect((await fetch(`http://${second.api}/flows`)).status).toBe(200);
    await statusReads("running");
    await browser.waitUntil(async () => (await $('[data-wire="process id"]').getText()) === String(second.pid), {
      timeout: 15_000,
      timeoutMsg: `the page never showed process ${second.pid}`,
    });
    expect(await $$('[data-testid="log-start"]').length).toBe(2);
  });

  it("J3 · quitting from the window stops the core and frees its ports", async () => {
    await browser.switchToWindow(MAIN);
    await quitTheApp();
    await browser.waitUntil(async () => !isAlive(second.pid) && !isAlive(appPid), {
      timeout: 15_000,
      timeoutMsg: `still alive after the quit: ${[appPid, second.pid].filter(isAlive).join(", ")}`,
    });
    expect(await bindable(portOf(second.proxy))).toBe(true);
    expect(await bindable(portOf(second.api))).toBe(true);
  });
});
