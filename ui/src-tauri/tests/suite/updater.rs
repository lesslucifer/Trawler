use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};
use tauri_plugin_updater::{Updater, UpdaterExt};
use trawler_ui_lib::keep_capturing;
use trawler_ui_lib::lifecycle::{self, Lifecycle};
use trawler_ui_lib::settings::MAIN_WINDOW;
use trawler_ui_lib::update;

const TARGET: &str = "trawler-test";
const ARCHIVE_PATH: &str = "/Trawler.app.tar.gz";
const MANIFEST_PATH: &str = "/latest.json";
const CURRENT: &[u8] = b"trawler-ui 0.1.0";
const NEXT: &[u8] = b"trawler-ui 0.2.0, the marker the update carries";
const SHIPPED_CONFIG: &str = include_str!("../../tauri.conf.json");
const CA: &[u8] = b"-----BEGIN CERTIFICATE-----\nthe CA a user trusted\n";

fn shipped_windows() -> serde_json::Value {
    let config: serde_json::Value = serde_json::from_str(SHIPPED_CONFIG).unwrap();
    config["app"]["windows"].clone()
}

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

struct Key(minisign::KeyPair);

impl Key {
    fn new() -> Self {
        Self(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
    }

    fn public(&self) -> String {
        b64(&self.0.pk.to_box().unwrap().into_string())
    }

    /// Signed as `tauri build` signs: the version rides in the trusted comment.
    fn sign(&self, data: &[u8], version: &str) -> String {
        let trusted = format!("timestamp:1793870000\tfile:Trawler.app.tar.gz\tversion:{version}");
        let signed = minisign::sign(Some(&self.0.pk), &self.0.sk, data, Some(&trusted), None);
        b64(&signed.unwrap().into_string())
    }
}

/// An installed bundle as the updater sees it, with the CA in an app data
/// directory beside it.
struct Installed {
    _root: tempfile::TempDir,
    executable: PathBuf,
    ca: PathBuf,
}

impl Installed {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("Trawler.app/Contents/MacOS/trawler-ui");
        let ca = root.path().join("data/ca/trawler-ca.pem");
        for (path, bytes) in [(&executable, CURRENT), (&ca, CA)] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        Self { _root: root, executable, ca }
    }

    fn binary(&self) -> Vec<u8> {
        std::fs::read(&self.executable).unwrap()
    }
}

fn archive(binary: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(binary.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "Trawler.app/Contents/MacOS/trawler-ui", binary).unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

/// A release channel on loopback: the manifest and the archive, any number of
/// requests, one connection each.
fn channel(version: &str, signature: &str, archive: Vec<u8>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = listener.local_addr().unwrap();
    let manifest = serde_json::json!({
        "version": version,
        "notes": "",
        "pub_date": "2026-11-05T00:00:00Z",
        "platforms": { TARGET: { "signature": signature, "url": format!("http://{origin}{ARCHIVE_PATH}") } }
    })
    .to_string()
    .into_bytes();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            answer(stream.unwrap(), |path| match path {
                MANIFEST_PATH => ok(&manifest),
                ARCHIVE_PATH => ok(&archive),
                _ => ok(&[]),
            });
        }
    });
    origin
}

fn answer(mut stream: impl Read + Write, respond: impl FnOnce(&str) -> Vec<u8>) {
    let mut request = BufReader::new(&mut stream);
    let mut first = String::new();
    if request.read_line(&mut first).is_err() {
        return;
    }
    let mut line = String::new();
    while request.read_line(&mut line).is_ok_and(|n| n > 2) {
        line.clear();
    }
    let reply = respond(first.split(' ').nth(1).unwrap_or_default());
    let _ = stream.write_all(&reply).and_then(|()| stream.flush());
}

fn ok(body: &[u8]) -> Vec<u8> {
    let head =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    [head.as_bytes(), body].concat()
}

fn redirect(to: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

/// A loopback https origin answering every request with `respond`.
fn https_channel(respond: impl Fn(&str) -> Vec<u8> + Send + 'static) -> SocketAddr {
    let (_, config) = trawler::test_support::tls_origin(&["http/1.1"]);
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let tls = rustls::ServerConnection::new(Arc::clone(&config)).unwrap();
            let mut stream = rustls::StreamOwned::new(tls, stream.unwrap());
            answer(&mut stream, &respond);
            stream.conn.send_close_notify();
            let _ = stream.flush();
        }
    });
    origin
}

/// A plain http origin that must never be reached; the kernel completes a
/// connection into its backlog even though nothing accepts it.
struct Untouched(TcpListener);

impl Untouched {
    fn new() -> Self {
        Self(TcpListener::bind("127.0.0.1:0").unwrap())
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.0.local_addr().unwrap())
    }

    fn was_contacted(&self) -> bool {
        self.0.set_nonblocking(true).unwrap();
        self.0.accept().is_ok()
    }
}

