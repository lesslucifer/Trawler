// A #[tauri::command] receives every argument by value, and a scenario command
// keeps the signature of the real command it stands in for.
#![allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]

#[cfg(test)]
mod bundle;
pub mod core;
mod core_address;
pub mod data_window;
pub mod keep_capturing;
pub mod layout;
pub mod lifecycle;
pub mod login_item;
mod menu;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod reach;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod route;
pub mod settings;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod setup;
pub mod supervisor;
pub mod tray;
pub mod view_prefs;
pub mod window_geometry;

#[cfg(feature = "scenario")]
mod scenario;

pub const CORE_FLAG: &str = "--core";
pub const BACKGROUND_FLAG: &str = "--background";
pub const EPHEMERAL_PORTS: bool = cfg!(feature = "e2e");

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

/// A launch with no window (the login item's): the menu-bar icon only.
pub fn background_launch(mut args: impl Iterator<Item = std::ffi::OsString>) -> bool {
    args.any(|arg| arg == BACKGROUND_FLAG)
}

use tauri::Manager;

fn second_launch_opens_window(argv: Vec<String>) -> bool {
    !background_launch(argv.into_iter().map(std::ffi::OsString::from))
}

pub fn run() {
    let background = background_launch(std::env::args_os().skip(1));
    let mode = core::mode(std::env::var(core::ATTACH_VAR).ok().as_deref());
    let builder = tauri::Builder::default();
    let builder = if core::hands_over(&mode, core::launch_ports(EPHEMERAL_PORTS)) {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _| {
            if second_launch_opens_window(argv) {
                drop(lifecycle::reopen_main_in_background(app));
            }
        }))
    } else {
        builder
    };
    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .on_window_event(window_geometry::track)
        .on_window_event(data_window::on_window_event)
        .menu(menu::build)
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                && lifecycle::on_window_destroyed(window.app_handle(), window.label())
            {
                window.app_handle().exit(0);
            }
        })
        .setup(move |app| {
            lifecycle::install(app.handle(), setup::config_dir(app.handle()).ok());
            login_item::install(app.handle());
            core::setup(app, mode, background)?;
            window_geometry::install(app.handle());
            Ok(())
        });
    let builder = data_window::install(builder);
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
        layout::layout_read,
        layout::layout_write,
        view_prefs::view_prefs_read,
        view_prefs::view_prefs_write,
        route::open_route,
        reach::confirm_reach,
        reach::confirm_replace_proxy,
        core::core_status,
        core::core_log,
        core::restart_core,
        settings::open_settings,
        keep_capturing::keep_capturing_read,
        keep_capturing::keep_capturing_write,
        login_item::login_item_read,
        login_item::login_item_write,
        lifecycle::tray_available,
        lifecycle::take_quit_request,
        lifecycle::quit_app,
        #[cfg(feature = "e2e")]
        lifecycle::e2e_request_quit,
        data_window::open_data_window,
        data_window::take_data_payload
    ]);
    #[cfg(feature = "scenario")]
    let builder = scenario::install(builder);
    #[cfg(feature = "e2e")]
    let builder =
        builder.plugin(tauri_plugin_wdio::init()).plugin(tauri_plugin_wdio_webdriver::init());

    builder.build(tauri::generate_context!()).expect("error while running trawler-ui").run(
        |app, event| {
            if let tauri::RunEvent::Exit = event {
                window_geometry::save(app);
            }
            on_run_event(app, &event);
        },
    );
}

pub fn on_run_event<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: &tauri::RunEvent) {
    match event {
        tauri::RunEvent::ExitRequested { code: None, api, .. }
            if lifecycle::holds_last_window_exit(app) =>
        {
            api.prevent_exit();
        }
        tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::Destroyed, .. } => {
            lifecycle::on_window_gone(app);
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => drop(lifecycle::reopen_main_in_background(app)),
        tauri::RunEvent::Exit => {
            if let Some(core) = app.try_state::<core::Core>() {
                core.stop();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{background_launch, core_args, second_launch_opens_window};
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

    #[test]
    fn the_background_flag_is_read_wherever_it_stands() {
        assert!(background_launch(os(&["--background"])));
        assert!(background_launch(os(&["-psn_0_1", "--background"])));
        assert!(!background_launch(os(&[])));
        assert!(!background_launch(os(&["--backgrounds"])));
    }

    #[test]
    fn a_second_launch_opens_the_window_unless_the_login_item_started_it() {
        let argv = |args: &[&str]| args.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(second_launch_opens_window(argv(&[
            "/Applications/Trawler.app/Contents/MacOS/trawler-ui"
        ])));
        assert!(!second_launch_opens_window(argv(&[
            "/Applications/Trawler.app/Contents/MacOS/trawler-ui",
            "--background"
        ])));
    }

    #[test]
    fn the_core_flag_first_wins_over_the_background_flag() {
        assert!(core_args(os(&["--core", "--background"])).is_some());
    }
}
