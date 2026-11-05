use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager, RunEvent};
use trawler_ui_lib::core::Core;
use trawler_ui_lib::supervisor::{CoreState, Supervisor};

use crate::common::launch;

fn app() -> App<MockRuntime> {
    mock_builder().build(mock_context(noop_assets())).expect("a mock app")
}

fn supervised(ca: &tempfile::TempDir) -> (Supervisor, u32, SocketAddr, SocketAddr) {
    let supervisor =
        Supervisor::start(launch(ca.path(), 0, 0), Arc::new(|_| {})).expect("a supervised core");
    let CoreState::Running { pid, proxy, api, .. } = supervisor.snapshot() else {
        panic!("expected running, got {:?}", supervisor.snapshot());
    };
    (supervisor, pid, proxy, api)
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

fn bindable(addr: SocketAddr) -> bool {
    TcpListener::bind(addr).is_ok()
}

#[test]
fn the_exit_event_stops_an_owned_core_and_frees_its_ports() {
    let app = app();
    let ca = tempfile::tempdir().unwrap();
    let (supervisor, pid, proxy, api) = supervised(&ca);
    app.manage(Core::Owned(Arc::new(supervisor)));

    trawler_ui_lib::on_run_event(app.handle(), &RunEvent::Exit);

    assert!(!alive(pid), "the core process {pid} is gone");
    assert!(bindable(proxy), "the proxy port {proxy} is free");
    assert!(bindable(api), "the API port {api} is free");
}

#[test]
fn the_exit_event_leaves_an_attached_core_running() {
    let app = app();
    let ca = tempfile::tempdir().unwrap();
    let (elsewhere, pid, _, api) = supervised(&ca);
    app.manage(Core::Attached(format!("http://{api}")));

    trawler_ui_lib::on_run_event(app.handle(), &RunEvent::Exit);

    assert!(alive(pid), "the attached core {pid} still runs");
    assert!(TcpStream::connect(api).is_ok(), "the attached core still answers at {api}");
    elsewhere.stop();
}
