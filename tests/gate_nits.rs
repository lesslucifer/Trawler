//! E-SIMPWF.W5 integration — the severity rule at the merge gate, over a real
//! temp repo driven through the real `xtask` binary and the real merge hook.
//!
//! The unit tests fix what `check_review` does with a note already parsed. What
//! only a real seam shows is the contract the WU actually promises: a tip whose
//! verified findings are dispositioned nits merges, and the same tip with one
//! left undecided is refused by name.
#![cfg(all(unix, feature = "hook-tests"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

#[path = "common/gitrepo.rs"]
mod gitrepo;
use gitrepo::{xtask_bin, GitRepo};

const REPORT: &str = "docs/work/result.html";

fn seeded_repo() -> GitRepo {
    let repo = GitRepo::new();
    repo.write(".mr-allowed", "email: a@b.co\n");
    repo.write("src/lib.rs", "pub fn base() {}\n");
    repo.write(REPORT, "<html><body>the result</body></html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    repo.install_hook(
        "pre-merge-commit",
        &format!(
            "#!/bin/sh\nset -e\nbranch=$(git symbolic-ref --short -q HEAD || true)\n\
             case \"$branch\" in\nmain) gate=epic ;;\nepic/*) gate=wu ;;\n*) exit 0 ;;\nesac\n\
             exec \"{}\" verify-dod hook \"$gate\"\n",
            xtask_bin().display()
        ),
    );
    repo
}

fn merge(repo: &GitRepo) -> (bool, String) {
    repo.git(&["checkout", "-q", "epic/x"]);
    let out = repo.git_raw(&["merge", "--no-ff", "-m", "merge wu/x/one", "wu/x/one"]);
    let said =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        repo.git_raw(&["merge", "--abort"]);
    }
    (out.status.success(), said)
}

#[test]
fn a_dispositioned_nit_merges_and_an_undecided_one_is_refused_by_name() {
    let repo = seeded_repo();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add a function"]);

    let tip = repo.sha("HEAD");
    let base = repo.sha("epic/x");
    let report_sha = repo.report_sha(&tip, REPORT);
    repo.note(
        "test",
        &tip,
        "schema: test/v2\nlevel: working-unit\nepic: E-SW\nwu: E-SW.W5\n\
         tests:\n  integration: { count: 1, covers: \"the added function\" }\n",
    );
    repo.note(
        "doc",
        &tip,
        "schema: doc/v1\nepic: E-SW\nwu: E-SW.W5\nchanges: []\nt2_waived: \"the diff changes no document\"\n",
    );
    repo.note(
        "mr",
        &tip,
        &format!(
            "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\n\
             signer: a@b.co\nreport: {REPORT}\nreport_sha256: {report_sha}\n\
             date: 2026-09-13\n"
        ),
    );
    // The diff touches `src/` alone, so the code paths `cr/v4` binds are the
    // whole diff - the one case where the harness's whole-diff hash is the hash
    // the gate recomputes.
    let review = |nits: &str| {
        repo.note(
            "cr",
            &tip,
            &format!(
                "schema: cr/v4\nepic: E-SW\nwu: E-SW.W5\nbase: {base}\nhead: {tip}\n\
                 diff_sha256: {}\nverdict: PASS\n{nits}date: 2026-09-13\n",
                repo.diff_sha(&base, &tip)
            ),
        );
    };

    review("nits:\n  - finding: \"the loop variable is named x\"\n");
    let (merged, said) = merge(&repo);
    assert!(!merged, "a recorded nit nobody decided about must not merge: {said}");
    assert!(said.contains("3·review-nits"), "the red names the check: {said}");
    assert!(said.contains("the loop variable is named x"), "and the finding: {said}");

    review(
        "nits:\n  - finding: \"the loop variable is named x\"\n    fixed: \"renamed to frame\"\n  \
         - finding: \"two branches could share a helper\"\n    waived: \"the helper would take three flags\"\n",
    );
    let (merged, said) = merge(&repo);
    assert!(merged, "a PASS whose only findings are dispositioned nits merges: {said}");
}
