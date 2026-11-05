//! E-NMWF.W1 — the authoriser record & writer commands (`cargo xtask mr ...`).
//! E-MRSIG.W1 — the key-backed signature-verify predicates.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

    /// As [`Repo::new`], stopping before the initial commit: `HEAD` is unborn,
    /// the state a first commit moves away from.
    fn new_unborn(allowed: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Self { _dir: dir, path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write(".mr-allowed", allowed);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        xtask::git::git(&self.path, args).unwrap_or_else(|e| panic!("{e}"))
    }

    /// [`Repo::git`] without the panic — for asking whether a ref resolves.
    fn git_raw(&self, args: &[&str]) -> Result<String, String> {
        xtask::git::git(&self.path, args)
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    fn p(&self) -> &Path {
        &self.path
    }

    /// Generate a software ed25519 key under the repo; return its public-key line.
    fn keygen(&self, name: &str) -> String {
        let key = self.path.join(name);
        assert!(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        std::fs::read_to_string(self.path.join(format!("{name}.pub"))).unwrap().trim().to_string()
    }

    fn ssh_sign(&self, key_name: &str, namespace: &str, data: &[u8]) -> String {
        let mut child = Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", namespace, "-f"])
            .arg(self.path.join(key_name))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(data).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "ssh-keygen sign failed");
        String::from_utf8(out.stdout).unwrap()
    }
}

const ALLOWED: &str = "email: a@b.co\nusername: vu\n";

/// Write a result report the confirm can attest; returns its repo-relative path.
fn result_report(repo: &Repo) -> &'static str {
    repo.write("docs/work/w-result.html", "<html>result</html>\n");
    "docs/work/w-result.html"
}

/// The tree-settled notes a confirm now demands: `test/v2` + `doc/v1` (the tree
/// has stopped moving) and a `cr/v3` PASS bound to this tip's diff.
fn settle(repo: &Repo, tip: &str) {
    settle_against(repo, tip, "main");
}

/// As [`settle`], for a tip whose confirm names an explicit `--target`.
fn settle_against(repo: &Repo, tip: &str, target: &str) {
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", tip]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", tip]);
    cr_note(repo, tip, "PASS", &diff_hash(repo, tip, target));
}

/// The `base..tip` diff hash a `cr/v3` note must carry to count as bound.
fn diff_hash(repo: &Repo, tip: &str, target: &str) -> String {
    let sha = repo.git(&["rev-parse", tip]);
    let base = repo.git(&["merge-base", target, &sha]);
    xtask::git::diff_sha256(repo.p(), &base, &sha).unwrap()
}

fn cr_note(repo: &Repo, tip: &str, verdict: &str, diff: &str) {
    let sha = repo.git(&["rev-parse", tip]);
    let body = format!(
        "schema: cr/v3\nbase: {sha}\ndiff_sha256: {diff}\nverdict: {verdict}\ndimensions: []\n"
    );
    repo.git(&["notes", "--ref=cr", "add", "-f", "-m", &body, tip]);
}

/// The `cr/v4` note the review writes now: bound to the code paths the diff
/// changes, not to the whole diff.
fn cr_v4_note(repo: &Repo, tip: &str, verdict: &str, target: &str) {
    let sha = repo.git(&["rev-parse", tip]);
    let base = repo.git(&["merge-base", target, &sha]);
    let body = format!(
        "schema: cr/v4\nbase: {base}\ndiff_sha256: {}\nverdict: {verdict}\ndimensions: []\n",
        code_hash(repo, tip, target)
    );
    repo.git(&["notes", "--ref=cr", "add", "-f", "-m", &body, tip]);
}

/// The hash of the code half of `base..tip` — what a `cr/v4` note binds.
fn code_hash(repo: &Repo, tip: &str, target: &str) -> String {
    let sha = repo.git(&["rev-parse", tip]);
    let base = repo.git(&["merge-base", target, &sha]);
    let paths: Vec<String> = xtask::git::diff_names(repo.p(), &base, &sha)
        .unwrap()
        .into_iter()
        .filter(|p| xtask::notes::is_code(p))
        .collect();
    xtask::git::diff_sha256_paths(repo.p(), &base, &sha, &paths).unwrap()
}

/// A tip on a branch off `main` carrying one code file, settled but for its
/// review note.
fn code_tip(repo: &Repo) -> &'static str {
    repo.git(&["checkout", "-q", "-b", "wu/x/one"]);
    repo.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "code"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    "wu/x/one"
}

// E-SIMPWF.W4 — the confirm reads a review note the way the merge gate reads it,
// or it refuses a tip the gate would green. A `cr/v4` note binds the code paths,
// so a doc commit landing after the PASS leaves the binding intact.
#[test]
fn a_doc_commit_after_a_cr_v4_pass_still_signs() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    code_tip(&repo);
    cr_v4_note(&repo, "HEAD", "PASS", "main");
    repo.write("docs/work/w-notes.html", "<html>notes</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "docs"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    repo.git(&["notes", "--ref=cr", "copy", "-f", "HEAD^", "HEAD"]);

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
}

#[test]
fn a_code_commit_after_a_cr_v4_pass_is_refused() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    code_tip(&repo);
    cr_v4_note(&repo, "HEAD", "PASS", "main");
    repo.write("src/other.rs", "pub fn two() -> u8 { 2 }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "more code"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    repo.git(&["notes", "--ref=cr", "copy", "-f", "HEAD^", "HEAD"]);

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("judged a different diff"), "{out}");
}

// A `cr/v4` note over a diff that changes no code judges no code path, so the
// confirm holds the signature on nothing it could compare.
#[test]
fn a_code_free_tip_carrying_a_cr_v4_note_still_signs() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    repo.git(&["checkout", "-q", "-b", "wu/x/docs"]);
    repo.write("docs/work/w-notes.html", "<html>notes</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "docs only"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    cr_v4_note(&repo, "HEAD", "PASS", "main");

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
}

