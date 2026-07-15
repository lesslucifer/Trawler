mod setup;

pub fn run() {
    let builder = tauri::Builder::default().invoke_handler(tauri::generate_handler![
        setup::install_ca,
        setup::ca_trusted,
        setup::set_proxy,
        setup::unset_proxy,
        setup::proxy_status,
        setup::remote_notice_dismissed,
        setup::dismiss_remote_notice
    ]);
    #[cfg(feature = "e2e")]
    let builder =
        builder.plugin(tauri_plugin_wdio::init()).plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .build(tauri::generate_context!())
        .expect("error while running trawler-ui")
        .run(|_, _| {});
}
