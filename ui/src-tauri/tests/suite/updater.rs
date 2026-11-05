use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};

use base64::Engine;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::App;
use tauri_plugin_updater::{Updater, UpdaterExt};

const TARGET: &str = "trawler-test";
const ARCHIVE_PATH: &str = "/Trawler.app.tar.gz";
const MANIFEST_PATH: &str = "/latest.json";
const CURRENT: &[u8] = b"trawler-ui 0.1.0";
const NEXT: &[u8] = b"trawler-ui 0.2.0, the marker the update carries";
const CA: &[u8] = b"-----BEGIN CERTIFICATE-----\nthe CA a user trusted\n";

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
            let mut stream = stream.unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            request.read_line(&mut first).unwrap();
            let mut line = String::new();
            while request.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let path = first.split(' ').nth(1).unwrap_or_default();
            let body = match path {
                MANIFEST_PATH => manifest.as_slice(),
                ARCHIVE_PATH => archive.as_slice(),
                _ => &[],
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
        }
    });
    origin
}

fn app() -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": "",
            "dangerousInsecureTransportProtocol": true,
            "requireSignedVersion": true
        }),
    );
    mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap()
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