// E-ONBOARD.W2 — the Working Unit the design flow produces most: a closed
// design run and no code at all. `is_gated` is true because of the artifact, so
// the docs-only exemption correctly does not apply and the tip owes test/v2,
// doc/v1 and design/v1 — but it changes no code path, so it owes no review
// (DEC-46), exactly as the merge gate reads it. The confirm demanded one anyway,
// which left the tip unsignable and so unmergeable: `8·manual-review` wanted an
// mr/v1 that `mr confirm` refused to write, and no review was entitled to exist.
// Asserted over a real tree, because the bug was in how the confirm derives the
// range rather than in the predicate it feeds.
#[test]
fn a_closed_design_run_with_no_code_signs_without_a_review() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    repo.git(&["checkout", "-q", "-b", "wu/x/design"]);
    repo.write(
        "docs/work/a-run.html",
        "<html><main data-artifact=\"design-run\" data-status=\"archived\">run</main></html>\n",
    );
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "a closed design run, no code"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(
        code, 0,
        "a design-bearing tip that changes no code owes no cr note - the merge gate greens it, \
         so the confirm must too, or the design flow's output can never be merged: {out}"
    );
    assert!(
        repo.git(&["notes", "--ref=mr", "show", "HEAD"]).contains("schema: mr/v1"),
        "the confirm should have written the note it was refusing"
    );
}

// The narrowing has one edge, found by the review of the commit that made it and
// reproduced against a real repo: the gate drops a note whose schema BINDS CODE
// on a codeless diff, but a legacy `cr/v2`/`cr/v3` binds the whole diff and is
// judged whatever the diff touched. Skipping the review arm for every codeless
// design tip therefore signed a tip `verify-dod` still refuses on a FAIL verdict
// — a signature attesting something unmergeable. The confirm now reads exactly
// the notes the gate reads.
#[test]
fn a_legacy_review_note_still_blocks_a_codeless_design_tip() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    repo.git(&["checkout", "-q", "-b", "wu/x/legacy"]);
    repo.write(
        "docs/work/a-run.html",
        "<html><main data-artifact=\"design-run\" data-status=\"archived\">run</main></html>\n",
    );
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "a closed design run, no code"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    // cr/v3: whole-diff-bound, so `reviewed_binding` hands it back even here.
    cr_note(&repo, "HEAD", "FAIL", &diff_hash(&repo, "HEAD", "main"));

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(
        code, 1,
        "a legacy note the merge gate still judges must block the signature: {out}"
    );
    assert!(out.contains("verdict is FAIL"), "{out}");
}

// The narrowing is not a hole: the same tip, once it also carries a code path,
// owes its review again. One test each way, because a predicate that always
// said "no review" would pass the test above and break every real Working Unit.
#[test]
fn a_design_run_that_also_changes_code_still_owes_its_review() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    repo.git(&["checkout", "-q", "-b", "wu/x/design-and-code"]);
    repo.write(
        "docs/work/a-run.html",
        "<html><main data-artifact=\"design-run\" data-status=\"archived\">run</main></html>\n",
    );
    repo.write("src/lib.rs", "pub fn one() -> u8 { 1 }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "a closed run and the code it describes"]);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);

    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--target", "main", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("nobody has reviewed this tip"), "{out}");
}

#[test]
fn confirm_writes_a_well_formed_note_attesting_the_report() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    settle(&repo, "HEAD");
    let (code, _) =
        mr::run(&["confirm", "epic", "HEAD", "--as", "a@b.co", "--report", report], repo.p())
            .unwrap();
    assert_eq!(code, 0);

    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("schema: mr/v1"), "{body}");
    assert!(body.contains("level: epic"), "{body}");
    assert!(body.contains("tier: email"), "{body}");
    assert!(body.contains("signer: a@b.co"), "{body}");
    assert!(body.contains(&format!("report: {report}")), "{body}");
    assert!(body.contains("report_sha256: "), "{body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_maps_wu_level_and_records_the_username_tier() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    settle(&repo, "HEAD");
    let (code, _) =
        mr::run(&["confirm", "wu", "HEAD", "--as", "vu", "--report", report], repo.p()).unwrap();
    assert_eq!(code, 0);
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("level: working-unit"), "{body}");
    assert!(body.contains("tier: username"), "{body}");
}

// E-DFLOW.W11 — a signature binds the tree, so the tree settles first. Each of
// these refusals happens BEFORE the signing prompt: a human is never asked to
// sign a tip that a later fix would orphan.
#[test]
fn confirm_is_refused_while_the_tip_carries_no_notes_at_all() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    let (code, out) =
        mr::run(&["confirm", "wu", "HEAD", "--as", "vu", "--report", report], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("no test/v2 note"), "{out}");
    assert!(out.contains("no doc/v1 note"), "{out}");
    assert!(out.contains("no cr/v5 note"), "{out}");
    assert!(out.contains("tree settles BEFORE"), "{out}");
    assert!(!mr::tip_authorised(repo.p(), "HEAD"), "nothing may be written");
}

#[test]
fn confirm_is_refused_when_the_review_has_not_run_yet() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    repo.git(&["notes", "--ref=test", "add", "-f", "-m", "schema: test/v2", "HEAD"]);
    repo.git(&["notes", "--ref=doc", "add", "-f", "-m", "schema: doc/v1", "HEAD"]);
    let (code, out) =
        mr::run(&["confirm", "wu", "HEAD", "--as", "vu", "--report", report], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("no cr/v5 note"), "{out}");
    assert!(!out.contains("no test/v2"), "the settled notes must not be reported: {out}");
    assert!(!mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_is_refused_while_the_review_verdict_is_fail() {
    let repo = Repo::new(ALLOWED);
    let report = result_report(&repo);
    settle(&repo, "HEAD");
    cr_note(&repo, "HEAD", "FAIL", &diff_hash(&repo, "HEAD", "main"));
    let (code, out) =
        mr::run(&["confirm", "wu", "HEAD", "--as", "vu", "--report", report], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("verdict is FAIL"), "{out}");
    assert!(!mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_is_refused_when_the_review_judged_a_different_diff() {
    let repo = Repo::new(ALLOWED);
    repo.git(&["checkout", "-q", "-b", "wu/e/w"]);
    let report = result_report(&repo);
    // The binding is only read on a gated range: an exempt tip carries no code
    // for a review to have judged, and `verify()` greens it before `5·diff-binding`
    // runs at all.
    repo.write("src/lib.rs", "pub fn added() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the work"]);
    settle(&repo, "HEAD");
    cr_note(&repo, "HEAD", "PASS", &"0".repeat(64));
    let (code, out) = mr::run(
        &["confirm", "wu", "HEAD", "--as", "vu", "--report", report, "--target", "main"],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("judged a different diff"), "{out}");
    assert!(!mr::tip_authorised(repo.p(), "HEAD"));
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
fn commit_msg_hook_skips_merge_commits() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let merge_head = repo.path.join(".git/MERGE_HEAD");
    std::fs::create_dir_all(merge_head.parent().unwrap()).unwrap();
    std::fs::write(&merge_head, format!("{head}\n")).unwrap();

    let msg = write_msg(&repo, "Merge branch x\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0, "a merge commit needs no trailer of its own");
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

// ── E-MRSIG.W1 — key-backed signature verification ──────────────────────────

/// A repo with one code commit past `base`, plus a `key` entry for `signer`.
/// Returns (base, tip, pubkey-line).
fn repo_with_key(signer: &str) -> (Repo, String, String, String) {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");
    repo.write(".mr-allowed", &format!("email: a@b.co\nkey: {signer} {pubkey}\n"));
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add key"]);
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    (repo, base, tip, pubkey)
}

fn write_signed_note(repo: &Repo, level: &str, base: &str, tip: &str, signer: &str, sig: &str) {
    let body = format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\nbase: {base}\ntier: key\nsigner: {signer}\nsig: |\n{}\n",
        sig.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
    );
    xtask::git::write_note(repo.p(), "mr", tip, &body).unwrap();
}

#[test]
fn tip_authorised_accepts_a_correctly_signed_note() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    assert!(mr::tip_authorised(repo.p(), &tip));
}

#[test]
fn tip_authorised_rejects_a_note_whose_diff_changed() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    // Amend the tip → the signed change-hash no longer matches.
    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    write_signed_note(&repo, "epic", &base, &amended, "vu", &sig);
    assert!(!mr::tip_authorised(repo.p(), &amended), "amend must invalidate the signature");
}

#[test]
fn tip_authorised_rejects_a_signature_from_an_unlisted_key() {
    let (repo, base, tip, _) = repo_with_key("vu");
    repo.keygen("other");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("other", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    assert!(!mr::tip_authorised(repo.p(), &tip));
}

#[test]
fn tip_authorised_rejects_a_bare_note_from_an_unlisted_signer() {
    // No sig/base falls back to identity-match (E-NMWF coexistence); an unlisted
    // signer still fails. A listed signer's bare note authorises — the key is an
    // added option in W1, not yet a mandate (mandate lands with W3 enforcement).
    let (repo, _base, tip, _) = repo_with_key("vu");
    let body = "schema: mr/v1\nlevel: epic\ntip: x\ntier: email\nsigner: ghost@x.co\n";
    xtask::git::write_note(repo.p(), "mr", &tip, body).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &tip));
}

