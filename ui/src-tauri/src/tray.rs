use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::about::{Page, Show};
use crate::core::Core;
use crate::lifecycle::Lifecycle;
use crate::settings::MAIN_WINDOW;
use crate::supervisor::CoreState;

pub const TRAY_ID: &str = "trawler-tray";
pub const STATUS_ID: &str = "tray-status";
pub const ROUTING_ID: &str = "tray-routing";
pub const OPEN_ID: &str = "tray-open";
pub const SETTINGS_ID: &str = "tray-settings";
pub const RESTART_ID: &str = "tray-restart";
pub const STOP_ROUTING_ID: &str = "tray-stop-routing";
pub const CHECK_UPDATES_ID: &str = "tray-check-updates";
pub const ABOUT_ID: &str = "tray-about";
pub const QUIT_ID: &str = "tray-quit";
/// How often a windowless app reads this machine's proxy again for the menu.
pub const WINDOWLESS_READ: Duration = Duration::from_secs(15);
/// macOS tints a template glyph to the menu bar; Windows and Linux draw the
/// coloured pair as is (DEC-89 rule 5).
pub const TEMPLATE: bool = cfg!(target_os = "macos");
#[cfg(target_os = "macos")]
const RUNNING_GLYPH: &[u8] = include_bytes!("../icons/tray-running.png");
#[cfg(target_os = "macos")]
const STOPPED_GLYPH: &[u8] = include_bytes!("../icons/tray-stopped.png");
#[cfg(not(target_os = "macos"))]
const RUNNING_GLYPH: &[u8] = include_bytes!("../icons/tray-running-color.png");
#[cfg(not(target_os = "macos"))]
const STOPPED_GLYPH: &[u8] = include_bytes!("../icons/tray-stopped-color.png");
const OURS_DOT: &[u8] = include_bytes!("../icons/routing-ours.png");
const FOREIGN_DOT: &[u8] = include_bytes!("../icons/routing-foreign.png");
const OFF_DOT: &[u8] = include_bytes!("../icons/routing-off.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    MacOs,
    Windows,
    Linux,
}

impl Host {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Linux has no automatic proxy setup (DEC-64 rule 7), so nothing to stop.
    pub const fn stops_routing(self) -> bool {
        !matches!(self, Self::Linux)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Settings,
    Restart,
    StopRouting,
    CheckForUpdates,
    About,
    Quit,
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Running,
    Stopped,
}

/// This machine's system proxy, as the quit check classifies it (DEC-64).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routing {
    Ours,
    Foreign,
    Off,
    Unread,
}

