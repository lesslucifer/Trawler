use std::process::Command;
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};
use trawler_ui_lib::core::{self, Core};
use trawler_ui_lib::supervisor::{CoreState, Supervisor};
use trawler_ui_lib::tray::{self, Glyph, Machine, Routing, TrayView, RESTART_ID, STOP_ROUTING_ID};

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
        TrayView {
            glyph: Glyph::Running,
            status: "Capturing".into(),
            restart: false,
            routing: Routing::Unread
        }
    );

    Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap();
    eventually("the icon reads stopped", || shown(&app).glyph == Glyph::Stopped);
    assert_eq!(
        shown(&app),
        TrayView {
            glyph: Glyph::Stopped,
            status: "Capture service stopped".into(),
            restart: true,
            routing: Routing::Unread
        }
    );

    tray::handle(app.handle(), RESTART_ID);
    eventually("the core runs again", || ports(&app.state::<Core>().state()).is_some());
    assert_eq!(ports(&app.state::<Core>().state()), ports(&running), "on the same ports");
    eventually("the icon reads capturing", || shown(&app).status == "Capturing");
    assert!(!shown(&app).restart, "Restart is greyed again");

    tray::on_state(app.handle(), &CoreState::Exited { code: None, signal: Some(9) });
    assert_eq!(shown(&app).status, "Capturing", "a late exit report never outranks the core");
    app.state::<Core>().stop();
}

struct Scripted {
    routing: std::sync::Mutex<Routing>,
    asked: std::sync::Mutex<Vec<std::net::SocketAddr>>,
}

fn scripted(app: &App<MockRuntime>, routing: Routing) {
    app.manage(Scripted { routing: routing.into(), asked: Vec::new().into() });
    app.manage(Machine::<MockRuntime> {
        read: |app, proxy, _| {
            let scripted = app.state::<Scripted>();
            scripted.asked.lock().unwrap().push(*proxy);
            let routing = *scripted.routing.lock().unwrap();
            routing
        },
        stop: |app| *app.state::<Scripted>().routing.lock().unwrap() = Routing::Off,
    });
}

fn running_at(proxy: &str) -> CoreState {
    CoreState::Running {
        pid: 7,
        proxy: proxy.parse().unwrap(),
        api: "127.0.0.1:7421".parse().unwrap(),
        started_at_ms: 0,
    }
}

#[test]
fn the_routing_line_follows_the_reading_and_stop_turns_it_off() {
    let app = app();
    scripted(&app, Routing::Ours);
    tray::track(app.handle(), &running_at("127.0.0.1:7420"));
    assert_eq!(shown(&app).routing, Routing::Unread, "nothing is read before the first reading");
    assert!(!shown(&app).stop_routing());

    tray::read_routing(app.handle());
    assert_eq!(shown(&app).routing, Routing::Ours);
    assert!(shown(&app).stop_routing(), "Stop is offered while this machine routes here");
    assert_eq!(shown(&app).status, "Capturing", "a reading leaves the status line as it was");
    assert_eq!(
        *app.state::<Scripted>().asked.lock().unwrap(),
        ["127.0.0.1:7420".parse().unwrap()],
        "read against the capture service's own address"
    );

    tray::handle(app.handle(), STOP_ROUTING_ID);
    eventually("the stop reads the proxy again", || shown(&app).routing == Routing::Off);
    assert!(!shown(&app).stop_routing(), "nothing left to stop");

    *app.state::<Scripted>().routing.lock().unwrap() = Routing::Foreign;
    tray::read_routing(app.handle());
    assert_eq!(shown(&app).routing, Routing::Foreign);
    assert!(!shown(&app).stop_routing(), "another proxy is never Trawler's to stop");
}

#[test]
fn a_stopped_service_is_read_at_the_address_it_last_listened_on_and_an_attached_one_is_not_read() {
    let app = app();
    scripted(&app, Routing::Ours);
    tray::track(app.handle(), &running_at("127.0.0.1:7555"));
    tray::on_state(app.handle(), &CoreState::Exited { code: None, signal: Some(9) });
    tray::read_routing(app.handle());
    assert_eq!(shown(&app).status, "Capture service stopped");
    assert_eq!(shown(&app).routing, Routing::Ours, "a dead service can still hold the proxy");
    assert_eq!(*app.state::<Scripted>().asked.lock().unwrap(), ["127.0.0.1:7555".parse().unwrap()]);

    let attached = self::app();
    scripted(&attached, Routing::Ours);
    tray::track(attached.handle(), &CoreState::Attached { api: "http://127.0.0.1:9000".into() });
    tray::read_routing(attached.handle());
    assert_eq!(shown(&attached).routing, Routing::Unread, "no proxy address is known");
    assert!(attached.state::<Scripted>().asked.lock().unwrap().is_empty());
}

// macOS refuses a native tray off the main thread, which is where a test runs.
#[cfg(target_os = "macos")]
#[test]
fn a_host_that_cannot_build_the_tray_is_never_left_windowless() {
    use trawler_ui_lib::lifecycle::{self, Lifecycle};
    use trawler_ui_lib::settings::MAIN_WINDOW;

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
