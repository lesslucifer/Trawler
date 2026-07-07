use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gate::short;
use crate::git::worktree_key;

// A local, per-checkout build shortcut - deliberately NOT a git note. Gate 0
// (fmt/clippy/nextest) proves the working tree green; when the tree is
// byte-identical to the last proven-green state we skip the re-run. The cache
// lives under target/ so `cargo clean` wipes it alongside the incremental build
// state it shortcuts, and it never syncs anywhere: another machine has not run
// this build. It stores the worktree key it vouches for, never a bare verdict,
// so it can never report green for a tree it did not verify (DEC-2's
// recompute-don't-trust, applied to a cache instead of a receipt).
fn cache_path(repo: &Path) -> PathBuf {
    repo.join("target").join(".gate0-green")
}

fn cached_key(repo: &Path) -> Option<String> {
    std::fs::read_to_string(cache_path(repo)).ok().map(|s| s.trim().to_string())
}

fn store_key(repo: &Path, key: &str) -> Result<(), String> {
    let path = cache_path(repo);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, key).map_err(|e| format!("write {}: {e}", path.display()))
}

fn run(repo: &Path, program: &str, args: &[&str]) -> Result<bool, String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(repo)
        .status()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    Ok(status.success())
}

fn nextest_available() -> bool {
    Command::new("cargo").args(["nextest", "--version"]).output().is_ok_and(|o| o.status.success())
}

fn run_checks(repo: &Path) -> Result<bool, String> {
    if !run(repo, "cargo", &["fmt", "--all", "--", "--check"])? {
        return Ok(false);
    }
    if !run(repo, "cargo", &["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"])?
    {
        return Ok(false);
    }
    let tests = if nextest_available() {
        run(repo, "cargo", &["nextest", "run", "--all"])?
    } else {
        run(repo, "cargo", &["test", "--all"])?
    };
    Ok(tests)
}

pub fn gate0(repo: &Path) -> Result<(i32, String), String> {
    let key = worktree_key(repo)?;
    if cached_key(repo).as_deref() == Some(key.as_str()) {
        return Ok((
            0,
            format!("gate0 skipped - working tree unchanged since last green ({})", short(&key)),
        ));
    }
    if run_checks(repo)? {
        store_key(repo, &key)?;
        Ok((0, format!("gate0 green - fmt+clippy+tests passed, cached ({})", short(&key))))
    } else {
        Ok((1, "gate0 red - fmt/clippy/tests failed; not cached".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_path, cached_key, store_key};
    use tempfile::TempDir;

    fn repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[test]
    fn absent_cache_reads_none() {
        let (_d, repo) = repo();
        assert_eq!(cached_key(&repo), None);
    }

    #[test]
    fn stored_key_round_trips_and_lives_under_target() {
        let (_d, repo) = repo();
        store_key(&repo, "abc123").unwrap();
        assert_eq!(cached_key(&repo).as_deref(), Some("abc123"));
        assert!(cache_path(&repo).starts_with(repo.join("target")));
    }

    #[test]
    fn a_different_key_is_a_miss() {
        let (_d, repo) = repo();
        store_key(&repo, "green-key").unwrap();
        assert_ne!(cached_key(&repo).as_deref(), Some("other-key"));
    }
}
