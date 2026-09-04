//! E-GATETRUTH epic e2e — the four fixes composed over one hermetic temp repo,
//! driven through the real `xtask` binary, the real merge hook and real git
//! (no shims).
//!
//! Each Working Unit proved its own fix in isolation and deferred this step in
//! identical words. What only the epic can show is that the four hold *at the
//! same seam*: one WU that adds a component, retires its design per DEC-16 and
//! cites it through history (P1/DEC-28), landed by a commit whose success is
//! reported as success (P2a), over docs whose links resolve (P2b), judged
//! against a roster this checkout actually lists (P3).
//!
//! The composition is the point: P1's citation must survive a merge whose
//! authorisation P3 decides, and the doc entry the citation's retirement owes
//! must pass P2b's link check.
#![cfg(all(unix, feature = "hook-tests"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs;
use std::process::Output;

#[path = "common/gitrepo.rs"]
mod gitrepo;
use gitrepo::{xtask_bin, GitRepo};

const DESIGN: &str = "docs/work/components/flow-badge.html";
const SURFACE: &str = "ui/src/components/FlowBadge.tsx";
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const REPORT: &str = "docs/work/plan.html";

/// The substrate this story reads: a roster listing one signer, a link target,
/// and the rulebook a design pin is written against.
fn seeded_repo() -> GitRepo {
    let repo = GitRepo::new();
    repo.write(".mr-allowed", "email: a@b.co\n");
    repo.write("src/lib.rs", "pub fn base() {}\n");
    repo.write("docs/target.html", "<html>the link target</html>\n");
    repo.write(
        RULEBOOK,
        "<html><body><table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>colour-tokens-only</code></td></tr></tbody></table></body></html>\n",
    );
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    // The real dispatch rule with the built binary in place of cargo: a merge
    // into epic/* runs the WU gate, into main the epic gate.
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

/// Merge through the real hook, reporting whether it merged and what the gate
/// said. A refused merge is aborted so the next attempt starts clean.
fn merge(repo: &GitRepo, target: &str, branch: &str) -> (bool, String) {
    repo.git(&["checkout", "-q", target]);
    let out = repo.git_raw(&["merge", "--no-ff", "-m", &format!("merge {branch}"), branch]);
    let said =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        repo.git_raw(&["merge", "--abort"]);
    }
    (out.status.success(), said)
}

fn said_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn design_html(body: &str) -> String {
    format!("<html><body><h1>FlowBadge</h1><p>{body}</p></body></html>\n")
}

/// P2b then P2a, over the doc the rest of the test then leans on: the link
/// check reds a dead `href` and goes green once it resolves, and the `mr commit`
/// that lands the result reports landing rather than `error:`.
fn land_the_report(repo: &GitRepo) {
    repo.write(REPORT, "<html><a href=\"nowhere.html\">gone</a></html>\n");
    let dead = repo.xtask(&["verify-doc", REPORT]);
    let dead_said = said_of(&dead);
    assert!(!dead.status.success(), "a dead relative href is red: {dead_said}");
    assert!(dead_said.contains("nowhere.html"), "the red names the link: {dead_said}");

    repo.write(REPORT, "<html><a href=\"../target.html\">the target</a></html>\n");
    let live = repo.xtask(&["verify-doc", REPORT]);
    assert!(
        live.status.success(),
        "the same doc is green once the href resolves: {}",
        said_of(&live)
    );

    repo.git(&["add", "-A"]);
    let before = repo.sha("HEAD");
    let committed = repo.xtask(&["mr", "commit", "-m", "docs: the plan", "--as", "a@b.co"]);
    let said = said_of(&committed);
    assert!(committed.status.success(), "the commit lands: {said}");
    assert!(!said.contains("error"), "a landed commit reports no error: {said}");
    assert_ne!(repo.sha("HEAD"), before, "and HEAD actually moved");
}

/// The history every design→build WU produces under DEC-16: the design lands,
/// the component is built from it, and the design is retired with it - so the
/// path resolves nowhere at the tip its citation is judged at. Returns the blob
/// the build read, the only pin DEC-28 accepts.
fn build_the_badge_and_retire_its_design(repo: &GitRepo) -> String {
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);

    repo.write(DESIGN, &design_html("the contract the build reads"));
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "design the badge"]);
    let handed = repo.blob("HEAD", DESIGN);

    repo.write(SURFACE, "export const FlowBadge = () => null;\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "build the badge"]);

    fs::remove_file(repo.path().join(DESIGN)).unwrap();
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "retire the design (DEC-16)"]);
    handed
}

