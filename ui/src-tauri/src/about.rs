use std::path::Path;
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

const NOTICE_FILE: &str = "THIRD-PARTY-NOTICES.txt";

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

pub fn open<R: Runtime>(app: &AppHandle<R>, show: Show) -> JoinHandle<()> {
    crate::lifecycle::request_dialog(app, crate::lifecycle::DialogRequest::About(show))
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
