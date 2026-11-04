use std::sync::{mpsc, Arc, Mutex};

use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, Manager, WebviewWindow, WebviewWindowBuilder};
use trawler_ui_lib::core::{self, Core, STATE_EVENT};
use trawler_ui_lib::settings::{self, SETTINGS_WINDOW};
use trawler_ui_lib::supervisor::Supervisor;

mod common;
use common::{launch, PATIENCE};

fn app() -> App<MockRuntime> {
    mock_builder()
        .invoke_handler(tauri::generate_handler![core::restart_core, settings::open_settings])
        .build(mock_context(noop_assets()))
        .expect("a mock app")
}

fn window(app: &App<MockRuntime>) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default()).build().expect("a window")
}

fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a url"),
            body: InvokeBody::default(),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize().expect("a JSON response"))
}

fn running(state: &Value) -> (u64, String, String) {
    assert_eq!(state["state"], "running", "{state}");
    (
        state["pid"].as_u64().expect("a pid"),
        state["proxy"].as_str().expect("a proxy").to_string(),
        state["api"].as_str().expect("an api").to_string(),
    )
}

#[test]
fn restart_core_round_trips_through_the_supervisor() {
    let app = app();
    let ca = tempfile::tempdir().unwrap();
    let supervisor = Supervisor::start(launch(ca.path(), 0, 0), core::observer(app.handle()))
        .expect("a supervised core");
    let (pid, proxy, api) = running(&serde_json::to_value(supervisor.snapshot()).unwrap());
    app.manage(Core::Owned(Arc::new(supervisor)));

    let (tx, states) = mpsc::channel();
    let tx = Mutex::new(tx);
    app.listen(STATE_EVENT, move |event| {
        let state: Value = serde_json::from_str(event.payload()).expect("a state payload");
        drop(tx.lock().expect("a listener lock").send(state["state"].as_str().map(String::from)));
    });

    let restarted = invoke(&window(&app), "restart_core").expect("restart answers");
    let (new_pid, new_proxy, new_api) = running(&restarted);
    assert_ne!(new_pid, pid, "a new core process");
    assert_eq!((new_proxy, new_api), (proxy, api), "on the same ports");

    let seen: Vec<String> =
        states.recv_timeout(PATIENCE).into_iter().chain(states.try_iter()).flatten().collect();
    assert_eq!(seen, ["exited", "starting", "running"]);
    app.state::<Core>().stop();
}

#[test]
fn an_attached_core_refuses_restart_and_spawns_nothing() {
    let app = app();
    app.manage(Core::Attached("http://127.0.0.1:9".into()));
    let refused = invoke(&window(&app), "restart_core").expect_err("restart is refused");
    assert!(refused.as_str().is_some_and(|why| why.contains("not managed")), "{refused}");
}

#[test]
fn settings_opens_once_and_is_focused_after() {
    let app = app();
    settings::open(app.handle()).expect("settings opens");
    settings::open(app.handle()).expect("settings is focused");
    let open: Vec<String> =
        app.webview_windows().into_keys().filter(|label| label == SETTINGS_WINDOW).collect();
    assert_eq!(open, [SETTINGS_WINDOW]);
}

#[test]
fn settings_from_the_menu_is_built_off_the_calling_thread() {
    let app = app();
    let opening = settings::open_in_background(app.handle());
    assert_ne!(opening.thread().id(), std::thread::current().id());
    opening.join().expect("the opening thread").expect("settings opens");
    assert!(app.get_webview_window(SETTINGS_WINDOW).is_some());
}

#[test]
fn settings_from_the_page_opens_through_its_command() {
    let app = app();
    let main = window(&app);
    invoke(&main, "open_settings").expect("open_settings answers");
    invoke(&main, "open_settings").expect("a second ask focuses it");
    let open: Vec<String> =
        app.webview_windows().into_keys().filter(|label| label == SETTINGS_WINDOW).collect();
    assert_eq!(open, [SETTINGS_WINDOW]);
}
