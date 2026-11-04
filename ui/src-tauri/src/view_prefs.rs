use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Leg {
    #[default]
    Request,
    Response,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyMode {
    #[default]
    Pretty,
    Raw,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewPrefs {
    #[serde(default, deserialize_with = "or_default")]
    pub leg: Leg,
    #[serde(default, deserialize_with = "or_default")]
    pub body_mode: BodyMode,
    #[serde(default, deserialize_with = "or_default", skip_serializing_if = "Option::is_none")]
    pub last_platform: Option<String>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

fn or_default<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(
    deserializer: D,
) -> Result<T, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).unwrap_or_default())
}

fn prefs_path(config_dir: &Path) -> PathBuf {
    config_dir.join("view-prefs.json")
}

pub fn read_view_prefs(config_dir: &Path) -> ViewPrefs {
    std::fs::read(prefs_path(config_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Map<String, Value>>(&bytes).ok())
        .and_then(|fields| serde_json::from_value(Value::Object(fields)).ok())
        .unwrap_or_default()
}

pub fn write_view_prefs(config_dir: &Path, prefs: &ViewPrefs) -> std::io::Result<()> {
    let mut other = read_view_prefs(config_dir).other;
    other.extend(prefs.other.clone());
    let whole = ViewPrefs { other, ..prefs.clone() };
    crate::layout::replace_whole(&prefs_path(config_dir), &serde_json::to_vec(&whole)?)
}

#[tauri::command]
pub fn view_prefs_read(app: tauri::AppHandle) -> ViewPrefs {
    crate::setup::config_dir(&app).map(|dir| read_view_prefs(&dir)).unwrap_or_default()
}

#[tauri::command]
pub fn view_prefs_write(app: tauri::AppHandle, prefs: ViewPrefs) -> Result<(), String> {
    let dir = crate::setup::config_dir(&app)?;
    write_view_prefs(&dir, &prefs).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("trawler-test-view-prefs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn saved(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(prefs_path(dir), text).unwrap();
    }

    fn on_file(dir: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(prefs_path(dir)).unwrap()).unwrap()
    }

    fn chosen() -> ViewPrefs {
        ViewPrefs {
            leg: Leg::Response,
            body_mode: BodyMode::Raw,
            last_platform: Some("android".into()),
            other: Map::new(),
        }
    }

    #[test]
    fn written_prefs_read_back_unchanged() {
        let dir = TempDir::new("round-trip");
        write_view_prefs(&dir.0, &chosen()).unwrap();
        assert_eq!(read_view_prefs(&dir.0), chosen());
        assert_eq!(
            on_file(&dir.0),
            json!({ "leg": "response", "bodyMode": "raw", "lastPlatform": "android" })
        );
    }

    #[test]
    fn no_dir_no_file_and_no_object_read_as_defaults() {
        let dir = TempDir::new("absent");
        assert_eq!(read_view_prefs(&dir.0), ViewPrefs::default());
        std::fs::create_dir_all(&dir.0).unwrap();
        assert_eq!(read_view_prefs(&dir.0), ViewPrefs::default());
        for text in ["{\"leg\":\"resp", "[]", "null", "\"raw\"", "7"] {
            saved(&dir.0, text);
            assert_eq!(read_view_prefs(&dir.0), ViewPrefs::default(), "{text}");
        }
        std::fs::write(prefs_path(&dir.0), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_view_prefs(&dir.0), ViewPrefs::default());
    }

    #[test]
    fn a_wrong_field_reads_as_its_default_and_keeps_the_rest() {
        let dir = TempDir::new("field");
        saved(&dir.0, r#"{"leg":7,"bodyMode":"raw","lastPlatform":"ios"}"#);
        assert_eq!(
            read_view_prefs(&dir.0),
            ViewPrefs {
                leg: Leg::Request,
                body_mode: BodyMode::Raw,
                last_platform: Some("ios".into()),
                other: Map::new(),
            }
        );
        saved(&dir.0, r#"{"leg":"response","bodyMode":"sideways","lastPlatform":["ios"]}"#);
        assert_eq!(
            read_view_prefs(&dir.0),
            ViewPrefs { leg: Leg::Response, ..ViewPrefs::default() }
        );
    }

    #[test]
    fn fields_another_version_wrote_survive_a_write() {
        let dir = TempDir::new("foreign");
        saved(&dir.0, r#"{"leg":"response","query":"host:api","tabs":[1,2]}"#);
        let read = read_view_prefs(&dir.0);
        assert_eq!(read.other.get("query"), Some(&json!("host:api")));

        let mut sent = chosen();
        sent.other.insert("tabs".into(), json!([3]));
        write_view_prefs(&dir.0, &sent).unwrap();
        assert_eq!(
            on_file(&dir.0),
            json!({
                "leg": "response",
                "bodyMode": "raw",
                "lastPlatform": "android",
                "query": "host:api",
                "tabs": [3]
            })
        );
    }

    #[test]
    fn a_write_makes_the_dir_replaces_a_corrupt_file_and_leaves_no_staged_copy() {
        let dir = TempDir::new("replace");
        saved(&dir.0, "not prefs");
        write_view_prefs(&dir.0, &chosen()).unwrap();
        assert_eq!(read_view_prefs(&dir.0), chosen());
        let mut staged = prefs_path(&dir.0).into_os_string();
        staged.push(".staged");
        assert!(!PathBuf::from(staged).exists());

        let fresh = TempDir::new("fresh");
        write_view_prefs(&fresh.0.join("nested"), &ViewPrefs::default()).unwrap();
        assert_eq!(
            on_file(&fresh.0.join("nested")),
            json!({ "leg": "request", "bodyMode": "pretty" })
        );
    }
}
