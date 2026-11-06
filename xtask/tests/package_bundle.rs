#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROFILE: &str = "package-test";
const WEBDRIVER_MARK: &str = "wdioTauri";
const LOGIN_AGENT: &str = "Contents/Library/LaunchAgents/dev.trawler.app.plist";

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
fn a_package_is_ad_hoc_signed_and_passes_the_release_smoke() {
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
    assert!(app.join(LOGIN_AGENT).is_file());
    let archived = Command::new("tar")
        .arg("-tzf")
        .arg(bundle.join("macos/Trawler.app.tar.gz"))
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&archived.stdout).contains(&format!("Trawler.app/{LOGIN_AGENT}")),
        "an update installs the login agent with the app, so the registered login item survives it"
    );
    let dmgs = std::fs::read_dir(bundle.join("dmg")).unwrap().flatten();
    assert!(dmgs.into_iter().any(|e| e.path().extension().is_some_and(|ext| ext == "dmg")));

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