impl Routing {
    pub fn from_status(status: &str) -> Self {
        match status {
            "ours" => Self::Ours,
            "foreign" => Self::Foreign,
            "none" => Self::Off,
            _ => Self::Unread,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    Ours,
    Foreign,
    Off,
}

/// The menu's routing line: its words and dot, or none while nothing was read.
pub fn routing_line(routing: Routing) -> Option<(&'static str, Dot)> {
    match routing {
        Routing::Ours => Some(("This machine routes through Trawler", Dot::Ours)),
        Routing::Foreign => Some(("Another proxy is set", Dot::Foreign)),
        Routing::Off => Some(("The system proxy is off", Dot::Off)),
        Routing::Unread => None,
    }
}

pub fn dot_image(dot: Dot) -> tauri::Result<Image<'static>> {
    Image::from_bytes(match dot {
        Dot::Ours => OURS_DOT,
        Dot::Foreign => FOREIGN_DOT,
        Dot::Off => OFF_DOT,
    })
}

pub fn action_for(id: &str) -> TrayAction {
    match id {
        OPEN_ID => TrayAction::Open,
        SETTINGS_ID => TrayAction::Settings,
        RESTART_ID => TrayAction::Restart,
        STOP_ROUTING_ID => TrayAction::StopRouting,
        CHECK_UPDATES_ID => TrayAction::CheckForUpdates,
        ABOUT_ID => TrayAction::About,
        QUIT_ID => TrayAction::Quit,
        _ => TrayAction::Ignore,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entry {
    Status,
    Routing,
    Item { id: &'static str, label: &'static str },
    Separator,
}

/// The menu top to bottom: macOS labels its commands in Title Case, Windows
/// and Linux in sentence case.
pub fn entries(host: Host) -> Vec<Entry> {
    let title = host == Host::MacOs;
    let item = |id, title_case, sentence_case| Entry::Item {
        id,
        label: if title { title_case } else { sentence_case },
    };
    let mut entries = vec![
        Entry::Status,
        Entry::Routing,
        Entry::Separator,
        item(OPEN_ID, "Open Trawler", "Open Trawler"),
        item(SETTINGS_ID, "Settings…", "Settings…"),
        item(RESTART_ID, "Restart Capture Service", "Restart capture service"),
        Entry::Separator,
    ];
    if host.stops_routing() {
        entries.extend([
            item(STOP_ROUTING_ID, "Stop Routing Through Trawler", "Stop routing through Trawler"),
            Entry::Separator,
        ]);
    }
    entries.extend([
        item(CHECK_UPDATES_ID, "Check for Updates…", "Check for updates…"),
        item(ABOUT_ID, "About Trawler", "About Trawler"),
        Entry::Separator,
        item(QUIT_ID, "Quit Trawler", "Quit Trawler"),
    ]);
    entries
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClickAction {
    Open,
    ReadRouting,
    Nothing,
}

/// Windows opens Trawler on a left click and keeps the menu for the right;
/// macOS shows the menu on any click. Every press that opens a menu reads the
/// proxy afresh. Linux's `AppIndicator` reports no clicks.
pub fn click_action(host: Host, button: MouseButton, state: MouseButtonState) -> ClickAction {
    match (host, button, state) {
        (Host::Windows, MouseButton::Left, MouseButtonState::Up) => ClickAction::Open,
        (Host::MacOs, _, MouseButtonState::Down)
        | (Host::Windows, MouseButton::Right | MouseButton::Middle, MouseButtonState::Down) => {
            ClickAction::ReadRouting
        }
        _ => ClickAction::Nothing,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayView {
    pub glyph: Glyph,
    pub status: String,
    pub restart: bool,
    pub routing: Routing,
}

impl TrayView {
    pub fn stop_routing(&self) -> bool {
        self.routing == Routing::Ours
    }

    pub fn tooltip(&self) -> String {
        format!("Trawler — {}", self.status)
    }
}

pub fn view(state: &CoreState, routing: Routing) -> TrayView {
    let (glyph, status) = match state {
        CoreState::Starting => (Glyph::Running, "Capture service starting"),
        CoreState::Running { .. } => (Glyph::Running, "Capturing"),
        CoreState::Exited { .. } => (Glyph::Stopped, "Capture service stopped"),
        CoreState::Attached { .. } => (Glyph::Running, "Attached to a capture service"),
    };
    TrayView {
        glyph,
        status: status.into(),
        restart: matches!(state, CoreState::Exited { .. }),
        routing,
    }
}

pub fn glyph_image(glyph: Glyph) -> tauri::Result<Image<'static>> {
    Image::from_bytes(match glyph {
        Glyph::Running => RUNNING_GLYPH,
        Glyph::Stopped => STOPPED_GLYPH,
    })
}

/// How the menu reads this machine's proxy and turns it off: the host's own
/// tools, the scenario build's script, or a test's.
pub struct Machine<R: Runtime> {
    pub read: fn(&AppHandle<R>, &SocketAddr, Vec<String>) -> Routing,
    pub stop: fn(&AppHandle<R>, &SocketAddr, Vec<String>),
}

struct Shown {
    view: Mutex<TrayView>,
    proxy: Mutex<Option<SocketAddr>>,
    reading: AtomicBool,
}

struct NativeItems<R: Runtime> {
    menu: Menu<R>,
    status: MenuItem<R>,
    routing: IconMenuItem<R>,
    routing_listed: Mutex<bool>,
    restart: MenuItem<R>,
    stop_routing: Option<MenuItem<R>>,
}

impl<R: Runtime> NativeItems<R> {
    fn menu(app: &AppHandle<R>, shown: &TrayView) -> tauri::Result<Self> {
        let status = MenuItem::with_id(app, STATUS_ID, &shown.status, false, None::<&str>)?;
        let routing = IconMenuItem::with_id(
            app,
            ROUTING_ID,
            "",
            false,
            Some(dot_image(Dot::Off)?),
            None::<&str>,
        )?;
        let host = Host::current();
        let restart = MenuItem::with_id(app, RESTART_ID, "", false, None::<&str>)?;
        let stop_routing = host
            .stops_routing()
            .then(|| MenuItem::with_id(app, STOP_ROUTING_ID, "", false, None::<&str>))
            .transpose()?;
        let menu = Menu::new(app)?;
        for entry in entries(host) {
            match entry {
                Entry::Status => menu.append(&status)?,
                Entry::Routing => {}
                Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
                Entry::Item { id: RESTART_ID, label } => {
                    restart.set_text(label)?;
                    menu.append(&restart)?;
                }
                Entry::Item { id: STOP_ROUTING_ID, label } => {
                    if let Some(stop) = &stop_routing {
                        stop.set_text(label)?;
                        menu.append(stop)?;
                    }
                }
                Entry::Item { id, label } => {
                    menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
                }
            }
        }
        let items = Self {
            menu,
            status,
            routing,
            routing_listed: Mutex::new(false),
            restart,
            stop_routing,
        };
        items.show(shown)?;
        Ok(items)
    }

    fn show(&self, shown: &TrayView) -> tauri::Result<()> {
        self.status.set_text(&shown.status)?;
        self.restart.set_enabled(shown.restart)?;
        if let Some(stop) = &self.stop_routing {
            stop.set_enabled(shown.stop_routing())?;
        }
        let mut listed = self.routing_listed.lock().unwrap_or_else(PoisonError::into_inner);
        match routing_line(shown.routing) {
            Some((line, dot)) => {
                self.routing.set_text(line)?;
                self.routing.set_icon(Some(dot_image(dot)?))?;
                if !*listed {
                    self.menu.insert(&self.routing as &dyn IsMenuItem<R>, 1)?;
                    *listed = true;
                }
            }
            None if *listed => {
                self.menu.remove(&self.routing as &dyn IsMenuItem<R>)?;
                *listed = false;
            }
            None => {}
        }
        Ok(())
    }
}

/// Starts recording what the menu-bar icon shows, from `state` on.
pub fn track<R: Runtime>(app: &AppHandle<R>, state: &CoreState) {
    app.manage(Shown {
        view: Mutex::new(view(state, Routing::Unread)),
        proxy: Mutex::new(running_proxy(state)),
        reading: AtomicBool::new(false),
    });
}

pub fn shown<R: Runtime>(app: &AppHandle<R>) -> Option<TrayView> {
    app.try_state::<Shown>()
        .map(|shown| shown.view.lock().unwrap_or_else(PoisonError::into_inner).clone())
}

/// Builds the menu-bar icon and records whether the host could show one: the
/// app is never left windowless without it (DEC-77 rule 1).
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn install<R: Runtime>(app: &AppHandle<R>) -> bool {
    track(app, &latest(app, &CoreState::Starting));
    let built = build(app);
    if let Err(err) = &built {
        app.remove_tray_by_id(TRAY_ID);
        eprintln!("no menu-bar icon on this host: {err}");
    }
    if let Some(lifecycle) = app.try_state::<Lifecycle>() {
        lifecycle.mark_tray(built.is_ok());
    }
    if built.is_ok() {
        read_while_windowless(app);
    }
    built.is_ok()
}

// The icon goes first: a host that cannot show one refuses it before any menu,
// which muda would rather panic over off the main thread.
fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let shown = view(&latest(app, &CoreState::Starting), Routing::Unread);
    let host = Host::current();
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph_image(shown.glyph)?)
        .icon_as_template(TEMPLATE)
        .tooltip(shown.tooltip())
        .show_menu_on_left_click(host != Host::Windows)
        .on_menu_event(|app, event| handle(app, event.id().as_ref()))
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button, button_state, .. } = event {
                clicked(tray.app_handle(), click_action(host, button, button_state));
            }
        })
        .build(app)?;
    let items = NativeItems::menu(app, &shown)?;
    tray.set_menu(Some(items.menu.clone()))?;
    app.manage(items);
    apply(app, &CoreState::Starting);
    Ok(())
}