/// A signed note whose payload covers change-hash + report-hash (DEC-10).
#[allow(clippy::too_many_arguments)]
fn write_attested_signed_note(
    repo: &Repo,
    level: &str,
    base: &str,
    tip: &str,
    signer: &str,
    report: &str,
    rsha: &str,
    sig: &str,
) {
    let body = format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\nbase: {base}\ntier: key\nsigner: {signer}\nreport: {report}\nreport_sha256: {rsha}\nsig: |\n{}\n",
        sig.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
    );
    xtask::git::write_note(repo.p(), "mr", tip, &body).unwrap();
}

#[test]
fn tip_authorised_requires_the_signature_to_cover_the_report_hash() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let rsha = "aaaa1111";
    let payload = format!("{hash}\n{rsha}");
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, payload.as_bytes());

    write_attested_signed_note(
        &repo,
        "epic",
        &base,
        &tip,
        "vu",
        "docs/.archive/r.html",
        rsha,
        &sig,
    );
    assert!(mr::tip_authorised(repo.p(), &tip), "signature over change+report verifies");

    write_attested_signed_note(
        &repo,
        "epic",
        &base,
        &tip,
        "vu",
        "docs/.archive/r.html",
        "forged",
        &sig,
    );
    assert!(
        !mr::tip_authorised(repo.p(), &tip),
        "forging the note's report_sha256 must invalidate the signature"
    );
}

#[test]
fn commit_signed_accepts_a_commit_signed_by_a_listed_key() {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");
    repo.write(".mr-allowed", &format!("key: vu {pubkey}\n"));
    repo.git(&["config", "gpg.format", "ssh"]);
    repo.git(&["config", "user.signingkey", repo.p().join("id").to_str().unwrap()]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-S", "-m", "signed"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(mr::commit_signed(repo.p(), &tip));
}

#[test]
fn commit_signed_rejects_an_unsigned_commit() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    assert!(!mr::commit_signed(repo.p(), &tip), "the tip commit was never signed");
}

#[test]
fn trailer_key_backed_only_for_identities_with_a_key_entry() {
    let repo = Repo::new("email: a@b.co\nkey: vu ssh-ed25519 AAAA t\n");
    assert!(mr::trailer_key_backed(repo.p(), "m\n\nReviewed-By: key vu\n"));
    assert!(
        mr::trailer_key_backed(repo.p(), "m\n\nReviewed-By: username vu\n"),
        "a weaker claimed tier must not dodge the signature requirement"
    );
    assert!(!mr::trailer_key_backed(repo.p(), "m\n\nReviewed-By: email a@b.co\n"));
    assert!(!mr::trailer_key_backed(repo.p(), "no trailer at all"));
}

#[test]
fn commit_signed_rejects_a_commit_signed_by_an_unlisted_key() {
    let repo = Repo::new("email: a@b.co\n");
    repo.keygen("id");
    let other = repo.keygen("other");
    repo.write(".mr-allowed", &format!("key: vu {other}\n"));
    repo.git(&["config", "gpg.format", "ssh"]);
    repo.git(&["config", "user.signingkey", repo.p().join("id").to_str().unwrap()]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-S", "-m", "signed by id, listed is other"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(!mr::commit_signed(repo.p(), &tip));
}

// ── E-MRSIG.W2 — setup + signing UX ──────────────────────────────────────────

/// Run `mr setup --as <id> --key <path>` over a software key; return its pubkey.
fn setup_with_software_key(repo: &Repo, identity: &str) -> String {
    let pubkey = repo.keygen("id");
    let key_path = repo.p().join("id");
    let (code, _) =
        mr::run(&["setup", "--as", identity, "--key", key_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    pubkey
}

#[test]
fn setup_registers_the_key_and_configures_signing() {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = setup_with_software_key(&repo, "vu");

    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vu {pubkey}")), "{allowed}");
    assert!(allowed.contains("email: a@b.co"), "existing entries are preserved: {allowed}");
    assert_eq!(repo.git(&["config", "--get", "gpg.format"]), "ssh");
    assert_eq!(
        repo.git(&["config", "--get", "user.signingkey"]),
        repo.p().join("id").to_str().unwrap()
    );

    // Idempotent: re-running setup against the same key does not duplicate it.
    let key_path = repo.p().join("id");
    mr::run(&["setup", "--as", "vu", "--key", key_path.to_str().unwrap()], repo.p()).unwrap();
    let again = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert_eq!(again.matches("key: vu").count(), 1, "{again}");
}

#[test]
fn confirm_signs_a_note_that_the_w1_predicate_accepts() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let report = result_report(&repo);
    settle_against(&repo, &tip, &base);
    let (code, out) = mr::run(
        &["confirm", "wu", &tip, "--as", "vu", "--target", &base, "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");

    let body = repo.git(&["notes", "--ref=mr", "show", &tip]);
    assert!(body.contains("tier: key"), "{body}");
    assert!(body.contains("sig: |"), "the signature rides in the note: {body}");
    assert!(body.contains(&format!("report: {report}")), "{body}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the signed note authorises the tip");
}

#[test]
fn confirm_signature_breaks_when_the_tip_is_amended() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    let report = result_report(&repo);
    settle_against(&repo, &tip, &base);
    mr::run(
        &["confirm", "epic", &tip, "--as", "vu", "--target", &base, "--report", report],
        repo.p(),
    )
    .unwrap();
    assert!(mr::tip_authorised(repo.p(), &tip));

    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    let old_note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    xtask::git::write_note(repo.p(), "mr", &amended, &old_note).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &amended), "amending invalidates the signature");
}

