use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::utils::config::WindowConfig;
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, Manager, WebviewWindow};
use trawler_ui_lib::core::Core;
use trawler_ui_lib::keep_capturing::{self, write_keep_capturing};
use trawler_ui_lib::lifecycle::{self, Lifecycle};
use trawler_ui_lib::settings::MAIN_WINDOW;
use trawler_ui_lib::supervisor::{CoreState, Supervisor};

use crate::common::{alive, bindable, eventually, flows_status, launch};

const SHIPPED_CONFIG: &str = include_str!("../../tauri.conf.json");
const ROLE_VAR: &str = "TRAWLER_WINDOWLESS_ROLE_DIR";
const CORE_REPORT: &str = "windowless core: ";

struct Role(std::process::Child);

impl Drop for Role {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn shipped_windows() -> Vec<WindowConfig> {
    let config: serde_json::Value = serde_json::from_str(SHIPPED_CONFIG).unwrap();
    serde_json::from_value(config["app"]["windows"].clone()).unwrap()
}

fn app(config_dir: &Path, tray: bool) -> App<MockRuntime> {
    app_declaring(shipped_windows(), config_dir, tray)
}

fn app_declaring(windows: Vec<WindowConfig>, config_dir: &Path, tray: bool) -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = windows;
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            keep_capturing::keep_capturing_read,
            keep_capturing::keep_capturing_write,
            lifecycle::tray_available,
            lifecycle::take_quit_request
        ])
        .build(context)
        .expect("a mock app");
    lifecycle::install(app.handle(), Some(config_dir.to_path_buf()));
    app.state::<Lifecycle>().mark_tray(tray);
    app
}

fn supervised(ca: &Path) -> (Supervisor, u32, SocketAddr) {
    let supervisor =
        Supervisor::start(launch(ca, 0, 0), Arc::new(|_| {})).expect("a supervised core");
    let CoreState::Running { pid, api, .. } = supervisor.snapshot() else {
        panic!("expected running, got {:?}", supervisor.snapshot());
    };
    (supervisor, pid, api)
}

fn main_window_destroyed(app: &App<MockRuntime>) -> bool {
    let main = lifecycle::build_main(app.handle()).expect("the main window");
    main.destroy().expect("the main window is destroyed");
    lifecycle::on_window_destroyed(app.handle(), MAIN_WINDOW)
}

fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str, body: serde_json::Value) -> String {
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
    match response {
        InvokeResponseBody::Json(json) => json,
        InvokeResponseBody::Raw(bytes) => panic!("{cmd} answered raw bytes: {bytes:?}"),
    }
}

#[test]
fn unless_switched_on_with_a_tray_destroying_the_main_window_ends_the_app() {
    let dir = tempfile::tempdir().unwrap();
    assert!(main_window_destroyed(&app(dir.path(), true)), "unanswered quits");

    write_keep_capturing(dir.path(), false).unwrap();
    assert!(main_window_destroyed(&app(dir.path(), true)), "off quits");

    write_keep_capturing(dir.path(), true).unwrap();
    let trayless = app(dir.path(), false);
    assert!(main_window_destroyed(&trayless), "on without a tray quits");
    assert!(!lifecycle::holds_last_window_exit(trayless.handle()));
}

#[test]
fn switched_on_with_a_tray_the_app_and_its_core_outlive_the_main_window() {
    let (dir, ca) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write_keep_capturing(dir.path(), true).unwrap();
    let app = app(dir.path(), true);
    let (supervisor, pid, api) = supervised(ca.path());
    app.manage(Core::Owned(Arc::new(supervisor)));

    assert!(!main_window_destroyed(&app), "the app stays");
    assert!(lifecycle::holds_last_window_exit(app.handle()), "the last window's exit is held");

    assert!(alive(pid), "the core {pid} still runs");
    assert_eq!(flows_status(api), "HTTP/1.1 200 OK");
    app.state::<Core>().stop();
}

#[test]
fn a_reopen_with_no_main_window_builds_the_shipped_one_and_a_second_reopen_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    lifecycle::open_at_launch(app.handle(), true).unwrap();
    assert!(app.webview_windows().is_empty(), "a background launch with a tray opens nothing");

    let reopened = lifecycle::reopen_main(app.handle()).expect("a rebuilt main window");
    assert_eq!(reopened.label(), MAIN_WINDOW);

    lifecycle::reopen_main(app.handle()).expect("the same main window");
    assert_eq!(app.webview_windows().len(), 1);
}

#[test]
fn the_main_window_is_only_ever_the_one_the_config_declares() {
    let dir = tempfile::tempdir().unwrap();
    let undeclared = app_declaring(Vec::new(), dir.path(), true);
    assert!(matches!(
        lifecycle::reopen_main(undeclared.handle()),
        Err(tauri::Error::WindowNotFound)
    ));
    assert!(undeclared.webview_windows().is_empty());
}

#[test]
fn a_background_launch_without_a_tray_opens_the_main_window() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), false);
    lifecycle::open_at_launch(app.handle(), true).unwrap();
    assert!(app.get_webview_window(MAIN_WINDOW).is_some());
}

