use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW: &str = "settings";
pub const MAIN_WINDOW: &str = "main";
const PAGE_SCRIPT: &str = r#"window.__TRAWLER_PAGE__ = "settings";"#;

/// The main window is the app: once it is gone the app quits, whatever
/// secondary window is still open (Q7 — closing the last window quits).
pub fn ends_the_app(label: &str) -> bool {
    label == MAIN_WINDOW
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
        .initialization_script(PAGE_SCRIPT)
        .build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_main_window_ends_the_app() {
        assert!(ends_the_app("main"));
        assert!(!ends_the_app(SETTINGS_WINDOW));
    }
}