#[test]
fn confirm_falls_back_to_a_plain_note_for_an_identity_only_signer() {
    let repo = Repo::new("email: a@b.co\nusername: vu\n");
    let report = result_report(&repo);
    settle(&repo, "HEAD");
    // No key entry and no --target: the plain-note path (E-NMWF coexistence).
    let (code, _) =
        mr::run(&["confirm", "epic", "HEAD", "--as", "vu", "--report", report], repo.p()).unwrap();
    assert_eq!(code, 0);
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("tier: username"), "{body}");
    assert!(!body.contains("sig:"), "an identity-only note carries no signature: {body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn mr_commit_signs_a_doc_commit_a_listed_key_verifies() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, out) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert_eq!(
        mr::trailer_of(&repo.git(&["log", "-1", "--format=%B", &tip])),
        Some(("key".into(), "vu".into())),
        "the trailer still rides along for the commit-msg hook"
    );
    assert!(mr::commit_signed(repo.p(), &tip), "the doc commit is natively SSH-signed");
}

#[test]
fn setup_accepts_a_public_key_path_for_a_secure_enclave_key() {
    // A Secure-Enclave / Touch-ID key (via an ssh-agent like Secretive) exposes
    // only a public key on disk. Passing the .pub directly must register it and
    // point user.signingkey at the .pub (the agent holds the private half) -
    // not at a nonexistent <path>.pub.pub.
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("se");
    std::fs::remove_file(repo.p().join("se")).unwrap();
    let pub_path = repo.p().join("se.pub");

    let (code, _) =
        mr::run(&["setup", "--as", "vuluu", "--key", pub_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vuluu {pubkey}")), "{allowed}");
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), pub_path.to_str().unwrap());
}

#[test]
fn confirm_signs_when_user_signingkey_is_a_public_key_path() {
    // user.signingkey set to a .pub (the Secure-Enclave shape) must still drive
    // `ssh-keygen -Y sign -f <pub>` to a note W1 accepts. Hermetic proxy for the
    // agent: the private half sits on disk next to the .pub (no agent/Touch ID
    // to reproduce), so this exercises the .pub signing-key code path end to end.
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("se");
    let pub_path = repo.p().join("se.pub");
    mr::run(&["setup", "--as", "vuluu", "--key", pub_path.to_str().unwrap()], repo.p()).unwrap();
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), pub_path.to_str().unwrap());
    assert!(pubkey.starts_with("ssh-ed25519"));

    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let report = result_report(&repo);
    settle_against(&repo, &tip, &base);
    let (code, out) = mr::run(
        &["confirm", "wu", &tip, "--as", "vuluu", "--target", &base, "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(repo.git(&["notes", "--ref=mr", "show", &tip]).contains("sig: |"));
    assert!(mr::tip_authorised(repo.p(), &tip), "signed via a .pub signing key");
}

// ── E-MRSIG.W7 — native Secure-Enclave signer ────────────────────────────────

/// Compile the bundled Secure-Enclave signer from its embedded source. `None`
/// skips the test where `swiftc` is absent (non-mac CI); a failed compile of
/// present source is a real failure. The Enclave paths themselves (keygen +
/// Touch-ID sign) need hardware + a human and stay a deployment waiver; these
/// tests prove the helper builds and its ssh-keygen passthrough delegates
/// correctly for every non-Enclave key.
fn compiled_se_helper(dir: &Path) -> Option<PathBuf> {
    let src = dir.join("trawler-signer.swift");
    let bin = dir.join("trawler-signer");
    std::fs::write(&src, include_str!("../../src/trawler_signer.swift")).unwrap();
    let out = Command::new("swiftc").arg("-O").arg(&src).arg("-o").arg(&bin).output().ok()?;
    assert!(out.status.success(), "swiftc failed: {}", String::from_utf8_lossy(&out.stderr));
    Some(bin)
}

#[test]
fn se_helper_passes_software_keys_through_to_ssh_keygen() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");

    let mut child = Command::new(&helper)
        .args(["-Y", "sign", "-n", "trawler-mr", "-f"])
        .arg(repo.p().join("id"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"payload").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "helper sign failed");
    let sig = String::from_utf8(out.stdout).unwrap();
    assert!(sig.starts_with("-----BEGIN SSH SIGNATURE-----"), "{sig}");
    let signers = format!("vu {pubkey}\n");
    assert!(
        xtask::git::ssh_verify("vu", &signers, "trawler-mr", b"payload", &sig),
        "a passthrough signature verifies with stock ssh-keygen"
    );
}

#[test]
fn se_helper_signs_and_verifies_git_commits_via_gpg_ssh_program() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.git(&["config", "gpg.ssh.program", helper.to_str().unwrap()]);

    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(
        mr::commit_signed(repo.p(), &tip),
        "git signs and verifies through the helper (gpg.ssh.program both ways)"
    );
}

#[test]
fn confirm_signs_through_the_configured_signer_program() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.git(&["config", "gpg.ssh.program", helper.to_str().unwrap()]);

    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let report = result_report(&repo);
    settle_against(&repo, &tip, &base);
    let (code, out) = mr::run(
        &["confirm", "wu", &tip, "--as", "vu", "--target", &base, "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the note signed via the helper authorises");
}

// ── E-MRSIG.W8 — the mr wizard ───────────────────────────────────────────────

fn answers(text: &str) -> std::io::Cursor<Vec<u8>> {
    std::io::Cursor::new(text.as_bytes().to_vec())
}

