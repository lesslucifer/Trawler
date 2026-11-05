#![cfg(unix)]
//! E-NMWF epic e2e — the full manual-review flow over a hermetic temp repo,
//! driven through the real `xtask` binary and real git (no shims).
//!
//! Covers the rules end to end (DEC-7/DEC-9/DEC-10): a rule-bearing doc commit
//! (T0 CLAUDE.md / T1 flat docs/) needs a listed `Reviewed-By` trailer at
//! commit time while T2/T3 doc commits pass unsigned; an epic tip always needs
//! a listed `mr/v1` note attesting its result report; unlisted identities fail.
#![cfg(all(unix, feature = "hook-tests"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

#[path = "common/gitrepo.rs"]
mod gitrepo;
use gitrepo::{xtask_bin, GitRepo};

/// The substrate the manual-review rules are read against: a roster listing two
/// identities, one code file and one T2 doc.
fn seeded_repo() -> GitRepo {
    let repo = GitRepo::new();
    repo.write(".mr-allowed", "email: a@b.co\nusername: vu\n");
    repo.write("src/lib.rs", "pub fn base() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v1</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    repo.install_hook(
        "commit-msg",
        &format!("#!/bin/sh\nexec \"{}\" verify-dod commit-msg \"$1\"\n", xtask_bin().display()),
    );
    repo
}

#[test]
fn full_manual_review_flow() {
    let repo = seeded_repo();

    // (a) a rule-bearing (T1) doc commit with no trailer is refused by the
    // commit-msg hook — while a T2 doc commit passes unsigned (DEC-10).
    repo.write("docs/use-cases/api.html", "<html>v2 edit</html>\n");
    repo.git(&["add", "-A"]);
    let t2_ok = repo.git_raw(&["commit", "-q", "-m", "docs: t2 edit, no trailer"]);
    assert!(t2_ok.status.success(), "a T2 doc commit needs no trailer");

    repo.write("docs/standard.html", "<html>t1 standard</html>\n");
    repo.git(&["add", "-A"]);
    let refused = repo.git_raw(&["commit", "-q", "-m", "docs: t1 edit without trailer"]);
    assert!(!refused.status.success(), "a T1 doc commit without a trailer must be refused");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("self-authorise")
            || String::from_utf8_lossy(&refused.stdout).contains("self-authorise"),
        "the refusal explains a human must confirm"
    );

    // (b) an unlisted identity is refused by `mr commit`.
    let unlisted = repo.xtask(&["mr", "commit", "-m", "docs: edit", "--as", "ghost@x.co"]);
    assert!(
        String::from_utf8_lossy(&unlisted.stdout).contains("not listed in .mr-allowed"),
        "an unlisted identity is refused"
    );

    // (c) a listed identity via `mr commit` lands the doc commit with a trailer.
    let ok = repo.xtask(&["mr", "commit", "-m", "docs: edit", "--as", "a@b.co"]);
    assert!(
        ok.status.success(),
        "listed identity commits: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let msg = String::from_utf8_lossy(&repo.git_raw(&["log", "-1", "--format=%B", "HEAD"]).stdout)
        .to_string();
    assert!(msg.contains("Reviewed-By: email a@b.co"), "the trailer is recorded");

    // Build an epic with one merged WU so the epic gate has a real seam.
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "wu code"]);
    let wu_tip = repo.sha("HEAD");
    let wu_base = repo.sha("epic/x");
    let wu_diff = repo.diff_sha(&wu_base, &wu_tip);
    repo.note("test", &wu_tip, "schema: test/v2\nlevel: working-unit\nepic: E9\nwu: E9.W1\ntests:\n  integration: { count: 1 }\n");
    repo.note(
        "doc",
        &wu_tip,
        "schema: doc/v1\nepic: E9\nwu: E9.W1\nchanges: []\nt2_waived: \"n/a\"\n",
    );
    repo.note("cr", &wu_tip, &format!("schema: cr/v2\nepic: E9\nwu: E9.W1\nbase: {wu_base}\nhead: {wu_tip}\ndiff_sha256: {wu_diff}\nverdict: PASS\n"));
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);

    // Bless the epic tip fully — everything EXCEPT the mr note.
    let epic_tip = repo.sha("epic/x");
    let ebase = repo.sha("main");
    let ediff = repo.diff_sha(&ebase, &epic_tip);
    repo.note(
        "test",
        &epic_tip,
        "schema: test/v2\nlevel: epic\nepic: E9\ntests:\n  e2e: { count: 1 }\n",
    );
    repo.note("doc", &epic_tip, "schema: doc/v1\nepic: E9\nchanges: []\nt2_waived: \"n/a\"\n");
    repo.note("cr", &epic_tip, &format!("schema: cr/v2\nepic: E9\nbase: {ebase}\nhead: {epic_tip}\ndiff_sha256: {ediff}\nverdict: PASS\n"));

    // (d) with every other note present, the epic is red for ONE reason: no mr note.
    let red = repo.xtask(&["verify-dod", "epic", "epic/x", "--target", "main"]);
    let red_out = String::from_utf8_lossy(&red.stdout);
    assert!(!red.status.success(), "an otherwise-blessed epic without an mr note is red");
    assert!(red_out.contains("8·manual-review"), "the red is the manual-review check: {red_out}");
    assert_eq!(
        red_out.matches("FAIL").count(),
        1,
        "manual review is the SOLE remaining failure: {red_out}"
    );

    // (e) signing the epic — attesting its (archived) result report — clears
    // the last check and the gate goes green.
    repo.write("docs/.archive/e9-result.html", "<html>epic result</html>\n");
    let signed = repo.xtask(&[
        "mr",
        "confirm",
        "epic",
        "epic/x",
        "--as",
        "a@b.co",
        "--report",
        "docs/.archive/e9-result.html",
    ]);
    assert!(signed.status.success(), "epic sign-off: {}", String::from_utf8_lossy(&signed.stderr));

    let green = repo.xtask(&["verify-dod", "epic", "epic/x", "--target", "main"]);
    assert!(
        green.status.success(),
        "a fully-blessed, signed epic is green: {}",
        String::from_utf8_lossy(&green.stdout)
    );
}

