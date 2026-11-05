#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

pub const AUTHOR_EMAIL: &str = "parity@test.invalid";

pub fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub fn executable(path: &Path, text: &str) {
    write(path, text);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn said(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn inherited_from_the_outer_run(key: &str) -> bool {
    key.starts_with("GIT_")
        || key.starts_with("NEXTEST")
        || key.starts_with("TRAWLER_")
        || (key.starts_with("CARGO_") && key != "CARGO_HOME")
}

pub fn hermetic(program: impl AsRef<Path>, dir: &Path) -> Command {
    let mut cmd = Command::new(program.as_ref());
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy();
        if inherited_from_the_outer_run(&key) {
            cmd.env_remove(key.as_ref());
        }
    }
    cmd.current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "parity")
        .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", "parity")
        .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL);
    cmd
}

pub fn git(dir: &Path, args: &[&str]) -> Output {
    let out = hermetic("git", dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", said(&out));
    out
}
