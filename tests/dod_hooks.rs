#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// Git exports GIT_DIR to hook processes; inherited (e.g. when this suite runs under
// pre-push), it redirects every "hermetic" repo call to the real repository.
fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd
}

struct Env {
    home: PathBuf,
    fake_path: String,
    log: PathBuf,
}

impl Env {
    fn new(tmp: &Path, cargo_exit: i32) -> Self {
        let home = tmp.join("home");
        let bin = tmp.join("bin");
        fs::create_dir_all(&home).expect("mkdir home");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let log = tmp.join("cargo-calls.log");
        let shim = format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit {cargo_exit}\n", log.display());
        let cargo = bin.join("cargo");
        fs::write(&cargo, shim).expect("write cargo shim");
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).expect("chmod shim");
        let orig = std::env::var("PATH").unwrap_or_default();
        Self { home, fake_path: format!("{}:{orig}", bin.display()), log }
    }

    fn git(&self, dir: &Path, args: &[&str]) -> bool {
        git_cmd()
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .env("PATH", &self.fake_path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.invalid")
            .output()
            .expect("failed to spawn git")
            .status
            .success()
    }

    fn git_ok(&self, dir: &Path, args: &[&str]) {
        assert!(self.git(dir, args), "git {args:?} failed");
    }

    fn calls(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }
}

fn install_hook(repo: &Path, name: &str) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(".cargo-husky/hooks").join(name);
    let hooks = repo.join(".git/hooks");
    fs::create_dir_all(&hooks).expect("mkdir hooks");
    let dest = hooks.join(name);
    fs::copy(&src, &dest).expect("copy hook");
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).expect("chmod hook");
}

fn repo_with_wu_branch(env: &Env, tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");
    env.git_ok(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("a.txt"), "base").expect("write file");
    env.git_ok(&repo, &["add", "-A"]);
    env.git_ok(&repo, &["commit", "-q", "-m", "init"]);
    env.git_ok(&repo, &["branch", "epic/x"]);
    env.git_ok(&repo, &["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    fs::write(repo.join("a.txt"), "changed").expect("write file");
    env.git_ok(&repo, &["add", "-A"]);
    env.git_ok(&repo, &["commit", "-q", "-m", "wu change"]);
    repo
}

#[test]
fn test_repos_ignore_inherited_hook_env() {
    std::env::set_var("GIT_DIR", "/nonexistent/.git");
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 0);
    let repo = repo_with_wu_branch(&env, tmp.path());
    assert!(env.git(&repo, &["rev-parse", "-q", "--verify", "wu/x/one"]));
    std::env::remove_var("GIT_DIR");
}

#[test]
fn merge_into_epic_runs_the_wu_gate_and_a_red_gate_blocks_the_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 1);
    let repo = repo_with_wu_branch(&env, tmp.path());
    install_hook(&repo, "pre-merge-commit");

    env.git_ok(&repo, &["checkout", "-q", "epic/x"]);
    let merged = env.git(&repo, &["merge", "--no-ff", "-q", "-m", "merge wu", "wu/x/one"]);

    assert!(!merged, "red gate must block the merge");
    assert!(env.calls().contains("xtask verify-dod hook wu"));
}

#[test]
fn merge_into_epic_with_a_green_gate_lands_as_a_merge_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 0);
    let repo = repo_with_wu_branch(&env, tmp.path());
    install_hook(&repo, "pre-merge-commit");

    env.git_ok(&repo, &["checkout", "-q", "epic/x"]);
    env.git_ok(&repo, &["merge", "--no-ff", "-q", "-m", "merge wu", "wu/x/one"]);

    assert!(env.calls().contains("xtask verify-dod hook wu"));
    env.git_ok(&repo, &["rev-parse", "-q", "--verify", "HEAD^2"]);
}

#[test]
fn merge_into_main_runs_the_epic_gate_and_other_targets_are_ungated() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 0);
    let repo = repo_with_wu_branch(&env, tmp.path());
    install_hook(&repo, "pre-merge-commit");

    env.git_ok(&repo, &["checkout", "-q", "main"]);
    env.git_ok(&repo, &["merge", "--no-ff", "-q", "-m", "merge wu", "wu/x/one"]);
    assert!(env.calls().contains("xtask verify-dod hook epic"));

    env.git_ok(&repo, &["checkout", "-q", "-b", "scratch", "main~1"]);
    env.git_ok(&repo, &["merge", "--no-ff", "-q", "-m", "merge wu", "wu/x/one"]);
    assert!(!env.calls().contains("hook wu"), "non-epic targets must stay ungated");
}

#[test]
fn pre_commit_gates_conflicted_merge_completion_on_epic() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 0);
    let repo = repo_with_wu_branch(&env, tmp.path());
    install_hook(&repo, "pre-commit");

    env.git_ok(&repo, &["checkout", "-q", "epic/x"]);
    let wu_tip = {
        let out = git_cmd()
            .args(["rev-parse", "wu/x/one"])
            .current_dir(&repo)
            .output()
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    fs::write(repo.join(".git/MERGE_HEAD"), format!("{wu_tip}\n")).expect("write MERGE_HEAD");
    fs::write(repo.join("a.txt"), "resolved").expect("write file");
    env.git_ok(&repo, &["add", "-A"]);
    env.git_ok(&repo, &["commit", "-q", "-m", "complete conflicted merge"]);

    assert!(env.calls().contains("verify-dod hook wu"), "got: {}", env.calls());
}

#[test]
fn pre_push_syncs_notes_refs_and_range_checks_epic_pushes() {
    let tmp = tempfile::tempdir().unwrap();
    let env = Env::new(tmp.path(), 0);
    let repo = repo_with_wu_branch(&env, tmp.path());
    install_hook(&repo, "pre-push");

    let bare = tmp.path().join("origin.git");
    env.git_ok(tmp.path(), &["init", "-q", "--bare", "origin.git"]);
    env.git_ok(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    env.git_ok(&repo, &["notes", "--ref=test", "add", "-m", "schema: test/v2", "HEAD"]);

    env.git_ok(&repo, &["push", "-q", "origin", "epic/x"]);
    assert!(env.calls().contains("nextest run --all") || env.calls().contains("test --all"));
    env.git_ok(&bare, &["show-ref", "--verify", "-q", "refs/notes/test"]);

    env.git_ok(&repo, &["checkout", "-q", "epic/x"]);
    env.git_ok(&repo, &["merge", "-q", "--no-ff", "-m", "merge wu", "wu/x/one"]);
    env.git_ok(&repo, &["push", "-q", "origin", "epic/x"]);
    assert!(env.calls().contains("xtask verify-dod range"), "updated epic push must range-check");
}
