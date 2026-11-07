// muda builds macOS menu items on the main thread only, which libtest's
// workers are not: this target runs without the harness, on the main thread.

use tauri::menu::{Menu, MenuId, MenuItemKind};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::utils::config::WindowConfig;
use trawler_ui_lib::lifecycle;
use trawler_ui_lib::menu::{self, CLOSE_WINDOW_ID};

fn entries(menu: &Menu<MockRuntime>) -> Vec<MenuItemKind<MockRuntime>> {
    menu.items()
        .expect("the menu's submenus")
        .into_iter()
        .filter_map(|item| item.as_submenu().cloned())
        .flat_map(|submenu| submenu.items().expect("a submenu's items"))
        .collect()
}

fn reads_close(text: &str) -> bool {
    matches!(text.replace('&', "").as_str(), "Close Window" | "Close")
}

fn the_shipped_menu_closes_through_trawler_s_own_item_only() {
    let app = mock_builder().build(mock_context(noop_assets())).expect("a mock app");
    let built = menu::build(app.handle()).expect("the shipped menu");
    let entries = entries(&built);
    let own = entries.iter().filter(|entry| entry.id().as_ref() == CLOSE_WINDOW_ID).count();
    assert_eq!(own, 1, "Trawler's own Close Window, once");
    let native: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.as_predefined_menuitem())
        .map(|predefined| predefined.text().expect("a native item's text"))
        .filter(|text| reads_close(text))
        .collect();
    assert!(native.is_empty(), "no native close item remains: {native:?}");
}

// Ubuntu, rc5: Alt+H and Alt+E opened nothing.
fn every_menu_off_macos_opens_with_alt_and_its_own_letter() {
    let app = mock_builder().build(mock_context(noop_assets())).expect("a mock app");
    let built = menu::build(app.handle()).expect("the shipped menu");
    let labels: Vec<String> = built
        .items()
        .expect("the menu's submenus")
        .into_iter()
        .filter_map(|item| item.as_submenu().map(|submenu| submenu.text().expect("a label")))
        .collect();
    if cfg!(target_os = "macos") {
        assert!(labels.iter().all(|label| !label.contains('&')), "{labels:?}");
        return;
    }
    let keys: Vec<char> = labels
        .iter()
        .map(|label| {
            let (_, key) = label.split_once('&').unwrap_or_else(|| panic!("no Alt key: {label}"));
            key.chars().next().expect("a letter after &").to_ascii_lowercase()
        })
        .collect();
    assert!(keys.contains(&'h') && keys.contains(&'e'), "{labels:?}");
    let mut distinct = keys.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), keys.len(), "one menu per Alt key: {labels:?}");
}

fn menu_id(menu: Option<Menu<MockRuntime>>) -> MenuId {
    menu.expect("a menu").id().clone()
}

// GNOME, rc5: the main window reopened from the tray had no menu bar and no
// Ctrl+, — muda had the app's one menu "already initialized" for the address
// the new window was built at. A menu built for the window has no such entry.
fn a_main_window_carries_a_menu_built_for_it() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid json");
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows =
        serde_json::from_value::<Vec<WindowConfig>>(config["app"]["windows"].clone())
            .expect("the shipped windows");
    let app = mock_builder().menu(menu::build).build(context).expect("a mock app");
    let app_menu = menu_id(app.menu());

    let main = menu_id(lifecycle::build_main(app.handle()).expect("the main window").menu());

    if cfg!(target_os = "macos") {
        assert_eq!(main, app_menu, "one menu bar, the app's");
    } else {
        assert_ne!(main, app_menu);
    }
}

// cargo-nextest lists a test binary with libtest's `--list` before it runs each
// test by name; a binary that ignores the list runs nothing under the gate.
#[allow(clippy::print_stdout)]
fn main() {
    const TESTS: [(&str, fn()); 3] = [
        (
            "the_shipped_menu_closes_through_trawler_s_own_item_only",
            the_shipped_menu_closes_through_trawler_s_own_item_only,
        ),
        ("a_main_window_carries_a_menu_built_for_it", a_main_window_carries_a_menu_built_for_it),
        (
            "every_menu_off_macos_opens_with_alt_and_its_own_letter",
            every_menu_off_macos_opens_with_alt_and_its_own_letter,
        ),
    ];
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list") {
        if !args.iter().any(|arg| arg == "--ignored") {
            for (name, _) in TESTS {
                println!("{name}: test");
            }
        }
        return;
    }
    let only = args.iter().find(|arg| !arg.starts_with('-'));
    for (name, test) in TESTS {
        if only.map_or(true, |wanted| name.contains(wanted.as_str())) {
            test();
        }
    }
}
