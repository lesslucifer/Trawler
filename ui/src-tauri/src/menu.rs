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
// About is Trawler's own dialog on every OS (DEC-90): the default menu's
// About items, the system panel on macOS, are replaced. So is Close Window: its
// native item closes the window under an open dialog, which the page never sees.

use tauri::menu::{
    Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindowBuilder};

use crate::about::{self, Page, Show};
use crate::links::{self, Link};
use crate::settings::{self, MAIN_WINDOW};

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
pub const CLOSE_WINDOW_ID: &str = "trawler-close-window";
pub const CLOSE_WINDOW_EVENT: &str = "close-window-requested";
const CLOSE_WINDOW_ACCELERATOR: &str = "CmdOrCtrl+W";
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
    CloseWindow,
    Ignore,
}

/// What Close Window acts on: the main window's page closes its topmost dialog
/// before the window; any other window simply closes.
#[derive(Debug, PartialEq, Eq)]
pub enum CloseTarget {
    MainPage,
    Window,
    Nothing,
}

pub fn close_target(focused: Option<&str>) -> CloseTarget {
    match focused {
        Some(MAIN_WINDOW) => CloseTarget::MainPage,
        Some(_) => CloseTarget::Window,
        None => CloseTarget::Nothing,
    }
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
        CLOSE_WINDOW_ID => MenuAction::CloseWindow,
        _ => MenuAction::Ignore,
    }
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn handle<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match action_for(id) {
        MenuAction::RequestQuit => drop(crate::lifecycle::request_quit(app)),
        MenuAction::OpenSettings => drop(settings::open(app)),
        MenuAction::CheckForUpdates => crate::update::check(app, crate::update::Asked::ByUser),
        MenuAction::ResetLayout => {
            if let Err(err) = app.emit(RESET_LAYOUT_EVENT, ()) {
                eprintln!("announcing Reset Layout failed: {err}");
            }
        }
        MenuAction::OpenAbout(page) => drop(about::open(app, Show::page(page))),
        MenuAction::OpenLink(link) => {
            if let Err(address) = links::open(app, link) {
                drop(about::open(app, Show { page: Page::About, unopened: Some(address) }));
            }
        }
        MenuAction::CloseWindow => close_focused(app),
        MenuAction::Ignore => {}
    }
}

// A menu click, which WebDriver cannot make.
#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_menu<R: Runtime>(app: AppHandle<R>, id: String) {
    handle(&app, &id);
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
fn close_focused<R: Runtime>(app: &AppHandle<R>) {
    let focused =
        app.webview_windows().into_values().find(|window| window.is_focused().unwrap_or(false));
    let closed = match (close_target(focused.as_ref().map(tauri::WebviewWindow::label)), focused) {
        (CloseTarget::MainPage, Some(window)) => {
            window.emit_to(MAIN_WINDOW, CLOSE_WINDOW_EVENT, ())
        }
        (CloseTarget::Window, Some(window)) => window.close(),
        _ => Ok(()),
    };
    if let Err(err) = closed {
        eprintln!("Close Window failed: {err}");
    }
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
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
    if let Err(err) = own_close_window(app, &menu) {
        eprintln!("Close Window keeps its native item: {err}");
    }
    Ok(menu)
}

// muda keys a window's menu bar by the window's address, and tauri drops no key
// when its window goes: a window built where a closed one lived found the app's
// menu "already initialized" and showed none, nor answered its shortcuts (the
// main window reopened from the tray on GNOME). Off macOS, whose menu bar is
// the app's, each window gets a menu of its own. ponytail: a closed window's
// menu stays in tauri's stash, a few items per reopen; drop it on Destroyed if
// that ever matters.
pub fn of_its_own<'a, R: Runtime, M: Manager<R>>(
    app: &AppHandle<R>,
    window: WebviewWindowBuilder<'a, R, M>,
) -> tauri::Result<WebviewWindowBuilder<'a, R, M>> {
    if cfg!(target_os = "macos") {
        return Ok(window);
    }
    Ok(window.menu(build(app)?))
}

// The default menu holds the native item once per OS menu that lists it (File
// and Window off Linux); the first place keeps Trawler's own, the rest go.
// Tauri names a predefined item by neither kind nor a stable id, so its label
// finds it: macOS shows the label with its mnemonic stripped, a fresh item
// keeps the `&`.
fn own_close_window<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let native = PredefinedMenuItem::close_window(app, None)?.text()?;
    let mut found = Vec::new();
    for item in menu.items()? {
        let MenuItemKind::Submenu(submenu) = item else { continue };
        for (index, entry) in submenu.items()?.into_iter().enumerate() {
            if let MenuItemKind::Predefined(predefined) = entry {
                if same_label(&predefined.text()?, &native) {
                    found.push((submenu.clone(), index));
                }
            }
        }
    }
    for (submenu, index) in found.iter().rev() {
        submenu.remove_at(*index)?;
    }
    let own = MenuItem::with_id(
        app,
        CLOSE_WINDOW_ID,
        "Close Window",
        true,
        Some(CLOSE_WINDOW_ACCELERATOR),
    )?;
    let Some((submenu, index)) = found.first() else {
        return Err(std::io::Error::other(format!("no \"{native}\" item in the menu")).into());
    };
    submenu.insert(&own, *index)
}

fn same_label(shown: &str, native: &str) -> bool {
    shown.replace('&', "") == native.replace('&', "")
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
    fn close_window_closes_the_main_window_s_dialog_first_and_any_other_window_outright() {
        assert_eq!(action_for(CLOSE_WINDOW_ID), MenuAction::CloseWindow);
        assert_eq!(close_target(Some(MAIN_WINDOW)), CloseTarget::MainPage);
        assert_eq!(close_target(Some("data-6162")), CloseTarget::Window);
        assert_eq!(close_target(None), CloseTarget::Nothing);
        let page = include_str!("../../src/shellDialogs.ts");
        assert!(page.contains(&format!("\"{CLOSE_WINDOW_EVENT}\"")));
    }

    #[test]
    fn the_native_close_item_is_found_with_or_without_its_mnemonic() {
        assert!(same_label("Close Window", "C&lose Window"));
        assert!(same_label("C&lose Window", "C&lose Window"));
        assert!(same_label("Close", "Close"));
        assert!(!same_label("Close", "C&lose Window"));
        assert!(!same_label("Minimize", "C&lose Window"));
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
