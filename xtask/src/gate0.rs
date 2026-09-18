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
/// Where the green proof lives, so a caller can see what a skip is standing on.
pub fn cache_path(repo: &Path) -> PathBuf {
    repo.join("target").join(".gate0-green")
}

/// Where the fmt+clippy proof lives — the weaker sibling of [`cache_path`],
/// which the commit hook can afford to demand on every commit. Two files, not
/// one with a level in it: a marker means "this content passed this check set",
/// and collapsing them would either let a lint-only pass answer for tests that
/// never ran, or make the hook pay for the suite.
pub fn lint_cache_path(repo: &Path) -> PathBuf {
    repo.join("target").join(".lint-green")
}

fn cached_key_at(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn store_key_at(path: &Path, key: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, key).map_err(|e| format!("write {}: {e}", path.display()))
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

// `--workspace` is the load-bearing flag, not boilerplate: the repo root is both
// the workspace and a package, so plain `cargo clippy` lints only `trawler` - and
// every gate, hook and design check lives in the `xtask` member, which then goes
// unlinted by the very gate that proves the tree green. `fmt --all` and
// `nextest --all` are already workspace-wide; this makes the three agree.
const CLIPPY: &[&str] =
    &["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"];

fn run_lints(repo: &Path) -> Result<bool, String> {
    if !run(repo, "cargo", &["fmt", "--all", "--", "--check"])? {
        return Ok(false);
    }
    run(repo, "cargo", CLIPPY)
}

fn run_checks(repo: &Path) -> Result<bool, String> {
    if !run_lints(repo)? {
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
    if cached_key_at(&cache_path(repo)).as_deref() == Some(key.as_str()) {
        return Ok((
            0,
            format!("gate0 skipped - working tree unchanged since last green ({})", short(&key)),
        ));
    }
    if run_checks(repo)? {
        store_key_at(&cache_path(repo), &key)?;
        Ok((0, format!("gate0 green - fmt+clippy+tests passed, cached ({})", short(&key))))
    } else {
        Ok((1, "gate0 red - fmt/clippy/tests failed; not cached".to_string()))
    }
}

/// Which stored proof vouches for `key` at the lint level. A full Gate 0 green
/// counts because it is written only once fmt and clippy have both passed, so it
/// is strictly stronger evidence — the implication never runs the other way, and
/// [`gate0`] reads its own marker alone.
fn lint_proof(repo: &Path, key: &str) -> Option<&'static str> {
    let vouches = |path: PathBuf| cached_key_at(&path).as_deref() == Some(key);
    if vouches(lint_cache_path(repo)) {
        Some("the last fmt+clippy green")
    } else if vouches(cache_path(repo)) {
        Some("the last full Gate 0 green")
    } else {
        None
    }
}

/// The commit hook's gate: fmt + clippy, skipped when this exact tree content
/// has already passed them. The hook used to run the pair blind on every commit
/// (DEC-54).
pub fn lint(repo: &Path) -> Result<(i32, String), String> {
    let key = worktree_key(repo)?;
    if let Some(proof) = lint_proof(repo, &key) {
        return Ok((
            0,
            format!("lint skipped - working tree unchanged since {proof} ({})", short(&key)),
        ));
    }
    if run_lints(repo)? {
        store_key_at(&lint_cache_path(repo), &key)?;
        Ok((0, format!("lint green - fmt+clippy passed, cached ({})", short(&key))))
    } else {
        Ok((1, "lint red - fmt/clippy failed; not cached".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_path, cached_key_at, lint_cache_path, lint_proof, store_key_at, CLIPPY};
    use tempfile::TempDir;

    // The gate's own implementation is a workspace member, so a package-scoped
    // lint would exempt exactly the code that enforces everything else. The
    // pre-commit hook now delegates here, so this is the only place the scoping
    // is stated.
    #[test]
    fn clippy_lints_every_workspace_member() {
        assert!(
            CLIPPY.contains(&"--workspace"),
            "without --workspace the root package is linted and xtask is not: {CLIPPY:?}"
        );
    }

    fn repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[test]
    fn absent_cache_reads_none() {
        let (_d, repo) = repo();
        assert_eq!(cached_key_at(&cache_path(&repo)), None);
    }

    #[test]
    fn stored_key_round_trips_and_lives_under_target() {
        let (_d, repo) = repo();
        store_key_at(&cache_path(&repo), "abc123").unwrap();
        assert_eq!(cached_key_at(&cache_path(&repo)).as_deref(), Some("abc123"));
        assert!(cache_path(&repo).starts_with(repo.join("target")));
    }

    #[test]
    fn a_different_key_is_a_miss() {
        let (_d, repo) = repo();
        store_key_at(&cache_path(&repo), "green-key").unwrap();
        assert_ne!(cached_key_at(&cache_path(&repo)).as_deref(), Some("other-key"));
    }

    #[test]
    fn the_lint_marker_round_trips_and_lives_under_target() {
        let (_d, repo) = repo();
        store_key_at(&lint_cache_path(&repo), "lint-key").unwrap();
        assert_eq!(cached_key_at(&lint_cache_path(&repo)).as_deref(), Some("lint-key"));
        assert!(lint_cache_path(&repo).starts_with(repo.join("target")));
    }

    #[test]
    fn the_two_markers_are_separate_files() {
        let (_d, repo) = repo();
        assert_ne!(cache_path(&repo), lint_cache_path(&repo));
    }

    #[test]
    fn no_marker_is_no_proof() {
        let (_d, repo) = repo();
        assert_eq!(lint_proof(&repo, "key"), None);
    }

    #[test]
    fn the_lint_marker_proves_the_lint_level() {
        let (_d, repo) = repo();
        store_key_at(&lint_cache_path(&repo), "key").unwrap();
        assert!(lint_proof(&repo, "key").is_some());
    }

    // Gate 0 caches only after fmt and clippy have both passed, so its marker is
    // strictly stronger evidence at this level - the preflight's run must not be
    // paid for a second time by the next commit.
    #[test]
    fn a_full_gate0_green_proves_the_lint_level_too() {
        let (_d, repo) = repo();
        store_key_at(&cache_path(&repo), "key").unwrap();
        assert!(lint_proof(&repo, "key").is_some());
    }

    // The load-bearing direction: fmt+clippy green says nothing about the suite,
    // so `gate0` reads its own marker and never this one.
    #[test]
    fn a_lint_green_does_not_prove_gate0() {
        let (_d, repo) = repo();
        store_key_at(&lint_cache_path(&repo), "key").unwrap();
        assert_eq!(
            cached_key_at(&cache_path(&repo)),
            None,
            "gate0 must see no proof from a lint-only marker"
        );
    }

    #[test]
    fn a_marker_holding_another_key_proves_nothing() {
        let (_d, repo) = repo();
        store_key_at(&lint_cache_path(&repo), "other").unwrap();
        store_key_at(&cache_path(&repo), "other").unwrap();
        assert_eq!(lint_proof(&repo, "key"), None);
    }

    #[test]
    fn the_proof_names_which_marker_answered() {
        let (_d, repo) = repo();
        store_key_at(&cache_path(&repo), "key").unwrap();
        assert!(lint_proof(&repo, "key").unwrap().contains("Gate 0"));
        store_key_at(&lint_cache_path(&repo), "key").unwrap();
        assert!(lint_proof(&repo, "key").unwrap().contains("fmt+clippy"));
    }
}
