// The default macOS menu quits through `terminate:`, which never raises the
// window's close request, so ⌘Q skipped the quit-time proxy check (W11's D1).
// Trawler's own Quit is a quit request: the window runs the check and exits,
// whether or not the app was windowless. The Dock's Quit and a logout still
// terminate: tao offers no `applicationShouldTerminate` to hold them.
//
// Reset Layout lives in View because the layout is the window's, not the
// capture's. `Menu::default` builds a View submenu on macOS only, so elsewhere
// the item brings its own, placed where macOS puts it: before Window.
//
// About is Trawler's own window on every OS (DEC-90): the default menu's
// About items, the system panel on macOS, are replaced.

use tauri::menu::{
    Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Runtime};

use crate::about::{self, Page, Show};
use crate::links::{self, Link};
use crate::settings;

pub const QUIT_ID: &str = "trawler-quit";
pub const SETTINGS_ID: &str = "trawler-settings";
pub const CHECK_UPDATES_ID: &str = "trawler-check-updates";
pub const RESET_LAYOUT_ID: &str = "trawler-reset-layout";
pub const RESET_LAYOUT_EVENT: &str = "layout-reset";
pub const ABOUT_ID: &str = "trawler-about";
pub const GUIDE_ID: &str = "trawler-guide";
pub const REPORT_ID: &str = "trawler-report-issue";
pub const LICENCES_ID: &str = "trawler-licences";
pub const PRIVACY_ID: &str = "trawler-privacy";
const ABOUT_LABEL: &str = "About Trawler";
const HELP_SUBMENU: &str = "Help";
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
    OpenAbout(Page),
    OpenLink(Link),
    Ignore,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HelpEntry {
    Item { id: &'static str, label: &'static str, accelerator: Option<&'static str> },
    Separator,
}

const fn item(id: &'static str, label: &'static str) -> HelpEntry {
    HelpEntry::Item { id, label, accelerator: None }
}

/// macOS keeps About in the app menu and its own Help search (⇧⌘/); elsewhere
/// Help carries About and F1 opens the guide.
pub fn help_entries(macos: bool) -> Vec<HelpEntry> {
    let guide = if macos {
        item(GUIDE_ID, "Trawler User Guide")
    } else {
        HelpEntry::Item { id: GUIDE_ID, label: "User Guide", accelerator: Some("F1") }
    };
    let mut entries = vec![
        guide,
        item(REPORT_ID, "Report an Issue…"),
        HelpEntry::Separator,
        item(LICENCES_ID, "Third-Party Licences"),
        item(PRIVACY_ID, "Privacy"),
    ];
    if !macos {
        entries.extend([HelpEntry::Separator, item(ABOUT_ID, ABOUT_LABEL)]);
    }
    entries
}

pub fn action_for(id: &str) -> MenuAction {
    match id {
        QUIT_ID => MenuAction::RequestQuit,
        SETTINGS_ID => MenuAction::OpenSettings,
        CHECK_UPDATES_ID => MenuAction::CheckForUpdates,
        RESET_LAYOUT_ID => MenuAction::ResetLayout,
        ABOUT_ID => MenuAction::OpenAbout(Page::About),
        LICENCES_ID => MenuAction::OpenAbout(Page::Licences),
        PRIVACY_ID => MenuAction::OpenAbout(Page::Privacy),
        GUIDE_ID => MenuAction::OpenLink(Link::Guide),
        REPORT_ID => MenuAction::OpenLink(Link::Issue),
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
        MenuAction::OpenAbout(page) => drop(about::open_in_background(app, Show::page(page))),
        MenuAction::OpenLink(link) => {
            if let Err(address) = links::open(app, link) {
                let show = Show { page: Page::About, unopened: Some(address) };
                drop(about::open_in_background(app, show));
            }
        }
        MenuAction::Ignore => {}
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
    fill_help(app, &menu)?;
    Ok(menu)
}

fn fill_help<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let help =
        if let Some(help) = menu.get(HELP_SUBMENU_ID).and_then(|item| item.as_submenu().cloned()) {
            help
        } else {
            let help = Submenu::with_id(app, HELP_SUBMENU_ID, HELP_SUBMENU, true)?;
            menu.append(&help)?;
            help
        };
    for _ in help.items()? {
        help.remove_at(0)?;
    }
    for entry in help_entries(cfg!(target_os = "macos")) {
        match entry {
            HelpEntry::Item { id, label, accelerator } => {
                help.append(&MenuItem::with_id(app, id, label, true, accelerator)?)?;
            }
            HelpEntry::Separator => help.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(())
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
    Submenu::with_items(
        app,
        info.name.clone(),
        true,
        &[
            &MenuItem::with_id(app, ABOUT_ID, ABOUT_LABEL, true, None::<&str>)?,
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

    fn ids(entries: &[HelpEntry]) -> Vec<&'static str> {
        entries
            .iter()
            .map(|entry| match entry {
                HelpEntry::Item { id, .. } => id,
                HelpEntry::Separator => "-",
            })
            .collect()
    }

    #[test]
    fn help_on_macos_opens_the_guide_a_report_the_licences_and_privacy() {
        let mac = help_entries(true);
        assert_eq!(ids(&mac), [GUIDE_ID, REPORT_ID, "-", LICENCES_ID, PRIVACY_ID]);
        assert_eq!(
            mac[0],
            HelpEntry::Item { id: GUIDE_ID, label: "Trawler User Guide", accelerator: None }
        );
    }

    #[test]
    fn help_elsewhere_adds_about_and_f1_opens_the_guide() {
        let other = help_entries(false);
        assert_eq!(ids(&other), [GUIDE_ID, REPORT_ID, "-", LICENCES_ID, PRIVACY_ID, "-", ABOUT_ID]);
        assert_eq!(
            other[0],
            HelpEntry::Item { id: GUIDE_ID, label: "User Guide", accelerator: Some("F1") }
        );
        let labels: Vec<&str> = other
            .iter()
            .filter_map(|entry| match entry {
                HelpEntry::Item { label, .. } => Some(*label),
                HelpEntry::Separator => None,
            })
            .collect();
        assert_eq!(
            labels,
            ["User Guide", "Report an Issue…", "Third-Party Licences", "Privacy", "About Trawler"]
        );
    }

    #[test]
    fn each_help_item_opens_its_page_or_its_link() {
        assert_eq!(action_for(ABOUT_ID), MenuAction::OpenAbout(Page::About));
        assert_eq!(action_for(LICENCES_ID), MenuAction::OpenAbout(Page::Licences));
        assert_eq!(action_for(PRIVACY_ID), MenuAction::OpenAbout(Page::Privacy));
        assert_eq!(action_for(GUIDE_ID), MenuAction::OpenLink(Link::Guide));
        assert_eq!(action_for(REPORT_ID), MenuAction::OpenLink(Link::Issue));
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
