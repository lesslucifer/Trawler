#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").canonicalize().unwrap()
}

// The core workspace must never pull in the GUI crate: a headless
// `cargo build`/`cargo test` of the proxy stays free of the Tauri toolchain.
#[test]
fn core_workspace_excludes_the_ui_crate() {
    let repo = repo_root();
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata = String::from_utf8(out.stdout).unwrap();
    assert!(
        !metadata.contains("\"trawler-ui\""),
        "core workspace unexpectedly includes the ui/src-tauri crate"
    );
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
