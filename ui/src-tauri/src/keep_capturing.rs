use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::lifecycle::Lifecycle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeepCapturing {
    Unanswered,
    On,
    Off,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stored {
    keep_capturing: bool,
}

fn preference_path(config_dir: &Path) -> PathBuf {
    config_dir.join("keep-capturing.json")
}

pub fn read_keep_capturing(config_dir: &Path) -> KeepCapturing {
    let Ok(bytes) = std::fs::read(preference_path(config_dir)) else {
        return KeepCapturing::Unanswered;
    };
    match serde_json::from_slice::<Stored>(&bytes) {
        Ok(Stored { keep_capturing: true }) => KeepCapturing::On,
        _ => KeepCapturing::Off,
    }
}

pub fn write_keep_capturing(config_dir: &Path, on: bool) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&Stored { keep_capturing: on })?;
    crate::layout::replace_whole(&preference_path(config_dir), &bytes)
}

#[tauri::command]
pub fn keep_capturing_read(lifecycle: State<'_, Lifecycle>) -> KeepCapturing {
    lifecycle.keep_capturing()
}

#[tauri::command]
pub fn keep_capturing_write(lifecycle: State<'_, Lifecycle>, on: bool) -> Result<(), String> {
    let dir = lifecycle.config_dir().ok_or("the app has no config directory")?;
    write_keep_capturing(dir, on).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(dir: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(preference_path(dir), bytes).unwrap();
    }

    #[test]
    fn an_answer_reads_back_as_written() {
        let dir = tempfile::tempdir().unwrap();
        write_keep_capturing(dir.path(), true).unwrap();
        assert_eq!(read_keep_capturing(dir.path()), KeepCapturing::On);
        assert_eq!(
            std::fs::read_to_string(preference_path(dir.path())).unwrap(),
            r#"{"keepCapturing":true}"#
        );
        write_keep_capturing(dir.path(), false).unwrap();
        assert_eq!(read_keep_capturing(dir.path()), KeepCapturing::Off);
    }

    #[test]
    fn no_dir_and_no_file_read_as_unanswered() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_keep_capturing(&dir.path().join("absent")), KeepCapturing::Unanswered);
        assert_eq!(read_keep_capturing(dir.path()), KeepCapturing::Unanswered);
    }

    #[test]
    fn a_file_that_is_not_an_answer_reads_as_off() {
        let dir = tempfile::tempdir().unwrap();
        for bytes in [
            &b"{\"keepCapturing\":tr"[..],
            b"",
            b"null",
            b"true",
            b"{}",
            b"{\"keepCapturing\":null}",
            b"{\"keepCapturing\":\"on\"}",
            b"{\"keepCapturing\":true,\"extra\":1}",
            &[0xff, 0xfe, 0x00],
        ] {
            saved(dir.path(), bytes);
            assert_eq!(
                read_keep_capturing(dir.path()),
                KeepCapturing::Off,
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn a_write_makes_the_dir_replaces_a_corrupt_file_and_leaves_no_staged_copy() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        write_keep_capturing(&nested, true).unwrap();
        saved(&nested, b"not an answer");
        write_keep_capturing(&nested, false).unwrap();
        assert_eq!(read_keep_capturing(&nested), KeepCapturing::Off);
        assert_eq!(std::fs::read_dir(&nested).unwrap().count(), 1);
    }
}