#[test]
fn setup_wizard_runs_the_software_key_path_with_piped_answers() {
    let repo = Repo::new("email: a@b.co\n");
    let se_absent = Err("no Secure Enclave on this hardware".to_string());
    // Answers: method = software key, passphrase step skipped.
    let mut input = answers("software\n\n");
    let (code, out) =
        mr::setup_wizard(repo.p(), "vu", repo.p().to_str().unwrap(), &se_absent, &mut input)
            .unwrap();
    assert_eq!(code, 0, "{out}");

    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains("key: vu ssh-ed25519"), "{allowed}");
    assert!(allowed.contains("email: a@b.co"), "adds, never overwrites: {allowed}");
    assert_eq!(repo.git(&["config", "--get", "gpg.format"]), "ssh");
    let signingkey = repo.git(&["config", "--get", "user.signingkey"]);
    assert!(signingkey.ends_with("/.ssh/id_trawler_vu"), "{signingkey}");
    assert!(out.contains("registered vu in .mr-allowed"), "{out}");
    assert!(out.contains("each signature asks for:"), "{out}");
    assert!(out.contains(&signingkey), "the summary names the active signer: {out}");
}

#[test]
fn setup_wizard_identity_only_adds_an_entry_without_signing_config() {
    let repo = Repo::new("");
    let se_absent = Err("no Secure Enclave on this hardware".to_string());
    let (code, out) = mr::setup_wizard(
        repo.p(),
        "bo@x.co",
        repo.p().to_str().unwrap(),
        &se_absent,
        &mut answers("identity\n"),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert_eq!(allowed, "email: bo@x.co\n", "'@' means the email tier");
    assert!(
        xtask::git::git(repo.p(), &["config", "--get", "user.signingkey"]).is_err(),
        "identity-only configures no signer"
    );
    assert!(out.contains("weakest"), "{out}");
}

#[test]
fn setup_wizard_refuses_an_unavailable_method_then_accepts_a_listed_one() {
    let repo = Repo::new("");
    let se_absent = Err("Secure Enclave signing is macOS-only".to_string());
    // "touch-id" is greyed out: the wizard re-asks, then accepts "identity".
    let (code, _) = mr::setup_wizard(
        repo.p(),
        "vu",
        repo.p().to_str().unwrap(),
        &se_absent,
        &mut answers("touch-id\nidentity\n"),
    )
    .unwrap();
    assert_eq!(code, 0);
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert_eq!(allowed, "username: vu\n");
}

#[test]
fn commit_wizard_picks_the_identity_from_mr_allowed() {
    let repo = Repo::new("email: a@b.co\nusername: vu\n");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let mut input = answers("vu\n");
    let (code, out) =
        mr::run_with_input(&["commit", "-m", "docs: change"], repo.p(), &mut input).unwrap();
    assert_eq!(code, 0, "{out}");
    let body = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(mr::trailer_of(&body), Some(("username".into(), "vu".into())));
}

#[test]
fn commit_wizard_asks_for_a_missing_message() {
    let repo = Repo::new("username: vu\n");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    // Message prompted; the single listed identity is taken without a prompt.
    let mut input = answers("docs: asked for\n");
    let (code, out) = mr::run_with_input(&["commit"], repo.p(), &mut input).unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(repo.git(&["log", "-1", "--format=%B"]).starts_with("docs: asked for"));
}

#[test]
fn confirm_wizard_infers_level_tip_target_and_report_from_the_branch() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/y"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.write("docs/work/y-result.html", "<html>wu result</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change + its result report"]);
    settle_against(&repo, "HEAD", "epic/x");

    // No level, no tip, no --target, no --report: wu from the branch name, HEAD,
    // epic/x, and the single tracked docs/work/*-result.html.
    let (code, out) =
        mr::run_with_input(&["confirm", "--as", "vu"], repo.p(), &mut answers("")).unwrap();
    assert_eq!(code, 0, "{out}");
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("level: working-unit"), "{body}");
    assert!(body.contains("report: docs/work/y-result.html"), "{body}");
    assert!(body.contains("sig: |"), "{body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_wizard_asks_for_the_level_and_the_report_when_neither_infers() {
    let repo = Repo::new("username: vu\n");
    repo.write("docs/work/z-result.html", "<html>result</html>\n");
    settle(&repo, "HEAD");
    // Identity-only signer on `main`: level and report come from piped answers.
    let (code, out) = mr::run_with_input(
        &["confirm", "--as", "vu"],
        repo.p(),
        &mut answers("epic\ndocs/work/z-result.html\n"),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("level: epic"), "{body}");
    assert!(body.contains("report: docs/work/z-result.html"), "{body}");
}

#[test]
fn wizard_eof_is_a_usage_error_naming_the_missing_input() {
    let repo = Repo::new("username: vu\n");
    let err = mr::run_with_input(&["setup"], repo.p(), &mut answers("")).unwrap_err();
    assert!(err.contains("missing input: identity"), "{err}");
}

// ── E-MRSIG e2e — the full signed manual-review flow ─────────────────────────

