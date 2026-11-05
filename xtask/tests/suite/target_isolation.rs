#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use tempfile::TempDir;
use xtask::target::ISOLATE;

struct Env {
    dir: TempDir,
    main: PathBuf,
    linked: PathBuf,
}

fn app_source(value: &str) -> String {
    format!("fn main() {{\n    println!(\"{{}} {{}}\", \"{value}\", vendored::NAME);\n}}\n")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn said(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

fn any_named(dir: &Path, prefix: &str) -> bool {
    names(dir).iter().any(|n| n.starts_with(prefix))
}

fn hermetic(program: impl AsRef<Path>, dir: &Path) -> Command {
    let mut cmd = Command::new(program.as_ref());
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy();
        if key.starts_with("GIT_") || (key.starts_with("CARGO_") && key != "CARGO_HOME") {
            cmd.env_remove(key.as_ref());
        }
    }
    cmd.current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "target")
        .env("GIT_AUTHOR_EMAIL", "target@test.invalid")
        .env("GIT_COMMITTER_NAME", "target")
        .env("GIT_COMMITTER_EMAIL", "target@test.invalid");
    cmd
}

fn git(dir: &Path, args: &[&str]) {
    let out = hermetic("git", dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", said(&out));
}

fn cargo(dir: &Path, args: &[&str]) -> Output {
    let out = hermetic("cargo", dir).args(args).arg("--offline").output().unwrap();
    assert!(out.status.success(), "cargo {args:?}: {}", said(&out));
    out
}

fn run_app(dir: &Path) -> String {
    String::from_utf8_lossy(&cargo(dir, &["run", "-q", "-p", "app"]).stdout).trim().to_string()
}

fn xtask(dir: &Path, args: &[&str]) -> Output {
    hermetic(env!("CARGO_BIN_EXE_xtask"), dir).args(args).output().unwrap()
}

impl Env {
    // `vendored` sits outside the workspace root, so cargo treats it as a
    // dependency, not a member: it stands in for the registry crates.
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let main = dir.path().join("main");
        let linked = dir.path().join("linked");
        write(
            &dir.path().join("vendored/Cargo.toml"),
            "[package]\nname = \"vendored\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(&dir.path().join("vendored/src/lib.rs"), "pub const NAME: &str = \"vendored\";\n");
        write(&main.join("Cargo.toml"), "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n");
        write(
            &main.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvendored = { path = \"../../vendored\" }\n",
        );
        write(&main.join("app/src/main.rs"), &app_source("main"));
        write(&main.join(".gitignore"), "/target\n/Cargo.lock\n");
        let env = Self { dir, main, linked };
        git(&env.main, &["init", "-q", "-b", "main"]);
        git(&env.main, &["add", "-A"]);
        git(&env.main, &["commit", "-q", "--no-verify", "-m", "init"]);
        git(&env.main, &["worktree", "add", "-q", "-b", "side", env.linked.to_str().unwrap()]);
        env
    }

    fn xtask_with_fake_cargo(&self, dir: &Path, args: &[&str]) -> (Output, Vec<String>) {
        let bin = self.dir.path().join("fake-bin");
        let log = self.dir.path().join("fake-cargo.log");
        let cargo = bin.join("cargo");
        write(&cargo, &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", log.display()));
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_file(&log);
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        let out = hermetic(env!("CARGO_BIN_EXE_xtask"), dir)
            .env("PATH", path)
            .args(args)
            .output()
            .unwrap();
        let calls =
            fs::read_to_string(&log).unwrap_or_default().lines().map(str::to_string).collect();
        (out, calls)
    }

    fn link_target_to_main(&self) {
        fs::create_dir_all(self.main.join("target")).unwrap();
        std::os::unix::fs::symlink(self.main.join("target"), self.linked.join("target")).unwrap();
    }
}

#[test]
#[cfg(target_os = "macos")]
fn a_seeded_worktree_holds_only_third_party_artifacts_and_compiles_only_its_own_crates() {
    let env = Env::new();
    cargo(&env.main, &["build"]);

    let out = xtask(&env.linked, &["target", "seed"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("target seeded"), "{}", said(&out));
    let debug = env.linked.join("target/debug");
    assert!(!fs::symlink_metadata(env.linked.join("target")).unwrap().file_type().is_symlink());
    assert!(any_named(&debug.join(".fingerprint"), "vendored-"));
    assert!(any_named(&debug.join("deps"), "libvendored-"));
    assert!(!any_named(&debug.join(".fingerprint"), "app-"));
    assert!(!any_named(&debug.join("deps"), "app"));
    assert!(!debug.join("incremental").exists());
    assert!(!debug.join("app").exists());

    let build = said(&cargo(&env.linked, &["build"]));
    assert!(build.contains("Compiling app"), "{build}");
    assert!(
        !build.contains("Compiling vendored"),
        "a seeded third-party crate must not rebuild:\n{build}"
    );
}

#[test]
fn two_worktrees_building_one_crate_from_different_sources_each_run_their_own() {
    let env = Env::new();
    let source = env.linked.join("app/src/main.rs");
    write(&source, &app_source("linked"));
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    fs::File::options().write(true).open(&source).unwrap().set_modified(long_ago).unwrap();
    cargo(&env.main, &["build"]);

    let seeded = xtask(&env.linked, &["target", "seed"]);
    assert!(seeded.status.success(), "{}", said(&seeded));

    assert_eq!(run_app(&env.linked), "linked vendored");
    assert_eq!(run_app(&env.main), "main vendored");
}

#[test]
fn a_symlinked_target_refuses_the_gate_until_isolated() {
    let env = Env::new();
    env.link_target_to_main();

    let (refused, calls) = env.xtask_with_fake_cargo(&env.linked, &["lint"]);
    assert_eq!(refused.status.code(), Some(1), "{}", said(&refused));
    assert!(said(&refused).contains(ISOLATE), "{}", said(&refused));
    assert!(calls.is_empty(), "a refused gate spawns no cargo: {calls:?}");

    let isolated = xtask(&env.linked, &["target", "isolate"]);
    assert!(isolated.status.success(), "{}", said(&isolated));
    let target = fs::symlink_metadata(env.linked.join("target")).unwrap();
    assert!(target.is_dir() && !target.file_type().is_symlink());

    let (ran, calls) = env.xtask_with_fake_cargo(&env.linked, &["lint"]);
    assert!(!said(&ran).contains(ISOLATE), "{}", said(&ran));
    assert!(!calls.is_empty(), "an isolated checkout's gate runs");
}

#[test]
fn isolate_names_another_checkout_and_leaves_a_directory_alone() {
    let env = Env::new();
    env.link_target_to_main();

    let out = xtask(&env.main, &["target", "isolate", env.linked.to_str().unwrap()]);
    assert!(out.status.success(), "{}", said(&out));
    assert!(!fs::symlink_metadata(env.linked.join("target")).unwrap().file_type().is_symlink());

    let again = xtask(&env.main, &["target", "isolate", env.linked.to_str().unwrap()]);
    assert!(
        again.status.success() && said(&again).contains("already isolated"),
        "{}",
        said(&again)
    );
}

#[test]
fn prune_drops_the_workspace_build_keeps_third_party_and_folds_the_legacy_tally() {
    let env = Env::new();
    cargo(&env.main, &["build"]);
    let legacy = env.main.join("target/.gate-tally");
    fs::write(&legacy, "a legacy record\n").unwrap();

    let out = xtask(&env.main, &["target", "prune"]);

    assert!(out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("target pruned"), "{}", said(&out));
    let debug = env.main.join("target/debug");
    assert!(!any_named(&debug.join(".fingerprint"), "app-"));
    assert!(!any_named(&debug.join("deps"), "app"));
    assert!(!debug.join("incremental").exists() && !debug.join("app").exists());
    assert!(any_named(&debug.join(".fingerprint"), "vendored-"));
    assert!(!legacy.exists());
    let tally = fs::read_to_string(env.main.join(".git/trawler-gate-tally")).unwrap();
    assert_eq!(tally, "a legacy record\n");

    let rebuild = said(&cargo(&env.main, &["build"]));
    assert!(
        rebuild.contains("Compiling app") && !rebuild.contains("Compiling vendored"),
        "{rebuild}"
    );
}
