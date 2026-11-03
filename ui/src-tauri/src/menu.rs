// The default macOS menu quits through `terminate:`, which never raises the
// window's close request, so ⌘Q skipped the quit-time proxy check (W11's D1).
// Trawler's own Quit asks the window to close instead, and the window's
// listener decides. The Dock's Quit and a logout still terminate: tao offers no
// `applicationShouldTerminate` to hold them.

use tauri::menu::{AboutMetadata, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

use crate::data_window::MAIN_WINDOW;

pub const QUIT_ID: &str = "trawler-quit";
const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";

#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    CloseMainWindow,
    Ignore,
}

pub fn action_for(id: &str) -> MenuAction {
    if id == QUIT_ID {
        MenuAction::CloseMainWindow
    } else {
        MenuAction::Ignore
    }
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if action_for(id) != MenuAction::CloseMainWindow {
        return;
    }
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
    }
    Ok(menu)
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
    fn quit_asks_the_window_to_close_and_nothing_else_does() {
        assert_eq!(action_for(QUIT_ID), MenuAction::CloseMainWindow);
        assert_eq!(action_for("about"), MenuAction::Ignore);
        assert_eq!(action_for(""), MenuAction::Ignore);
    }
}