/// E-SIMPWF.W7 — a Working Unit that changes no code is signable and merges.
///
/// `8·manual-review` demands an `mr/v1` on every WU merge, exempt or not, and
/// `mr confirm` is its only writer. While the readiness check demanded
/// `test/v2`, `doc/v1` and `cr/v5` unconditionally, a docs-only WU could not be
/// merged at all: `notes write` declines to write the first two for an exempt
/// range and the review declines the third, so the one note the gate wanted was
/// unreachable.
#[test]
fn a_docs_only_working_unit_is_signable_and_a_code_bearing_one_is_not() {
    let repo = seeded_repo();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/docs-only", "epic/x"]);
    repo.write("docs/work/x-result.html", "<html>result</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "docs: a working unit that changes no code"]);

    let confirm = repo.xtask(&[
        "mr",
        "confirm",
        "wu",
        "HEAD",
        "--as",
        "a@b.co",
        "--target",
        "epic/x",
        "--report",
        "docs/work/x-result.html",
    ]);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&confirm.stdout),
        String::from_utf8_lossy(&confirm.stderr)
    );
    assert!(confirm.status.success(), "an exempt tip is signable: {said}");
    assert!(!said.contains("not ready to sign"), "no note is owed by an exempt range: {said}");
    assert!(
        repo.git_raw(&["notes", "--ref=mr", "show", "HEAD"]).status.success(),
        "the mr/v1 note 8·manual-review reads is written"
    );

    let gate = repo.xtask(&["verify-dod", "wu", "HEAD", "--target", "epic/x"]);
    assert!(
        gate.status.success(),
        "the merge gate greens the tip it just let a human sign: {}{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    // The exemption is the range's, not the signer's: a tip that touches code
    // still owes its receipts before anyone is asked to sign.
    repo.git(&["checkout", "-q", "-b", "wu/x/with-code", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/work/y-result.html", "<html>result</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "feat: a working unit that changes code"]);
    let refused = repo.xtask(&[
        "mr",
        "confirm",
        "wu",
        "HEAD",
        "--as",
        "a@b.co",
        "--target",
        "epic/x",
        "--report",
        "docs/work/y-result.html",
    ]);
    let why = String::from_utf8_lossy(&refused.stdout).to_string();
    assert!(why.contains("not ready to sign"), "a code-bearing tip still owes its notes: {why}");
    assert!(why.contains("test/v2") && why.contains("cr/v5"), "{why}");
}
