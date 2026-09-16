//! E-SIMPWF epic e2e — the whole flow the epic changed, over one hermetic temp
//! repo driven through the real `xtask` binary, the real merge hook and real
//! git (no shims): an epic of three Working Units lands on `main`, every note
//! the gate reads written by `cargo xtask notes write`, every review bound to
//! the code paths and nothing else.
//!
//! Each Working Unit proved its own rule at the integration level and waived
//! this step in the same words. What only the epic can show is the rules
//! composed at real seams: the tool's notes are the ones the gate accepts
//! (W3); a review's binding is the code half of the diff, so a document landing
//! after the verdict leaves it standing while a code fix voids it (W4, DEC-45);
//! a design run rides beside the code without entering that binding and is
//! answered on its own `design/v1` (W4, DEC-46); a diff that changes no code
//! owes no review and is still signable (W7); the epic owes no review of its own
//! diff while code that landed outside any Working Unit still owes one of its
//! own (W9, DEC-51); and the epic walk re-reads every place code landed. The
//! second test puts a real failing test and
//! a malformed note in front of the review, where the preflight refuses them
//! before any finder would spawn (W2, DEC-43).
#![cfg(all(unix, feature = "hook-tests"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::process::Output;

#[path = "common/gitrepo.rs"]
mod gitrepo;
use gitrepo::{xtask_bin, GitRepo};

const SIGNER: &str = "a@b.co";
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const CONTRACT: &str = "docs/use-cases/api.html";
const DESIGN_RUN: &str = "docs/work/2026-09-15-run-1-badge.html";
const EPIC_REPORT: &str = "docs/.archive/2026-09-15-x-epic-result.html";

