mod reach;
mod setup;

#[cfg(feature = "e2e")]
mod e2e_core_address;

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            setup::ca_trusted,
            setup::set_proxy,
            setup::set_up_this_machine,
            setup::unset_proxy,
            setup::proxy_status,
            setup::interface_names,
            setup::notice_dismissed,
            setup::dismiss_notice,
            reach::confirm_reach,
            reach::confirm_replace_proxy
        ]);
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(e2e_core_address::init())
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .build(tauri::generate_context!())
        .expect("error while running trawler-ui")
        .run(|_, _| {});
}
