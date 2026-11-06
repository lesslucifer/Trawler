import { browser, $, expect } from "@wdio/globals";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { theAppExits } from "./quit.ts";
import {
  appOf,
  askToCloseTheWindow,
  dialogShown,
  everythingStopped,
  forgetTheAnswer,
  invoke,
  isAlive,
  onWindow,
  pressButton,
  runningCore,
  sendThroughProxy,
  storedKeepCapturing,
  switchesRead,
  type Running,
} from "./app.ts";

// J1: the answer that makes the app outlive its window. Starts unanswered; the
// menu-bar icon's Settings… then rebuilds the main window with Settings over it
// (W19), the window quit_app is asked from.
const TRAY_SETTINGS = "tray-settings";
const WINDOWLESS_MS = 5_000;
describe("UC-BG — keep capturing with no window", () => {
  let origin: Server;
  let core: Running;
  let appPid: number;

  before(async () => {
    forgetTheAnswer();
    origin = await new Promise<Server>((ready) => {
      const server = createServer((_, res) => res.end("hello from the background origin"));
      server.listen(0, "127.0.0.1", () => ready(server));
    });
    await $("header").waitForExist({ timeout: 30_000 });
    core = await runningCore();
    appPid = appOf(core.pid);
  });

  after(() => {
    origin?.close();
  });

  it("Keep capturing stores on, closes the main window, and a request is still captured", async () => {
    await invoke("e2e_tray_when_windowless", { id: TRAY_SETTINGS, afterMs: WINDOWLESS_MS });
    await askToCloseTheWindow();
    await $('[data-testid="keep-capturing-dialog"]').waitForDisplayed({ timeout: 15_000 });
    expect(await pressButton("keep-capturing", "keep capturing")).toBe(true);
    await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes("main"), {
      timeout: 15_000,
      timeoutMsg: "the main window never closed",
    });
    expect(storedKeepCapturing()).toBe(true);
    expect(isAlive(appPid)).toBe(true);
    expect(isAlive(core.pid)).toBe(true);

    const { port } = origin.address() as AddressInfo;
    const url = `http://127.0.0.1:${port}/background`;
    expect(await sendThroughProxy(core.proxy, url)).toBe(200);
    expect(await browser.getWindowHandles()).toEqual([]);
    await browser.waitUntil(
      async () => {
        const flows = (await (await fetch(`http://${core.api}/flows`)).json()) as { uri: string }[];
        return flows.some((flow) => flow.uri === url);
      },
      { timeout: 15_000, timeoutMsg: `${url} never reached GET /flows` },
    );
  });

  it("the menu-bar Settings… rebuilds the main window with Settings over it, reading keep capturing on", async () => {
    await onWindow("main");
    await dialogShown("settings");
    expect(await browser.getWindowHandles()).toEqual(["main"]);
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    expect((await switchesRead())[0].on).toBe(true);
  });

  it("quit_app ends the app and frees both ports", async () => {
    await invoke("quit_app");
    await theAppExits();
    expect(await everythingStopped(core, appPid)).toBe(true);
  });
});
