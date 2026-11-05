#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;
use xtask::hooks::{INSTALL, TRACKED_DIR};

struct Env {
    dir: TempDir,
    repo: PathBuf,
    home: PathBuf,
    cargo_log: PathBuf,
    path: String,
}

impl Env {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        let bin = dir.path().join("bin");
        let home = dir.path().join("home");
        for d in [&repo, &bin, &home] {
            fs::create_dir_all(d).unwrap();
        }
        let cargo_log = dir.path().join("cargo-calls.log");
        executable(
            &bin.join("cargo"),
            &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", cargo_log.display()),
        );
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        let env = Self { dir, repo, home, cargo_log, path };
        env.git(&env.repo, &["init", "-q", "-b", "main"]);
        env.git(&env.repo, &["config", "commit.gpgsign", "false"]);
        fs::write(env.repo.join(".gitignore"), "/target\n").unwrap();
        env.tracked_pre_commit(&env.repo, "main-v1");
        env.git(&env.repo, &["add", "-A"]);
        env.git(&env.repo, &["commit", "-q", "--no-verify", "-m", "init"]);
        env
    }

    fn runs_log(&self) -> PathBuf {
        self.dir.path().join("hook-runs.log")
    }

    fn runs(&self) -> Vec<String> {
        fs::read_to_string(self.runs_log())
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn tracked_pre_commit(&self, checkout: &Path, tag: &str) {
        let hooks = checkout.join(TRACKED_DIR);
        fs::create_dir_all(&hooks).unwrap();
        executable(
            &hooks.join("pre-commit"),
            &format!("#!/bin/sh\necho {tag} >> \"{}\"\n", self.runs_log().display()),
        );
    }

    fn hermetic(&self, program: impl AsRef<Path>, dir: &Path) -> Command {
        let mut cmd = Command::new(program.as_ref());
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.current_dir(dir)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "hooks")
            .env("GIT_AUTHOR_EMAIL", "hooks@test.invalid")
            .env("GIT_COMMITTER_NAME", "hooks")
            .env("GIT_COMMITTER_EMAIL", "hooks@test.invalid");
        cmd
    }

    fn git(&self, dir: &Path, args: &[&str]) {
        let out = self.hermetic("git", dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn xtask(&self, dir: &Path, args: &[&str]) -> Output {
        self.hermetic(env!("CARGO_BIN_EXE_xtask"), dir).args(args).output().unwrap()
    }

    fn commit(&self, dir: &Path, file: &str) {
        fs::write(dir.join(file), file).unwrap();
        self.git(dir, &["add", "-A"]);
        self.git(dir, &["commit", "-q", "-m", file]);
    }
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_gate_in_an_uninstalled_checkout_refuses_with_the_install_line_and_runs_nothing() {
    let env = Env::new();
    let out = env.xtask(&env.repo, &["lint"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("core.hooksPath is unset"), "{said}");
    assert!(said.contains(INSTALL), "{said}");
    assert!(!env.cargo_log.exists(), "a refused gate must spawn no cargo");
}

#[test]
fn after_install_an_edited_tracked_hook_runs_on_the_next_commit() {
    let env = Env::new();
    let out = env.xtask(&env.repo, &["hooks", "install"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    env.commit(&env.repo, "one");
    env.tracked_pre_commit(&env.repo, "main-v2");
    env.commit(&env.repo, "two");
    assert_eq!(env.runs(), ["main-v1", "main-v2"]);
}

#[test]
fn after_install_the_gate_entrypoint_runs() {
    let env = Env::new();
    assert!(env.xtask(&env.repo, &["hooks", "install"]).status.success());
    let out = env.xtask(&env.repo, &["lint"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    assert!(env.cargo_log.exists(), "an installed checkout reaches the lint pair");
}

// One install covers every worktree, and each runs the hook its own branch
// carries rather than the checkout that ran the install.
#[test]
fn a_linked_worktree_runs_its_own_checkouts_hook_without_a_second_install() {
    let env = Env::new();
    assert!(env.xtask(&env.repo, &["hooks", "install"]).status.success());
    let linked = env.dir.path().join("linked");
    env.git(&env.repo, &["worktree", "add", "-q", "-b", "side", linked.to_str().unwrap()]);
    env.tracked_pre_commit(&linked, "linked-v2");
    env.commit(&linked, "side");
    assert_eq!(env.runs(), ["linked-v2"]);
    let out = env.xtask(&linked, &["lint"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
