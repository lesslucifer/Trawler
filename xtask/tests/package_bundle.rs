#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs::File;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROFILE: &str = "package-test";
const WEBDRIVER_MARK: &str = "wdioTauri";
const LOGIN_AGENT: &str = "Contents/Library/LaunchAgents/dev.trawler.app.plist";
const LAUNCHD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const APP_DATA: &str = "Library/Application Support/dev.trawler.app";
const PROXY: &str = "127.0.0.1:7420";
const API: &str = "127.0.0.1:7421";

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

fn wait_for(what: &str, within: Duration, mut holds: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !holds() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::park_timeout(Duration::from_millis(200));
    }
}

fn free(port: SocketAddr) -> bool {
    TcpListener::bind(port).is_ok()
}

// The release app owns the fixed ports and the machine's single-instance socket,
// so a Trawler already running here would answer in its place.
fn refuse_a_running_trawler() {
    for port in [PROXY, API] {
        assert!(free(port.parse().unwrap()), "{port} is taken: quit Trawler, then run this test");
    }
}

struct Window(Child);

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn install_from_the_dmg(dmg: &Path, into: &Path) -> PathBuf {
    let mount = into.join("mnt");
    let attached = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(dmg)
        .output()
        .unwrap();
    assert!(attached.status.success(), "hdiutil attach: {attached:?}");
    let installed = into.join("Applications/Trawler.app");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    let copied = Command::new("ditto").arg(mount.join("Trawler.app")).arg(&installed).status();
    let detached = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    assert!(copied.unwrap().success(), "ditto Trawler.app out of {}", dmg.display());
    assert!(detached.unwrap().success(), "hdiutil detach {}", mount.display());
    installed
}

// Today's check is on record, so the launch never asks the public release channel.
fn mark_the_update_check_done(home: &Path) {
    let data = home.join(APP_DATA);
    std::fs::create_dir_all(&data).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(data.join("last-update-check"), now.to_string()).unwrap();
}

fn launch_window(app: &Path, home: &Path) -> Window {
    let binary = app.join("Contents/MacOS/trawler-ui");
    let child = Command::new(&binary)
        .env_clear()
        .env("PATH", LAUNCHD_PATH)
        .env("HOME", home)
        .current_dir(home)
        .spawn()
        .unwrap();
    Window(child)
}

fn left_running(installed: &Path) -> String {
    let found = Command::new("pgrep").arg("-fl").arg(installed).output().unwrap();
    String::from_utf8_lossy(&found.stdout).into_owned()
}

fn the_installed_window_owns_a_core_that_captures_and_frees_its_ports(dmg: &Path) {
    let home = tempfile::tempdir().unwrap();
    let app = install_from_the_dmg(dmg, home.path());
    mark_the_update_check_done(home.path());
    let (proxy, api): (SocketAddr, SocketAddr) = (PROXY.parse().unwrap(), API.parse().unwrap());

    refuse_a_running_trawler();
    let mut window = launch_window(&app, home.path());
    let ca = home.path().join(APP_DATA).join("ca/trawler-ca.pem");
    wait_for(
        "the app's own core to mint its CA and answer on 7421",
        Duration::from_secs(60),
        || ca.is_file() && TcpStream::connect(api).is_ok(),
    );
    xtask::smoke::capture_through_the_proxy(proxy, api).unwrap();
    assert_eq!(window.0.try_wait().unwrap(), None, "the window app is still running");

    let ended = Command::new("kill").arg(window.0.id().to_string()).status();
    assert!(ended.unwrap().success());
    window.0.wait().unwrap();
    wait_for("both ports to be released", Duration::from_secs(15), || free(proxy) && free(api));
    wait_for("no process from the install", Duration::from_secs(5), || {
        left_running(&app).is_empty()
    });
}

fn copyright_of(app: &Path) -> String {
    let read = Command::new("plutil")
        .args(["-extract", "NSHumanReadableCopyright", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .unwrap();
    assert!(read.status.success(), "plutil: {read:?}");
    String::from_utf8(read.stdout).unwrap().trim_end().to_string()
}

#[test]
fn a_package_is_ad_hoc_signed_and_passes_the_release_smoke() {
    let repo = repo();
    refuse_a_running_trawler();
    let _turn = exclusive(&repo);
    let ui = repo.join("ui");
    e2e_bundle(&ui);
    let bundle = repo.join("target").join(PROFILE).join("bundle");
    let _ = std::fs::remove_dir_all(bundle.join("dmg"));

    let packaged = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["package", "--profile", PROFILE])
        .env("VITE_E2E", "1")
        .current_dir(&repo)
        .status();
    assert!(packaged.unwrap().success(), "cargo xtask package --profile {PROFILE}");
    assert_eq!(flavour(&ui), "release");
    assert!(!carries_webdriver(&ui), "the packaged bundle carries {WEBDRIVER_MARK}");

    let app = bundle.join("macos/Trawler.app");
    let verified =
        Command::new("codesign").args(["--verify", "--deep", "--strict"]).arg(&app).status();
    assert!(verified.unwrap().success(), "codesign --verify --deep --strict {}", app.display());
    assert!(app.join(LOGIN_AGENT).is_file());
    assert_eq!(copyright_of(&app), format!("Copyright \u{a9} 2026 {}", xtask::package::HOLDER));
    let notice = std::fs::read(app.join("Contents/Resources").join(xtask::licenses::NOTICE));
    assert_eq!(
        notice.unwrap(),
        std::fs::read(repo.join(xtask::licenses::NOTICE)).unwrap(),
        "the app carries the committed third-party notice"
    );
    let archived = Command::new("tar")
        .arg("-tzf")
        .arg(bundle.join("macos/Trawler.app.tar.gz"))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&archived.stdout).contains(&format!("Trawler.app/{LOGIN_AGENT}")),
        "an update installs the login agent with the app, so the registered login item survives it"
    );
    let dmg = std::fs::read_dir(bundle.join("dmg"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "dmg"))
        .expect("a .dmg in the bundle");

    let smoked = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["smoke", "--profile", PROFILE])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        smoked.status.success(),
        "cargo xtask smoke --profile {PROFILE}: {}{}",
        String::from_utf8_lossy(&smoked.stdout),
        String::from_utf8_lossy(&smoked.stderr)
    );

    the_installed_window_owns_a_core_that_captures_and_frees_its_ports(&dmg);
}

// The registering half is the user's step: SMAppService answers only to the running .app.
#[test]
fn after_the_login_item_is_unticked_background_task_management_lists_nothing() {
    if std::env::var_os("TRAWLER_BTM_UNTICKED").is_none() {
        return;
    }
    let dump = Command::new("sfltool").arg("dumpbtm").output().unwrap();
    assert!(dump.status.success(), "sfltool dumpbtm");
    let dump = String::from_utf8_lossy(&dump.stdout);
    assert!(!dump.contains("dev.trawler.app"), "sfltool dumpbtm still lists dev.trawler.app");
}
