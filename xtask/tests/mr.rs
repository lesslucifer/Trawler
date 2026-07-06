//! E-NMWF.W1 — the authoriser record & writer commands (`cargo xtask mr ...`).
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xtask::mr;

struct Repo {
    _dir: TempDir,
    path: PathBuf,
}

impl Repo {
    fn new(allowed: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Self { _dir: dir, path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write(".mr-allowed", allowed);
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "init"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        xtask::git::git(&self.path, args).unwrap_or_else(|e| panic!("{e}"))
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    fn p(&self) -> &Path {
        &self.path
    }
}

const ALLOWED: &str = "email: a@b.co\nusername: vu\n";

#[test]
fn confirm_writes_a_well_formed_note_for_a_listed_identity() {
    let repo = Repo::new(ALLOWED);
    let (code, _) = mr::run(&["confirm", "epic", "HEAD", "--as", "a@b.co"], repo.p()).unwrap();
    assert_eq!(code, 0);

    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("schema: mr/v1"), "{body}");
    assert!(body.contains("level: epic"), "{body}");
    assert!(body.contains("tier: email"), "{body}");
    assert!(body.contains("signer: a@b.co"), "{body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_maps_wu_level_and_records_the_username_tier() {
    let repo = Repo::new(ALLOWED);
    let (code, _) = mr::run(&["confirm", "wu", "HEAD", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0);
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("level: working-unit"), "{body}");
    assert!(body.contains("tier: username"), "{body}");
}

#[test]
fn unlisted_identity_is_refused_and_writes_nothing() {
    let repo = Repo::new(ALLOWED);
    let (code, out) =
        mr::run(&["confirm", "epic", "HEAD", "--as", "ghost@x.co"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("not listed in .mr-allowed"));
    assert!(out.contains("MUST NOT self-authorise"));
    assert!(!mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn commit_appends_the_reviewed_by_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, _) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0);

    let body = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(mr::trailer_of(&body), Some(("username".into(), "vu".into())));
}

#[test]
fn commit_with_an_empty_message_is_refused() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "   ", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("empty"));
}

fn write_msg(repo: &Repo, body: &str) -> std::path::PathBuf {
    let f = repo.path.join("COMMIT_MSG");
    std::fs::write(&f, body).unwrap();
    f
}

#[test]
fn commit_msg_hook_ignores_a_non_doc_commit() {
    let repo = Repo::new(ALLOWED);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "feat: no docs here\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn commit_msg_hook_refuses_a_doc_commit_without_a_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n");
    let (code, out) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("no valid Reviewed-By trailer"));
    assert!(out.contains("MUST NOT self-authorise"));
}

#[test]
fn commit_msg_hook_accepts_a_doc_commit_with_a_listed_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n\nReviewed-By: email a@b.co\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn commit_msg_hook_refuses_a_doc_commit_with_an_unlisted_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n\nReviewed-By: email ghost@x.co\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn commit_msg_hook_sees_a_doc_renamed_out_of_docs() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/movable.html", "content that is long enough to be a similar rename\n");
    repo.write("src/keep.rs", "// keep\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add doc"]);
    repo.git(&["mv", "docs/movable.html", "src/moved.rs"]);
    let msg = write_msg(&repo, "move doc out, no trailer\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1, "a rename out of docs/ must still require a trailer");
}

#[test]
fn commit_with_unlisted_identity_is_refused() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) =
        mr::run(&["commit", "-m", "docs: change", "--as", "ghost"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("not listed in .mr-allowed"));
}
