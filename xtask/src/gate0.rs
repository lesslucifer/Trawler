use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gate::short;
use crate::git::{git, git_outcome, worktree_key};
use crate::launch::{self, Launch};
use crate::target::{self, Workspace};

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
///
/// `pub` so `xtask/tests/suite/lint_spawn.rs` can name the marker rather than restate
/// its path — a second copy of the name would go stale silently, leaving the
/// test asserting the absence of a file nothing writes any more.
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

// Git exports GIT_INDEX_FILE, GIT_REFLOG_ACTION and friends into the hooks that
// call these checks; a suite inheriting them reads the hook's merge as its own.
fn check_command(
    repo: &Path,
    program: &str,
    args: &[&str],
    inherited: impl IntoIterator<Item = OsString>,
) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(repo);
    for key in inherited {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd
}

fn inherited_names() -> impl Iterator<Item = OsString> {
    std::env::vars_os().map(|(key, _)| key)
}

fn run(repo: &Path, program: &str, args: &[&str]) -> Result<bool, String> {
    let status = check_command(repo, program, args, inherited_names())
        .status()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    Ok(status.success())
}

fn nextest_available(repo: &Path) -> bool {
    check_command(repo, "cargo", &["nextest", "--version"], inherited_names())
        .output()
        .is_ok_and(|o| o.status.success())
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
    run(repo, "cargo", &["nextest", "run", "--all"])
}

/// Exit code of a refusal about the machine rather than the tree, so a merge or
/// push names the machine's fix and not the tree's.
pub const MACHINE_REFUSED: i32 = 2;

pub const NEXTEST_MISSING: &str =
    "gate0 refused - cargo-nextest is not installed; the suite runs only through it.
  cargo install cargo-nextest --locked";

pub fn slow_launch_refusal() -> String {
    format!(
        "gate0 refused - a freshly linked binary took over {}s to first launch: the OS is assessing each new test binary, \
which costs minutes per suite and fails tests on their deadlines.
  macOS: System Settings > Privacy & Security > Developer Tools - enable the terminal or editor running this, then retry
  or accept the wait: {}=1",
        launch::THRESHOLD.as_secs(),
        launch::ACCEPT_SLOW
    )
}

fn machine_refusal(repo: &Path) -> Result<Option<String>, String> {
    if !nextest_available(repo) {
        return Ok(Some(NEXTEST_MISSING.to_string()));
    }
    if launch::slow_accepted() {
        return Ok(None);
    }
    Ok((launch::machine()? == Launch::Slow).then(slow_launch_refusal))
}

pub fn gate0(repo: &Path) -> Result<(i32, String), String> {
    let key = worktree_key(repo)?;
    if cached_key_at(&cache_path(repo)).as_deref() == Some(key.as_str()) {
        return Ok((
            0,
            format!("gate0 skipped - working tree unchanged since last green ({})", short(&key)),
        ));
    }
    if let Some(refusal) = machine_refusal(repo)? {
        return Ok((MACHINE_REFUSED, refusal));
    }
    if run_checks(repo)? {
        store_key_at(&cache_path(repo), &key)?;
        Ok((0, format!("gate0 green - fmt+clippy+tests passed, cached ({})", short(&key))))
    } else {
        Ok((1, "gate0 red - fmt/clippy/tests failed; not cached".to_string()))
    }
}

/// What a merge into `main` stands on: Gate 0 over the merged working tree, so
/// the merge commit is refused before it exists when the combination is red.
pub fn merge(repo: &Path) -> Result<(i32, String), String> {
    gate0(repo).map(merge_verdict)
}

pub const MERGE_REFUSED: &str = "merge refused - the merged tree fails Gate 0; main is unchanged.
  git merge --abort
then back-merge main into the epic, fix the combination there, and merge again";

fn merge_verdict((code, said): (i32, String)) -> (i32, String) {
    if code == 0 || code == MACHINE_REFUSED {
        (code, said)
    } else {
        (code, format!("{said}\n{MERGE_REFUSED}"))
    }
}

const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

/// The shas a push would publish that carry a build tree: git's pre-push stdin
/// minus deletions and notes refs.
fn pushed_shas(refspecs: &str) -> Vec<&str> {
    refspecs
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            match fields.as_slice() {
                [_, local_sha, remote_ref, _]
                    if *local_sha != ZERO_SHA && !remote_ref.starts_with("refs/notes/") =>
                {
                    Some(*local_sha)
                }
                _ => None,
            }
        })
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PushProofs {
    checkout: bool,
    detached: Vec<String>,
}