fn seeded_repo() -> GitRepo {
    let repo = GitRepo::new();
    repo.write(".gitignore", "target/\ndocs/.archive/\n");
    repo.write(".mr-allowed", &format!("email: {SIGNER}\n"));
    repo.write("src/lib.rs", "pub fn base() {}\n");
    repo.write(CONTRACT, "<html>v1</html>\n");
    repo.write(
        RULEBOOK,
        "<html><body><table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>colour-tokens-only</code></td></tr></tbody></table></body></html>\n",
    );
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

fn said_of(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn commit(repo: &GitRepo, msg: &str) -> String {
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", msg]);
    repo.sha("HEAD")
}

/// The judgment file lives under `.git/` so it never dirties the tree the
/// preflight measures.
fn write_notes(repo: &GitRepo, level: &str, target: &str, judgment: &str) -> (bool, String) {
    let path = repo.path().join(".git/judgment.yaml");
    std::fs::write(&path, judgment).unwrap();
    let out = repo.xtask(&[
        "notes",
        "write",
        level,
        "HEAD",
        "--target",
        target,
        "--judgment",
        path.to_str().unwrap(),
    ]);
    (out.status.success(), said_of(&out))
}

/// The binding the tool prints for the review to carry.
fn printed_code_sha(said: &str) -> String {
    said.lines()
        .find_map(|l| l.trim_start_matches("# ").strip_prefix("code_sha256: "))
        .unwrap_or_else(|| panic!("no code_sha256 line in: {said}"))
        .trim()
        .to_string()
}

fn gate(repo: &GitRepo, level: &str, target: &str) -> (bool, Vec<String>, String) {
    let out = repo.xtask(&["verify-dod", level, "HEAD", "--target", target]);
    let said = said_of(&out);
    let ids = said
        .lines()
        .filter_map(|l| l.strip_prefix("FAIL  "))
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    (out.status.success(), ids, said)
}

/// The commit level takes a sha and derives its own base - the commit's parent -
/// so it has no `--target` to pass.
fn gate_commit(repo: &GitRepo, sha: &str) -> (bool, Vec<String>, String) {
    let out = repo.xtask(&["verify-dod", "commit", sha]);
    let said = said_of(&out);
    let ids = said
        .lines()
        .filter_map(|l| l.strip_prefix("FAIL  "))
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    (out.status.success(), ids, said)
}

fn confirm(repo: &GitRepo, level: &str, target: &str, report: &str) {
    let out = repo.xtask(&[
        "mr", "confirm", level, "HEAD", "--as", SIGNER, "--target", target, "--report", report,
    ]);
    assert!(out.status.success(), "the human signs a settled tip: {}", said_of(&out));
}

fn merge(repo: &GitRepo, target: &str, branch: &str) -> (bool, String) {
    repo.git(&["checkout", "-q", target]);
    let out = repo.git_raw(&["merge", "--no-ff", "-m", &format!("merge {branch}"), branch]);
    if !out.status.success() {
        repo.git_raw(&["merge", "--abort"]);
    }
    (out.status.success(), said_of(&out))
}

fn has_note(repo: &GitRepo, note_ref: &str, tip: &str) -> bool {
    repo.git_raw(&["notes", &format!("--ref={note_ref}"), "show", tip]).status.success()
}

fn identity(wu: Option<&str>) -> String {
    match wu {
        Some(wu) => format!("epic: E-X\nwu: E-X.{wu}\nbranch: wu/x/{}\n", wu.to_lowercase()),
        None => "epic: E-X\nbranch: epic/x\n".to_string(),
    }
}

/// The note the review writes now: bound to the code paths, carrying the
/// round's bill with the clock W8 added, and no design section.
fn cr_v5(wu: Option<&str>, base: &str, code_sha: &str) -> String {
    format!(
        "schema: cr/v5\nscope: \"the caller's label\"\n{}base: {base}\nhead: reviewed\n\
         diff_sha256: {code_sha}\n\
         dimensions: [correctness, reuse, simplification, efficiency, altitude]\n\
         fleet:\n  finders: 5\n  verifiers: 2\n  verifier_model: sonnet-5\n  shards: 1\n  \
         candidates: {{ blocker: 0, important: 1, nit: 1 }}\n  \
         stage_seconds: {{ gate0: 58, finders: 145, verifiers: 90, total: 235 }}\n  \
         by_finder:\n    correctness: {{ found: 1, verified: 1 }}\n    reuse: {{ found: 1, verified: 0 }}\n\
         findings: {{ found: 2, verified: 1, waived: 0 }}\nwaivers: []\nnits: []\n\
         verdict: PASS\ndate: 2026-09-15\n",
        identity(wu)
    )
}

/// The design step's own verdict, pinned to the artifact bytes it read and the
/// rulebook the tip ships.
fn design_v1(repo: &GitRepo, wu: &str, base: &str, tip: &str) -> String {
    format!(
        "schema: design/v1\n{}base: {base}\ndesign_sha256: {}\nverdict: PASS\n\
         artifacts: [{DESIGN_RUN}]\nrulebook: {}\ndate: 2026-09-15\n",
        identity(Some(wu)),
        repo.paths_sha(base, tip, &[DESIGN_RUN]),
        &repo.blob(tip, RULEBOOK)[..7]
    )
}

fn closed_design_run(body: &str) -> String {
    format!(
        "<html><body><main data-artifact=\"design-run\" data-status=\"archived\">\
         <p>{body}</p></main></body></html>\n"
    )
}

const W1_JUDGMENT: &str = "\
wu: E-X.W1
tests:
  integration:
    files: [tests/wu.rs]
    covers: \"the WU's boundary\"
  e2e:
    files: [tests/e2e_x.rs]
    covers: \"the flow the epic later counts\"
docs:
  - path: docs/use-cases/api.html
    action: updated
    distills: \"the contract this WU settles\"
  - path: docs/work/w1-result.html
    action: added
    distills: \"the WU result - archived with the epic\"
";

const W2_JUDGMENT: &str = "\
wu: E-X.W2
tests:
  integration:
    files: [tests/two.rs]
    covers: \"the second WU's boundary\"
docs:
  - path: docs/work/2026-09-15-run-1-badge.html
    action: added
    distills: \"the badge's closed design run\"
  - path: docs/work/w2-result.html
    action: added
    distills: \"the WU result - archived with the epic\"
t2_waived: \"the run is the deliverable; no contract moved\"
";

const EPIC_JUDGMENT: &str = "\
epic: E-X
tests:
  integration:
    files: [tests/wu.rs, tests/two.rs]
    covers: \"each WU's boundary\"
  e2e:
    files: [tests/e2e_x.rs]
    covers: \"the whole flow\"
docs:
  - path: docs/use-cases/api.html
    action: updated
    distills: \"the contract W1 settled\"
";

/// W1 · one diff mixing code and documents, reviewed at its tip.
///
/// Returns the tip the seam lands, whose review note the epic walk re-reads.
fn land_w1_code_beside_docs(repo: &GitRepo) -> String {
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("tests/wu.rs", "#[test]\nfn the_contract_holds() {}\n");
    repo.write("tests/e2e_x.rs", "#[test]\nfn the_flow_holds() {}\n");
    repo.write(CONTRACT, "<html>v2 - the contract W1 settles</html>\n");
    repo.write("docs/work/w1-result.html", "<html>W1 result</html>\n");
    let tip = commit(repo, "W1: code and docs in one diff");
    let base = repo.sha("epic/x");

    // The tool writes the notes: derived fields the gate then accepts first time.
    let (ok, said) = write_notes(repo, "wu", "epic/x", W1_JUDGMENT);
    assert!(ok, "a judgment the range agrees with is written: {said}");
    assert!(said.contains("test/v2 + doc/v1"), "both notes, one command: {said}");
    let code = printed_code_sha(&said);
    assert_eq!(code, repo.code_sha(&base, &tip), "the tool prints the gate's own subset");
    assert_ne!(code, repo.diff_sha(&base, &tip), "the documents sit outside that subset");

    // A review bound to the whole diff is binding bytes it no longer judges.
    repo.note("cr", &tip, &cr_v5(Some("W1"), &base, &repo.diff_sha(&base, &tip)));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert!(ids.contains(&"5·diff-binding".to_string()), "a whole-diff binding is refused: {said}");

    repo.note("cr", &tip, &cr_v5(Some("W1"), &base, &code));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "only the human is still owed: {said}");

    // A document lands after the verdict: the same note rides to the moved tip.
    repo.write("docs/work/w1-plan.html", "<html>the plan the review never reads</html>\n");
    let moved = commit(repo, "W1: the plan lands after the review");
    let judgment = format!(
        "{W1_JUDGMENT}  - path: docs/work/w1-plan.html\n    action: added\n    \
         distills: \"the plan - archived with the epic\"\n"
    );
    let (ok, said) = write_notes(repo, "wu", "epic/x", &judgment);
    assert!(ok, "{said}");
    repo.git(&["notes", "--ref=cr", "copy", "-f", &tip, &moved]);
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "a document cannot void a PASS: {said}");

    // Code lands after the verdict: the same note is void where the code moved.
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\npub fn fixed() {}\n");
    let fixed = commit(repo, "W1: a fix after the review");
    let (ok, said) = write_notes(repo, "wu", "epic/x", &judgment);
    assert!(ok, "{said}");
    let rebound = printed_code_sha(&said);
    assert_ne!(rebound, code, "the fix moved the code half of the diff");
    repo.git(&["notes", "--ref=cr", "copy", "-f", &moved, &fixed]);
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert!(ids.contains(&"5·diff-binding".to_string()), "a code fix voids the PASS: {said}");

    repo.note("cr", &fixed, &cr_v5(Some("W1"), &base, &rebound));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "re-reviewed at the moved code: {said}");

    confirm(repo, "wu", "epic/x", "docs/work/w1-result.html");
    let (merged, said) = merge(repo, "epic/x", "wu/x/one");
    assert!(merged, "the first seam lands through the hook: {said}");
    fixed
}

