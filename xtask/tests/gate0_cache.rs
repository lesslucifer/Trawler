#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

// W2 (E-GATEFAST.W2) contract: the incremental-Gate-0 cache keys on the FULL
// working-tree state, so it invalidates on any change a build would see - the
// load-bearing invariant (a stale "green" must never yield a skip). These pin
// the key; the cache file round-trip is unit-tested in xtask::gate0.

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
