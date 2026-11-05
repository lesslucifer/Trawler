use std::path::{Path, PathBuf};

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};
use trawler_ui_lib::login_item::{self, Format, LoginItem, LoginItemPlace, Place};

const THIS: &str = "/Applications/Trawler.app/Contents/MacOS/trawler-ui";
const OTHER: &str = "/Users/me/src/Trawler/target/debug/trawler-ui";

fn on(starts: &str) -> LoginItem {
    LoginItem { on: true, starts: Some(PathBuf::from(starts)) }
}

fn off() -> LoginItem {
    LoginItem { on: false, starts: None }
}

#[test]
fn a_launch_agent_is_a_valid_plist_starting_this_binary_and_off_removes_it() {
    let home = tempfile::tempdir().unwrap();
    let entry = home.path().join("Library/LaunchAgents/dev.trawler.app.plist");
    assert_eq!(login_item::read(Format::LaunchAgent, home.path()), off());

    let written = login_item::write(Format::LaunchAgent, home.path(), true, Path::new(THIS));
    assert_eq!(written.unwrap(), on(THIS));
    assert!(entry.is_file());
    if cfg!(target_os = "macos") {
        let lint = std::process::Command::new("plutil").arg("-lint").arg(&entry).output().unwrap();
        assert!(lint.status.success(), "{}", String::from_utf8_lossy(&lint.stdout));
    }

    let removed = login_item::write(Format::LaunchAgent, home.path(), false, Path::new(THIS));
    assert_eq!(removed.unwrap(), off());
    assert!(!entry.exists());
    let again = login_item::write(Format::LaunchAgent, home.path(), false, Path::new(THIS));
    assert_eq!(again.unwrap(), off(), "off with no entry stays off");
}

#[test]
fn whichever_binary_an_entry_names_it_reads_on_and_the_last_write_wins() {
    for format in [Format::LaunchAgent, Format::Autostart] {
        let root = tempfile::tempdir().unwrap();
        login_item::write(format, root.path(), true, Path::new(OTHER)).unwrap();
        assert_eq!(login_item::read(format, root.path()), on(OTHER), "{format:?}");
        login_item::write(format, root.path(), true, Path::new(THIS)).unwrap();
        assert_eq!(login_item::read(format, root.path()), on(THIS), "{format:?}");
    }
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
    let home = tempfile::tempdir().unwrap();
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            login_item::login_item_read,
            login_item::login_item_write
        ])
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage(LoginItemPlace(Some(Place::File {
        format: Format::LaunchAgent,
        root: home.path().to_path_buf(),
    })));
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
