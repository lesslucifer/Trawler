import { expect, test, vi } from "vitest";
import {
  CORE_LOG_EVENT,
  CORE_STATE_EVENT,
  onCoreLog,
  onCoreState,
  readCoreLog,
  readCoreStatus,
  restartCore,
  type CoreState,
  type LogEntry,
} from "./coreService";

const line: LogEntry = { seq: 0, kind: "line", text: "INFO trawler: proxy listening" };

const running: CoreState = {
  state: "running",
  pid: 42,
  proxy: "127.0.0.1:7420",
  api: "127.0.0.1:7421",
  started_at_ms: 1000,
};

test("status, log and restart go to the shell's three core commands", async () => {
  const call = vi.fn(async (command: string) => (command === "core_log" ? [line] : running)) as never;

  await expect(readCoreStatus(call)).resolves.toEqual(running);
  await expect(readCoreLog(call)).resolves.toEqual([line]);
  await expect(restartCore(call)).resolves.toEqual(running);

  expect(call).toHaveBeenNthCalledWith(1, "core_status");
  expect(call).toHaveBeenNthCalledWith(2, "core_log");
  expect(call).toHaveBeenNthCalledWith(3, "restart_core");
});

test("a page with no app around it reads no service and no log", async () => {
  const unavailable = vi.fn(async () => {
    throw new Error("invoke is unavailable");
  }) as never;

  await expect(readCoreStatus(unavailable)).resolves.toBeNull();
  await expect(readCoreLog(unavailable)).resolves.toEqual([]);
});

test("a refused restart reaches its caller", async () => {
  const refuses = vi.fn(async () => {
    throw "the capture service at http://127.0.0.1:9 is not managed by this app";
  }) as never;

  await expect(restartCore(refuses)).rejects.toMatch("not managed");
});

test("state and log events hand over their payloads", async () => {
  const handlers = new Map<string, (event: { payload: unknown }) => void>();
  const on = vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(event, handler);
    return () => undefined;
  }) as never;
  const states: CoreState[] = [];
  const lines: LogEntry[] = [];

  await onCoreState((state) => states.push(state), on);
  await onCoreLog((entry) => lines.push(entry), on);
  handlers.get(CORE_STATE_EVENT)?.({ payload: { state: "exited", code: null, signal: 9 } });
  handlers.get(CORE_LOG_EVENT)?.({ payload: line });

  expect(states).toEqual([{ state: "exited", code: null, signal: 9 }]);
  expect(lines).toEqual([line]);
  expect([CORE_STATE_EVENT, CORE_LOG_EVENT]).toEqual(["core://state", "core://log"]);
});