/// End-to-end over one temp repo: `mr setup` registers a key + signing config →
/// a rule-bearing (T1) doc commit signed via `mr commit` verifies while T2/T3
/// doc commits pass the hook unsigned and CLAUDE.md (T0) is refused (DEC-10) →
/// a WU/epic note signed via `mr confirm` attests the result report → tamper
/// (amend, or forging the attested report hash) breaks the signature → an
/// unsigned commit / unlisted signer is rejected. Uses a software key: the
/// hardware-touch half (FIDO2 / Secure-Enclave Touch ID) is a deployment
/// property not reproducible in a hermetic run — this proves the crypto half
/// end to end.
#[test]
fn e2e_signed_manual_review_flow_with_a_software_key() {
    let repo = Repo::new("email: a@b.co\n");

    // 1. setup: register the key + configure SSH signing in one step.
    let pubkey = repo.keygen("id");
    let key_path = repo.p().join("id");
    let (code, _) =
        mr::run(&["setup", "--as", "vuluu", "--key", key_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vuluu {pubkey}")), "{allowed}");
    assert_eq!(repo.git(&["config", "--get", "gpg.format"]), "ssh");
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), key_path.to_str().unwrap());

    // 2. a doc commit signed via `mr commit` verifies (the unsigned negative is step 5).
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "docs: x", "--as", "vuluu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let doc_tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(mr::commit_signed(repo.p(), &doc_tip), "the signed doc commit verifies");
    assert_eq!(
        mr::trailer_of(&repo.git(&["log", "-1", "--format=%B", &doc_tip])),
        Some(("key".into(), "vuluu".into())),
        "the trailer rides along for the commit-msg hook"
    );

    // 2b. the DEC-10 narrowing at the commit-msg hook: T2/T3 doc commits pass
    // unsigned; a CLAUDE.md (T0) commit is refused without a trailer.
    repo.write("docs/work/scratch.html", "<html>t3</html>\n");
    repo.write("docs/use-cases/uc.html", "<html>t2</html>\n");
    repo.git(&["add", "-A"]);
    let msg = repo.path.join("MSG");
    std::fs::write(&msg, "t2+t3 docs, no trailer\n").unwrap();
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0, "T2/T3 doc commits need no commit-time signature");
    repo.git(&["commit", "-q", "-m", "t2+t3 docs, no trailer"]);
    repo.write("CLAUDE.md", "# rules\n");
    repo.git(&["add", "-A"]);
    std::fs::write(&msg, "t0 edit, no trailer\n").unwrap();
    let (code, out) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1, "a T0 commit without a trailer is refused: {out}");
    repo.git(&["reset", "-q", "--hard", "HEAD"]);

    // 3. a WU/epic tip confirmed via `mr confirm` attests the result report and
    // authorises over the signed range.
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.write("docs/work/e2e-result.html", "<html>epic result</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change + result report"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    settle_against(&repo, &tip, &base);
    let (code, out) = mr::run(
        &[
            "confirm",
            "epic",
            &tip,
            "--as",
            "vuluu",
            "--target",
            &base,
            "--report",
            "docs/work/e2e-result.html",
        ],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    let note_body = repo.git(&["notes", "--ref=mr", "show", &tip]);
    assert!(note_body.contains("report: docs/work/e2e-result.html"), "{note_body}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the signed note authorises the tip");

    // 3b. forging the attested report hash in the note breaks the signature.
    let forged_report = note_body.replace(
        &note_body.lines().find(|l| l.starts_with("report_sha256:")).unwrap().to_string(),
        "report_sha256: forged",
    );
    xtask::git::write_note(repo.p(), "mr", &tip, &forged_report).unwrap();
    assert!(
        !mr::tip_authorised(repo.p(), &tip),
        "a forged report_sha256 must invalidate the signature"
    );
    xtask::git::write_note(repo.p(), "mr", &tip, &note_body).unwrap();
    assert!(mr::tip_authorised(repo.p(), &tip), "restoring the note restores authorisation");

    // 4. tamper-evidence: amending the tip breaks the signed change-hash.
    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    let note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    xtask::git::write_note(repo.p(), "mr", &amended, &note).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &amended), "amend invalidates the signature");

    // 5. negatives: an unsigned commit and a bare note from an unlisted signer fail.
    let root = repo.git(&["rev-list", "--max-parents=0", "HEAD"]);
    assert!(!mr::commit_signed(repo.p(), &root), "the unsigned init commit is not signed");
    let forged = "schema: mr/v1\nlevel: epic\ntip: x\ntier: email\nsigner: ghost@x.co\n";
    xtask::git::write_note(repo.p(), "mr", &root, forged).unwrap();
    assert!(
        !mr::tip_authorised(repo.p(), &root),
        "an unlisted signer's bare note never authorises"
    );
}

// ---------------------------------------------------------------------------
// E-GATETRUTH.W3 — P2a: `mr commit` reports what happened, not git's exit code.
// ---------------------------------------------------------------------------

/// Install a `commit-msg` hook that refuses every commit, so git fails and
/// `HEAD` stays put — the only quadrant a hook can produce, because git ignores
/// a `post-commit` hook's exit status.
fn refusing_commit_msg_hook(repo: &Repo) {
    let hooks = repo.p().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("commit-msg");
    std::fs::write(&hook, "#!/bin/sh\necho 'the hook refused this message' >&2\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    repo.git(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
}

const BODY_SECOND_LINE: &str = "the-body-line-a-reader-should-never-be-shown";

#[test]
fn commit_that_lands_reports_success_and_no_error() {
    let repo = Repo::new("email: a@b.co\n");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let before = repo.git(&["rev-parse", "HEAD"]);

    let (code, out) =
        mr::run(&["commit", "-m", "docs: change", "--as", "a@b.co"], repo.p()).unwrap();

    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("error"), "a landed commit prints no error: {out}");
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), before, "HEAD moved");
}

/// The first commit in a repo: `HEAD` resolves to nothing before it and to a
/// commit after, so "did HEAD move" has to answer over an absent state too -
/// the one input `head_commit` returns `None` for.
#[test]
fn the_first_commit_in_a_repo_lands_from_an_unborn_head() {
    let repo = Repo::new_unborn("email: a@b.co\n");
    assert!(repo.git_raw(&["rev-parse", "HEAD"]).is_err(), "HEAD is unborn");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, out) =
        mr::run(&["commit", "-m", "docs: the first commit", "--as", "a@b.co"], repo.p()).unwrap();

    assert_eq!(code, 0, "the first commit lands like any other: {out}");
    assert!(!out.contains("HEAD did not move"), "it moved - from nothing to a commit: {out}");
    assert!(!repo.git(&["rev-parse", "HEAD"]).is_empty(), "HEAD now names a commit");
}

/// An unborn `HEAD` that stays unborn: git refuses, nothing landed, and the
/// verdict must not claim a first commit that was never made.
#[test]
fn a_refused_first_commit_is_a_failure_not_a_phantom_landing() {
    let repo = Repo::new_unborn("email: a@b.co\n");
    refusing_commit_msg_hook(&repo);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, out) =
        mr::run(&["commit", "-m", "docs: refused", "--as", "a@b.co"], repo.p()).unwrap();

    assert_eq!(code, 1, "nothing landed: {out}");
    assert!(out.contains("HEAD did not move"), "and the verdict says so: {out}");
    assert!(repo.git_raw(&["rev-parse", "HEAD"]).is_err(), "HEAD is still unborn");
}

#[test]
fn a_commit_git_refuses_names_gits_own_line_and_never_the_message_body() {
    let repo = Repo::new("email: a@b.co\n");
    refusing_commit_msg_hook(&repo);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let before = repo.git(&["rev-parse", "HEAD"]);
    let message = format!("docs: change\n\n{BODY_SECOND_LINE}");

    let (code, out) = mr::run(&["commit", "-m", &message, "--as", "a@b.co"], repo.p()).unwrap();

    assert_eq!(code, 1, "the commit did not land: {out}");
    assert!(out.contains("HEAD did not move"), "the verdict names the fact it checked: {out}");
    assert!(out.contains("the hook refused this message"), "git's own line is the cause: {out}");
    assert!(!out.contains(BODY_SECOND_LINE), "the message body is never echoed: {out}");
    assert!(!out.contains("--cleanup=verbatim"), "nor is the argument vector: {out}");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), before, "HEAD is where it was");
}

