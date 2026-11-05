import { browser, $, expect } from "@wdio/globals";
import { createServer, request, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { theAppExits } from "./quit.ts";
import {
  appOf,
  askToCloseTheWindow,
  everythingStopped,
  forgetTheAnswer,
  invoke,
  isAlive,
  onWindow,
  pressButton,
  runningCore,
  storedKeepCapturing,
  switchesRead,
  type Running,
} from "./background.ts";

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

// J1: the answer that makes the app outlive its window. Starts unanswered, ends
// with quit_app from the Settings window, the one window left to ask from.
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

  it("Keep capturing stores on and closes only the main window", async () => {
    await invoke("open_settings");
    await onWindow("settings");
    await browser.switchToWindow("main");
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
  });

  it("General, still driven, reads keep capturing on", async () => {
    await browser.switchToWindow("settings");
    await $('[role="tab"]=Capture service').click();
    await $('[role="tab"]=General').click();
    await $("h1=General").waitForDisplayed({ timeout: 15_000 });
    expect((await switchesRead())[0].on).toBe(true);
  });

  it("a request through the proxy is captured with no window open", async () => {
    const { port } = origin.address() as AddressInfo;
    const url = `http://127.0.0.1:${port}/background`;
    expect(await sendThroughProxy(core.proxy, url)).toBe(200);
    const flows = (await (await fetch(`http://${core.api}/flows`)).json()) as { uri: string }[];
    expect(flows.some((flow) => flow.uri === url)).toBe(true);
  });

  it("quit_app ends the app and frees both ports", async () => {
    await invoke("quit_app");
    await theAppExits();
    expect(await everythingStopped(core, appPid)).toBe(true);
  });
});
