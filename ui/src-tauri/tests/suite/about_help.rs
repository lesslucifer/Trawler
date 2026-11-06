use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};
use trawler_ui_lib::lifecycle::Lifecycle;
use trawler_ui_lib::{about, links, update};

fn ask(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    get_ipc_response(
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
    .map(|response| {
        let InvokeResponseBody::Json(json) = response else { panic!("{cmd} answered raw bytes") };
        serde_json::from_str(&json).unwrap()
    })
}

fn shell(config_dir: &std::path::Path) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            update::update_prefs_read,
            update::update_automatic_write,
            about::about_read,
            links::open_link
        ])
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    app.manage(Lifecycle::new(Some(config_dir.to_path_buf())));
    let window =
        WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::default()).build().unwrap();
    (app, window)
}

#[test]
fn settings_reads_the_running_version_and_the_switch_it_stores() {
    let dir = tempfile::tempdir().unwrap();
    let (app, settings) = shell(dir.path());
    let version = app.package_info().version.to_string();
    let installed = update::installed(tauri::utils::platform::bundle_type());

    assert_eq!(
        ask(&settings, "update_prefs_read", serde_json::json!({})).unwrap(),
        serde_json::json!({ "version": version, "installed": installed, "automatic": true })
    );
    ask(&settings, "update_automatic_write", serde_json::json!({ "on": false })).unwrap();
    assert_eq!(
        ask(&settings, "update_prefs_read", serde_json::json!({})).unwrap()["automatic"],
        false
    );
    assert!(!update::read_automatic(dir.path()), "the launch check reads the same answer");
}

#[test]
fn about_reads_the_running_version() {
    let dir = tempfile::tempdir().unwrap();
    let (app, about) = shell(dir.path());
    let facts = ask(&about, "about_read", serde_json::json!({})).unwrap();
    assert_eq!(facts["version"], app.package_info().version.to_string());
    assert!(facts["notice"].is_null() || facts["notice"].is_string());
}

#[test]
fn the_shell_refuses_an_address_where_it_takes_a_link_name() {
    let dir = tempfile::tempdir().unwrap();
    let (_app, window) = shell(dir.path());
    for link in ["https://example.com", "file:///etc/passwd", "Guide"] {
        assert!(
            ask(&window, "open_link", serde_json::json!({ "link": link })).is_err(),
            "{link} was taken"
        );
    }
}
