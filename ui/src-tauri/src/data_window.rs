// The data window (DEC-70). The shell only creates or focuses the window and
// relays the opener's payload to it by value; it reads nothing inside the bytes.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{
    AppHandle, Manager, Runtime, State, Webview, WebviewUrl, WebviewWindowBuilder, Window,
    WindowEvent,
};

use crate::window_geometry::{self, MinSize};

pub const LABEL_PREFIX: &str = "data-";
pub const MAIN_WINDOW: &str = "main";
const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 800.0;
const MIN_WIDTH: f64 = 600.0;
const MIN_HEIGHT: f64 = 400.0;
const LENGTH_BYTES: usize = 4;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    pub caption: Caption,
    pub bytes: Vec<u8>,
}

pub fn label_for(key: &str) -> String {
    key.bytes().fold(String::from(LABEL_PREFIX), |mut label, byte| {
        let _ = write!(label, "{byte:02x}");
        label
    })
}

pub fn is_data_window(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

pub fn closes_with(destroyed: &str, label: &str) -> bool {
    destroyed == MAIN_WINDOW && is_data_window(label)
}

pub fn frame(payload: &Payload) -> Vec<u8> {
    let caption = serde_json::to_vec(&payload.caption).expect("a caption serialises");
    let length = u32::try_from(caption.len()).expect("a caption under 4 GiB");
    let mut framed = Vec::with_capacity(LENGTH_BYTES + caption.len() + payload.bytes.len());
    framed.extend_from_slice(&length.to_be_bytes());
    framed.extend_from_slice(&caption);
    framed.extend_from_slice(&payload.bytes);
    framed
}

pub fn unframe(framed: &[u8]) -> Result<Payload, String> {
    let (length, rest) = framed
        .split_first_chunk::<LENGTH_BYTES>()
        .ok_or("a data window payload starts with its caption's length")?;
    let length = usize::try_from(u32::from_be_bytes(*length)).map_err(|e| e.to_string())?;
    if rest.len() < length {
        return Err(format!("the caption claims {length} bytes and {} follow", rest.len()));
    }
    let (caption, bytes) = rest.split_at(length);
    let caption = serde_json::from_slice(caption).map_err(|e| format!("the caption: {e}"))?;
    Ok(Payload { caption, bytes: bytes.to_vec() })
}

#[derive(Default)]
pub struct Parked(Mutex<HashMap<String, Payload>>);

impl Parked {
    pub fn park(&self, label: String, payload: Payload) -> bool {
        let mut parked = self.0.lock().expect("the parked payloads");
        if parked.contains_key(&label) {
            return false;
        }
        parked.insert(label, payload);
        true
    }

    pub fn take(&self, label: &str) -> Option<Payload> {
        self.0.lock().expect("the parked payloads").remove(label)
    }
}

fn focus<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        if let Err(err) = window.set_focus() {
            eprintln!("focusing {label} failed: {err}");
        }
    }
}

#[tauri::command]
pub async fn open_data_window<R: Runtime>(
    app: AppHandle<R>,
    parked: State<'_, Parked>,
    request: Request<'_>,
) -> Result<(), String> {
    let InvokeBody::Raw(framed) = request.body() else {
        return Err("open_data_window takes the framed payload as a raw body".into());
    };
    let payload = unframe(framed)?;
    let label = label_for(&payload.caption.key);
    let title = payload.caption.title.clone();
    if app.get_webview_window(&label).is_some() || !parked.park(label.clone(), payload) {
        focus(&app, &label);
        return Ok(());
    }
    // Read before the build: every data window shares one record, and the new
    // window's own first Moved/Resized would otherwise overwrite it first.
    let saved = window_geometry::saved(&app, &label);
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .visible(false)
        .build()
        .map_err(|err| {
            parked.take(&label);
            err.to_string()
        })?;
    window_geometry::place(
        &window.as_ref().window(),
        saved,
        MinSize { width: MIN_WIDTH, height: MIN_HEIGHT },
    );
    window.show().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn take_data_payload<R: Runtime>(webview: Webview<R>, parked: State<'_, Parked>) -> Response {
    Response::new(parked.take(webview.label()).as_ref().map(frame).unwrap_or_default())
}

pub fn on_destroyed<R: Runtime>(app: &AppHandle<R>, destroyed: &str) -> Vec<String> {
    if is_data_window(destroyed) {
        app.state::<Parked>().take(destroyed);
    }
    let mut closed = Vec::new();
    for (label, data_window) in app.webview_windows() {
        if closes_with(destroyed, &label) {
            match data_window.destroy() {
                Ok(()) => closed.push(label),
                Err(err) => eprintln!("closing {label} with the main window failed: {err}"),
            }
        }
    }
    closed
}

pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if matches!(event, WindowEvent::Destroyed) {
        on_destroyed(window.app_handle(), window.label());
    }
}

