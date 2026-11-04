pub mod layout;
mod menu;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod reach;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod route;
#[cfg_attr(feature = "scenario", allow(dead_code))]
mod setup;
pub mod window_geometry;

#[cfg(feature = "e2e")]
mod e2e_core_address;

#[cfg(feature = "scenario")]
mod scenario;

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            window_geometry::install(app.handle());
            Ok(())
        })
        .on_window_event(window_geometry::track)
        .menu(menu::build)
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()));
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
        route::open_route,
        reach::confirm_reach,
        reach::confirm_replace_proxy
    ]);
    #[cfg(feature = "scenario")]
    let builder = scenario::install(builder);
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(e2e_core_address::init())
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder.build(tauri::generate_context!()).expect("error while running trawler-ui").run(
        |app, event| {
            if let tauri::RunEvent::Exit = event {
                window_geometry::save(app);
            }
        },
    );
}
