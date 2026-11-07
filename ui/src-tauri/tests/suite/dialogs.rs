use std::sync::mpsc::{channel, Receiver};

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::utils::config::WindowConfig;
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, Manager, WebviewWindow};
use trawler_ui_lib::lifecycle::{self, Lifecycle, DIALOG_REQUEST_EVENT};
use trawler_ui_lib::settings::{self, MAIN_WINDOW};
use trawler_ui_lib::{menu, tray};

use crate::common::LOCAL_URL;

fn shipped_windows() -> Vec<WindowConfig> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    serde_json::from_value(config["app"]["windows"].clone()).unwrap()
}

fn app_declaring(windows: Vec<WindowConfig>) -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = windows;
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            settings::open_settings,
            lifecycle::take_dialog_requests,
            lifecycle::close_window
        ])
        .build(context)
        .expect("a mock app");
    lifecycle::install(app.handle(), None);
    app.state::<Lifecycle>().mark_tray(true);
    app
}

fn app() -> App<MockRuntime> {
    app_declaring(shipped_windows())
}

fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str) -> serde_json::Value {
    let response = get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_URL.parse().expect("a url"),
            body: InvokeBody::Json(serde_json::json!({})),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .unwrap_or_else(|error| panic!("{cmd} refused: {error}"));
    match response {
        InvokeResponseBody::Json(json) => serde_json::from_str(&json).unwrap(),
        InvokeResponseBody::Raw(bytes) => panic!("{cmd} answered raw bytes: {bytes:?}"),
    }
}

fn announced(app: &App<MockRuntime>) -> Receiver<()> {
    let (sender, receiver) = channel();
    app.listen(DIALOG_REQUEST_EVENT, move |_| {
        let _ = sender.send(());
    });
    receiver
}

fn labels(app: &App<MockRuntime>) -> Vec<String> {
    app.webview_windows().into_keys().collect()
}

fn main_takes(app: &App<MockRuntime>) -> serde_json::Value {
    invoke(&app.get_webview_window(MAIN_WINDOW).expect("the main window"), "take_dialog_requests")
}

#[test]
fn settings_with_no_main_window_builds_it_and_asks_it_for_the_dialog() {
    let app = app();
    assert!(app.webview_windows().is_empty());
    let told = announced(&app);

    tray::handle(app.handle(), tray::SETTINGS_ID);

    crate::common::eventually("the main window opens", || {
        app.get_webview_window(MAIN_WINDOW).is_some()
    });
    crate::common::eventually("and is told", || told.try_recv().is_ok());
    assert_eq!(labels(&app), [MAIN_WINDOW]);
    assert_eq!(main_takes(&app), serde_json::json!([{ "dialog": "settings" }]));
    assert_eq!(main_takes(&app), serde_json::json!([]), "taken once");
}

#[test]
fn every_entry_asks_the_one_main_window_and_builds_no_window_of_its_own() {
    let app = app();
    let main = lifecycle::build_main(app.handle()).unwrap();
    let told = announced(&app);

    assert_eq!(invoke(&main, "open_settings"), serde_json::Value::Null);
    settings::open(app.handle()).join().unwrap();
    menu::handle(app.handle(), menu::SETTINGS_ID);
    menu::handle(app.handle(), menu::ABOUT_ID);
    menu::handle(app.handle(), menu::LICENCES_ID);
    menu::handle(app.handle(), menu::PRIVACY_ID);

    let mut taken = Vec::new();
    crate::common::eventually("all six requests reach the window", || {
        taken.extend(main_takes(&app).as_array().unwrap().iter().cloned());
        taken.len() == 6
    });
    assert!(told.try_recv().is_ok());
    assert_eq!(labels(&app), [MAIN_WINDOW], "no settings or about window");
    let about =
        |page: &str| serde_json::json!({ "dialog": "about", "page": page, "unopened": null });
    for asked in [about("about"), about("licences"), about("privacy")] {
        assert!(taken.contains(&asked), "{asked} in {taken:?}");
    }
    let settings = serde_json::json!({ "dialog": "settings" });
    assert_eq!(taken.iter().filter(|request| **request == settings).count(), 3);
}

#[test]
fn a_dialog_request_that_cannot_open_a_main_window_is_not_left_pending() {
    let app = app_declaring(Vec::new());

    settings::open(app.handle()).join().expect("the app was not exited");

    assert_eq!(app.state::<Lifecycle>().take_dialog_requests(), []);
    assert!(app.webview_windows().is_empty());
}

#[test]
fn the_shipped_capabilities_name_no_settings_or_about_window() {
    let capabilities = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut named: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(capabilities).unwrap() {
        let capability: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap();
        for window in capability["windows"].as_array().unwrap() {
            named.push(window.as_str().unwrap().to_owned());
        }
    }
    named.sort();
    assert_eq!(named, ["data-*", MAIN_WINDOW]);
}