/// W2 · a closed design run rides beside code. The review's binding never sees
/// the artifact; the design step's own note is what `9·design` reads.
fn land_w2_design_beside_code(repo: &GitRepo) -> String {
    repo.git(&["checkout", "-q", "-b", "wu/x/two", "epic/x"]);
    repo.write("src/two.rs", "pub fn two() {}\n");
    repo.write("tests/two.rs", "#[test]\nfn the_second_contract_holds() {}\n");
    repo.write(DESIGN_RUN, &closed_design_run("the badge, designed"));
    repo.write("docs/work/w2-result.html", "<html>W2 result</html>\n");
    let tip = commit(repo, "W2: a design run beside code");
    let base = repo.sha("epic/x");

    let (ok, said) = write_notes(repo, "wu", "epic/x", W2_JUDGMENT);
    assert!(ok, "{said}");
    let code = printed_code_sha(&said);
    assert_eq!(code, repo.code_sha(&base, &tip));

    repo.note("cr", &tip, &cr_v5(Some("W2"), &base, &code));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert!(
        ids.contains(&"9·design".to_string()),
        "the design verdict is owed on its own note: {said}"
    );
    assert!(
        !ids.contains(&"5·diff-binding".to_string()),
        "the code review is not what is owed: {said}"
    );

    repo.note("design", &tip, &design_v1(repo, "W2", &base, &tip));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "two verdicts, two notes, one seam: {said}");

    // The artifact moves after both verdicts: the design pin is void, the code
    // binding does not notice.
    repo.write(DESIGN_RUN, &closed_design_run("the badge, amended after the verdicts"));
    let amended = commit(repo, "W2: the run is amended");
    let (ok, said) = write_notes(repo, "wu", "epic/x", W2_JUDGMENT);
    assert!(ok, "{said}");
    assert_eq!(printed_code_sha(&said), code, "the artifact is outside the review's binding");
    repo.git(&["notes", "--ref=cr", "copy", "-f", &tip, &amended]);
    repo.git(&["notes", "--ref=design", "copy", "-f", &tip, &amended]);
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert!(ids.contains(&"9·design".to_string()), "the design verdict is void: {said}");
    assert!(!ids.contains(&"5·diff-binding".to_string()), "the review stands: {said}");

    repo.note("design", &amended, &design_v1(repo, "W2", &base, &amended));
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "{said}");

    confirm(repo, "wu", "epic/x", "docs/work/w2-result.html");
    let (merged, said) = merge(repo, "epic/x", "wu/x/two");
    assert!(merged, "the second seam lands through the hook: {said}");
    amended
}

