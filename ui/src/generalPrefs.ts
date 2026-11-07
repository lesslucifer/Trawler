import { invoke } from "@tauri-apps/api/core";
import type { Invoke } from "./coreService";
import type { HostOs } from "./setup";

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

export interface UpdatePrefs {
  version: string;
  installed: boolean;
  automatic: boolean;
}

export function readUpdatePrefs(call: Invoke = invoke): Promise<UpdatePrefs> {
  return call<UpdatePrefs>("update_prefs_read");
}

export function writeUpdateAutomatic(on: boolean, call: Invoke = invoke): Promise<void> {
  return call<void>("update_automatic_write", { on });
}

export function checkForUpdates(call: Invoke = invoke): Promise<void> {
  return call<void>("check_for_updates");
}

export function readTrayAvailable(call: Invoke = invoke): Promise<boolean> {
  return call<boolean>("tray_available");
}

export function refusalWords(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.replace(/\s*\(os error \d+\)$/, "").toLowerCase();
}

export function trayPlace(os: HostOs): string {
  if (os === "macos") return "the menu bar";
  return os === "windows" ? "the system tray (notification area)" : "the system tray";
}

export function trayIcon(os: HostOs): string {
  return os === "macos" ? "menu-bar icon" : "tray icon";
}

export const GNOME_TRAY_HINT = "on GNOME, the AppIndicator extension shows";