/// `pushed` is `(sha, tree)`; `clean_head_tree` is `HEAD`'s tree only when the
/// worktree holds exactly it, since only then does this checkout's marker speak
/// for the pushed content.
fn split_proofs(pushed: &[(String, String)], clean_head_tree: Option<&str>) -> PushProofs {
    let mut proofs = PushProofs::default();
    let mut seen = std::collections::BTreeSet::new();
    for (sha, tree) in pushed {
        if Some(tree.as_str()) == clean_head_tree {
            proofs.checkout = true;
        } else if seen.insert(tree.as_str()) {
            proofs.detached.push(sha.clone());
        }
    }
    proofs
}

fn clean_head_tree(repo: &Path) -> Result<Option<String>, String> {
    let dirty = git(repo, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !dirty.trim().is_empty() {
        return Ok(None);
    }
    git(repo, &["rev-parse", "HEAD^{tree}"]).map(|tree| Some(tree.trim().to_string()))
}

fn scratch_path(repo: &Path, sha: &str) -> PathBuf {
    repo.join("target").join("push-proof").join(short(sha))
}

fn discard_scratch(repo: &Path, scratch: &Path) -> Result<(), String> {
    let scratch_arg = scratch.to_string_lossy();
    if scratch.exists() {
        git_outcome(repo, &["worktree", "remove", "--force", &scratch_arg], &[])?;
        if scratch.exists() {
            std::fs::remove_dir_all(scratch)
                .map_err(|e| format!("remove {}: {e}", scratch.display()))?;
        }
    }
    git(repo, &["worktree", "prune"]).map(drop)
}

const UI_DIR: &str = "ui";
const PNPM_INSTALL: &[&str] = &["install", "--frozen-lockfile", "--prefer-offline"];

// The app's build script embeds the front-end bundle and refuses without the
// UI's packages, which a fresh worktree never has.
fn install_ui_packages(checkout: &Path) -> Result<bool, String> {
    let ui = checkout.join(UI_DIR);
    if !ui.join("package.json").is_file() {
        return Ok(true);
    }
    run(&ui, "pnpm", PNPM_INSTALL)
}

// The tracked post-checkout seeds by walking the process tree for a source
// checkout; skipping it and seeding from `repo` directly names the source.
fn prove_detached(repo: &Path, sha: &str) -> Result<(i32, String), String> {
    let scratch = scratch_path(repo, sha);
    discard_scratch(repo, &scratch)?;
    let scratch_arg = scratch.to_string_lossy();
    git(
        repo,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "worktree",
            "add",
            "--quiet",
            "--detach",
            &scratch_arg,
            sha,
        ],
    )?;
    if let Ok(own) = Workspace::of(&scratch) {
        let _ = target::seed(&repo.join("target"), &scratch.join("target"), &own);
    }
    let verdict = match install_ui_packages(&scratch) {
        Ok(true) => gate0(&scratch),
        Ok(false) => Ok((1, "gate0 red - pnpm install failed in the scratch worktree".to_string())),
        Err(e) => Err(e),
    };
    discard_scratch(repo, &scratch)?;
    verdict.map(|(code, said)| (code, format!("pushed {}: {said}", short(sha))))
}

pub const PUSH_REFUSED: &str = "push refused - a pushed tree fails Gate 0; nothing was sent.
  fix it on the branch, commit, and push again";

fn push_verdict((code, said): (i32, String)) -> (i32, String) {
    if code == 0 || code == MACHINE_REFUSED {
        (code, said)
    } else {
        (code, format!("{said}\n{PUSH_REFUSED}"))
    }
}

