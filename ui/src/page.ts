// The shell marks a window's page with this global (ui/src-tauri/src/settings.rs),
// and a Rust test pins both strings to the shell's constants.
export const PAGE_GLOBAL = "__TRAWLER_PAGE__";
export const SETTINGS_PAGE = "settings";

export type Page = "settings" | "app";

export function pageOf(scope: Record<string, unknown>): Page {
  return scope[PAGE_GLOBAL] === SETTINGS_PAGE ? "settings" : "app";
}