fn clicked<R: Runtime>(app: &AppHandle<R>, action: ClickAction) {
    match action {
        ClickAction::Open => drop(crate::lifecycle::reopen_main_in_background(app)),
        ClickAction::ReadRouting => drop(read_routing_in_background(app)),
        ClickAction::Nothing => {}
    }
}

fn latest<R: Runtime>(app: &AppHandle<R>, reported: &CoreState) -> CoreState {
    app.try_state::<Core>().map_or_else(|| reported.clone(), |core| core.state())
}

fn running_proxy(state: &CoreState) -> Option<SocketAddr> {
    match state {
        CoreState::Running { proxy, .. } => Some(*proxy),
        _ => None,
    }
}

// Observer calls race each other (an exit and a restart's start publish from two
// threads), so the payload's order is not the core's. Every apply runs on the
// main thread, in the order queued, and shows the supervisor's state at that
// moment: the last one to run always shows the newest.
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn on_state<R: Runtime>(app: &AppHandle<R>, state: &CoreState) {
    let (main, reported) = (app.clone(), state.clone());
    if let Err(err) = app.run_on_main_thread(move || apply(&main, &reported)) {
        eprintln!("queueing the menu bar's update failed: {err}");
    }
}

fn apply<R: Runtime>(app: &AppHandle<R>, reported: &CoreState) {
    let Some(recorded) = app.try_state::<Shown>() else { return };
    let state = latest(app, reported);
    if let Some(proxy) = running_proxy(&state) {
        *recorded.proxy.lock().unwrap_or_else(PoisonError::into_inner) = Some(proxy);
    }
    {
        let mut shown = recorded.view.lock().unwrap_or_else(PoisonError::into_inner);
        *shown = view(&state, shown.routing);
    }
    draw(app);
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
fn draw<R: Runtime>(app: &AppHandle<R>) {
    let Some(shown) = shown(app) else { return };
    let listed = app.try_state::<NativeItems<R>>().map(|items| items.show(&shown));
    // tray-icon's set_icon drops the template flag the builder set (0.24).
    let drawn = app.tray_by_id(TRAY_ID).map(|tray| {
        glyph_image(shown.glyph)
            .and_then(|image| tray.set_icon(Some(image)))
            .and_then(|()| tray.set_icon_as_template(TEMPLATE))
            .and_then(|()| tray.set_tooltip(Some(shown.tooltip())))
    });
    for failed in [listed, drawn].into_iter().flatten().filter_map(Result::err) {
        eprintln!("showing the capture service's state in the menu bar failed: {failed}");
    }
}

/// Reads this machine's proxy against the address the capture service last
/// listened on and shows the answer; with no such address, or no reader,
/// nothing is read.
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn read_routing<R: Runtime>(app: &AppHandle<R>) -> Routing {
    let Some(recorded) = app.try_state::<Shown>() else { return Routing::Unread };
    let routing = match (app.try_state::<Machine<R>>(), last_proxy(app)) {
        (Some(machine), Some(proxy)) => (machine.read)(app, &proxy, own_addresses()),
        _ => Routing::Unread,
    };
    recorded.view.lock().unwrap_or_else(PoisonError::into_inner).routing = routing;
    let main = app.clone();
    if let Err(err) = app.run_on_main_thread(move || draw(&main)) {
        eprintln!("queueing the menu bar's routing line failed: {err}");
    }
    routing
}

fn last_proxy<R: Runtime>(app: &AppHandle<R>) -> Option<SocketAddr> {
    let recorded = app.try_state::<Shown>()?;
    let proxy = *recorded.proxy.lock().unwrap_or_else(PoisonError::into_inner);
    proxy
}

fn own_addresses() -> Vec<String> {
    trawler::reach::interfaces().into_iter().map(|found| found.address.to_string()).collect()
}

struct ReadClaim<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Drop for ReadClaim<R> {
    fn drop(&mut self) {
        if let Some(recorded) = self.0.try_state::<Shown>() {
            recorded.reading.store(false, Ordering::SeqCst);
        }
    }
}

// A host tool can hang for its whole patience, and presses arrive faster: one
// reading at a time, and a press or poll during it takes that reading's answer.
fn claim_read<R: Runtime>(app: &AppHandle<R>) -> Option<ReadClaim<R>> {
    let recorded = app.try_state::<Shown>()?;
    (!recorded.reading.swap(true, Ordering::SeqCst)).then(|| ReadClaim(app.clone()))
}

// Reading runs the host's proxy tools, which may take a moment: never on the
// main thread, where the menu is.
#[allow(clippy::disallowed_methods)] // a thread of its own, never a tokio worker
pub fn read_routing_in_background<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<std::thread::JoinHandle<()>> {
    let claim = claim_read(app)?;
    Some(std::thread::spawn(move || {
        read_routing(&claim.0);
    }))
}

fn read_unless_reading<R: Runtime>(app: &AppHandle<R>) {
    if let Some(claim) = claim_read(app) {
        read_routing(&claim.0);
    }
}

#[allow(clippy::disallowed_methods)] // a thread of its own, never a tokio worker
fn read_while_windowless<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        read_unless_reading(&app);
        loop {
            std::thread::sleep(WINDOWLESS_READ);
            if app.get_webview_window(MAIN_WINDOW).is_none() {
                read_unless_reading(&app);
            }
        }
    });
}

