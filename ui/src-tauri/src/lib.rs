mod setup;

pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|_app| {
            // Crash reconciliation off the main thread: a prior run that set the
            // proxy and died hard (no ExitRequested, no clean quit) leaves a stale
            // marker; clear it on the next launch. Off-thread so the elevation prompt
            // it may spawn does not gate the window's first paint (else the OS
            // password dialog shows with no app behind it). The consented quit path
            // is the webview confirm (App.tsx onCloseRequested); the ExitRequested
            // net below is the un-consented-quit backstop (Cmd-Q).
            std::thread::spawn(setup::revert_if_marked);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            setup::install_ca,
            setup::set_proxy,
            setup::unset_proxy,
            setup::proxy_status,
            setup::clear_proxy_marker,
            setup::remote_notice_dismissed,
            setup::dismiss_remote_notice
        ]);
    #[cfg(feature = "e2e")]
    let builder =
        builder.plugin(tauri_plugin_wdio::init()).plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .build(tauri::generate_context!())
        .expect("error while running trawler-ui")
        .run(|_app, event| {
            // Last-chance safety net: on macOS, Cmd-Q / the menu Quit go through
            // [NSApp terminate:], which never fires the window's onCloseRequested —
            // so the webview confirm (App.tsx) is skipped on that path. tao doesn't
            // expose a cancellable applicationShouldTerminate:, so we can't ask here;
            // the safe default on an un-consented quit is to revert (restore the
            // user's internet), never leave the machine on a dead proxy. We match
            // BOTH exit events because that path delivers RunEvent::Exit, while the
            // red-button close delivers ExitRequested — synchronous in either, run
            // before process teardown. If the user answered the confirm (Remove or
            // Keep), the marker is already gone and this is a no-op, so it never
            // overrides a deliberate "keep".
            if matches!(event, tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit) {
                setup::revert_if_marked();
            }
        });
}
