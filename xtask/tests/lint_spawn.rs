#![cfg(unix)]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

// What `cargo xtask lint` actually spawns. The unit test beside CLIPPY pins the
// constant's contents; only this pins that `run_lints` forwards it to a real
// process - the guarantee the pre-commit hook used to carry in its own text,
// where a test could read the executed command verbatim. Dropping --workspace
// inside run_lints while leaving CLIPPY untouched fails here and nowhere else.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

struct Env {
    _dir: TempDir,
    repo: std::path::PathBuf,
    log: std::path::PathBuf,
    path: String,
}

impl Env {
    // `cargo_exit` is what the shim reports: 0 stands in for a green fmt/clippy,
    // non-zero for a red one.
    fn new(cargo_exit: i32) -> Self {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        let bin = dir.path().join("bin");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let log = dir.path().join("cargo-calls.log");

        let shim = bin.join("cargo");
        fs::write(
            &shim,
            format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit {cargo_exit}\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();

        let env = Self {
            _dir: dir,
            repo,
            log,
            path: format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
        };
        env.git(&["init", "-q", "-b", "main"]);
        env.git(&["config", "user.email", "lint@test"]);
        env.git(&["config", "user.name", "lint"]);
        // The marker lands under target/, which the key would otherwise hash as
        // an untracked build input - writing the proof would invalidate the very
        // tree it vouches for. Every cargo repo ignores target/; this one must
        // too, or the skip can never fire.
        fs::write(env.repo.join(".gitignore"), "/target\n").unwrap();
        fs::write(env.repo.join("src.rs"), "pub fn a() {}\n").unwrap();
        env.git(&["add", "-A"]);
        env.git(&["commit", "-q", "-m", "init"]);
        env
    }

    fn git(&self, args: &[&str]) {
        let out = scrubbed("git").args(args).current_dir(&self.repo).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn lint(&self) -> (bool, String) {
        let out = scrubbed(env!("CARGO_BIN_EXE_xtask"))
            .arg("lint")
            .current_dir(&self.repo)
            .env("PATH", &self.path)
            .output()
            .unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log).unwrap_or_default().lines().map(str::to_string).collect()
    }

    fn clippy_call(&self) -> String {
        self.calls()
            .into_iter()
            .find(|c| c.starts_with("clippy"))
            .expect("lint must spawn clippy on a cache miss")
    }
}

// Git exports GIT_* to a test process running under a hook; inherited, GIT_DIR
// overrides current_dir and redirects every "hermetic" call at the real repo.
fn scrubbed(program: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new(program.as_ref());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd
}

// The whole point of this file: the scoping reaches the spawned process. Without
// --workspace the root package is linted and xtask - where every gate, hook and
// design check lives - is not.
#[test]
fn the_spawned_clippy_lints_every_workspace_member() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let clippy = env.clippy_call();
    for flag in ["--workspace", "--all-targets", "--all-features", "-D warnings"] {
        assert!(clippy.contains(flag), "the executed clippy dropped {flag}: cargo {clippy}");
    }
}

#[test]
fn fmt_runs_before_clippy() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let calls = env.calls();
    let fmt = calls.iter().position(|c| c.starts_with("fmt")).expect("lint must spawn fmt");
    let clippy =
        calls.iter().position(|c| c.starts_with("clippy")).expect("lint must spawn clippy");
    assert!(fmt < clippy, "fmt is the cheaper refusal and goes first: {calls:?}");
}

#[test]
fn a_green_run_caches_and_the_next_run_spawns_nothing() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let after_first = env.calls().len();
    let (ok, said) = env.lint();
    assert!(ok);
    assert!(said.contains("skipped"), "the second run must skip: {said}");
    assert_eq!(env.calls().len(), after_first, "a skip spawns no cargo process");
}

#[test]
fn a_red_run_refuses_and_caches_nothing() {
    let env = Env::new(1);
    let (ok, said) = env.lint();
    assert!(!ok, "a red lint must exit non-zero: {said}");
    assert!(!env.repo.join("target/.lint-green").exists(), "a red run must not vouch for the tree");
    let (still_red, _) = env.lint();
    assert!(!still_red, "nothing was cached, so the next run must re-run and refuse again");
}

// The marker stores the key it vouches for, never a bare verdict (DEC-11), so a
// tree that moved after the green re-runs rather than riding the old proof.
#[test]
fn a_tree_change_after_the_green_re_runs_the_pair() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let after_first = env.calls().len();
    fs::write(env.repo.join("src.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    assert!(env.lint().0);
    assert!(env.calls().len() > after_first, "an edited tree must re-run fmt and clippy");
}