/// Reads this machine's proxy again and turns it off only while that reading
/// is still Trawler's: the menu may have opened on an older one. Either way
/// the menu ends on what this machine routes through now.
#[allow(clippy::disallowed_methods)] // a thread of its own, never a tokio worker
pub fn stop_routing_in_background<R: Runtime>(app: &AppHandle<R>) -> std::thread::JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        if read_routing(&app) != Routing::Ours {
            return;
        }
        if let (Some(machine), Some(proxy)) = (app.try_state::<Machine<R>>(), last_proxy(&app)) {
            (machine.stop)(&app, &proxy, own_addresses());
        }
        read_routing(&app);
    })
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match action_for(id) {
        TrayAction::Open => drop(crate::lifecycle::reopen_main_in_background(app)),
        TrayAction::Settings => drop(crate::settings::open(app)),
        TrayAction::Restart => drop(crate::core::restart_in_background(app)),
        TrayAction::StopRouting => drop(stop_routing_in_background(app)),
        TrayAction::CheckForUpdates => crate::update::check(app, crate::update::Asked::ByUser),
        TrayAction::About => drop(crate::about::open(app, Show::page(Page::About))),
        TrayAction::Quit => drop(crate::lifecycle::request_quit(app)),
        TrayAction::Ignore => {}
    }
}

