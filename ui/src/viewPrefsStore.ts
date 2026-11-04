import { invoke } from "@tauri-apps/api/core";
import type { Leg } from "./api";
import { PLATFORM_IDS, type PlatformId } from "./setup";

export type BodyMode = "pretty" | "raw";

export type ViewPrefs = {
  leg: Leg;
  bodyMode: BodyMode;
  lastPlatform: PlatformId | null;
};

export const DEFAULT_VIEW_PREFS: ViewPrefs = { leg: "request", bodyMode: "pretty", lastPlatform: null };

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

const LEGS: readonly Leg[] = ["request", "response"];
const BODY_MODES: readonly BodyMode[] = ["pretty", "raw"];

const oneOf = <T>(known: readonly T[], value: unknown): T | undefined =>
  known.includes(value as T) ? (value as T) : undefined;

export async function readViewPrefs(call: Invoke = invoke): Promise<Partial<ViewPrefs>> {
  let saved: Record<string, unknown> | null;
  try {
    saved = await call<Record<string, unknown> | null>("view_prefs_read");
  } catch {
    return {};
  }
  const read: Partial<ViewPrefs> = {
    leg: oneOf(LEGS, saved?.leg),
    bodyMode: oneOf(BODY_MODES, saved?.bodyMode),
    lastPlatform: oneOf(PLATFORM_IDS, saved?.lastPlatform),
  };
  return Object.fromEntries(Object.entries(read).filter(([, value]) => value !== undefined));
}

export async function writeViewPrefs(prefs: ViewPrefs, call: Invoke = invoke): Promise<void> {
  await call("view_prefs_write", { prefs });
}