pub fn install<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.manage(Parked::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(key: &str, bytes: &[u8]) -> Payload {
        Payload {
            caption: Caption {
                key: key.into(),
                title: format!("title of {key}"),
                type_hint: Some("application/json".into()),
            },
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn a_label_is_the_prefix_and_the_key_in_lowercase_hex() {
        assert_eq!(label_for("ab"), "data-6162");
        assert_eq!(label_for(""), "data-");
        assert_eq!(label_for("é/1"), "data-c3a92f31");
    }

    #[test]
    fn a_label_uses_only_characters_tauri_accepts() {
        let label = label_for("flow:42 response/\u{1F600}\"<>");
        assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{label}");
    }

    #[test]
    fn distinct_keys_never_share_a_label() {
        let keys = ["a", "aa", "a\0", "1", "01", "6", "61", "flow-1", "flow-1:response", ""];
        let labels: std::collections::BTreeSet<_> = keys.iter().map(|k| label_for(k)).collect();
        assert_eq!(labels.len(), keys.len());
    }

    #[test]
    fn a_framed_payload_reads_back_as_itself() {
        let original = payload("k", &[0, 255, 1, 254, b'{']);
        assert_eq!(unframe(&frame(&original)), Ok(original));
    }

    #[test]
    fn a_caption_without_a_type_hint_frames_without_one() {
        let mut original = payload("k", b"x");
        original.caption.type_hint = None;
        let framed = frame(&original);
        assert!(!String::from_utf8_lossy(&framed).contains("typeHint"));
        assert_eq!(unframe(&framed), Ok(original));
    }

    #[test]
    fn the_frame_is_a_big_endian_length_the_caption_then_the_bytes() {
        let framed = frame(&payload("k", b"BODY"));
        let length = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
        assert_eq!(&framed[4 + length..], b"BODY");
        let caption: serde_json::Value = serde_json::from_slice(&framed[4..4 + length]).unwrap();
        assert_eq!(caption["typeHint"], "application/json");
    }

    #[test]
    fn a_malformed_frame_is_refused_with_why() {
        assert!(unframe(&[0, 0]).unwrap_err().contains("length"));
        assert!(unframe(&[0, 0, 0, 9, b'{']).unwrap_err().contains("claims 9 bytes"));
        assert!(unframe(&[0, 0, 0, 1, b'x']).unwrap_err().contains("caption"));
    }

    #[test]
    fn a_parked_payload_is_claimed_exactly_once() {
        let parked = Parked::default();
        assert!(parked.park("data-6b".into(), payload("k", b"1")));
        assert_eq!(parked.take("data-6b"), Some(payload("k", b"1")));
        assert_eq!(parked.take("data-6b"), None);
    }

    #[test]
    fn parking_again_under_a_label_keeps_the_first_payload() {
        let parked = Parked::default();
        assert!(parked.park("data-6b".into(), payload("k", b"first")));
        assert!(!parked.park("data-6b".into(), payload("k", b"second")));
        assert_eq!(parked.take("data-6b").unwrap().bytes, b"first");
    }

    #[test]
    fn a_window_with_nothing_parked_claims_nothing() {
        assert_eq!(Parked::default().take(MAIN_WINDOW), None);
    }

    #[test]
    fn the_data_window_capability_names_exactly_the_labels_the_shell_mints() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/data-window.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!([format!("{LABEL_PREFIX}*")]));
    }

    #[test]
    fn only_the_main_window_takes_the_data_windows_with_it() {
        assert!(closes_with(MAIN_WINDOW, "data-6b"));
        assert!(!closes_with(MAIN_WINDOW, MAIN_WINDOW));
        assert!(!closes_with("data-6b", "data-6c"));
        assert!(!closes_with("other", "data-6b"));
    }
}
