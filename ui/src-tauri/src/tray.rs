use std::sync::{Mutex, PoisonError};

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

use crate::core::Core;
use crate::lifecycle::Lifecycle;
use crate::supervisor::CoreState;

pub const TRAY_ID: &str = "trawler-tray";
pub const STATUS_ID: &str = "tray-status";
pub const OPEN_ID: &str = "tray-open";
pub const SETTINGS_ID: &str = "tray-settings";
pub const RESTART_ID: &str = "tray-restart";
pub const QUIT_ID: &str = "tray-quit";
const RUNNING_GLYPH: &[u8] = include_bytes!("../icons/tray-running.png");
const STOPPED_GLYPH: &[u8] = include_bytes!("../icons/tray-stopped.png");

#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Settings,
    Restart,
    Quit,
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Running,
    Stopped,
}

pub fn action_for(id: &str) -> TrayAction {
    match id {
        OPEN_ID => TrayAction::Open,
        SETTINGS_ID => TrayAction::Settings,
        RESTART_ID => TrayAction::Restart,
        QUIT_ID => TrayAction::Quit,
        _ => TrayAction::Ignore,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayView {
    pub glyph: Glyph,
    pub status: String,
    pub restart: bool,
}

pub fn view(state: &CoreState) -> TrayView {
    let (glyph, status) = match state {
        CoreState::Starting => (Glyph::Running, "Capture service starting".into()),
        CoreState::Running { .. } => (Glyph::Running, "Capturing".into()),
        CoreState::Exited { .. } => (Glyph::Stopped, "Capture service stopped".into()),
        CoreState::Attached { api } => (Glyph::Running, format!("Attached to {api}")),
    };
    TrayView { glyph, status, restart: matches!(state, CoreState::Exited { .. }) }
}

pub fn glyph_image(glyph: Glyph) -> tauri::Result<Image<'static>> {
    Image::from_bytes(match glyph {
        Glyph::Running => RUNNING_GLYPH,
        Glyph::Stopped => STOPPED_GLYPH,
    })
}

struct Shown(Mutex<TrayView>);

struct NativeItems<R: Runtime> {
    status: MenuItem<R>,
    restart: MenuItem<R>,
}

impl<R: Runtime> NativeItems<R> {
    fn menu(app: &AppHandle<R>, shown: &TrayView) -> tauri::Result<(Menu<R>, Self)> {
        let status = MenuItem::with_id(app, STATUS_ID, &shown.status, false, None::<&str>)?;
        let restart = MenuItem::with_id(
            app,
            RESTART_ID,
            "Restart capture service",
            shown.restart,
            None::<&str>,
        )?;
        let menu = Menu::with_items(
            app,
            &[
                &status,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, OPEN_ID, "Open Trawler", true, None::<&str>)?,
                &MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, None::<&str>)?,
                &restart,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, QUIT_ID, "Quit Trawler", true, None::<&str>)?,
            ],
        )?;
        Ok((menu, Self { status, restart }))
    }

    fn show(&self, shown: &TrayView) -> tauri::Result<()> {
        self.status.set_text(&shown.status)?;
        self.restart.set_enabled(shown.restart)
    }
}

/// Starts recording what the menu-bar icon shows, from `state` on.
pub fn track<R: Runtime>(app: &AppHandle<R>, state: &CoreState) {
    app.manage(Shown(Mutex::new(view(state))));
}

pub fn shown<R: Runtime>(app: &AppHandle<R>) -> Option<TrayView> {
    app.try_state::<Shown>()
        .map(|shown| shown.0.lock().unwrap_or_else(PoisonError::into_inner).clone())
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
    built.is_ok()
}

// The icon goes first: a host that cannot show one refuses it before any menu,
// which muda would rather panic over off the main thread.
fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let shown = view(&latest(app, &CoreState::Starting));
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(glyph_image(shown.glyph)?)
        .icon_as_template(true)
        .tooltip("Trawler")
        .on_menu_event(|app, event| handle(app, event.id().as_ref()))
        .build(app)?;
    let (menu, items) = NativeItems::menu(app, &shown)?;
    tray.set_menu(Some(menu))?;
    app.manage(items);
    apply(app, &CoreState::Starting);
    Ok(())
}

fn latest<R: Runtime>(app: &AppHandle<R>, reported: &CoreState) -> CoreState {
    app.try_state::<Core>().map_or_else(|| reported.clone(), |core| core.state())
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

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
fn apply<R: Runtime>(app: &AppHandle<R>, reported: &CoreState) {
    let Some(recorded) = app.try_state::<Shown>() else { return };
    let shown = view(&latest(app, reported));
    *recorded.0.lock().unwrap_or_else(PoisonError::into_inner) = shown.clone();
    let listed = app.try_state::<NativeItems<R>>().map(|items| items.show(&shown));
    // tray-icon's set_icon drops the template flag the builder set (0.24).
    let drawn = app.tray_by_id(TRAY_ID).map(|tray| {
        glyph_image(shown.glyph)
            .and_then(|image| tray.set_icon(Some(image)))
            .and_then(|()| tray.set_icon_as_template(true))
    });
    for failed in [listed, drawn].into_iter().flatten().filter_map(Result::err) {
        eprintln!("showing the capture service's state in the menu bar failed: {failed}");
    }
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match action_for(id) {
        TrayAction::Open => drop(crate::lifecycle::reopen_main_in_background(app)),
        TrayAction::Settings => drop(crate::settings::open_in_background(app)),
        TrayAction::Restart => drop(crate::core::restart_in_background(app)),
        TrayAction::Quit => crate::menu::quit(app),
        TrayAction::Ignore => {}
    }
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

    #[test]
    fn each_core_state_has_its_glyph_line_and_restart() {
        let shown: Vec<TrayView> = every_state().iter().map(view).collect();
        let row = |glyph, status: &str, restart| TrayView { glyph, status: status.into(), restart };
        assert_eq!(
            shown,
            [
                row(Glyph::Running, "Capture service starting", false),
                row(Glyph::Running, "Capturing", false),
                row(Glyph::Stopped, "Capture service stopped", true),
                row(Glyph::Running, "Attached to http://127.0.0.1:9000", false),
            ]
        );
    }

    #[test]
    fn each_item_has_its_action_and_every_other_menu_event_is_ignored() {
        assert_eq!(action_for(OPEN_ID), TrayAction::Open);
        assert_eq!(action_for(SETTINGS_ID), TrayAction::Settings);
        assert_eq!(action_for(RESTART_ID), TrayAction::Restart);
        assert_eq!(action_for(QUIT_ID), TrayAction::Quit);
        for other in [STATUS_ID, crate::menu::QUIT_ID, crate::menu::SETTINGS_ID, "anything"] {
            assert_eq!(action_for(other), TrayAction::Ignore, "{other}");
        }
    }

    #[test]
    fn the_app_menu_ignores_every_tray_item() {
        for id in [STATUS_ID, OPEN_ID, SETTINGS_ID, RESTART_ID, QUIT_ID] {
            assert_eq!(crate::menu::action_for(id), crate::menu::MenuAction::Ignore, "{id}");
        }
    }

    #[test]
    fn both_glyphs_decode_at_menu_bar_size_and_differ() {
        let running = glyph_image(Glyph::Running).unwrap();
        let stopped = glyph_image(Glyph::Stopped).unwrap();
        for glyph in [&running, &stopped] {
            assert_eq!((glyph.width(), glyph.height()), (36, 36));
        }
        assert_ne!(running.rgba(), stopped.rgba());
    }
}
