import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type CoreState =
  | { state: "starting" }
  | { state: "running"; pid: number; proxy: string; api: string; started_at_ms: number }
  | { state: "exited"; code: number | null; signal: number | null }
  | { state: "attached"; api: string };

export type LogEntry =
  | { seq: number; kind: "start"; pid: number; at_ms: number }
  | { seq: number; kind: "line"; text: string };

export const LOG_LINES = 2000;
export const CORE_STATE_EVENT = "core://state";
export const CORE_LOG_EVENT = "core://log";

export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export type Listen = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<UnlistenFn>;

// The web shell (UC-3) has no app around it, so it has no service to read.
export async function readCoreStatus(call: Invoke = invoke): Promise<CoreState | null> {
  try {
    return await call<CoreState>("core_status");
  } catch {
    return null;
  }
}

export async function readCoreLog(call: Invoke = invoke): Promise<LogEntry[]> {
  try {
    return await call<LogEntry[]>("core_log");
  } catch {
    return [];
  }
}

export function restartCore(call: Invoke = invoke): Promise<CoreState> {
  return call<CoreState>("restart_core");
}

export function openSettings(call: Invoke = invoke): Promise<void> {
  return call<void>("open_settings");
}

export function onCoreState(handler: (state: CoreState) => void, on: Listen = listen): Promise<UnlistenFn> {
  return on<CoreState>(CORE_STATE_EVENT, (event) => handler(event.payload));
}

export function onCoreLog(handler: (entry: LogEntry) => void, on: Listen = listen): Promise<UnlistenFn> {
  return on<LogEntry>(CORE_LOG_EVENT, (event) => handler(event.payload));
}

export function stoppedWords({ code, signal }: { code: number | null; signal: number | null }): string {
  if (signal !== null) return `killed (signal ${signal})`;
  if (code !== null) return `exit code ${code}`;
  return "for an unknown reason";
}
