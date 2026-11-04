import { vi } from "vitest";
import { CORE_LOG_EVENT, CORE_STATE_EVENT, type CoreState, type LogEntry } from "./coreService";
import type { CoreBridge } from "./useCoreService";

// Test support: a shell bridge whose commands answer from the test and whose
// events the test fires, standing in for the supervisor behind Tauri.
export function coreStub({
  state = null,
  log = [],
  restart = async () => state,
}: {
  state?: CoreState | null;
  log?: LogEntry[];
  restart?: () => Promise<CoreState | null>;
} = {}) {
  const handlers = new Map<string, (event: { payload: unknown }) => void>();
  const unlisten = vi.fn();
  const call = vi.fn(async (command: string) => {
    if (command === "core_status") return state;
    if (command === "core_log") return log;
    if (command === "restart_core") return restart();
    if (command === "open_settings") return undefined;
    throw new Error(`${command} is not stubbed`);
  });
  const on = vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(event, handler);
    return unlisten;
  });
  return {
    bridge: { call, on } as unknown as CoreBridge,
    call,
    on,
    unlisten,
    emitState: (next: CoreState) => handlers.get(CORE_STATE_EVENT)?.({ payload: next }),
    emitLog: (entry: LogEntry) => handlers.get(CORE_LOG_EVENT)?.({ payload: entry }),
  };
}

export const RUNNING: Extract<CoreState, { state: "running" }> = {
  state: "running",
  pid: 48213,
  proxy: "127.0.0.1:7420",
  api: "127.0.0.1:7421",
  started_at_ms: new Date(2026, 10, 4, 14, 10, 4).getTime(),
};

export const line = (seq: number, text: string): LogEntry => ({ seq, kind: "line", text });
