#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;
use xtask::hooks::TRACKED_DIR;
use xtask::target::ISOLATE;

use crate::hermetic::{executable, git, hermetic, said, write};

struct Repo {
    dir: TempDir,
    main: PathBuf,
}

fn app_source(value: &str) -> String {
    format!(
        "const VALUE: &str = \"{value}\";\n\nfn main() {{\n    println!(\"{{VALUE}} {{}}\", vendored::NAME);\n}}\n"
    )
}

fn names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

fn any_named(dir: &Path, prefix: &str) -> bool {
    names(dir).iter().any(|n| n.starts_with(prefix))
}

fn tracked_post_checkout() -> String {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    fs::read_to_string(repo_root.join(TRACKED_DIR).join("post-checkout")).unwrap()
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    let out = hermetic("cargo", dir).args(args).arg("--offline").output().unwrap();
    assert!(out.status.success(), "cargo {args:?}: {}", said(&out));
    out
}

fn run_app(dir: &Path) -> String {
    let out = cargo(dir, &["run", "-q", "-p", "app"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn xtask(dir: &Path, args: &[&str]) -> Output {
    hermetic(env!("CARGO_BIN_EXE_xtask"), dir).args(args).output().unwrap()
}

fn commit(checkout: &Path, file: &str) {
    write(&checkout.join(file), file);
    git(checkout, &["add", "-A"]);
    git(checkout, &["commit", "-q", "-m", file]);
}

impl Repo {
    // `vendored` sits outside the workspace root, so cargo treats it as a
    // dependency, not a member: it stands in for the registry crates.
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let main = dir.path().join("main");
        let repo = Self { dir, main };
        write(
            &repo.dir.path().join("vendored/Cargo.toml"),
            "[package]\nname = \"vendored\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(
            &repo.dir.path().join("vendored/src/lib.rs"),
            "pub const NAME: &str = \"vendored\";\n",
        );
        write(
            &repo.main.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n",
        );
        write(
            &repo.main.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvendored = { path = \"../../vendored\" }\n",
        );
        write(&repo.main.join("app/src/main.rs"), &app_source("main"));
        write(&repo.main.join(".gitignore"), "/target\n/Cargo.lock\n");
        write(&repo.main.join(".gitworktreecopy"), "# nothing to copy\n");
        executable(&repo.main.join(TRACKED_DIR).join("post-checkout"), &tracked_post_checkout());
        repo.tracked_pre_commit(&repo.main, "main-v1");
        git(&repo.main, &["init", "-q", "-b", "main"]);
        git(&repo.main, &["add", "-A"]);
        git(&repo.main, &["commit", "-q", "--no-verify", "-m", "init"]);
        repo
    }

    fn hook_log(&self) -> PathBuf {
        self.dir.path().join("pre-commit-runs.log")
    }

    fn hook_runs(&self) -> Vec<String> {
        fs::read_to_string(self.hook_log())
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn tracked_pre_commit(&self, checkout: &Path, tag: &str) {
        executable(
            &checkout.join(TRACKED_DIR).join("pre-commit"),
            &format!("#!/bin/sh\necho {tag} >> \"{}\"\n", self.hook_log().display()),
        );
    }

    // post-checkout seeds with the source checkout's already-built xtask, as it
    // finds one in a real clone after the first `cargo xtask` run.
    fn build_main_with_its_xtask(&self) {
        cargo(&self.main, &["build"]);
        let seeder = self.main.join("target/debug/xtask");
        fs::copy(env!("CARGO_BIN_EXE_xtask"), &seeder).unwrap();
        fs::set_permissions(&seeder, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn add_worktree(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        git(&self.main, &["worktree", "add", "-q", "-b", name, path.to_str().unwrap()]);
        path
    }
}

#[test]
fn every_checkout_runs_its_own_hooks_and_builds_its_own_code() {
    let repo = Repo::new();
    let installed = xtask(&repo.main, &["hooks", "install"]);
    assert!(installed.status.success(), "{}", said(&installed));

    commit(&repo.main, "one");
    repo.tracked_pre_commit(&repo.main, "main-v2");
    commit(&repo.main, "two");
    assert_eq!(repo.hook_runs(), ["main-v1", "main-v2"]);

    repo.build_main_with_its_xtask();
    let linked = repo.add_worktree("linked");

    let target = fs::symlink_metadata(linked.join("target")).unwrap();
    assert!(target.is_dir() && !target.file_type().is_symlink());
    let debug = linked.join("target/debug");
    assert!(!any_named(&debug.join(".fingerprint"), "app-"));
    assert!(!any_named(&debug.join("deps"), "app"));
    assert!(!debug.join("incremental").exists() && !debug.join("app").exists());
    if cfg!(target_os = "macos") {
        assert!(any_named(&debug.join(".fingerprint"), "vendored-"));
    }

    repo.tracked_pre_commit(&linked, "linked-v3");
    commit(&linked, "three");
    commit(&repo.main, "four");
    assert_eq!(repo.hook_runs(), ["main-v1", "main-v2", "linked-v3", "main-v2"]);

    let source = linked.join("app/src/main.rs");
    write(&source, &app_source("linked"));
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    fs::File::options().write(true).open(&source).unwrap().set_modified(long_ago).unwrap();
    let first_build = said(&cargo(&linked, &["build"]));
    assert!(first_build.contains("Compiling app"), "{first_build}");
    if cfg!(target_os = "macos") {
        assert!(!first_build.contains("Compiling vendored"), "{first_build}");
    }
    assert_eq!(run_app(&linked), "linked vendored");
    assert_eq!(run_app(&repo.main), "main vendored");

    let stale = repo.add_worktree("stale");
    fs::remove_dir_all(stale.join("target")).unwrap();
    std::os::unix::fs::symlink(repo.main.join("target"), stale.join("target")).unwrap();
    let refused = xtask(&stale, &["lint"]);
    assert_eq!(refused.status.code(), Some(1), "{}", said(&refused));
    assert!(said(&refused).contains(ISOLATE), "{}", said(&refused));

    let isolated = xtask(&stale, &["target", "isolate"]);
    assert!(isolated.status.success(), "{}", said(&isolated));
    let green_lint = xtask(&stale, &["lint"]);
    assert!(green_lint.status.success(), "{}", said(&green_lint));
    assert!(said(&green_lint).contains("lint green"), "{}", said(&green_lint));
}