#[test]
fn the_four_fixes_hold_at_one_seam() {
    let repo = seeded_repo();
    land_the_report(&repo);

    let handed = build_the_badge_and_retire_its_design(&repo);
    let tip = repo.sha("HEAD");
    let base = repo.sha("epic/x");
    assert!(
        !repo.git_raw(&["cat-file", "-e", &format!("{tip}:{DESIGN}")]).status.success(),
        "the design is gone at the tip - the condition P1 was about"
    );

    let bless = |design: &str| {
        repo.note(
            "test",
            &tip,
            "schema: test/v2\nlevel: working-unit\nepic: E-GT\nwu: E-GT.W1\n\
             tests:\n  integration: { count: 1, covers: \"the badge\" }\n",
        );
        repo.note(
            "doc",
            &tip,
            &format!(
                "schema: doc/v1\nepic: E-GT\nwu: E-GT.W1\nchanges:\n  \
                 - path: {DESIGN}\n    tier: T3\n    action: archived\n    \
                 distilled_to: \"the @use and @avoid tags on {SURFACE}\"\n\
                 t2_waived: \"no T2 fact moved\"\n"
            ),
        );
        repo.note(
            "cr",
            &tip,
            &format!(
                "schema: cr/v3\nepic: E-GT\nwu: E-GT.W1\nbase: {base}\nhead: {tip}\n\
                 diff_sha256: {}\nverdict: PASS\n{design}date: 2026-09-04\n",
                repo.diff_sha(&base, &tip)
            ),
        );
    };

    // The design dimension judges the retired design at its last state (DEC-29),
    // pinned against the rulebook the tip ships - the section every citation
    // below sits under.
    let rulebook_pin = repo.blob(&tip, RULEBOOK);
    let judged = format!(
        "design:\n  artifacts: [{DESIGN}]\n  rulebook: {rulebook_pin}\n  \
         findings: {{ found: 0, verified: 0, waived: 0 }}\n  verdict: PASS\n"
    );

    // ── P1 · a citation the tip does not ship, pinned at a blob it never held ──
    let fabricated = format!(
        "{judged}  implements:\n    - path: {DESIGN}\n      blob: 0000000\n      \
         built: [{SURFACE}]\n"
    );
    bless(&fabricated);
    let (merged, said) = merge(&repo, "epic/x", "wu/x/one");
    assert!(!merged, "a pin the design never carried must not merge: {said}");

    // ── P1 · the same citation, pinned at the bytes the build actually read ──
    let cited = format!(
        "{judged}  implements:\n    - path: {DESIGN}\n      blob: {}\n      \
         built: [{SURFACE}]\n",
        &handed[..7]
    );
    bless(&cited);

    // ── P3 · the roster decides authorisation, and says so when it cannot ────
    // The note is repo-wide; `.mr-allowed` is per-checkout (DEC-10). A signer
    // this checkout does not list is a roster gap, not a bad signature — and the
    // gate must name the roster rather than the note.
    let report_sha = repo.report_sha(&tip, REPORT);
    repo.note(
        "mr",
        &tip,
        &format!(
            "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\n\
             signer: ghost@x.co\nreport: {REPORT}\nreport_sha256: {report_sha}\n\
             date: 2026-09-04\n"
        ),
    );
    let (merged, said) = merge(&repo, "epic/x", "wu/x/one");
    assert!(!merged, "an unlisted signer must not merge: {said}");
    assert!(said.contains("ghost@x.co"), "the red names the signer: {said}");
    assert!(said.contains(".mr-allowed"), "and the roster it is missing from: {said}");

    // ── all four, composed: the seam merges ─────────────────────────────────
    repo.note(
        "mr",
        &tip,
        &format!(
            "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\n\
             signer: a@b.co\nreport: {REPORT}\nreport_sha256: {report_sha}\n\
             date: 2026-09-04\n"
        ),
    );
    let (merged, said) = merge(&repo, "epic/x", "wu/x/one");
    assert!(
        merged,
        "a retired design cited at its last state, signed by a listed signer, merges: {said}"
    );

    // The citation resolved through history at a real merge — the whole point of
    // DEC-28, and the state that was unreachable before this epic.
    assert_eq!(repo.sha("HEAD"), repo.sha("epic/x"), "the seam is on the epic mainline");
    assert!(
        !repo
            .git_raw(&["cat-file", "-e", &format!("{}:{DESIGN}", repo.sha("HEAD"))])
            .status
            .success(),
        "and the design stayed retired - the merge did not resurrect it"
    );
}
