// A #[tauri::command] receives every argument by value, and a scenario command
// keeps the signature of the real command it stands in for.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

#[cfg(test)]
mod bundle;
pub mod core;
mod core_address;
mod menu;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod reach;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod route;
pub mod settings;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod setup;
pub mod supervisor;

#[cfg(feature = "scenario")]
mod scenario;

pub const CORE_FLAG: &str = "--core";

/// The app binary is also its own core: launched with `--core` first, the rest
/// of its arguments go to the same entry as `cargo run -p trawler`.
pub fn core_args(
    mut args: impl Iterator<Item = std::ffi::OsString>,
) -> Option<impl Iterator<Item = std::ffi::OsString>> {
    match args.next() {
        Some(first) if first == CORE_FLAG => Some(args),
        _ => None,
    }
}

use tauri::Manager;

pub fn run() {
    let mode = core::mode(std::env::var(core::ATTACH_VAR).ok().as_deref());
    let builder = tauri::Builder::default();
    // Only an owned core holds the fixed ports, so only then does a second
    // launch hand over to the first; an attached app (the e2e) never yields to
    // a Trawler already running on this machine.
    let builder = if mode == core::Mode::Own {
        builder.plugin(tauri_plugin_single_instance::init(|app, _, _| core::focus_main(app)))
    } else {
        builder
    };
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .menu(menu::build)
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                && settings::ends_the_app(window.label())
            {
                window.app_handle().exit(0);
            }
        })
        .setup(move |app| core::setup(app, mode));
    #[cfg(not(feature = "scenario"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        setup::ca_trusted,
        setup::simulator_trust,
        setup::emulator_readings,
        setup::set_proxy,
        setup::set_up_this_machine,
        setup::unset_proxy,
        setup::remove_ca,
        setup::trust_simulators,
        setup::install_on_emulators,
        setup::route_emulators,
        setup::unroute_emulators,
        setup::proxy_status,
        setup::proxy_in_use,
        setup::interface_names,
        setup::notice_dismissed,
        setup::dismiss_notice,
        setup::notes_shown,
        setup::show_notes,
        route::open_route,
        reach::confirm_reach,
        reach::confirm_replace_proxy,
        core::core_status,
        core::core_log,
        core::restart_core
    ]);
    #[cfg(feature = "scenario")]
    let builder = scenario::install(builder);
    #[cfg(feature = "e2e")]
    let builder =
        builder.plugin(tauri_plugin_wdio::init()).plugin(tauri_plugin_wdio_webdriver::init());

    builder.build(tauri::generate_context!()).expect("error while running trawler-ui").run(
        |app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(core) = app.try_state::<core::Core>() {
                    core.stop();
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::core_args;
    use std::ffi::OsString;

    fn os(args: &[&str]) -> std::vec::IntoIter<OsString> {
        args.iter().map(OsString::from).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn core_first_hands_the_rest_to_the_core_entry() {
        let rest: Vec<OsString> =
            core_args(os(&["--core", "--api-port", "0", "--lifeline"])).unwrap().collect();
        assert_eq!(rest, os(&["--api-port", "0", "--lifeline"]).collect::<Vec<_>>());
    }

    #[test]
    fn no_arguments_open_the_window() {
        assert!(core_args(os(&[])).is_none());
    }

    #[test]
    fn core_anywhere_but_first_opens_the_window() {
        assert!(core_args(os(&["--lifeline", "--core"])).is_none());
    }
}
