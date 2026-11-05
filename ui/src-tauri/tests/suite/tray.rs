use std::process::Command;
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};
use trawler_ui_lib::core::{self, Core};
use trawler_ui_lib::lifecycle::{self, Lifecycle};
use trawler_ui_lib::settings::MAIN_WINDOW;
use trawler_ui_lib::supervisor::{CoreState, Supervisor};
use trawler_ui_lib::tray::{self, Glyph, TrayView, RESTART_ID};

use crate::common::{eventually, launch};

fn app() -> App<MockRuntime> {
    mock_builder().build(mock_context(noop_assets())).expect("a mock app")
}

fn shown(app: &App<MockRuntime>) -> TrayView {
    tray::shown(app.handle()).expect("the tray's view is tracked")
}

fn ports(state: &CoreState) -> Option<(std::net::SocketAddr, std::net::SocketAddr)> {
    match state {
        CoreState::Running { proxy, api, .. } => Some((*proxy, *api)),
        _ => None,
    }
}

#[test]
fn the_menu_follows_a_killed_core_and_restarts_it_on_its_ports() {
    let app = app();
    let ca = tempfile::tempdir().unwrap();
    let supervisor = Supervisor::start(launch(ca.path(), 0, 0), core::observer(app.handle()))
        .expect("a supervised core");
    let running = supervisor.snapshot();
    let CoreState::Running { pid, .. } = running else {
        panic!("expected running, got {running:?}")
    };
    tray::track(app.handle(), &running);
    app.manage(Core::Owned(Arc::new(supervisor)));
    assert_eq!(
        shown(&app),
        TrayView { glyph: Glyph::Running, status: "Capturing".into(), restart: false }
    );

    Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap();
    eventually("the icon reads stopped", || shown(&app).glyph == Glyph::Stopped);
    assert_eq!(
        shown(&app),
        TrayView { glyph: Glyph::Stopped, status: "Capture service stopped".into(), restart: true }
    );

    tray::handle(app.handle(), RESTART_ID);
    eventually("the core runs again", || ports(&app.state::<Core>().state()).is_some());
    assert_eq!(ports(&app.state::<Core>().state()), ports(&running), "on the same ports");
    eventually("the icon reads capturing", || shown(&app).status == "Capturing");
    assert!(!shown(&app).restart, "Restart is greyed again");
    app.state::<Core>().stop();
}

#[test]
fn a_host_that_cannot_build_the_tray_is_never_left_windowless() {
    let app = app();
    let dir = tempfile::tempdir().unwrap();
    trawler_ui_lib::keep_capturing::write_keep_capturing(dir.path(), true).unwrap();
    lifecycle::install(app.handle(), Some(dir.path().to_path_buf()));
    app.manage(Core::Attached("http://127.0.0.1:9".into()));

    assert!(!tray::install(app.handle()), "the native tray refuses off the main thread");

    assert!(!app.state::<Lifecycle>().tray_available());
    assert!(lifecycle::on_window_destroyed(app.handle(), MAIN_WINDOW), "closing main still quits");
    assert!(!lifecycle::holds_last_window_exit(app.handle()));
}