/// A `git` shim first on `PATH` that execs the real git and then exits 1 after a
/// `commit` — the only way to reach the landed-and-non-zero quadrant, since git
/// ignores a `post-commit` hook's exit status.
#[cfg(unix)]
#[test]
fn a_commit_that_lands_while_git_exits_non_zero_is_reported_as_a_success() {
    use std::os::unix::fs::PermissionsExt as _;

    let repo = Repo::new("email: a@b.co\n");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let before = repo.git(&["rev-parse", "HEAD"]);

    let shim_dir = repo.p().join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let real_git = String::from_utf8(
        Command::new("/usr/bin/env").args(["which", "git"]).output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let shim = shim_dir.join("git");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\n\"{real_git}\" \"$@\"\nstatus=$?\nfor a in \"$@\"; do\n  if [ \"$a\" = commit ]; then\n    echo 'warning: the signer complained after the commit landed' >&2\n    exit 1\n  fi\ndone\nexit $status\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();

    let path = format!("{}:{}", shim_dir.display(), std::env::var("PATH").unwrap_or_default());
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["mr", "commit", "-m", "docs: change", "--as", "a@b.co"])
        .current_dir(repo.p())
        .env("PATH", path)
        .output()
        .unwrap();

    let said =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the commit landed, so this is a success: {said}");
    assert!(said.contains("committed with"), "the success line comes first: {said}");
    assert!(said.contains("after the commit landed"), "the anomaly is stated too: {said}");
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), before, "and HEAD really moved");
}

// ---------------------------------------------------------------------------
// E-GATETRUTH.W3 — P3: each way an mr/v1 note fails to authorise says so.
// ---------------------------------------------------------------------------

/// The classification, rendered for assertions: the leaf's name plus the fields
/// a reader needs, so a test names the cause rather than a bool.
fn leaf(repo: &Repo, tip: &str) -> String {
    match mr::authorisation(repo.p(), tip) {
        mr::Authorisation::Authorised => "authorised".to_string(),
        mr::Authorisation::NoNote => "no-note".to_string(),
        mr::Authorisation::Unreadable(schema) => {
            format!("unreadable:{}", schema.unwrap_or_else(|| "unparseable".to_string()))
        }
        mr::Authorisation::UnhashableRange { base } => format!("unhashable:{base}"),
        mr::Authorisation::UnlistedSigner { signer, roster } => {
            format!("unlisted:{signer}:{roster}")
        }
        mr::Authorisation::StaleSignature { signer } => format!("stale:{signer}"),
    }
}

/// Every leaf agrees with the bool `tip_authorised` has always returned.
fn assert_bool_agrees(repo: &Repo, tip: &str) {
    let authorised = leaf(repo, tip) == "authorised";
    assert_eq!(
        mr::tip_authorised(repo.p(), tip),
        authorised,
        "the enum and the bool must never disagree"
    );
}

#[test]
fn a_tip_with_no_note_is_classified_as_having_none() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    assert_eq!(leaf(&repo, &tip), "no-note");
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn a_note_of_another_schema_is_unreadable_and_names_the_schema_it_declares() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    let body = "schema: mr/v2\nlevel: epic\ntip: x\ntier: key\nsigner: vu\n";
    xtask::git::write_note(repo.p(), "mr", &tip, body).unwrap();
    assert_eq!(leaf(&repo, &tip), "unreadable:mr/v2");
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn an_unparseable_note_is_unreadable_rather_than_absent() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    xtask::git::write_note(repo.p(), "mr", &tip, "schema: [this is: not: a: note").unwrap();
    assert_eq!(leaf(&repo, &tip), "unreadable:unparseable");
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn a_signed_note_whose_signer_this_checkout_lists_no_key_for_names_the_signer_and_the_roster() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "working-unit", &base, &tip, "vu", &sig);
    assert_eq!(leaf(&repo, &tip), "authorised", "the roster lists vu's key");

    // The DEC-10 shape: same repo-wide note, a roster without that signer.
    repo.write(".mr-allowed", "email: a@b.co\n");
    let roster = repo.p().join(".mr-allowed").display().to_string();
    assert_eq!(leaf(&repo, &tip), format!("unlisted:vu:{roster}"));
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn a_signed_note_from_an_identity_only_listing_is_an_unlisted_signer_not_a_bad_signature() {
    // `allowed_signers` is empty for an identity-only entry, so there is nothing
    // to verify against - a roster gap, not a signature that failed.
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "working-unit", &base, &tip, "vu", &sig);
    repo.write(".mr-allowed", "email: a@b.co\nusername: vu\n");

    assert!(leaf(&repo, &tip).starts_with("unlisted:vu:"), "got: {}", leaf(&repo, &tip));
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn a_listed_key_whose_signature_no_longer_covers_the_range_is_stale_not_unlisted() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    write_signed_note(&repo, "working-unit", &base, &amended, "vu", &sig);

    assert_eq!(leaf(&repo, &amended), "stale:vu");
    assert_bool_agrees(&repo, &amended);
}

#[test]
fn a_note_naming_a_base_this_repo_does_not_hold_says_the_range_cannot_be_recomputed() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    let ghost = "0000000000000000000000000000000000000000";
    write_signed_note(&repo, "working-unit", ghost, &tip, "vu", &sig);

    assert_eq!(leaf(&repo, &tip), format!("unhashable:{ghost}"));
    assert_bool_agrees(&repo, &tip);
}

#[test]
fn a_bare_note_is_authorised_by_a_listing_and_unlisted_without_one() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    let body = "schema: mr/v1\nlevel: epic\ntip: x\ntier: username\nsigner: vu\n";
    xtask::git::write_note(repo.p(), "mr", &tip, body).unwrap();
    assert_eq!(leaf(&repo, &tip), "authorised", "a listed signer's bare note still authorises");
    assert_bool_agrees(&repo, &tip);

    let ghost = "schema: mr/v1\nlevel: epic\ntip: x\ntier: email\nsigner: ghost@x.co\n";
    xtask::git::write_note(repo.p(), "mr", &tip, ghost).unwrap();
    assert!(leaf(&repo, &tip).starts_with("unlisted:ghost@x.co:"), "got: {}", leaf(&repo, &tip));
    assert_bool_agrees(&repo, &tip);
}