/// W9 · code owes a review where it LANDS (DEC-51), and a Working Unit seam is
/// not the only place it lands: a fixup committed straight onto the epic branch
/// carries a commit-level `test/v2` and a `cr/v5` bound to its own code. Before
/// W9 the epic's own review was the backstop for it; the epic owes none now, so
/// this commit owes one. Returns its sha.
fn land_a_code_fixup_outside_any_wu(repo: &GitRepo) -> String {
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.write("src/fixup.rs", "pub fn fixed_in_place() {}\n");
    let tip = commit(repo, "a code fixup, straight onto the epic");
    let parent = repo.sha("HEAD^");
    repo.note(
        "test",
        &tip,
        "schema: test/v2\nlevel: commit\ntests:\n  unit: { count: 1, covers: \"the fixup\" }\n",
    );

    let (_, ids, said) = gate_commit(repo, &tip);
    assert_eq!(ids, vec!["1·notes"], "it owes a review of its own code: {said}");

    // Standalone commits omit epic/wu (workflow.html), so its review omits them
    // too - `1·notes` refuses a tip whose notes disagree about who owns it.
    let review = cr_v5(None, &parent, &repo.code_sha(&parent, &tip))
        .replace("epic: E-X\nbranch: epic/x\n", "");
    repo.note("cr", &tip, &review);
    let (green, ids, said) = gate_commit(repo, &tip);
    assert!(green, "and greens once it carries one: {ids:?} {said}");
    tip
}

