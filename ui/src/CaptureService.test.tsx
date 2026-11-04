import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test } from "vitest";
import { CaptureService } from "./CaptureService";
import { line, RUNNING, coreStub } from "./coreStub";
import type { CoreState } from "./coreService";

async function shown(state: CoreState, log = [line(0, "INFO trawler: proxy listening")]) {
  const stub = coreStub({ state, log });
  render(<CaptureService bridge={stub.bridge} />);
  await waitFor(() => expect(stub.call).toHaveBeenCalledWith("core_log"));
  return stub;
}

const restarts = (stub: ReturnType<typeof coreStub>) =>
  stub.call.mock.calls.filter(([command]) => command === "restart_core").length;

test("running shows its process, start time, both addresses and the log", async () => {
  await shown(RUNNING);
  expect(await screen.findByText("running")).toBeInTheDocument();
  expect(screen.getByText("48213")).toBeInTheDocument();
  expect(screen.getByText("14:10:04")).toBeInTheDocument();
  expect(screen.getByText("127.0.0.1:7420")).toBeInTheDocument();
  expect(screen.getByText("127.0.0.1:7421")).toBeInTheDocument();
  expect(screen.getByRole("log")).toHaveTextContent("INFO trawler: proxy listening");
});

test("starting holds restart disabled beside what it waits for", async () => {
  await shown({ state: "starting" });
  expect(await screen.findByText("starting")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "restart" })).toBeDisabled();
  expect(screen.getByText("waiting for the capture service to start…")).toBeInTheDocument();
});

test("an exit names its code, and a kill its signal", async () => {
  const stub = await shown(RUNNING);
  act(() => stub.emitState({ state: "exited", code: 101, signal: null }));
  expect(screen.getByText("stopped — exit code 101")).toBeInTheDocument();
  expect(screen.getByText("127.0.0.1:7420")).toBeInTheDocument();
  act(() => stub.emitState({ state: "exited", code: null, signal: 9 }));
  expect(screen.getByText("stopped — killed (signal 9)")).toBeInTheDocument();
});

test("restart asks first while a core runs, and a cancel sends nothing", async () => {
  const stub = await shown(RUNNING);
  fireEvent.click(await screen.findByRole("button", { name: "restart" }));
  expect(screen.getByText("Restarting clears the captured list")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "cancel" }));
  expect(screen.queryByText("Restarting clears the captured list")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "restart" }));
  fireEvent.keyDown(screen.getByText("Restarting clears the captured list"), { key: "Escape" });
  expect(screen.queryByText("Restarting clears the captured list")).toBeNull();
  expect(restarts(stub)).toBe(0);
});

test("a confirmed restart is sent once, held while it runs", async () => {
  let finish: () => void = () => {};
  const stub = coreStub({
    state: RUNNING,
    restart: () => new Promise((resolve) => (finish = () => resolve({ ...RUNNING, pid: 7 }))),
  });
  render(<CaptureService bridge={stub.bridge} />);
  fireEvent.click(await screen.findByRole("button", { name: "restart" }));
  fireEvent.click(screen.getByRole("button", { name: "restart" }));
  expect(screen.getByRole("button", { name: "restart" })).toBeDisabled();
  expect(screen.getByText("waiting for the capture service to start…")).toBeInTheDocument();
  await act(async () => finish());
  expect(screen.getByText("7")).toBeInTheDocument();
  expect(restarts(stub)).toBe(1);
});

test("an exited core restarts at once: nothing is left to clear", async () => {
  const stub = await shown({ state: "exited", code: 101, signal: null });
  fireEvent.click(await screen.findByRole("button", { name: "restart" }));
  expect(screen.queryByText("Restarting clears the captured list")).toBeNull();
  await waitFor(() => expect(restarts(stub)).toBe(1));
});

test("a refused restart is said, with a way to try again", async () => {
  const stub = coreStub({
    state: { state: "exited", code: 1, signal: null },
    restart: () => Promise.reject("Another program is using port 7420. Quit it, then try again."),
  });
  render(<CaptureService bridge={stub.bridge} />);
  fireEvent.click(await screen.findByRole("button", { name: "restart" }));
  expect(await screen.findByText("The capture service couldn't restart")).toBeInTheDocument();
  expect(screen.getByText("Another program is using port 7420. Quit it, then try again.")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "try again" }));
  await waitFor(() => expect(restarts(stub)).toBe(2));
});

test("attached shows its address and offers no restart and no log", async () => {
  await shown({ state: "attached", api: "http://127.0.0.1:52817" });
  expect(await screen.findByText("attached — started outside this app")).toBeInTheDocument();
  expect(screen.getByText("http://127.0.0.1:52817")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "restart" })).toBeNull();
  expect(screen.queryByRole("log")).toBeNull();
});

test("each core's lines begin under its start mark", async () => {
  await shown(RUNNING, [
    { seq: 0, kind: "start", pid: 48207, at_ms: RUNNING.started_at_ms },
    line(1, "ERROR trawler: proxy stopped"),
  ]);
  const mark = await screen.findByTestId("log-start");
  expect(mark).toHaveTextContent("started 14:10:04 · process 48207");
});

test("the log follows new lines only while it is at the bottom", async () => {
  const stub = await shown(RUNNING);
  const log = await screen.findByRole("log");
  let scrollTop = 0;
  Object.defineProperty(log, "scrollHeight", { configurable: true, get: () => 1000 });
  Object.defineProperty(log, "clientHeight", { configurable: true, get: () => 200 });
  Object.defineProperty(log, "scrollTop", {
    configurable: true,
    get: () => scrollTop,
    set: (value: number) => (scrollTop = value),
  });

  act(() => stub.emitLog(line(1, "next")));
  expect(scrollTop).toBe(1000);

  scrollTop = 100;
  fireEvent.scroll(log);
  act(() => stub.emitLog(line(2, "later")));
  expect(scrollTop).toBe(100);

  fireEvent.click(screen.getByRole("button", { name: "show newest" }));
  expect(scrollTop).toBe(1000);
  expect(screen.queryByRole("button", { name: "show newest" })).toBeNull();
});
