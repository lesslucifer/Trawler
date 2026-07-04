#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.invalid")
        .status()
        .expect("failed to spawn git");
    assert!(status.success(), "git {args:?} failed");
}

fn install_post_checkout_hook(repo: &Path) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(".cargo-husky/hooks/post-checkout");
    let hooks_dir = repo.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).expect("failed to create hooks dir");
    let dest = hooks_dir.join("post-checkout");
    fs::copy(&src, &dest).expect("failed to copy post-checkout hook");
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).expect("failed to chmod hook");
}

#[test]
fn worktree_add_copies_symlinks_and_runs_commands_from_gitworktreecopy() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    git(&repo, &["init", "-q", "-b", "main"]);
    install_post_checkout_hook(&repo);

    fs::write(
        repo.join(".gitworktreecopy"),
        "# comment line\n\n**/*.env*\nca/*\n~> target\n! touch hook-cmd-ran\n",
    )
    .unwrap();
    fs::write(repo.join("tracked.txt"), "tracked").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    fs::write(repo.join(".env"), "SECRET=1").unwrap();
    fs::create_dir(repo.join("sub")).unwrap();
    fs::write(repo.join("sub/.env.local"), "LOCAL=1").unwrap();
    fs::create_dir(repo.join("ca")).unwrap();
    fs::write(repo.join("ca/trawler-ca.pem"), "pem").unwrap();
    fs::write(repo.join("ca/trawler-ca.key"), "key").unwrap();
    fs::create_dir(repo.join("target")).unwrap();
    fs::write(repo.join("target/artifact"), "bin").unwrap();

    let wt = tmp.path().join("wt");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feature"]);

    assert_eq!(fs::read_to_string(wt.join(".env")).unwrap(), "SECRET=1");
    assert!(!wt.join(".env").symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(wt.join("sub/.env.local")).unwrap(), "LOCAL=1");

    assert_eq!(fs::read_to_string(wt.join("ca/trawler-ca.pem")).unwrap(), "pem");
    assert_eq!(fs::read_to_string(wt.join("ca/trawler-ca.key")).unwrap(), "key");

    let target = wt.join("target");
    assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::canonicalize(&target).unwrap(), fs::canonicalize(repo.join("target")).unwrap());
    assert_eq!(fs::read_to_string(target.join("artifact")).unwrap(), "bin");

    assert!(wt.join("hook-cmd-ran").exists());
}

#[test]
fn existing_files_in_new_worktree_are_never_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();

    git(&repo, &["init", "-q", "-b", "main"]);
    install_post_checkout_hook(&repo);

    fs::write(repo.join(".gitworktreecopy"), "tracked.env\n").unwrap();
    fs::write(repo.join("tracked.env"), "FROM_SOURCE=1").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    fs::write(repo.join("tracked.env"), "SOURCE_LOCAL_EDIT=1").unwrap();

    let wt = tmp.path().join("wt");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feature"]);

    assert_eq!(fs::read_to_string(wt.join("tracked.env")).unwrap(), "FROM_SOURCE=1");
}
