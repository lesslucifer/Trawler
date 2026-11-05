import { invoke } from "@tauri-apps/api/core";
import type { Invoke } from "./coreService";

export type KeepCapturing = "unanswered" | "on" | "off";

export interface LoginItem {
  on: boolean;
  starts: string | null;
}

export const LOGIN_OFF: LoginItem = { on: false, starts: null };

export function readKeepCapturing(call: Invoke = invoke): Promise<KeepCapturing> {
  return call<KeepCapturing>("keep_capturing_read");
}

export function writeKeepCapturing(on: boolean, call: Invoke = invoke): Promise<void> {
  return call<void>("keep_capturing_write", { on });
}

export function readLoginItem(call: Invoke = invoke): Promise<LoginItem> {
  return call<LoginItem>("login_item_read");
}

export function writeLoginItem(on: boolean, call: Invoke = invoke): Promise<LoginItem> {
  return call<LoginItem>("login_item_write", { on });
}

export function readTrayAvailable(call: Invoke = invoke): Promise<boolean> {
  return call<boolean>("tray_available");
}

export function refusalWords(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/\s*\(os error \d+\)$/, "").toLowerCase();
}