#[test]
fn the_preference_commands_round_trip_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    let main = lifecycle::build_main(app.handle()).unwrap();

    assert_eq!(invoke(&main, "keep_capturing_read", serde_json::json!({})), r#""unanswered""#);
    assert_eq!(invoke(&main, "keep_capturing_write", serde_json::json!({ "on": true })), "null");
    assert_eq!(invoke(&main, "keep_capturing_read", serde_json::json!({})), r#""on""#);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("keep-capturing.json")).unwrap(),
        r#"{"keepCapturing":true}"#
    );
    assert!(
        !lifecycle::on_window_destroyed(app.handle(), MAIN_WINDOW),
        "the written answer decides the next close"
    );
}

#[test]
fn the_page_reads_whether_this_host_has_a_menu_bar_icon() {
    let dir = tempfile::tempdir().unwrap();
    for tray in [true, false] {
        let app = app(dir.path(), tray);
        let main = lifecycle::build_main(app.handle()).unwrap();
        assert_eq!(invoke(&main, "tray_available", serde_json::json!({})), tray.to_string());
    }
}

fn announced_quits(app: &App<MockRuntime>) -> std::sync::mpsc::Receiver<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.listen(lifecycle::QUIT_REQUEST_EVENT, move |_| {
        let _ = sender.send(());
    });
    receiver
}

#[test]
fn a_quit_request_with_no_window_builds_the_main_window_and_announces_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    lifecycle::open_at_launch(app.handle(), true).unwrap();
    assert!(app.webview_windows().is_empty());
    let announced = announced_quits(&app);

    lifecycle::request_quit(app.handle()).join().unwrap();

    assert!(app.get_webview_window(MAIN_WINDOW).is_some(), "the window opens to ask");
    assert!(announced.try_recv().is_ok(), "and is told");
}

#[test]
fn a_quit_request_with_the_window_open_reuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true);
    let main = lifecycle::build_main(app.handle()).unwrap();
    let announced = announced_quits(&app);

    lifecycle::request_quit(app.handle()).join().unwrap();

    assert_eq!(app.webview_windows().len(), 1);
    assert!(announced.try_recv().is_ok());
    assert_eq!(invoke(&main, "take_quit_request", serde_json::json!({})), "true");
    assert_eq!(invoke(&main, "take_quit_request", serde_json::json!({})), "false");
}

#[test]
fn a_quit_request_that_cannot_open_a_window_does_not_exit_and_is_not_left_pending() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_declaring(Vec::new(), dir.path(), true);

    lifecycle::request_quit(app.handle()).join().expect("the app was not exited");

    assert!(!app.state::<Lifecycle>().take_quit_request(), "no flag waits for a later window");
}

#[test]
fn a_main_window_destroyed_under_a_pending_quit_request_is_rebuilt_to_ask() {
    let dir = tempfile::tempdir().unwrap();
    write_keep_capturing(dir.path(), true).unwrap();
    let app = app(dir.path(), true);
    app.state::<Lifecycle>().request_quit();

    assert!(!lifecycle::on_window_destroyed(app.handle(), MAIN_WINDOW), "the app stays");

    eventually("the window opens to ask", || app.get_webview_window(MAIN_WINDOW).is_some());
    assert!(app.state::<Lifecycle>().take_quit_request(), "and the new window finds the request");
}

#[test]
fn a_main_window_destroyed_with_nothing_pending_is_not_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    write_keep_capturing(dir.path(), true).unwrap();
    let app = app(dir.path(), true);
    assert!(!lifecycle::on_window_destroyed(app.handle(), MAIN_WINDOW));
    assert!(app.webview_windows().is_empty());
    assert!(!app.state::<Lifecycle>().take_quit_request());
}

#[test]
fn a_killed_windowless_app_leaves_no_core_behind() {
    let dir = tempfile::tempdir().unwrap();
    let mut role = Role(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "outlive_window::windowless_role",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(ROLE_VAR, dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut lines = BufReader::new(role.0.stdout.take().unwrap()).lines();
    let report = lines
        .find_map(|line| {
            line.unwrap().split_once(CORE_REPORT).map(|(_, report)| report.to_string())
        })
        .expect("the role reports its core");
    let (pid, api) = report.split_once(' ').unwrap();
    let (pid, api): (u32, SocketAddr) = (pid.parse().unwrap(), api.parse().unwrap());
    assert!(alive(pid));

    drop(role);
    eventually("the windowless app's core exits", || !alive(pid));
    assert!(bindable(api));
}

#[test]
#[ignore = "the windowless app a_killed_windowless_app_leaves_no_core_behind re-executes"]
fn windowless_role() {
    let Some(dir) = std::env::var_os(ROLE_VAR) else { return };
    let dir = Path::new(&dir);
    write_keep_capturing(&dir.join("config"), true).unwrap();
    let app = app(&dir.join("config"), true);
    let (supervisor, pid, api) = supervised(&dir.join("ca"));
    app.manage(Core::Owned(Arc::new(supervisor)));
    assert!(!main_window_destroyed(&app), "the role is windowless");
    println!("{CORE_REPORT}{pid} {api}");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