// A menu-bar click `after_ms` into the windowless state, which WebDriver can
// neither make nor ask for from a page: the windowless app has none.
#[cfg(feature = "e2e")]
#[tauri::command]
#[allow(clippy::disallowed_methods)] // a thread of its own, never a tokio worker
pub fn e2e_tray_when_windowless<R: Runtime>(app: AppHandle<R>, id: String, after_ms: u64) {
    std::thread::spawn(move || {
        while app.get_webview_window(MAIN_WINDOW).is_some() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        std::thread::sleep(std::time::Duration::from_millis(after_ms));
        handle(&app, &id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_state() -> [CoreState; 4] {
        [
            CoreState::Starting,
            CoreState::Running {
                pid: 7,
                proxy: "127.0.0.1:7420".parse().unwrap(),
                api: "127.0.0.1:7421".parse().unwrap(),
                started_at_ms: 0,
            },
            CoreState::Exited { code: None, signal: Some(9) },
            CoreState::Attached { api: "http://127.0.0.1:9000".into() },
        ]
    }

    fn labels(host: Host) -> Vec<&'static str> {
        entries(host)
            .into_iter()
            .map(|entry| match entry {
                Entry::Status => "status",
                Entry::Routing => "routing",
                Entry::Separator => "|",
                Entry::Item { label, .. } => label,
            })
            .collect()
    }

    #[test]
    fn each_core_state_has_its_glyph_line_and_restart() {
        let shown: Vec<TrayView> =
            every_state().iter().map(|state| view(state, Routing::Unread)).collect();
        let row = |glyph, status: &str, restart| TrayView {
            glyph,
            status: status.into(),
            restart,
            routing: Routing::Unread,
        };
        assert_eq!(
            shown,
            [
                row(Glyph::Running, "Capture service starting", false),
                row(Glyph::Running, "Capturing", false),
                row(Glyph::Stopped, "Capture service stopped", true),
                row(Glyph::Running, "Attached to a capture service", false),
            ]
        );
    }

    #[test]
    fn no_line_and_no_tooltip_shows_an_address() {
        for state in every_state() {
            let shown = view(&state, Routing::Ours);
            for text in [shown.status.clone(), shown.tooltip()] {
                assert!(!text.contains("://") && !text.contains("127.0.0.1"), "{text}");
            }
        }
    }

    #[test]
    fn the_tooltip_mirrors_the_status_line() {
        let tooltips: Vec<String> =
            every_state().iter().map(|state| view(state, Routing::Off).tooltip()).collect();
        assert_eq!(
            tooltips,
            [
                "Trawler — Capture service starting",
                "Trawler — Capturing",
                "Trawler — Capture service stopped",
                "Trawler — Attached to a capture service",
            ]
        );
    }

    #[test]
    fn the_routing_line_and_its_dot_follow_the_reading_and_hide_while_unread() {
        assert_eq!(
            routing_line(Routing::Ours),
            Some(("This machine routes through Trawler", Dot::Ours))
        );
        assert_eq!(routing_line(Routing::Foreign), Some(("Another proxy is set", Dot::Foreign)));
        assert_eq!(routing_line(Routing::Off), Some(("The system proxy is off", Dot::Off)));
        assert_eq!(routing_line(Routing::Unread), None);
    }

    #[test]
    fn the_shells_proxy_classification_maps_to_a_routing_reading() {
        assert_eq!(Routing::from_status("ours"), Routing::Ours);
        assert_eq!(Routing::from_status("foreign"), Routing::Foreign);
        assert_eq!(Routing::from_status("none"), Routing::Off);
        assert_eq!(Routing::from_status("unknown"), Routing::Unread);
    }

    #[test]
    fn stop_routing_is_enabled_only_while_this_machine_routes_through_trawler() {
        let running = &every_state()[1];
        for (routing, enabled) in [
            (Routing::Ours, true),
            (Routing::Foreign, false),
            (Routing::Off, false),
            (Routing::Unread, false),
        ] {
            assert_eq!(view(running, routing).stop_routing(), enabled, "{routing:?}");
        }
    }

    #[test]
    fn each_dot_decodes_at_menu_size_and_the_three_differ() {
        let dots: Vec<Image<'_>> =
            [Dot::Ours, Dot::Foreign, Dot::Off].map(|dot| dot_image(dot).unwrap()).into();
        for dot in &dots {
            assert_eq!((dot.width(), dot.height()), (36, 36));
        }
        assert_ne!(dots[0].rgba(), dots[1].rgba());
        assert_ne!(dots[1].rgba(), dots[2].rgba());
        assert_ne!(dots[0].rgba(), dots[2].rgba());
    }

    #[test]
    fn the_menu_groups_state_window_routing_app_and_quit_in_title_case_on_macos() {
        assert_eq!(
            labels(Host::MacOs),
            [
                "status",
                "routing",
                "|",
                "Open Trawler",
                "Settings…",
                "Restart Capture Service",
                "|",
                "Stop Routing Through Trawler",
                "|",
                "Check for Updates…",
                "About Trawler",
                "|",
                "Quit Trawler",
            ]
        );
    }

    #[test]
    fn windows_has_the_same_menu_in_sentence_case() {
        assert_eq!(
            labels(Host::Windows),
            [
                "status",
                "routing",
                "|",
                "Open Trawler",
                "Settings…",
                "Restart capture service",
                "|",
                "Stop routing through Trawler",
                "|",
                "Check for updates…",
                "About Trawler",
                "|",
                "Quit Trawler",
            ]
        );
    }

    #[test]
    fn linux_has_the_routing_line_but_no_stop() {
        assert_eq!(
            labels(Host::Linux),
            [
                "status",
                "routing",
                "|",
                "Open Trawler",
                "Settings…",
                "Restart capture service",
                "|",
                "Check for updates…",
                "About Trawler",
                "|",
                "Quit Trawler",
            ]
        );
    }

    #[test]
    fn every_listed_item_has_its_own_action() {
        for host in [Host::MacOs, Host::Windows, Host::Linux] {
            for entry in entries(host) {
                if let Entry::Item { id, .. } = entry {
                    assert_ne!(action_for(id), TrayAction::Ignore, "{id}");
                }
            }
        }
    }

    #[test]
    fn each_item_has_its_action_and_every_other_menu_event_is_ignored() {
        assert_eq!(action_for(OPEN_ID), TrayAction::Open);
        assert_eq!(action_for(SETTINGS_ID), TrayAction::Settings);
        assert_eq!(action_for(RESTART_ID), TrayAction::Restart);
        assert_eq!(action_for(STOP_ROUTING_ID), TrayAction::StopRouting);
        assert_eq!(action_for(CHECK_UPDATES_ID), TrayAction::CheckForUpdates);
        assert_eq!(action_for(ABOUT_ID), TrayAction::About);
        assert_eq!(action_for(QUIT_ID), TrayAction::Quit);
        for other in
            [STATUS_ID, ROUTING_ID, crate::menu::QUIT_ID, crate::menu::SETTINGS_ID, "anything"]
        {
            assert_eq!(action_for(other), TrayAction::Ignore, "{other}");
        }
    }

    #[test]
    fn the_app_menu_ignores_every_tray_item() {
        for id in [
            STATUS_ID,
            ROUTING_ID,
            OPEN_ID,
            SETTINGS_ID,
            RESTART_ID,
            STOP_ROUTING_ID,
            CHECK_UPDATES_ID,
            ABOUT_ID,
            QUIT_ID,
        ] {
            assert_eq!(crate::menu::action_for(id), crate::menu::MenuAction::Ignore, "{id}");
        }
    }

    #[test]
    fn a_windows_left_click_opens_trawler_and_a_right_click_reads_for_the_menu() {
        use MouseButtonState::{Down, Up};
        let click = |button, state| click_action(Host::Windows, button, state);
        assert_eq!(click(MouseButton::Left, Up), ClickAction::Open);
        assert_eq!(click(MouseButton::Left, Down), ClickAction::Nothing);
        assert_eq!(click(MouseButton::Right, Down), ClickAction::ReadRouting);
        assert_eq!(click(MouseButton::Right, Up), ClickAction::Nothing);
    }

    #[test]
    fn a_macos_click_of_any_button_reads_for_the_menu_and_linux_reports_none() {
        use MouseButtonState::{Down, Up};
        for button in [MouseButton::Left, MouseButton::Right] {
            assert_eq!(click_action(Host::MacOs, button, Down), ClickAction::ReadRouting);
            assert_eq!(click_action(Host::MacOs, button, Up), ClickAction::Nothing);
            for state in [Down, Up] {
                assert_eq!(click_action(Host::Linux, button, state), ClickAction::Nothing);
            }
        }
    }

    #[test]
    fn only_linux_has_no_stop_and_the_host_is_the_build_target() {
        assert!(Host::MacOs.stops_routing() && Host::Windows.stops_routing());
        assert!(!Host::Linux.stops_routing());
        let expected = if cfg!(target_os = "macos") {
            Host::MacOs
        } else if cfg!(windows) {
            Host::Windows
        } else {
            Host::Linux
        };
        assert_eq!(Host::current(), expected);
    }

    #[test]
    fn both_glyphs_decode_at_menu_bar_size_and_differ() {
        let running = glyph_image(Glyph::Running).unwrap();
        let stopped = glyph_image(Glyph::Stopped).unwrap();
        let side = if cfg!(target_os = "macos") { 36 } else { 32 };
        for glyph in [&running, &stopped] {
            assert_eq!((glyph.width(), glyph.height()), (side, side));
        }
        assert_ne!(running.rgba(), stopped.rgba());
    }

    #[test]
    fn only_macos_shows_a_black_template_glyph_every_other_host_a_coloured_one() {
        assert_eq!(TEMPLATE, cfg!(target_os = "macos"));
        for glyph in [Glyph::Running, Glyph::Stopped] {
            let image = glyph_image(glyph).unwrap();
            let black = image.rgba().chunks(4).all(|px| px[..3] == [0, 0, 0]);
            assert_eq!(black, TEMPLATE, "{glyph:?}");
        }
    }
}
