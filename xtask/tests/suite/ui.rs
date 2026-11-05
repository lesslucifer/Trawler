use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").canonicalize().unwrap()
}

fn normal_dependencies_of(package: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "-p", package, "-e", "normal,build", "--prefix", "none", "--format", "{p}"])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(out.status.success(), "cargo tree failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

fn gui_crates(dependencies: &[String]) -> Vec<&String> {
    dependencies
        .iter()
        .filter(|name| {
            name.as_str() == "wry" || name.as_str() == "tao" || name.starts_with("tauri")
        })
        .collect()
}

#[test]
fn the_gui_detector_names_tauri_wry_and_tao() {
    let tree = ["tokio", "tauri-runtime-wry", "wry", "tao", "hudsucker"].map(String::from);
    assert_eq!(gui_crates(&tree), ["tauri-runtime-wry", "wry", "tao"]);
}

// The app links the core, never the reverse: whatever builds the trawler
// package alone stays free of the GUI toolchain.
#[test]
fn the_core_package_links_no_gui() {
    let core = normal_dependencies_of("trawler");
    assert!(core.iter().any(|name| name == "hudsucker"), "the tree read is the core's: {core:?}");
    assert_eq!(gui_crates(&core), Vec::<&String>::new());
}

#[test]
fn the_app_package_links_the_core() {
    let app = normal_dependencies_of("trawler-ui");
    assert!(app.iter().any(|name| name == "trawler"), "trawler-ui does not depend on trawler");
    assert!(app.iter().any(|name| name == "wry"), "the tree read is the app's: {app:?}");
}

// The WU seam: `cargo xtask ui build` runs the real pnpm pipeline end to end
// and emits ui/dist/index.html. Slow (pnpm install + Vite build); opt in with
// TRAWLER_UI_BUILD_TEST=1 so it does not gate the fast inner loop.
#[test]
fn xtask_ui_build_emits_a_bundle() {
    if std::env::var("TRAWLER_UI_BUILD_TEST").is_err() {
        eprintln!("skipping: set TRAWLER_UI_BUILD_TEST=1 to run the real pnpm build");
        return;
    }
    let repo = repo_root();
    let dist_index = repo.join("ui/dist/index.html");
    let _ = std::fs::remove_file(&dist_index);
    let status = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["ui", "build"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success(), "cargo xtask ui build failed");
    assert!(dist_index.is_file(), "ui/dist/index.html was not produced");
}
