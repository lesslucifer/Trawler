use std::thread::JoinHandle;

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW: &str = "settings";
pub const MAIN_WINDOW: &str = "main";
const PAGE_GLOBAL: &str = "__TRAWLER_PAGE__";
const SETTINGS_PAGE: &str = "settings";

fn page_script() -> String {
    format!("window.{PAGE_GLOBAL} = \"{SETTINGS_PAGE}\";")
}

/// The main window is the app: once it is gone the app quits, whatever
/// secondary window is still open (Q7 — closing the last window quits).
pub fn ends_the_app(label: &str) -> bool {
    label == MAIN_WINDOW
}

// WebviewWindowBuilder::build deadlocks on Windows when called from an event
// handler (Tauri's own warning, wry#583), and a menu item's handler is one.
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn open_in_background<R: Runtime>(app: &AppHandle<R>) -> JoinHandle<tauri::Result<()>> {
    let app = app.clone();
    std::thread::spawn(move || {
        let opened = open(&app);
        if let Err(err) = &opened {
            eprintln!("opening Settings failed: {err}");
        }
        opened
    })
}

pub fn open<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App("index.html".into()))
        .title("Settings")
        .inner_size(560.0, 640.0)
        .min_inner_size(440.0, 360.0)
        .initialization_script(page_script())
        .build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_reads_the_mark_this_window_is_given() {
        let page = include_str!("../../src/page.ts");
        assert!(page.contains(&format!("export const PAGE_GLOBAL = \"{PAGE_GLOBAL}\";")));
        assert!(page.contains(&format!("export const SETTINGS_PAGE = \"{SETTINGS_PAGE}\";")));
        assert_eq!(page_script(), r#"window.__TRAWLER_PAGE__ = "settings";"#);
    }

    #[test]
    fn only_the_main_window_ends_the_app() {
        assert!(ends_the_app("main"));
        assert!(!ends_the_app(SETTINGS_WINDOW));
    }
}