/// The pre-push gate: every pushed tree is proven before it leaves, by this
/// checkout's marker when the push is its clean `HEAD`, else in a scratch
/// worktree at the pushed sha.
pub fn push(repo: &Path, refspecs: &str) -> Result<(i32, String), String> {
    let mut pushed = Vec::new();
    for sha in pushed_shas(refspecs) {
        let tree = git(repo, &["rev-parse", &format!("{sha}^{{tree}}")])?;
        pushed.push((sha.to_string(), tree.trim().to_string()));
    }
    if pushed.is_empty() {
        return Ok((0, "gate0 push - nothing to prove: only deletions or notes".to_string()));
    }
    let proofs = split_proofs(&pushed, clean_head_tree(repo)?.as_deref());
    let mut said = Vec::new();
    let runs = proofs.checkout.then_some(None).into_iter().chain(proofs.detached.iter().map(Some));
    for detached in runs {
        let (code, verdict) = match detached {
            None => gate0(repo)?,
            Some(sha) => prove_detached(repo, sha)?,
        };
        said.push(verdict);
        if code != 0 {
            return Ok(push_verdict((code, said.join("\n"))));
        }
    }
    Ok((0, said.join("\n")))
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
    use super::{
        cache_path, cached_key_at, check_command, lint_cache_path, lint_proof, merge_verdict,
        push_verdict, pushed_shas, slow_launch_refusal, split_proofs, store_key_at, PushProofs,
        CLIPPY, MACHINE_REFUSED, MERGE_REFUSED, NEXTEST_MISSING, PUSH_REFUSED, ZERO_SHA,
    };
    use std::ffi::{OsStr, OsString};
    use tempfile::TempDir;

    fn removed(cmd: &std::process::Command) -> Vec<&OsStr> {
        cmd.get_envs().filter(|(_, value)| value.is_none()).map(|(key, _)| key).collect()
    }

    #[test]
    fn a_check_runs_without_any_inherited_git_variable() {
        let inherited = ["GIT_INDEX_FILE", "GIT_REFLOG_ACTION", "GIT_EDITOR", "PATH", "HOME"]
            .map(OsString::from);
        let cmd = check_command(std::path::Path::new("."), "cargo", &["test"], inherited);
        let mut gone = removed(&cmd);
        gone.sort();
        assert_eq!(gone, ["GIT_EDITOR", "GIT_INDEX_FILE", "GIT_REFLOG_ACTION"]);
    }

    #[test]
    fn a_red_merge_names_the_abort_and_the_back_merge() {
        let (code, said) = merge_verdict((1, "gate0 red".to_string()));
        assert_eq!(code, 1);
        assert!(said.starts_with("gate0 red"), "the cause stays first: {said}");
        assert!(said.contains("git merge --abort"), "{said}");
        assert!(said.contains("back-merge main into the epic"), "{said}");
    }

    #[test]
    fn a_green_or_skipped_merge_passes_through_unchanged() {
        for said in ["gate0 green - cached (k)", "gate0 skipped - unchanged (k)"] {
            assert_eq!(merge_verdict((0, said.to_string())), (0, said.to_string()));
        }
    }

    #[test]
    fn a_missing_nextest_names_the_install_line() {
        assert!(NEXTEST_MISSING.contains("cargo install cargo-nextest --locked"));
    }

    #[test]
    fn a_slow_launch_names_developer_tools_and_the_override() {
        let said = slow_launch_refusal();
        assert!(said.contains("Developer Tools"), "{said}");
        assert!(said.contains("TRAWLER_ACCEPT_SLOW_LAUNCH=1"), "{said}");
    }

    #[test]
    fn a_machine_refusal_never_blames_the_tree() {
        for verdict in [merge_verdict, push_verdict] {
            let (code, said) = verdict((MACHINE_REFUSED, NEXTEST_MISSING.to_string()));
            assert_eq!(code, MACHINE_REFUSED);
            assert_eq!(said, NEXTEST_MISSING);
            assert!(!said.contains(MERGE_REFUSED) && !said.contains(PUSH_REFUSED));
        }
    }

    #[test]
    fn a_push_proves_heads_and_tags_but_not_deletions_or_notes() {
        let refspecs = format!(
            "refs/heads/epic/x aaa refs/heads/epic/x {ZERO_SHA}\n\n\
             (delete) {ZERO_SHA} refs/heads/gone bbb\n\
             refs/notes/test ccc refs/notes/test ddd\n\
             refs/tags/v1 eee refs/tags/v1 {ZERO_SHA}\n"
        );
        assert_eq!(pushed_shas(&refspecs), ["aaa", "eee"]);
    }

    fn pushed(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(sha, tree)| ((*sha).to_string(), (*tree).to_string())).collect()
    }

    #[test]
    fn the_clean_head_tree_is_proven_by_this_checkout() {
        let proofs = split_proofs(&pushed(&[("s1", "head"), ("s2", "head")]), Some("head"));
        assert_eq!(proofs, PushProofs { checkout: true, detached: vec![] });
    }

    #[test]
    fn a_dirty_worktree_proves_every_pushed_tree_detached() {
        let proofs = split_proofs(&pushed(&[("s1", "head")]), None);
        assert_eq!(proofs, PushProofs { checkout: false, detached: vec!["s1".to_string()] });
    }

    #[test]
    fn a_mixed_push_splits_and_proves_each_distinct_tree_once() {
        let proofs = split_proofs(
            &pushed(&[("s1", "head"), ("s2", "other"), ("s3", "other"), ("s4", "third")]),
            Some("head"),
        );
        assert_eq!(
            proofs,
            PushProofs { checkout: true, detached: vec!["s2".to_string(), "s4".to_string()] }
        );
    }

    #[test]
    fn a_red_push_names_the_refusal_and_a_green_one_passes_through() {
        let (code, said) = push_verdict((1, "gate0 red".to_string()));
        assert_eq!(code, 1);
        assert!(said.starts_with("gate0 red") && said.ends_with(PUSH_REFUSED), "{said}");
        assert_eq!(push_verdict((0, "gate0 green".to_string())), (0, "gate0 green".to_string()));
    }

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
