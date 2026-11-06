// muda builds macOS menu items on the main thread only, which libtest's
// workers are not: this target runs without the harness, on the main thread.

use tauri::menu::{Menu, MenuItemKind};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
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

// cargo-nextest lists a test binary with libtest's `--list` before it runs each
// test by name; a binary that ignores the list runs nothing under the gate.
#[allow(clippy::print_stdout)]
fn main() {
    const NAME: &str = "the_shipped_menu_closes_through_trawler_s_own_item_only";
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list") {
        if !args.iter().any(|arg| arg == "--ignored") {
            println!("{NAME}: test");
        }
        return;
    }
    the_shipped_menu_closes_through_trawler_s_own_item_only();
}