/// W3 · a diff that changes no code owes no review and is still signable.
fn land_w3_docs_only(repo: &GitRepo) -> String {
    repo.git(&["checkout", "-q", "-b", "wu/x/three", "epic/x"]);
    repo.write("docs/work/w3-result.html", "<html>W3 result</html>\n");
    let tip = commit(repo, "W3: a working unit that changes no code");

    let (ok, said) = write_notes(repo, "wu", "epic/x", "wu: E-X.W3\n");
    assert!(ok, "an exempt diff is reported, not refused: {said}");
    assert!(!has_note(repo, "test", &tip), "nothing is written for a diff the merge exempts");
    let (_, ids, said) = gate(repo, "wu", "epic/x");
    assert_eq!(ids, vec!["8·manual-review"], "no review is demanded of it: {said}");

    confirm(repo, "wu", "epic/x", "docs/work/w3-result.html");
    let (merged, said) = merge(repo, "epic/x", "wu/x/three");
    assert!(merged, "the exempt seam lands through the hook: {said}");
    assert!(!has_note(repo, "cr", &tip), "and it carries no review note");
    tip
}

#[test]
fn an_epic_lands_on_main_with_every_review_bound_to_its_code_and_every_note_tool_written() {
    let repo = seeded_repo();
    repo.git(&["branch", "epic/x"]);

    let w1 = land_w1_code_beside_docs(&repo);
    land_w2_design_beside_code(&repo);
    land_w3_docs_only(&repo);
    let fixup = land_a_code_fixup_outside_any_wu(&repo);

    // The epic closes: the work docs leave the tree (DEC-3), the tool writes the
    // epic notes, the review binds the code half of the whole range.
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["rm", "-r", "-q", "docs/work"]);
    let tip = commit(&repo, "the work docs leave the tree");
    let base = repo.sha("main");
    let (ok, said) = write_notes(&repo, "epic", "main", EPIC_JUDGMENT);
    assert!(ok, "the epic's notes are written from one judgment: {said}");
    assert_eq!(printed_code_sha(&said), repo.code_sha(&base, &tip));

    // W9 · the epic owes no review of its own diff (DEC-51): its seams were each
    // reviewed at their own merge, so nothing re-judges their union. The tip
    // carries no cr note at all, and is signable and mergeable without one.
    assert!(!has_note(&repo, "cr", &tip), "no review is written for the epic's own diff");
    let (_, ids, said) = gate(&repo, "epic", "main");
    assert_eq!(ids, vec!["8·manual-review"], "the epic waits on its human alone: {said}");

    repo.write(EPIC_REPORT, "<html>the epic result</html>\n");
    confirm(&repo, "epic", "main", EPIC_REPORT);

    // What the epic never stopped owing: the walk re-reads every place code
    // landed, so a review that place has lost turns the epic red at the merge and
    // back green once restored. First the commit that landed code outside any
    // Working Unit - the position W9's own review round found unguarded.
    let fixup_review = repo.git(&["notes", "--ref=cr", "show", &fixup]);
    repo.git(&["notes", "--ref=cr", "remove", &fixup]);
    let (merged, said) = merge(&repo, "main", "epic/x");
    assert!(!merged, "code that landed outside a WU must carry its own review: {said}");
    assert!(said.contains("7·walk"), "the walk is what refuses it: {said}");
    repo.note("cr", &fixup, &fixup_review);

    // Then a seam's: the review left the epic tip; it never left the places code
    // actually lands.
    let w1_review = repo.git(&["notes", "--ref=cr", "show", &w1]);
    repo.git(&["notes", "--ref=cr", "remove", &w1]);
    let (merged, said) = merge(&repo, "main", "epic/x");
    assert!(!merged, "a seam whose review is gone must not reach main: {said}");
    assert!(said.contains("7·walk"), "the walk is what refuses it: {said}");

    repo.note("cr", &w1, &w1_review);
    let (merged, said) = merge(&repo, "main", "epic/x");
    assert!(merged, "the epic lands on main through the hook: {said}");
    assert_eq!(repo.sha("HEAD"), repo.sha("main"));
    assert_eq!(
        repo.git(&["rev-list", "--count", "--merges", "main"]),
        "4",
        "three WU seams and the epic's own merge"
    );
    assert!(
        !repo.git(&["ls-tree", "-r", "--name-only", "main"]).contains("docs/work/"),
        "no work doc is tracked on main"
    );
}

