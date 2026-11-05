#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PROFILE: &str = "package-test";
const LAUNCHD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const WEBDRIVER_MARK: &str = "wdioTauri";
const CAPTURED_PATH: &str = "/packaged-capture";
// macOS assesses a freshly signed binary before its first run.
const PATIENCE: Duration = Duration::from_mins(2);

fn repo() -> PathBuf {
    xtask::repo_root().unwrap()
}

// Both cases rewrite ui/dist, and nextest runs them in separate processes.
fn exclusive(repo: &Path) -> File {
    let lock = File::create(repo.join("target/package-tests.lock")).unwrap();
    lock.lock().unwrap();
    lock
}

fn flavour(ui: &Path) -> String {
    std::fs::read_to_string(ui.join("dist/bundle-flavour")).unwrap_or_default()
}

fn carries_webdriver(ui: &Path) -> bool {
    std::fs::read_dir(ui.join("dist/assets")).unwrap().flatten().any(|entry| {
        let path = entry.path();
        path.extension().is_some_and(|ext| ext == "js")
            && std::fs::read_to_string(&path).unwrap().contains(WEBDRIVER_MARK)
    })
}

fn e2e_bundle(ui: &Path) {
    let built =
        Command::new("pnpm").arg("build").env("VITE_E2E", "1").current_dir(ui).status().unwrap();
    assert!(built.success(), "VITE_E2E=1 pnpm build");
    assert_eq!(flavour(ui), "e2e");
    assert!(carries_webdriver(ui), "an e2e bundle carries {WEBDRIVER_MARK}");
}

#[test]
fn a_plain_build_after_an_e2e_bundle_embeds_a_release_bundle() {
    let repo = repo();
    let _turn = exclusive(&repo);
    let ui = repo.join("ui");
    e2e_bundle(&ui);

    let built =
        Command::new("cargo").args(["build", "-p", "trawler-ui"]).current_dir(&repo).status();
    assert!(built.unwrap().success(), "cargo build -p trawler-ui");

    assert_eq!(flavour(&ui), "release");
    assert!(!carries_webdriver(&ui), "the release bundle carries {WEBDRIVER_MARK}");
}

#[test]
fn a_package_is_ad_hoc_signed_and_its_binary_captures_under_a_launcher_env() {
    let repo = repo();
    let _turn = exclusive(&repo);
    let ui = repo.join("ui");
    e2e_bundle(&ui);

    let packaged = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["package", "--profile", PROFILE])
        .env("VITE_E2E", "1")
        .current_dir(&repo)
        .status();
    assert!(packaged.unwrap().success(), "cargo xtask package --profile {PROFILE}");
    assert_eq!(flavour(&ui), "release");
    assert!(!carries_webdriver(&ui), "the packaged bundle carries {WEBDRIVER_MARK}");

    let bundle = repo.join("target").join(PROFILE).join("bundle");
    let app = bundle.join("macos/Trawler.app");
    let verified =
        Command::new("codesign").args(["--verify", "--deep", "--strict"]).arg(&app).status();
    assert!(verified.unwrap().success(), "codesign --verify --deep --strict {}", app.display());
    let dmgs = std::fs::read_dir(bundle.join("dmg")).unwrap().flatten();
    assert!(dmgs.into_iter().any(|e| e.path().extension().is_some_and(|ext| ext == "dmg")));

    let home = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::launch(&only_executable(&app), home.path());
    let (proxy, api) = core.ready();
    let origin = serve_once();
    let through_proxy = http(
        proxy,
        &format!("GET http://{origin}{CAPTURED_PATH} HTTP/1.1\r\nHost: {origin}\r\nConnection: close\r\n\r\n"),
    );
    assert!(through_proxy.starts_with("HTTP/1.1 200"), "{through_proxy}");
    assert!(captured(api), "GET /flows never listed {CAPTURED_PATH}");
    core.stop();
}

fn only_executable(app: &Path) -> PathBuf {
    let mut binaries: Vec<PathBuf> = std::fs::read_dir(app.join("Contents/MacOS"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(binaries.len(), 1, "one executable in the .app: {binaries:?}");
    binaries.remove(0)
}

struct CoreProcess {
    child: Child,
}

impl CoreProcess {
    fn launch(binary: &Path, home: &Path) -> Self {
        let child = Command::new(binary)
            .env_clear()
            .env("PATH", LAUNCHD_PATH)
            .env("HOME", home)
            .current_dir("/")
            .arg("--core")
            .arg("--ca-dir")
            .arg(home.join("ca"))
            .args(["--proxy-port", "0", "--api-port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self { child }
    }

    fn ready(&mut self) -> (SocketAddr, SocketAddr) {
        let stdout = self.child.stdout.take().unwrap();
        let (sent, line) = mpsc::channel();
        std::thread::spawn(move || {
            let mut first = String::new();
            let _ = BufReader::new(stdout).read_line(&mut first);
            let _ = sent.send(first);
        });
        let line = line.recv_timeout(PATIENCE).expect("the core printed its ready line");
        let ready: serde_json::Value = serde_json::from_str(&line).expect("a ready line");
        let addr = |key: &str| ready[key].as_str().unwrap().parse().unwrap();
        (addr("proxy"), addr("api"))
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn serve_once() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        while request.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
            line.clear();
        }
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    addr
}

fn http(to: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(to).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

fn captured(api: SocketAddr) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let flows =
            http(api, &format!("GET /flows HTTP/1.1\r\nHost: {api}\r\nConnection: close\r\n\r\n"));
        if flows.contains(CAPTURED_PATH) {
            return true;
        }
        std::thread::park_timeout(Duration::from_millis(100));
    }
    false
}