fn app() -> App<MockRuntime> {
    app_with(|_| {})
}

fn app_with(adjust: impl FnOnce(&mut tauri::Context<MockRuntime>)) -> App<MockRuntime> {
    app_configured(
        serde_json::json!({
            "pubkey": "",
            "dangerousInsecureTransportProtocol": true,
            "requireSignedVersion": true
        }),
        adjust,
    )
}

fn app_configured(
    updater: serde_json::Value,
    adjust: impl FnOnce(&mut tauri::Context<MockRuntime>),
) -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    adjust(&mut context);
    context.config_mut().plugins.0.insert("updater".into(), updater);
    mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap()
}

/// As shipped (no insecure transport allowed), trusting the loopback origin's
/// test certificate.
fn shipped_transport_app() -> App<MockRuntime> {
    app_configured(
        serde_json::json!({
            "pubkey": "",
            "dangerousAcceptInvalidCerts": true,
            "requireSignedVersion": true
        }),
        |_| {},
    )
}

fn shipped_updater(app: &App<MockRuntime>, key: &Key, endpoint: &str, at: &Path) -> Updater {
    update::https_only(app.updater_builder())
        .pubkey(key.public())
        .target(TARGET)
        .executable_path(at)
        .timeout(Duration::from_secs(5))
        .endpoints(vec![endpoint.parse().unwrap()])
        .unwrap()
        .build()
        .unwrap()
}

