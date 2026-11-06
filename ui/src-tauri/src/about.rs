use std::path::Path;
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::settings::PAGE_GLOBAL;

pub const ABOUT_WINDOW: &str = "about";
pub const SHOW_EVENT: &str = "about-show";
const ABOUT_PAGE: &str = "about";
const SHOW_GLOBAL: &str = "__TRAWLER_ABOUT__";
const NOTICE_FILE: &str = "THIRD-PARTY-NOTICES.txt";
const SIZE: (f64, f64) = (600.0, 560.0);
const MIN_SIZE: (f64, f64) = (440.0, 360.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Page {
    About,
    Licences,
    Privacy,
}

/// The page to show, and the address of a link the browser would not open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Show {
    pub page: Page,
    pub unopened: Option<String>,
}

impl Show {
    pub fn page(page: Page) -> Self {
        Self { page, unopened: None }
    }
}

fn page_script(show: &Show) -> String {
    let show = serde_json::to_string(show).unwrap_or_else(|_| "null".into());
    format!("window.{PAGE_GLOBAL} = \"{ABOUT_PAGE}\"; window.{SHOW_GLOBAL} = {show};")
}

// Built off the event handler's thread, as Settings is (wry#583).
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn open_in_background<R: Runtime>(app: &AppHandle<R>, show: Show) -> JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(err) = open(&app, &show) {
            eprintln!("opening About failed: {err}");
        }
    })
}

pub fn open<R: Runtime>(app: &AppHandle<R>, show: &Show) -> tauri::Result<()> {
    crate::lifecycle::set_dock(app, crate::lifecycle::Dock::Shown);
    let opened = show_or_build(app, show);
    if opened.is_err() {
        crate::lifecycle::on_window_gone(app);
    }
    opened
}

fn show_or_build<R: Runtime>(app: &AppHandle<R>, show: &Show) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ABOUT_WINDOW) {
        window.emit_to(ABOUT_WINDOW, SHOW_EVENT, show)?;
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, ABOUT_WINDOW, WebviewUrl::App("index.html".into()))
        .title("About Trawler")
        .inner_size(SIZE.0, SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .initialization_script(page_script(show))
        .build()
        .map(drop)
}

pub fn read_notice(resources: &Path) -> Option<String> {
    std::fs::read_to_string(resources.join(NOTICE_FILE)).ok()
}

#[derive(Serialize)]
pub struct AboutFacts {
    version: String,
    notice: Option<String>,
}

#[tauri::command]
pub fn about_read<R: Runtime>(app: AppHandle<R>) -> AboutFacts {
    AboutFacts {
        version: app.package_info().version.to_string(),
        notice: app.path().resource_dir().ok().and_then(|dir| read_notice(&dir)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_reads_the_marks_this_window_is_given() {
        let page = include_str!("../../src/page.ts");
        assert!(page.contains(&format!("export const ABOUT_PAGE = \"{ABOUT_PAGE}\";")));
        assert!(page.contains(&format!("export const ABOUT_SHOW_GLOBAL = \"{SHOW_GLOBAL}\";")));
        assert!(page.contains(&format!("export const ABOUT_SHOW_EVENT = \"{SHOW_EVENT}\";")));
        assert_eq!(
            page_script(&Show { page: Page::Licences, unopened: Some("https://x/\"y".into()) }),
            r#"window.__TRAWLER_PAGE__ = "about"; window.__TRAWLER_ABOUT__ = {"page":"licences","unopened":"https://x/\"y"};"#
        );
    }

    #[test]
    fn the_notice_is_the_bundled_file_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_notice(dir.path()), None);
        std::fs::write(dir.path().join(NOTICE_FILE), "MIT License\n").unwrap();
        assert_eq!(read_notice(dir.path()).as_deref(), Some("MIT License\n"));
    }

    #[test]
    fn the_shipped_config_bundles_the_notice_the_page_reads() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid json");
        let resources = config["bundle"]["resources"].as_object().expect("resources");
        assert!(resources.values().any(|target| target == NOTICE_FILE), "{resources:?}");
    }
}
