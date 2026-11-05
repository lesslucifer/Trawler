import { browser } from "@wdio/globals";
import { execFileSync } from "node:child_process";
import { createServer as createListener } from "node:net";
import { existsSync, readFileSync, realpathSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, sep } from "node:path";

export type Running = { state: "running"; pid: number; proxy: string; api: string };

// The launcher points HOME at a directory it deletes, so this is the run's own file.
const KEEP_FILE = resolve(process.env.HOME ?? "", "Library/Application Support/dev.trawler.app/keep-capturing.json");

export const storedKeepCapturing = () =>
  existsSync(KEEP_FILE) ? (JSON.parse(readFileSync(KEEP_FILE, "utf8")) as { keepCapturing: boolean }).keepCapturing : null;

export const forgetTheAnswer = () => {
  const home = realpathSync(process.env.HOME ?? "");
  if (!home.startsWith(realpathSync(tmpdir()) + sep)) {
    throw new Error(`refusing to forget the answer under HOME=${home}: run through one-app.conf.ts, whose HOME is a temp dir`);
  }
  if (existsSync(KEEP_FILE)) unlinkSync(KEEP_FILE);
};

export async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (name: string, payload: Record<string, unknown>, done: (answer: { ok?: unknown; refused?: string }) => void) => {
      (window as unknown as { __TAURI__: { core: { invoke: (c: string, a: object) => Promise<unknown> } } }).__TAURI__.core
        .invoke(name, payload)
        .then(
          (ok) => done({ ok }),
          (refused: unknown) => done({ refused: String(refused) }),
        );
    },
    command,
    args,
  );
  if (answer.refused !== undefined) throw new Error(`${command} refused: ${answer.refused}`);
  return answer.ok as T;
}

export async function runningCore(): Promise<Running> {
  let state: { state: string } | undefined;
  await browser.waitUntil(
    async () => {
      state = await invoke<{ state: string }>("core_status");
      return state.state === "running";
    },
    { timeout: 30_000, timeoutMsg: `the core never ran (last read ${JSON.stringify(state)})` },
  );
  return state as Running;
}

export const portOf = (address: string) => Number(address.split(":").pop());

export const isAlive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return (err as NodeJS.ErrnoException).code === "EPERM";
  }
};

export const appOf = (corePid: number) =>
  Number(execFileSync("ps", ["-o", "ppid=", "-p", String(corePid)]).toString().trim());

const bindable = (port: number) =>
  new Promise<boolean>((done) => {
    const listener = createListener();
    listener.once("error", () => done(false));
    listener.listen(port, "127.0.0.1", () => listener.close(() => done(true)));
  });

export async function everythingStopped(core: Running, appPid: number) {
  await browser.waitUntil(async () => !isAlive(core.pid) && !isAlive(appPid), {
    timeout: 15_000,
    timeoutMsg: `still alive: ${[appPid, core.pid].filter(isAlive).join(", ")}`,
  });
  return (await bindable(portOf(core.proxy))) && (await bindable(portOf(core.api)));
}

export const askToCloseTheWindow = () =>
  browser.execute(() => {
    const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { close: () => Promise<void> } } } })
      .__TAURI__;
    setTimeout(() => void tauri.window.getCurrentWindow().close(), 50);
  });

export const askToQuitTheApp = () => invoke("e2e_request_quit");

export const switchesRead = () =>
  browser.execute(() =>
    Array.from(document.querySelectorAll('label:has(input[type="checkbox"])')).map((label) => ({
      words: label.textContent?.trim() ?? "",
      on: (label.querySelector("input") as HTMLInputElement).checked,
    })),
  );

export const pressButton = (dialog: string, label: string) =>
  browser.execute(
    (testId: string, words: string) => {
      const buttons = document.querySelectorAll(`[data-testid="${testId}-panel"] button`);
      const match = Array.from(buttons).find((b) => b.textContent?.trim().toLowerCase() === words);
      (match as HTMLButtonElement | undefined)?.click();
      return match !== undefined;
    },
    dialog,
    label,
  );

export async function onWindow(label: string) {
  await browser.waitUntil(async () => (await browser.getWindowHandles()).includes(label), {
    timeout: 15_000,
    timeoutMsg: `no ${label} window opened`,
  });
  await browser.switchToWindow(label);
}