fn updater(app: &App<MockRuntime>, key: &Key, origin: SocketAddr, at: &Path) -> Updater {
    let url = format!("http://{origin}{MANIFEST_PATH}").parse().unwrap();
    app.updater_builder()
        .pubkey(key.public())
        .target(TARGET)
        .executable_path(at)
        .endpoints(vec![url])
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn a_channel_with_no_newer_version_offers_nothing_and_changes_nothing() {
    let app = app();
    let key = Key::new();
    let installed = Installed::new();
    let current = app.package_info().version.to_string();
    let origin = channel(&current, &key.sign(&archive(NEXT), &current), archive(NEXT));

    let found =
        tauri::async_runtime::block_on(updater(&app, &key, origin, &installed.executable).check());

    assert!(found.unwrap().is_none(), "the same version is no update");
    assert_eq!(installed.binary(), CURRENT);
}

#[test]
fn an_update_signed_by_another_key_is_refused_and_the_app_is_untouched() {
    let app = app();
    let (trusted, forger) = (Key::new(), Key::new());
    let installed = Installed::new();
    let origin = channel("0.2.0", &forger.sign(&archive(NEXT), "0.2.0"), archive(NEXT));

    let refused = tauri::async_runtime::block_on(async {
        let update = updater(&app, &trusted, origin, &installed.executable)
            .check()
            .await
            .unwrap()
            .expect("0.2.0 is offered");
        update.download_and_install(|_, _| {}, || {}).await
    });

    assert!(
        matches!(refused, Err(tauri_plugin_updater::Error::Minisign(_))),
        "refused for its signature, got {refused:?}"
    );
    assert_eq!(installed.binary(), CURRENT);
    assert_eq!(std::fs::read(&installed.ca).unwrap(), CA);
}

#[cfg(target_os = "macos")]
#[test]
fn a_verified_update_replaces_the_app_and_leaves_the_ca_as_it_was() {
    let app = app();
    let key = Key::new();
    let installed = Installed::new();
    let origin = channel("0.2.0", &key.sign(&archive(NEXT), "0.2.0"), archive(NEXT));

    let installed_update = tauri::async_runtime::block_on(async {
        let update = updater(&app, &key, origin, &installed.executable)
            .check()
            .await
            .unwrap()
            .expect("0.2.0 is offered");
        assert_eq!(update.version, "0.2.0");
        update.download_and_install(|_, _| {}, || {}).await
    });

    installed_update.unwrap();
    assert_eq!(installed.binary(), NEXT);
    assert_eq!(std::fs::read(&installed.ca).unwrap(), CA);
}

#[test]
fn a_genuine_archive_announced_as_another_version_is_refused() {
    let app = app();
    let key = Key::new();
    let installed = Installed::new();
    let origin = channel("0.3.0", &key.sign(&archive(NEXT), "0.2.0"), archive(NEXT));

    let refused = tauri::async_runtime::block_on(async {
        let update = updater(&app, &key, origin, &installed.executable)
            .check()
            .await
            .unwrap()
            .expect("0.3.0 is offered");
        update.download_and_install(|_, _| {}, || {}).await
    });

    assert!(
        matches!(refused, Err(tauri_plugin_updater::Error::SignedVersionMismatch { .. })),
        "a signature for 0.2.0 cannot install as 0.3.0, got {refused:?}"
    );
    assert_eq!(installed.binary(), CURRENT);
}

#[test]
fn an_http_channel_is_refused_before_any_connection() {
    let app = shipped_transport_app();
    let untouched = Untouched::new();
    let installed = Installed::new();

    let found = tauri::async_runtime::block_on(
        shipped_updater(&app, &Key::new(), &untouched.url(MANIFEST_PATH), &installed.executable)
            .check(),
    );

    assert!(found.is_err(), "an http manifest is refused");
    assert!(!untouched.was_contacted(), "nothing is sent over http");
}

#[test]
fn an_https_manifest_naming_an_http_download_is_refused_before_it_downloads() {
    let app = shipped_transport_app();
    let key = Key::new();
    let untouched = Untouched::new();
    let installed = Installed::new();
    let manifest = serde_json::json!({
        "version": "0.2.0",
        "platforms": { TARGET: {
            "signature": key.sign(&archive(NEXT), "0.2.0"),
            "url": untouched.url(ARCHIVE_PATH)
        } }
    })
    .to_string();
    let origin = https_channel(move |_| ok(manifest.as_bytes()));

    let downloaded = tauri::async_runtime::block_on(async {
        let endpoint = format!("https://{origin}{MANIFEST_PATH}");
        let mut update = shipped_updater(&app, &key, &endpoint, &installed.executable)
            .check()
            .await
            .unwrap()
            .expect("0.2.0 is offered over https");
        update.timeout = Some(Duration::from_secs(5));
        update.download(|_, _| {}, || {}).await
    });

    assert!(downloaded.is_err(), "an http download is refused");
    assert!(!untouched.was_contacted(), "the archive is never fetched over http");
    assert_eq!(installed.binary(), CURRENT);
}

#[test]
fn an_https_channel_redirecting_to_http_is_refused_at_the_hop() {
    let app = shipped_transport_app();
    let untouched = Untouched::new();
    let installed = Installed::new();
    let downgrade = untouched.url(MANIFEST_PATH);
    let origin = https_channel(move |_| redirect(&downgrade));

    let found = tauri::async_runtime::block_on(
        shipped_updater(
            &app,
            &Key::new(),
            &format!("https://{origin}{MANIFEST_PATH}"),
            &installed.executable,
        )
        .check(),
    );

    assert!(found.is_err(), "a redirect to http is refused");
    assert!(!untouched.was_contacted(), "the http hop is never followed");
}

fn windowless(config_dir: &Path, windows: serde_json::Value) -> App<MockRuntime> {
    let app = app_with(|context| {
        context.config_mut().app.windows = serde_json::from_value(windows).unwrap();
    });
    app.manage(update::Pending::<update::Staged>::default());
    keep_capturing::write_keep_capturing(config_dir, true).unwrap();
    lifecycle::install(app.handle(), Some(config_dir.to_path_buf()));
    app.state::<Lifecycle>().mark_tray(true);
    lifecycle::open_at_launch(app.handle(), true).unwrap();
    assert!(app.webview_windows().is_empty(), "the app starts windowless");
    app
}

fn verified(app: &App<MockRuntime>, installed: &Installed) -> update::Staged {
    let key = Key::new();
    let origin = channel("0.2.0", &key.sign(&archive(NEXT), "0.2.0"), archive(NEXT));
    tauri::async_runtime::block_on(async {
        let update = updater(app, &key, origin, &installed.executable)
            .check()
            .await
            .unwrap()
            .expect("0.2.0 is offered");
        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        (update, bytes)
    })
}

#[test]
fn a_windowless_app_told_to_install_opens_its_window_to_quit_with_the_update_staged() {
    let dir = tempfile::tempdir().unwrap();
    let app = windowless(dir.path(), shipped_windows());
    let installed = Installed::new();
    let staged = verified(&app, &installed);

    update::stage_and_quit(app.handle(), staged).join().expect("the app was not exited");

    assert!(app.get_webview_window(MAIN_WINDOW).is_some(), "the window opens to run the check");
    assert!(app.state::<Lifecycle>().take_quit_request(), "and is asked to quit");
    assert!(app.state::<update::Pending>().take().is_some(), "the update waits for that quit");
    assert_eq!(installed.binary(), CURRENT, "nothing installs before the quit");
}

#[test]
fn an_update_whose_quit_cannot_open_a_window_is_dropped_not_kept_for_a_later_quit() {
    let dir = tempfile::tempdir().unwrap();
    let app = windowless(dir.path(), serde_json::json!([]));
    let installed = Installed::new();
    let staged = verified(&app, &installed);

    update::stage_and_quit(app.handle(), staged).join().expect("the app was not exited");

    assert!(!app.state::<Lifecycle>().take_quit_request());
    assert!(app.state::<update::Pending>().take().is_none(), "no update waits for a later quit");
    assert_eq!(installed.binary(), CURRENT);
}