/// The full WU-gate notes on `tip`: `test/v2`, `doc/v1` and a `cr/v3`-era
/// `cr/v2` PASS bound to this range. The e2e merge below is judged by every
/// check, not only check 8, so the stubs `settle` writes are not enough.
#[cfg(unix)]
fn settle_for_a_gated_merge(repo: &Repo, tip: &str) {
    let base = repo.git(&["rev-parse", "epic/x"]);
    let hash = xtask::git::diff_sha256(repo.p(), &base, tip).unwrap();
    repo.git(&[
        "notes",
        "--ref=test",
        "add",
        "-f",
        "-m",
        "schema: test/v2\nlevel: working-unit\nepic: E-GATETRUTH\nwu: E-GATETRUTH.W3\ntests:\n  integration: { count: 1, covers: \"the roster gap a second worktree opens\" }\n",
        tip,
    ]);
    repo.git(&[
        "notes",
        "--ref=doc",
        "add",
        "-f",
        "-m",
        "schema: doc/v1\nepic: E-GATETRUTH\nwu: E-GATETRUTH.W3\nchanges:\n  - path: docs/use-cases/api.html\n    tier: T2\n    action: updated\n    distills: \"the contract this WU moved\"\n  - path: docs/work/w-result.html\n    tier: T3\n    action: added\n    distills: \"wu result report\"\n",
        tip,
    ]);
    repo.git(&[
        "notes",
        "--ref=cr",
        "add",
        "-f",
        "-m",
        &format!("schema: cr/v2\nepic: E-GATETRUTH\nwu: E-GATETRUTH.W3\nbase: {base}\nhead: {tip}\ndiff_sha256: {hash}\nverdict: PASS\ndate: 2026-09-03\n"),
        tip,
    ]);
}

/// Notes are repo-wide, the roster is per-worktree: worktree A signs, worktree
/// B merges and must be told which of the two it is missing. `.mr-allowed` is
/// gitignored, exactly the DEC-10 layout — a committed roster would follow the
/// checkout and the gap would never open, so the fixture would prove nothing.
#[cfg(unix)]
#[test]
fn e2e_a_merge_judged_from_a_worktree_without_the_signers_key_says_so_and_lands_once_listed() {
    use std::os::unix::fs::PermissionsExt as _;

    let repo = Repo::new(".mr-allowed is gitignored, so Repo::new's write is untracked\n");
    // Undo the harness's committed roster and gitignore it, per DEC-10.
    std::fs::remove_file(repo.p().join(".mr-allowed")).unwrap();
    repo.write(".gitignore", ".mr-allowed\n");
    repo.git(&["rm", "-q", "--cached", ".mr-allowed"]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "gitignore the local roster (DEC-10)"]);
    repo.git(&["branch", "epic/x"]);

    // Worktree A: register a key, build a WU tip, settle it, sign it. The notes
    // are the full shapes the WU gate demands, because this merge is judged by
    // every check, not only check 8.
    let pubkey = setup_with_software_key(&repo, "vuluu");
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.write("docs/use-cases/api.html", "<html>the contract this WU moved</html>\n");
    let report = "docs/work/w-result.html";
    repo.write(report, "<html>result</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the WU change + its result report"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    settle_for_a_gated_merge(&repo, &tip);
    let (code, out) = mr::run(
        &["confirm", "wu", &tip, "--as", "vuluu", "--target", "epic/x", "--report", report],
        repo.p(),
    )
    .unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(mr::tip_authorised(repo.p(), &tip), "worktree A, which holds the key, authorises it");

    // Worktree B: same .git, its own working tree, and no roster at all.
    let b = repo.p().join("wt-b");
    repo.git(&["worktree", "add", "-q", "--detach", b.to_str().unwrap(), "epic/x"]);
    assert!(
        !b.join(".mr-allowed").exists(),
        "B has no roster - the gitignored file did not follow"
    );
    assert!(
        !mr::tip_authorised(&b, &tip),
        "B reads a valid note and a roster with no line for its signer"
    );
    let leaf_in_b = match mr::authorisation(&b, &tip) {
        mr::Authorisation::UnlistedSigner { signer, roster } => format!("{signer}|{roster}"),
        other => panic!("B must name the roster gap, got {}", leaf_name(&other)),
    };
    assert!(leaf_in_b.starts_with("vuluu|"), "the signer is named: {leaf_in_b}");
    assert!(leaf_in_b.contains("wt-b"), "and B's own roster path, not A's: {leaf_in_b}");

    // The merge hook in B refuses, and its message is the roster, not the note.
    let hooks = b.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-merge-commit");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\nexec \"{}\" verify-dod hook wu\n", env!("CARGO_BIN_EXE_xtask")),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let git_in_b = |args: &[&str]| {
        let mut cmd = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.arg("-C").arg(&b).args(args).output().unwrap()
    };
    git_in_b(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    git_in_b(&["config", "user.email", "b@test"]);
    git_in_b(&["config", "user.name", "b"]);
    git_in_b(&["checkout", "-q", "-B", "epic/x"]);

    let refused = git_in_b(&["merge", "--no-ff", "-m", "merge the WU", &tip]);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!refused.status.success(), "B has no key for the signer: {said}");
    assert!(said.contains("signed by vuluu"), "the signer is named: {said}");
    assert!(said.contains("wt-b"), "and B's roster is the one it read: {said}");
    assert!(!said.contains("has no mr/v1 note"), "the note is there and valid: {said}");
    git_in_b(&["merge", "--abort"]);

    // B lists the key: the same note, the same tip, and now the merge lands.
    std::fs::write(b.join(".mr-allowed"), format!("key: vuluu {pubkey}\n")).unwrap();
    assert!(mr::tip_authorised(&b, &tip), "the roster was the whole difference");
    let landed = git_in_b(&["merge", "--no-ff", "-m", "merge the WU", &tip]);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&landed.stdout),
        String::from_utf8_lossy(&landed.stderr)
    );
    assert!(landed.status.success(), "the gated merge lands from B once it lists the key: {said}");
}

/// A leaf's name, for a panic message that says which one was reached.
#[cfg(unix)]
fn leaf_name(a: &mr::Authorisation) -> &'static str {
    match a {
        mr::Authorisation::Authorised => "authorised",
        mr::Authorisation::NoNote => "no-note",
        mr::Authorisation::Unreadable(_) => "unreadable",
        mr::Authorisation::UnhashableRange { .. } => "unhashable-range",
        mr::Authorisation::UnlistedSigner { .. } => "unlisted-signer",
        mr::Authorisation::StaleSignature { .. } => "stale-signature",
    }
}
