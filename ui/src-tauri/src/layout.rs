use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Layout {
    pub pane: u32,
    pub headers_share: f64,
    pub columns: ColumnWidths,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnWidths {
    pub time: u32,
    pub method: u32,
    pub host: u32,
    pub status: u32,
    pub duration: u32,
}

fn layout_path(config_dir: &Path) -> PathBuf {
    config_dir.join("layout.json")
}

fn staged_path(path: &Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".staged");
    PathBuf::from(staged)
}

pub(crate) fn replace_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(staged_path(path), bytes)?;
    std::fs::rename(staged_path(path), path)
}

pub fn read_layout(config_dir: &Path) -> Option<Layout> {
    let bytes = std::fs::read(layout_path(config_dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_layout(config_dir: &Path, layout: &Layout) -> std::io::Result<()> {
    replace_whole(&layout_path(config_dir), &serde_json::to_vec(layout)?)
}

#[tauri::command]
pub fn layout_read(app: tauri::AppHandle) -> Option<Layout> {
    crate::setup::config_dir(&app).ok().and_then(|dir| read_layout(&dir))
}

#[tauri::command]
pub fn layout_write(app: tauri::AppHandle, layout: Layout) -> Result<(), String> {
    let dir = crate::setup::config_dir(&app)?;
    write_layout(&dir, &layout).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("trawler-test-layout-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sized() -> Layout {
        Layout {
            pane: 600,
            headers_share: 0.42,
            columns: ColumnWidths { time: 160, method: 70, host: 240, status: 70, duration: 90 },
        }
    }

    fn saved(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(layout_path(dir), text).unwrap();
    }

    const WHOLE: &str = r#"{"pane":600,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70,"duration":90}}"#;

    #[test]
    fn a_written_layout_reads_back_unchanged() {
        let dir = TempDir::new("round-trip");
        write_layout(&dir.0, &sized()).unwrap();
        assert_eq!(read_layout(&dir.0), Some(sized()));
    }

    #[test]
    fn a_layout_saved_whole_in_the_wire_shape_reads() {
        let dir = TempDir::new("wire");
        saved(&dir.0, WHOLE);
        assert_eq!(read_layout(&dir.0), Some(sized()));
    }

    #[test]
    fn no_dir_and_no_file_read_as_no_saved_layout() {
        let dir = TempDir::new("absent");
        assert_eq!(read_layout(&dir.0), None);
        std::fs::create_dir_all(&dir.0).unwrap();
        assert_eq!(read_layout(&dir.0), None);
    }

    #[test]
    fn bytes_that_are_not_json_read_as_no_saved_layout() {
        let dir = TempDir::new("corrupt");
        saved(&dir.0, "{\"pane\":6");
        assert_eq!(read_layout(&dir.0), None);
        std::fs::write(layout_path(&dir.0), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_layout(&dir.0), None);
    }

    #[test]
    fn any_other_shape_reads_as_no_saved_layout() {
        let dir = TempDir::new("shape");
        for shape in [
            r#"{"pane":600,"columns":{"time":160,"method":70,"host":240,"status":70,"duration":90}}"#,
            r#"{"pane":600,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70}}"#,
            r#"{"pane":600,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70,"duration":90},"version":2}"#,
            r#"{"pane":600,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"path":300,"status":70,"duration":90}}"#,
            r#"{"pane":"600","headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70,"duration":90}}"#,
            r#"{"pane":-1,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70,"duration":90}}"#,
            r#"{"pane":600,"headersShare":"wide","columns":{"time":160,"method":70,"host":240,"status":70,"duration":90}}"#,
            "[]",
            "null",
        ] {
            saved(&dir.0, shape);
            assert_eq!(read_layout(&dir.0), None, "{shape}");
        }
    }

    #[test]
    fn a_layout_saved_before_duration_was_named_reads_as_no_saved_layout() {
        let dir = TempDir::new("size-key");
        saved(
            &dir.0,
            r#"{"pane":600,"headersShare":0.42,"columns":{"time":160,"method":70,"host":240,"status":70,"size":90}}"#,
        );
        assert_eq!(read_layout(&dir.0), None);
    }

    #[test]
    fn a_write_makes_the_dir_replaces_the_file_and_leaves_no_staged_copy() {
        let dir = TempDir::new("replace");
        saved(&dir.0, "not a layout");
        let mut wider = sized();
        wider.pane = 700;
        write_layout(&dir.0, &sized()).unwrap();
        write_layout(&dir.0, &wider).unwrap();
        assert_eq!(read_layout(&dir.0), Some(wider));
        assert!(!staged_path(&layout_path(&dir.0)).exists());

        let fresh = TempDir::new("fresh");
        write_layout(&fresh.0.join("nested"), &sized()).unwrap();
        assert_eq!(read_layout(&fresh.0.join("nested")), Some(sized()));
    }
}
