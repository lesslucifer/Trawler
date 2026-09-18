#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

// The cache key's contract. E-GATEFAST.W2 pinned the invalidating half: the key
// covers the FULL working-tree state, so it changes on anything a build would
// see - a stale "green" must never yield a skip. E-SIMPDF.W3 (DEC-54) added the
// other half: the key is the tree's CONTENT, so a commit that changes no content
// does not invalidate it, which is what made the pre-commit hook affordable.
// These pin both; the marker files' round-trip is unit-tested in xtask::gate0.

use tempfile::TempDir;
use xtask::git::worktree_key;

struct Repo {
    _dir: TempDir,
    path: std::path::PathBuf,
}

impl Repo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Self { _dir: dir, path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write("src/lib.rs", "pub fn base() {}\n");
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "init"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        xtask::git::git(&self.path, args).unwrap_or_else(|e| panic!("{e}"));
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    fn key(&self) -> String {
        worktree_key(&self.path).unwrap()
    }

    fn remove(&self, rel: &str) {
        std::fs::remove_file(self.path.join(rel)).unwrap();
    }
}

#[test]
fn clean_tree_is_stable_across_calls() {
    let repo = Repo::new();
    assert_eq!(repo.key(), repo.key(), "identical tree must hash identically - the skip condition");
}

#[test]
fn unstaged_edit_changes_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.write("src/lib.rs", "pub fn base() {}\npub fn more() {}\n");
    assert_ne!(before, repo.key(), "any tracked edit must re-run Gate 0");
}

#[test]
fn a_whitespace_edit_changes_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.write("src/lib.rs", "pub fn base() {}\n\n");
    assert_ne!(before, repo.key(), "even whitespace must invalidate - the plan's explicit case");
}

#[test]
fn an_untracked_source_file_changes_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.write("src/new.rs", "pub fn added() {}\n");
    assert_ne!(
        before,
        repo.key(),
        "an untracked build input must invalidate - git stash create misses this"
    );
}

#[test]
fn a_gitignored_file_does_not_change_the_key() {
    let repo = Repo::new();
    repo.write(".gitignore", "/target\n");
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-q", "-m", "ignore target"]);
    let before = repo.key();
    repo.write("target/.gate0-green", "some-key");
    repo.write("target/debug-junk", "artifact");
    assert_eq!(
        before,
        repo.key(),
        "the cache's own file (and build artifacts) must not invalidate it"
    );
}

// The defect DEC-54 fixes, and the reason the pre-commit hook can now ask the
// cache at all: the old key hashed HEAD, so committing moved it even though the
// bytes every compiler reads were identical - and a Working Unit is many commits.
#[test]
fn a_commit_that_changes_no_content_keeps_the_key() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "pub fn base() {}\npub fn more() {}\n");
    repo.git(&["add", "-A"]);
    let staged = repo.key();
    repo.git(&["commit", "-q", "-m", "land the edit"]);
    assert_eq!(
        staged,
        repo.key(),
        "the content is identical either side of the commit - only HEAD moved"
    );
}

#[test]
fn checking_out_an_identical_tree_on_another_branch_keeps_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.git(&["checkout", "-q", "-b", "other"]);
    assert_eq!(before, repo.key(), "the same content is the same tree, whatever the branch");
}

// `git diff HEAD` used to cover these two for free; reading the worktree does
// not, so each is recorded as itself or it goes invisible.
#[test]
fn deleting_a_tracked_file_changes_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.remove("src/lib.rs");
    assert_ne!(before, repo.key(), "a deleted build input must re-run the checks");
}

#[test]
fn staging_the_deletion_of_a_tracked_file_changes_the_key() {
    let repo = Repo::new();
    let before = repo.key();
    repo.remove("src/lib.rs");
    repo.git(&["add", "-A"]);
    assert_ne!(before, repo.key(), "the path leaves the index too - still a changed tree");
}

#[test]
#[cfg(unix)]
fn flipping_the_executable_bit_changes_the_key() {
    use std::os::unix::fs::PermissionsExt as _;
    let repo = Repo::new();
    repo.write("run.sh", "#!/bin/sh\necho hi\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add a script"]);
    let before = repo.key();
    std::fs::set_permissions(repo.path.join("run.sh"), std::fs::Permissions::from_mode(0o755))
        .unwrap();
    assert_ne!(before, repo.key(), "git tracks this bit, and the suite runs on this key");
}

// An empty file and a missing one are different trees; a delimiter-only scheme
// would hash them alike.
#[test]
fn an_empty_tracked_file_is_not_a_deleted_one() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "");
    let emptied = repo.key();
    repo.remove("src/lib.rs");
    assert_ne!(emptied, repo.key());
}

// The key is recomputed on every hook run, so an unborn HEAD must not be an
// error the way `rev-parse HEAD` made it one.
#[test]
fn a_repo_with_no_commits_still_has_a_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    xtask::git::git(&path, &["init", "-q", "-b", "main"]).unwrap();
    std::fs::write(path.join("a.rs"), "pub fn a() {}\n").unwrap();
    assert!(worktree_key(&path).is_ok(), "the hook runs before the first commit too");
}
