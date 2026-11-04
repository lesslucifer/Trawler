import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test } from "vitest";
import { line, RUNNING, coreStub } from "./coreStub";
import { mergeLog, useCoreLog, useCoreService } from "./useCoreService";

test("an in-order arrival is appended", () => {
  expect(mergeLog([line(0, "a")], [line(1, "b"), line(2, "c")])).toEqual([
    line(0, "a"),
    line(1, "b"),
    line(2, "c"),
  ]);
});

test("a line heard live and read in the snapshot shows once, in sequence order", () => {
  expect(mergeLog([line(5, "live")], [line(4, "old"), line(5, "live")])).toEqual([
    line(4, "old"),
    line(5, "live"),
  ]);
});

test("the tail keeps only its newest entries", () => {
  const many = Array.from({ length: 2005 }, (_, seq) => line(seq, `line ${seq}`));
  const kept = mergeLog([], many);
  expect(kept).toHaveLength(2000);
  expect(kept[0].seq).toBe(5);
  expect(mergeLog([line(0, "a"), line(1, "b")], [line(2, "c")], 2)).toEqual([line(1, "b"), line(2, "c")]);
});

test("the state is read once, then followed", async () => {
  const stub = coreStub({ state: RUNNING });
  const { result } = renderHook(() => useCoreService(stub.bridge));
  await waitFor(() => expect(result.current.state).toEqual(RUNNING));
  act(() => stub.emitState({ state: "exited", code: null, signal: 9 }));
  expect(result.current.state).toEqual({ state: "exited", code: null, signal: 9 });
});

test("an event heard before the first read answers is not overwritten by it", async () => {
  let answer: (state: typeof RUNNING) => void = () => {};
  const stub = coreStub();
  stub.call.mockImplementation((command: string) =>
    command === "core_status" ? new Promise((resolve) => (answer = resolve)) : Promise.resolve([]),
  );
  const { result } = renderHook(() => useCoreService(stub.bridge));
  await waitFor(() => expect(stub.on).toHaveBeenCalled());
  act(() => stub.emitState({ state: "starting" }));
  await act(async () => answer(RUNNING));
  expect(result.current.state).toEqual({ state: "starting" });
});

test("a restart is in flight until it answers, then its state stands", async () => {
  let finish: () => void = () => {};
  const restarted = { ...RUNNING, pid: 1 };
  const stub = coreStub({
    state: RUNNING,
    restart: () => new Promise((resolve) => (finish = () => resolve(restarted))),
  });
  const { result } = renderHook(() => useCoreService(stub.bridge));
  act(() => void result.current.restart());
  expect(result.current.restarting).toBe(true);
  await act(async () => finish());
  expect(result.current.restarting).toBe(false);
  expect(result.current.state).toEqual(restarted);
  expect(stub.call.mock.calls.filter(([command]) => command === "restart_core")).toHaveLength(1);
});

test("a refused restart is kept as its sentence", async () => {
  const stub = coreStub({
    state: RUNNING,
    restart: () => Promise.reject("Another program is using port 7420. Quit it, then try again."),
  });
  const { result } = renderHook(() => useCoreService(stub.bridge));
  await act(() => result.current.restart());
  expect(result.current.refusal).toBe("Another program is using port 7420. Quit it, then try again.");
  expect(result.current.restarting).toBe(false);
});

test("the log joins its snapshot and live lines, and lets go on unmount", async () => {
  const stub = coreStub({ log: [line(0, "snapshot")] });
  const { result, unmount } = renderHook(() => useCoreLog(stub.bridge));
  await waitFor(() => expect(result.current).toEqual([line(0, "snapshot")]));
  act(() => stub.emitLog(line(1, "live")));
  act(() => stub.emitLog(line(1, "live")));
  expect(result.current).toEqual([line(0, "snapshot"), line(1, "live")]);
  unmount();
  await waitFor(() => expect(stub.unlisten).toHaveBeenCalled());
});