/// A tiny crate whose suite Gate 0 can actually run: the tree the preflight
/// measures is a real tree, red or green on a real test. `None` is the base;
/// `Some(n)` adds a function the test expects to return four.
fn crate_with_test(added_returns: Option<u32>) -> String {
    let base = "pub fn base() -> u32 {\n    2\n}\n";
    let Some(returns) = added_returns else {
        return base.to_string();
    };
    format!(
        "{base}\npub fn added() -> u32 {{\n    {returns}\n}}\n\n#[cfg(test)]\nmod tests {{\n    \
         #[test]\n    fn added_is_four() {{\n        assert_eq!(super::added(), 4);\n    }}\n}}\n"
    )
}

const CRATE_JUDGMENT: &str = "\
wu: E-X.W1
tests:
  unit:
    files: [src/lib.rs]
    covers: \"the crate's one function\"
  integration:
    waived: \"one function, no boundary to test\"
docs:
  - path: docs/work/w1-result.html
    action: added
    distills: \"the WU result\"
t2_waived: \"a fixture crate; no contract to distill\"
";

#[test]
fn a_failing_test_and_a_malformed_note_are_refused_before_any_finder_spawns() {
    let repo = GitRepo::new();
    repo.write(".gitignore", "target/\n");
    repo.write(".mr-allowed", &format!("email: {SIGNER}\n"));
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    );
    repo.write("src/lib.rs", &crate_with_test(None));
    commit(&repo, "init");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);

    repo.write("src/lib.rs", &crate_with_test(Some(5)));
    repo.write("docs/work/w1-result.html", "<html>W1 result</html>\n");
    commit(&repo, "W1: a change whose test fails");
    let (ok, said) = write_notes(&repo, "wu", "epic/x", CRATE_JUDGMENT);
    assert!(ok, "{said}");

    let red = repo.xtask(&["preflight", "wu", "HEAD", "--target", "epic/x"]);
    let said = said_of(&red);
    assert!(!red.status.success(), "a red suite stops the review: {said}");
    assert!(said.contains("FAIL  0·tree"), "the tree is named as the blocker: {said}");
    assert!(!has_note(&repo, "cr", "HEAD"), "no verdict exists for a tree nobody proved");

    repo.write("src/lib.rs", &crate_with_test(Some(4)));
    commit(&repo, "W1: the test passes");
    let (ok, said) = write_notes(&repo, "wu", "epic/x", CRATE_JUDGMENT);
    assert!(ok, "{said}");
    let green = repo.xtask(&["preflight", "wu", "HEAD", "--target", "epic/x"]);
    assert!(
        green.status.success(),
        "a green tree and its notes clear the preflight: {}",
        said_of(&green)
    );

    repo.note("test", "HEAD", "schema: test/v2\ntests: [not a manifest\n");
    let malformed = repo.xtask(&["preflight", "wu", "HEAD", "--target", "epic/x"]);
    let said = said_of(&malformed);
    assert!(!malformed.status.success(), "a malformed note stops the review: {said}");
    assert!(said.contains("FAIL  1·notes"), "the note is named as the blocker: {said}");
    assert!(!has_note(&repo, "cr", "HEAD"), "the preflight writes no verdict and spawns nothing");
}
