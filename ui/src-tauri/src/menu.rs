// The default macOS menu quits through `terminate:`, which never raises the
// window's close request, so ⌘Q skipped the quit-time proxy check (W11's D1).
// Trawler's own Quit is a quit request: the window runs the check and exits,
// whether or not the app was windowless. The Dock's Quit and a logout still
// terminate: tao offers no `applicationShouldTerminate` to hold them.
//
// Reset Layout lives in View because the layout is the window's, not the
// capture's. `Menu::default` builds a View submenu on macOS only, so elsewhere
// the item brings its own, placed where macOS puts it: before Window.

use tauri::menu::{
    AboutMetadata, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::settings::{self, MAIN_WINDOW};

pub const QUIT_ID: &str = "trawler-quit";
pub const SETTINGS_ID: &str = "trawler-settings";
pub const CHECK_UPDATES_ID: &str = "trawler-check-updates";
pub const RESET_LAYOUT_ID: &str = "trawler-reset-layout";
pub const RESET_LAYOUT_EVENT: &str = "layout-reset";
const RESET_LAYOUT_LABEL: &str = "Reset Layout";
const VIEW_SUBMENU: &str = "View";
const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";
const SETTINGS_ACCELERATOR: &str = "CmdOrCtrl+,";

#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    RequestQuit,
    OpenSettings,
    CheckForUpdates,
    ResetLayout,
    Ignore,
}

pub fn action_for(id: &str) -> MenuAction {
    match id {
        QUIT_ID => MenuAction::RequestQuit,
        SETTINGS_ID => MenuAction::OpenSettings,
        CHECK_UPDATES_ID => MenuAction::CheckForUpdates,
        RESET_LAYOUT_ID => MenuAction::ResetLayout,
        _ => MenuAction::Ignore,
    }
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match action_for(id) {
        MenuAction::RequestQuit => drop(crate::lifecycle::request_quit(app)),
        MenuAction::OpenSettings => drop(settings::open_in_background(app)),
        MenuAction::CheckForUpdates => crate::update::check(app, crate::update::Asked::ByUser),
        MenuAction::ResetLayout => {
            if let Err(err) = app.emit(RESET_LAYOUT_EVENT, ()) {
                eprintln!("announcing Reset Layout failed: {err}");
            }
        }
        MenuAction::Ignore => {}
    }
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub(crate) fn close_main_window<R: Runtime>(app: &AppHandle<R>) {
    match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => {
            if let Err(err) = window.close() {
                eprintln!("closing the window for Quit failed: {err}");
            }
        }
        None => app.exit(0),
    }
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    if cfg!(target_os = "macos") {
        if let Some(MenuItemKind::Submenu(_)) = menu.items()?.first() {
            menu.remove_at(0)?;
        }
        menu.insert(&app_submenu(app)?, 0)?;
    } else if let Some(MenuItemKind::Submenu(first)) = menu.items()?.first() {
        first.prepend(&check_updates_item(app)?)?;
        first.prepend(&settings_item(app)?)?;
    }
    add_reset_layout(app, &menu)?;
    Ok(menu)
}

fn settings_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, Some(SETTINGS_ACCELERATOR))
}

fn check_updates_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, CHECK_UPDATES_ID, "Check for Updates…", true, None::<&str>)
}

fn add_reset_layout<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let reset = MenuItem::with_id(app, RESET_LAYOUT_ID, RESET_LAYOUT_LABEL, true, None::<&str>)?;
    let submenus: Vec<Option<Submenu<R>>> = menu
        .items()?
        .into_iter()
        .map(|item| match item {
            MenuItemKind::Submenu(submenu) => Some(submenu),
            _ => None,
        })
        .collect();
    let mut view = None;
    for submenu in submenus.iter().flatten() {
        if submenu.text()? == VIEW_SUBMENU {
            view = Some(submenu);
        }
    }
    if let Some(view) = view {
        view.append(&PredefinedMenuItem::separator(app)?)?;
        return view.append(&reset);
    }
    let own = Submenu::with_items(app, VIEW_SUBMENU, true, &[&reset])?;
    let ids: Vec<Option<String>> = submenus
        .iter()
        .map(|submenu| submenu.as_ref().map(|s| s.id().as_ref().to_owned()))
        .collect();
    menu.insert(&own, view_position(&ids))
}

fn view_position(submenu_ids: &[Option<String>]) -> usize {
    submenu_ids
        .iter()
        .position(|id| id.as_deref() == Some(WINDOW_SUBMENU_ID))
        .unwrap_or(submenu_ids.len())
}

fn app_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Submenu<R>> {
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    Submenu::with_items(
        app,
        info.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(about))?,
            &check_updates_item(app)?,
            &PredefinedMenuItem::separator(app)?,
            &settings_item(app)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                QUIT_ID,
                format!("Quit {}", info.name),
                true,
                Some(QUIT_ACCELERATOR),
            )?,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_is_the_quit_request_and_nothing_else_is() {
        assert_eq!(action_for(QUIT_ID), MenuAction::RequestQuit);
        assert_eq!(action_for("about"), MenuAction::Ignore);
        assert_eq!(action_for(""), MenuAction::Ignore);
    }

    #[test]
    fn settings_opens_settings() {
        assert_eq!(action_for(SETTINGS_ID), MenuAction::OpenSettings);
    }

    #[test]
    fn check_for_updates_asks_the_channel() {
        assert_eq!(action_for(CHECK_UPDATES_ID), MenuAction::CheckForUpdates);
    }

    #[test]
    fn reset_layout_asks_the_webview_to_reset_every_size() {
        assert_eq!(action_for(RESET_LAYOUT_ID), MenuAction::ResetLayout);
        assert_eq!(RESET_LAYOUT_EVENT, "layout-reset");
    }

    #[test]
    fn a_view_of_its_own_goes_before_window_or_last_without_one() {
        let ids = |list: &[Option<&str>]| -> Vec<Option<String>> {
            list.iter().map(|id| id.map(str::to_owned)).collect()
        };
        assert_eq!(
            view_position(&ids(&[
                Some("file"),
                Some("edit"),
                Some(WINDOW_SUBMENU_ID),
                Some("help")
            ])),
            2
        );
        assert_eq!(view_position(&ids(&[Some("file"), None])), 2);
    }
}
