// The shell marks a window's page with this global (ui/src-tauri/src/settings.rs,
// about.rs), and Rust tests pin these strings to the shell's constants.
export const PAGE_GLOBAL = "__TRAWLER_PAGE__";
export const SETTINGS_PAGE = "settings";
export const ABOUT_PAGE = "about";
export const ABOUT_SHOW_GLOBAL = "__TRAWLER_ABOUT__";
export const ABOUT_SHOW_EVENT = "about-show";

export type Page = "settings" | "about" | "app";

export function pageOf(scope: Record<string, unknown>): Page {
  const mark = scope[PAGE_GLOBAL];
  if (mark === SETTINGS_PAGE) return "settings";
  return mark === ABOUT_PAGE ? "about" : "app";
}
