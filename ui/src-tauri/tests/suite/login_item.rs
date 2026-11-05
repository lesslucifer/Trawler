use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};
use trawler_ui_lib::login_item::{self, Format, LoginItem, LoginItemPlace, LoginRegistry, Place};

#[derive(Debug, Default)]
struct Registered(AtomicBool);

impl LoginRegistry for Registered {
    fn enabled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn register(&self) -> Result<(), String> {
        self.0.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn unregister(&self) -> Result<(), String> {
        self.0.store(false, Ordering::SeqCst);
        Ok(())
    }
}

const THIS: &str = "/Applications/Trawler.app/Contents/MacOS/trawler-ui";
const OTHER: &str = "/Users/me/src/Trawler/target/debug/trawler-ui";

fn on(starts: &str) -> LoginItem {
    LoginItem { on: true, starts: Some(PathBuf::from(starts)) }
}

#[test]
fn whichever_binary_an_entry_names_it_reads_on_and_the_last_write_wins() {
    let format = Format::Autostart;
    let root = tempfile::tempdir().unwrap();
    login_item::write(format, root.path(), true, Path::new(OTHER)).unwrap();
    assert_eq!(login_item::read(format, root.path()), on(OTHER));
    login_item::write(format, root.path(), true, Path::new(THIS)).unwrap();
    assert_eq!(login_item::read(format, root.path()), on(THIS));
}

#[test]
fn an_autostart_entry_carries_the_exact_exec_line_and_off_removes_it() {
    let config = tempfile::tempdir().unwrap();
    let exe = "/opt/My Apps/trawler-ui";
    login_item::write(Format::Autostart, config.path(), true, Path::new(exe)).unwrap();
    let entry = config.path().join("autostart/dev.trawler.app.desktop");
    let text = std::fs::read_to_string(&entry).unwrap();
    assert!(
        text.lines().any(|line| line == "Exec=\"/opt/My Apps/trawler-ui\" --background"),
        "{text}"
    );
    assert_eq!(login_item::read(Format::Autostart, config.path()), on(exe));

    login_item::write(Format::Autostart, config.path(), false, Path::new(exe)).unwrap();
    assert!(!entry.exists());
}

fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let response = get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a url"),
            body: InvokeBody::Json(body),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .unwrap_or_else(|error| panic!("{cmd} refused: {error}"));
    let InvokeResponseBody::Json(json) = response else { panic!("{cmd} answered raw bytes") };
    serde_json::from_str(&json).unwrap()
}

#[test]
fn the_commands_turn_the_entry_on_for_this_binary_and_off_again() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            login_item::login_item_read,
            login_item::login_item_write
        ])
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage(LoginItemPlace(Some(Place::Service(Arc::new(Registered::default())))));
    let main =
        WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default()).build().unwrap();
    let this = std::env::current_exe().unwrap();

    assert_eq!(
        invoke(&main, "login_item_read", serde_json::json!({})),
        serde_json::json!({ "on": false, "starts": null })
    );
    assert_eq!(
        invoke(&main, "login_item_write", serde_json::json!({ "on": true })),
        serde_json::json!({ "on": true, "starts": this })
    );
    assert_eq!(
        invoke(&main, "login_item_read", serde_json::json!({}))["starts"],
        serde_json::json!(this)
    );
    assert_eq!(
        invoke(&main, "login_item_write", serde_json::json!({ "on": false })),
        serde_json::json!({ "on": false, "starts": null })
    );
}

#[test]
fn from_a_binary_with_no_bundle_the_commands_say_the_item_is_unavailable() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![login_item::login_item_read])
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage(LoginItemPlace(None));
    let main =
        WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default()).build().unwrap();
    let refused = get_ipc_response(
        &main,
        InvokeRequest {
            cmd: "login_item_read".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a url"),
            body: InvokeBody::Json(serde_json::json!({})),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .unwrap_err();
    assert_eq!(refused, serde_json::json!(login_item::UNAVAILABLE));
}
