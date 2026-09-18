#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::PathBuf;

use tempfile::TempDir;
use xtask::gate::{format_report, Gate, Level};
use xtask::git::diff_sha256;
use xtask::notes::DodReceipt;

const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";

struct Repo {
    _dir: TempDir,
    path: PathBuf,
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
        repo.write("docs/use-cases/api.html", "<html>v1</html>\n");
        repo.write("docs/standard.html", "<html>t1 standard</html>\n");
        repo.write(RULEBOOK, "<html>the soft rules a design verdict is a claim about</html>\n");
        repo.commit("init");
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

    fn commit(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn commit_reviewed(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        let body = format!("{msg}\n\nReviewed-By: email a@b.co\n");
        self.git(&["commit", "-q", "--cleanup=verbatim", "-m", &body]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        let ref_arg = format!("--ref={note_ref}");
        self.git(&["notes", &ref_arg, "add", "-f", "-m", body, tip]);
    }

    fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev])
    }

    fn gate(&self) -> Gate<'_> {
        Gate { repo: &self.path }
    }

    fn keygen(&self) -> String {
        let key = self.path.join("signer");
        assert!(std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        std::fs::read_to_string(self.path.join("signer.pub")).unwrap().trim().to_string()
    }
}

fn identity(epic: &str, wu: Option<&str>) -> String {
    let wu_line = wu.map_or(String::new(), |w| format!("wu: {w}\n"));
    format!("epic: {epic}\n{wu_line}branch: test-branch\n")
}

fn test_note(level: &str, epic: &str, wu: Option<&str>, layers: &str) -> String {
    format!("schema: test/v2\nlevel: {level}\n{}tests:\n{layers}", identity(epic, wu))
}

fn cr_note(epic: &str, wu: Option<&str>, base: &str, sha: &str, verdict: &str) -> String {
    format!(
        "schema: cr/v2\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\nverdict: {verdict}\ndate: 2026-07-05\n",
        identity(epic, wu)
    )
}

#[test]
fn git_calls_ignore_inherited_hook_env() {
    std::env::set_var("GIT_DIR", "/nonexistent/.git");
    let repo = Repo::new();
    repo.write("src/lib.rs", "pub fn base() {}\npub fn more() {}\n");
    let tip = repo.commit("change");
    let base = repo.sha("HEAD~1");
    assert_eq!(diff_sha256(&repo.path, &base, &tip).unwrap().len(), 64);
    std::env::remove_var("GIT_DIR");
}

fn doc_note(epic: &str, wu: Option<&str>, changes: &str, t2_waived: Option<&str>) -> String {
    let waived = t2_waived.map_or(String::new(), |w| format!("t2_waived: \"{w}\"\n"));
    format!("schema: doc/v1\n{}changes:\n{changes}{waived}", identity(epic, wu))
}

const T2_ENTRY: &str = "  - path: docs/use-cases/api.html\n    tier: T2\n    action: updated\n    distills: \"api contract change\"\n";

const WU_RESULT: &str = "docs/work/one-result.html";
const WU_CHANGES: &str = "  - path: docs/use-cases/api.html\n    tier: T2\n    action: updated\n    distills: \"api contract change\"\n  - path: docs/work/one-result.html\n    tier: T3\n    action: added\n    distills: \"wu result report\"\n";

fn wu_with_change() -> Repo {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.commit("wu change");
    repo
}

/// An mr/v1 note attesting the WU result report tracked at `tip` — real path,
/// real sha256, the shape `mr confirm` writes (sign-what-you-read, DEC-10).
fn wu_mr_note(repo: &Repo, tip: &str) -> String {
    let bytes = xtask::git::show_bytes(&repo.path, tip, WU_RESULT).unwrap();
    let sha = xtask::git::sha256_hex(&bytes).unwrap();
    format!(
        "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: {WU_RESULT}\nreport_sha256: {sha}\ndate: 2026-07-06\n"
    )
}

fn bless_wu(repo: &Repo, tip: &str) {
    let base = repo.sha("epic/x");
    let sha = diff_sha256(&repo.path, &base, tip).unwrap();
    repo.note(
        "test",
        tip,
        &test_note(
            "working-unit",
            "E9",
            Some("E9.W1"),
            "  integration: { count: 1, covers: \"api contract\" }\n",
        ),
    );
    repo.note("doc", tip, &doc_note("E9", Some("E9.W1"), WU_CHANGES, None));
    repo.note("cr", tip, &cr_note("E9", Some("E9.W1"), &base, &sha, "PASS"));
    allow_file(repo);
    repo.note("mr", tip, &wu_mr_note(repo, tip));
}

fn checks_of(report: &xtask::gate::Report) -> Vec<&str> {
    report.failures.iter().map(|f| f.check.as_str()).collect()
}

#[test]
fn green_wu_passes_and_writes_a_parsable_receipt() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));

    repo.gate().write_receipt(&report).unwrap();
    let raw = repo.git(&["notes", "--ref=dod", "show", &tip]);
    let receipt: DodReceipt = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(receipt.schema, "dod/v1");
    assert_eq!(receipt.level, "working-unit");
    assert_eq!(receipt.epic.as_deref(), Some("E9"));
    assert_eq!(receipt.wu.as_deref(), Some("E9.W1"));
    assert_eq!(receipt.tip, tip);
    assert_eq!(receipt.diff_sha256, report.diff_sha256);
}

#[test]
fn missing_review_note_fails_check_1() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.git(&["notes", "--ref=cr", "remove", &tip]);

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["1·notes"]);
    assert!(format_report(&report).contains("code-review"));
}

#[test]
fn zero_count_without_waiver_fails_check_2() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note(
        "test",
        &tip,
        &test_note("working-unit", "E9", Some("E9.W1"), "  integration: { count: 0 }\n"),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["2·manifest"]);
    assert!(format_report(&report).contains("integration >= 1 or a waiver"));
}

#[test]
fn waived_layer_with_reason_passes() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note(
        "test",
        &tip,
        &test_note(
            "working-unit",
            "E9",
            Some("E9.W1"),
            "  integration:\n    waived: \"no new boundary - internal invariant only\"\n",
        ),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

#[test]
fn stale_review_base_fails_check_4() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let sha = diff_sha256(&repo.path, &repo.sha("epic/x"), &tip).unwrap();
    repo.note("cr", &tip, &cr_note("E9", Some("E9.W1"), &tip, &sha, "PASS"));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["4·review-base"]);
}

#[test]
fn tampered_diff_binding_fails_check_5() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let base = repo.sha("epic/x");
    repo.note("cr", &tip, &cr_note("E9", Some("E9.W1"), &base, "deadbeef", "PASS"));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["5·diff-binding"]);
}

#[test]
fn identity_disagreement_across_notes_fails_check_1() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let base = repo.sha("epic/x");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    repo.note("cr", &tip, &cr_note("E8", Some("E8.W9"), &base, &sha, "PASS"));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["1·notes"]);
    assert!(format_report(&report).contains("identity"));
}

#[test]
fn identity_omitted_in_one_note_while_siblings_carry_it_fails_check_1() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note("doc", &tip, &format!("schema: doc/v1\nchanges:\n{WU_CHANGES}"));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["1·notes"]);
    assert!(format_report(&report).contains("identity"));
}

#[test]
fn doc_note_must_account_for_every_changed_doc_file() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), "  []\n", Some("pure refactor")));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(!report.failures.is_empty());
    assert!(checks_of(&report).iter().all(|c| *c == "6·doc-note"), "{}", format_report(&report));
    assert!(format_report(&report).contains("docs/use-cases/api.html"));
}

// DEC-16: removing a design file is the irreversible step and its fork cards are
// what the removal loses, so the gate holds it until the distillation has landed.
// A dead-end reason clears the general lineage rule and must not clear this one.
fn wu_retiring_a_design_file(distilled_to: &str, with_decision: bool) -> Repo {
    let repo = Repo::new();
    repo.write("docs/work/components/button.html", "<html>the Button design</html>\n");
    repo.commit("the design file exists");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    if with_decision {
        repo.write("docs/decisions/dec-19-button-forks.html", "<html>why the label swap</html>\n");
    }
    std::fs::remove_file(repo.path.join("docs/work/components/button.html")).unwrap();
    repo.commit("retire the Button design file");

    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let decision = if with_decision {
        "  - path: docs/decisions/dec-19-button-forks.html\n    tier: T2\n    action: added\n    distills: \"Button's forks\"\n"
    } else {
        ""
    };
    let changes = format!(
        "{WU_CHANGES}{decision}  - path: docs/work/components/button.html\n    tier: T3\n    action: archived\n    distilled_to: {distilled_to}\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    repo
}

#[test]
fn a_design_file_may_not_be_removed_before_its_forks_are_distilled() {
    let repo = wu_retiring_a_design_file("\"none: local prop naming\"", false);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"6·retirement-order"),
        "a dead-end reason must not retire a design file: {}",
        format_report(&report)
    );

    let named_but_absent =
        wu_retiring_a_design_file("[docs/decisions/dec-19-button-forks.html]", false);
    let report = named_but_absent.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"6·retirement-order"),
        "naming a record that does not exist is not distilling to it: {}",
        format_report(&report)
    );
}

#[test]
fn a_design_file_retired_in_dec_16_order_passes() {
    let repo = wu_retiring_a_design_file("[docs/decisions/dec-19-button-forks.html]", true);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));
}

// DEC-16: "most fork cards are local and die with the file", and "a dead end named
// is not the same as a dead end assumed". Demanding a decision record refused the
// case DEC-16 calls the majority — measured over W7's own sixteen retirements, 2 of
// which correctly produced no record. So a dead end retires the file when it says
// WHAT CARRIES the fork; the reason that names nothing is still refused.
#[test]
fn a_dead_end_that_names_what_carries_the_fork_retires_the_file() {
    for reason in ["\"D1/D2 carried verbatim by @states\"", "\"the value cell is a @madeof edge\""]
    {
        let repo = wu_retiring_a_design_file(reason, false);
        let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
        assert!(report.green(), "{reason} names its carrier: {}", format_report(&report));
    }

    for reason in ["\"none\"", "\"nothing survives\"", "\"local prop shape\"", "\"  \""] {
        let repo = wu_retiring_a_design_file(reason, false);
        let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
        assert!(
            checks_of(&report).contains(&"6·retirement-order"),
            "{reason} names nothing - a dead end assumed: {}",
            format_report(&report)
        );
    }
}

// A rulebook slug is the other carrier, and it counts only when the rulebook really
// holds it: a citation nobody can follow is the same silence as no citation.
#[test]
fn a_rulebook_slug_carries_a_fork_only_while_the_rulebook_declares_it() {
    let repo = wu_retiring_a_design_file("\"superseded by wrap-not-truncate\"", false);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"6·retirement-order"),
        "the fixture rulebook declares no slugs, so the citation leads nowhere: {}",
        format_report(&report)
    );

    repo.write(
        RULEBOOK,
        "<table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>wrap-not-truncate</code></td></tr></tbody></table>\n",
    );
    repo.commit("the rulebook declares the slug the reason cites");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let changes = format!(
        "{WU_CHANGES}  - path: docs/work/components/button.html\n    tier: T3\n    action: archived\n    distilled_to: \"superseded by wrap-not-truncate\"\n  - path: {RULEBOOK}\n    tier: T2\n    action: updated\n    distills: \"the slug\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "now the citation resolves: {}", format_report(&report));
}

// The walk re-judges every landed seam with the working tree at the epic tip, so a
// citation has to be followed in the rulebook THAT SEAM shipped - what a slug means
// today is not what it meant when the retirement was reviewed. Reading the tree
// instead reddens history no one can fix: a gated branch is never amended, leaving
// a walk waiver as the only exit.
#[test]
fn a_seam_is_judged_against_the_rulebook_it_shipped_not_the_one_in_the_tree() {
    let repo = wu_retiring_a_design_file("\"superseded by wrap-not-truncate\"", false);
    repo.write(
        RULEBOOK,
        "<table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>wrap-not-truncate</code></td></tr></tbody></table>\n",
    );
    repo.commit("the rulebook declares the slug the reason cites");
    let seam = repo.sha("wu/x/one");
    bless_wu(&repo, &seam);
    let changes = format!(
        "{WU_CHANGES}  - path: docs/work/components/button.html\n    tier: T3\n    action: archived\n    distilled_to: \"superseded by wrap-not-truncate\"\n  - path: {RULEBOOK}\n    tier: T2\n    action: updated\n    distills: \"the slug\"\n"
    );
    repo.note("doc", &seam, &doc_note("E9", Some("E9.W1"), &changes, None));
    let report = repo.gate().verify(Level::Wu, &seam, "epic/x").unwrap();
    assert!(report.green(), "the seam ships the slug it cites: {}", format_report(&report));

    repo.write(
        RULEBOOK,
        "<table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>wrap-not-clip</code></td></tr></tbody></table>\n",
    );
    repo.commit("a later Working Unit renames the slug");
    let report = repo.gate().verify(Level::Wu, &seam, "epic/x").unwrap();
    assert!(
        report.green(),
        "the rename landed after this seam and cannot redden it: {}",
        format_report(&report)
    );
}

// Closing a run was nobody's step, so run #3 rode two Working Units open and red.
// The merge is where the tree becomes answerable for it: a run the diff carries is
// this seam's, and a seam that ends with it still open never closed it.
const DESIGN_RUN: &str = "docs/work/2026-08-13-dflow-run-4.html";

fn wu_carrying_a_design_run(status: &str) -> Repo {
    let repo = wu_with_change();
    let attr = if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
    repo.write(
        DESIGN_RUN,
        &format!("<html><body><main data-artifact=\"design-run\"{attr}><p>the run</p></main></body></html>\n"),
    );
    repo.commit("the fourth design run");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let changes = format!(
        "{WU_CHANGES}  - path: {DESIGN_RUN}\n    tier: T3\n    action: added\n    distills: \"the fourth run\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    repo
}

#[test]
fn a_design_run_left_open_at_the_tip_refuses_the_merge() {
    let repo = wu_carrying_a_design_run("");
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(checks_of(&report).contains(&"6·run-closed"), "{}", format_report(&report));
    assert!(format_report(&report).contains("archived"), "the fix names the close");

    let ready = wu_carrying_a_design_run("ready");
    let report = ready.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"6·run-closed"),
        "ready declares the run open, not closed: {}",
        format_report(&report)
    );
}

// A run still mid-design is closed by a later merge, and that merge is the one
// its design review rides. Only the closed one is design-bearing, which is why
// `draft` greens here on the same cr/v2 note the archived case is refused for.
#[test]
fn a_run_still_mid_design_passes_and_a_closed_one_needs_the_design_dimension() {
    let draft = wu_carrying_a_design_run("draft");
    let report = draft.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "draft: {}", format_report(&report));

    let closed = wu_carrying_a_design_run("archived");
    let report = closed.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design-schema"], "{}", format_report(&report));
    assert!(format_report(&report).contains("cr/v2"), "the red names the contract it found");
    assert!(format_report(&report).contains("design step"), "the fix names how to get one");
}

// The reviewer and the merge gate must never disagree about what is
// design-bearing, so they call one function - this is the reviewer's door to it.
#[test]
fn design_scope_reports_what_the_review_must_judge() {
    let repo = wu_carrying_a_design_run("archived");
    let args: Vec<String> =
        ["design-scope", "epic/x", "wu/x/one"].iter().map(|s| (*s).to_string()).collect();
    let (code, out) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 0);
    assert!(out.contains(DESIGN_RUN), "{out}");
    assert!(!out.contains("rulebook"), "nothing is pinned any more (DEC-53): {out}");

    let plain = wu_with_change();
    let (code, out) = xtask::run(&args, &plain.path).unwrap();
    assert_eq!(code, 0);
    assert!(out.contains("no design artifact"), "{out}");
}

fn design_section(artifacts: &[&str], verdict: &str) -> String {
    format!(
        "design:\n  artifacts: [{}]\n  findings: {{ found: 1, verified: 0, waived: 0 }}\n  verdict: {verdict}\n",
        artifacts.join(", ")
    )
}

/// The cr/v3 note the review writes for a design-bearing diff — the same
/// identity and binding as `cr_note`, plus the design dimension's verdict.
fn cr_note_v3(base: &str, sha: &str, design: &str) -> String {
    format!(
        "schema: cr/v3\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\nverdict: PASS\n{design}date: 2026-08-12\n",
        identity("E9", Some("E9.W1"))
    )
}

fn design_bearing_wu(design: &dyn Fn(&Repo, &str) -> String) -> (Repo, String) {
    let repo = wu_carrying_a_design_run("archived");
    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    repo.note("cr", &tip, &cr_note_v3(&base, &sha, &design(&repo, &tip)));
    (repo, tip)
}

fn verify_design_bearing(design: &dyn Fn(&Repo, &str) -> String) -> (Repo, xtask::gate::Report) {
    let (repo, _) = design_bearing_wu(design);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    (repo, report)
}

#[test]
fn a_complete_cr_v3_greens_a_design_bearing_tip_and_the_receipt_lists_the_check() {
    let (repo, report) = verify_design_bearing(&|_, _| design_section(&[DESIGN_RUN], "PASS"));
    assert!(report.green(), "{}", format_report(&report));
    let receipt = repo.gate().write_receipt(&report).unwrap();
    assert!(receipt.contains("design"), "the receipt names the check it passed: {receipt}");
}

#[test]
fn a_cr_v3_note_with_no_design_section_is_refused() {
    let (_repo, report) = verify_design_bearing(&|_, _| String::new());
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(format_report(&report).contains("cr/v3"));
}

// The silence the schema bump exists to break: one artifact judged, another in
// the same diff never opened, and a PASS covering both.
#[test]
fn a_design_section_that_skips_an_artifact_in_the_diff_is_refused() {
    let component = "docs/work/components/button.html";
    let repo = wu_carrying_a_design_run("archived");
    repo.write(component, "<html>the Button contract</html>\n");
    repo.commit("a component design file too");
    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    bless_wu(&repo, &tip);
    let changes = format!(
        "{WU_CHANGES}  - path: {DESIGN_RUN}\n    tier: T3\n    action: added\n    distills: \"the fourth run\"\n  - path: {component}\n    tier: T3\n    action: added\n    distills: \"the Button contract\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    let design = design_section(&[DESIGN_RUN], "PASS");
    repo.note("cr", &tip, &cr_note_v3(&base, &sha, &design));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(format_report(&report).contains(component), "the red names the artifact nobody read");
}

#[test]
fn a_failing_design_verdict_is_refused() {
    let (_repo, report) = verify_design_bearing(&|_, _| design_section(&[DESIGN_RUN], "FAIL"));
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
}

// The flow spec and two plans quote the declaration inside a <code> block. A
// substring scan would hold every doc that explains the contract to it.
#[test]
fn a_doc_quoting_the_declaration_is_not_a_run() {
    let repo = wu_with_change();
    repo.write(
        "docs/work/flow-spec.html",
        "<html><body><main data-artifact=\"wu-plan\"><p>a run declares \
         <code>data-artifact=\"design-run\"</code></p></main></body></html>\n",
    );
    repo.commit("the flow spec");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let changes = format!(
        "{WU_CHANGES}  - path: docs/work/flow-spec.html\n    tier: T3\n    action: added\n    distills: \"the flow spec\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));
}

fn epic_with_merged_wu() -> (Repo, String) {
    let repo = wu_with_change();
    let wu_tip = repo.sha("wu/x/one");
    bless_wu(&repo, &wu_tip);
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
    // T3 archival before the epic gate (DEC-3): the WU result leaves git; its
    // add + delete net out of the epic diff, so no doc-note entry is owed.
    std::fs::remove_file(repo.path.join(WU_RESULT)).unwrap();
    repo.commit("archive the wu result out of git");
    (repo, wu_tip)
}

fn mr_note(level: &str, tip: &str) -> String {
    format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: docs/.archive/e9-result.html\nreport_sha256: 0e9cafe\ndate: 2026-07-06\n"
    )
}

fn allow_file(repo: &Repo) {
    repo.write(".mr-allowed", "email: a@b.co\n");
}

fn bless_epic(repo: &Repo, changes: &str) {
    let tip = repo.sha("epic/x");
    let base = repo.sha("main");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    repo.note(
        "test",
        &tip,
        &test_note("epic", "E9", None, "  e2e: { count: 1, covers: \"full flow\" }\n"),
    );
    repo.note("doc", &tip, &doc_note("E9", None, changes, None));
    repo.note("cr", &tip, &cr_note("E9", None, &base, &sha, "PASS"));
    allow_file(repo);
    repo.note("mr", &tip, &mr_note("epic", &tip));
}

#[test]
fn green_epic_walk_verifies_the_merged_wu() {
    let (repo, _) = epic_with_merged_wu();
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

// W9 · a level that runs no review reads no review note at all. A standalone
// commit may carry one from an ad-hoc review run (SKILL.md permits reviewing
// recent commits and attaching the note), and parsing it at a level that demands
// nothing would let a note nobody asked for refuse a merge - including during an
// epic's own walk, which verifies every standalone commit at this level.
#[test]
fn a_commit_level_tip_never_reads_a_review_note_not_even_to_parse_it() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("src/extra.rs", "pub fn covered() {}\n");
    let sha = repo.commit("standalone fix");
    repo.note(
        "test",
        &sha,
        "schema: test/v2\nlevel: commit\ntests:\n  unit: { count: 1, covers: \"covered fn\" }\n",
    );

    let report = repo.gate().verify(Level::Commit, &sha, &format!("{sha}^")).unwrap();
    assert!(report.green(), "the baseline is green: {}", format_report(&report));

    repo.note("cr", &sha, "not: [valid, yaml: broken\n");
    let report = repo.gate().verify(Level::Commit, &sha, &format!("{sha}^")).unwrap();
    assert!(
        report.green(),
        "a note this level never demanded must not refuse it: {}",
        format_report(&report)
    );

    // The same malformed note where a review IS owed is still refused, so the
    // short-circuit is scoped to the level and is not a hole in note validation.
    let wu = wu_with_change();
    let tip = wu.sha("wu/x/one");
    bless_wu(&wu, &tip);
    wu.note("cr", &tip, "not: [valid, yaml: broken\n");
    let report = wu.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["1·notes"], "{}", format_report(&report));
}

// W9 · the receipt advertises the checks its level actually ran. The line used
// to be two hardcoded strings, and the non-epic one was wrong: it named the four
// review checks at commit level, where `check_review` has never run. Asserted
// here through the real gate over a real repo, at both levels, because the
// receipt is the only place a reader learns what was verified.
#[test]
fn the_receipt_names_the_checks_its_level_ran() {
    let (repo, _) = epic_with_merged_wu();
    bless_epic(&repo, T2_ENTRY);
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "{}", format_report(&report));
    let receipt = repo.gate().write_receipt(&report).unwrap();
    assert!(
        receipt.contains(
            "checks: [notes, manifest, review, review-nits, review-base, diff-binding, doc-note, \
             design, components-unbuilt, manual-review, walk]"
        ),
        "the epic's list: {receipt}"
    );

    let wu = wu_with_change();
    let tip = wu.sha("wu/x/one");
    bless_wu(&wu, &tip);
    let report = wu.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    let receipt = wu.gate().write_receipt(&report).unwrap();
    assert!(
        receipt.contains(
            "checks: [notes, manifest, review, review-nits, review-base, diff-binding, doc-note, \
             design, manual-review]"
        ),
        "a seam's list drops the epic's two rows: {receipt}"
    );

    let commit = repo.sha("epic/x");
    let report = repo.gate().verify(Level::Commit, &commit, &format!("{commit}^")).unwrap();
    let receipt = repo.gate().write_receipt(&report).unwrap();
    assert!(
        receipt.contains("checks: [notes, manifest, doc-note, design, manual-review]"),
        "a standalone commit runs no review, and the receipt no longer claims it does: {receipt}"
    );
}

// The design lives on the seam, not on the epic tip: work docs are archived out
// of git before the epic gate (DEC-3), so the run's add and delete net out of the
// epic diff. The seam is the only place a landed design review can be judged,
// which is exactly why the walk's treatment of it is the thing to pin.
fn epic_over_a_design_bearing_wu(wu_cr: &dyn Fn(&str, &str) -> String) -> Repo {
    let repo = wu_carrying_a_design_run("archived");
    let wu_tip = repo.sha("wu/x/one");
    let wu_base = repo.sha("epic/x");
    let wu_sha = diff_sha256(&repo.path, &wu_base, &wu_tip).unwrap();
    repo.note("cr", &wu_tip, &wu_cr(&wu_base, &wu_sha));
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
    std::fs::remove_file(repo.path.join(WU_RESULT)).unwrap();
    std::fs::remove_file(repo.path.join(DESIGN_RUN)).unwrap();
    repo.commit("archive the wu result and the run out of git");
    bless_epic(&repo, T2_ENTRY);
    repo
}

// A seam's cr/v2 note is evidence from before the design dimension existed, and
// no edit can make it say otherwise without claiming a judgment nobody made - so
// the walk drops that one check on immutable history. What it does NOT drop is a
// v3 note that omits its design section: that note's own schema says the
// dimension was available, which is the difference a date-based cutoff could
// never express.
#[test]
fn the_walk_grandfathers_a_cr_v2_seam_but_not_a_v3_one_missing_its_design_section() {
    let grandfathered =
        epic_over_a_design_bearing_wu(&|base, sha| cr_note("E9", Some("E9.W1"), base, sha, "PASS"));
    let report = grandfathered.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "a v2 seam predates the dimension: {}", format_report(&report));

    let claimed_v3 = epic_over_a_design_bearing_wu(&|base, sha| {
        cr_note("E9", Some("E9.W1"), base, sha, "PASS").replace("cr/v2", "cr/v3")
    });
    let report = claimed_v3.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["7·walk·9·design"], "{}", format_report(&report));
}

const COMPONENT_DESIGN: &str = "docs/work/components/status-pill.html";

// The epic that wrote a component contract and never built it. The contract is
// archived out of the tree before the epic gate, as check 6 demands - so the
// epic's endpoint diff carries no trace of it, and only the range does.
fn epic_over_an_unbuilt_component_design() -> Repo {
    let repo = wu_with_change();
    repo.write(COMPONENT_DESIGN, "<html><body>the StatusPill contract</body></html>\n");
    repo.commit("design the pill");
    let wu_tip = repo.sha("wu/x/one");
    bless_wu(&repo, &wu_tip);
    let changes = format!(
        "{WU_CHANGES}  - path: {COMPONENT_DESIGN}\n    tier: T3\n    action: added\n    distills: \"the pill contract\"\n"
    );
    repo.note("doc", &wu_tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
    std::fs::remove_file(repo.path.join(WU_RESULT)).unwrap();
    std::fs::remove_file(repo.path.join(COMPONENT_DESIGN)).unwrap();
    repo.commit("archive the wu result and the contract out of git");
    bless_epic(&repo, T2_ENTRY);
    repo
}

fn epic_cr_with_drop(repo: &Repo, why: &str) {
    let tip = repo.sha("epic/x");
    let base = repo.sha("main");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    // No rulebook pin and no design verdict: the epic's diff carries no design
    // artifact, so this section exists for the drop record and nothing else.
    let design =
        format!("design:\n  dropped:\n    - path: {COMPONENT_DESIGN}\n      why: \"{why}\"\n");
    repo.note(
        "cr",
        &tip,
        &format!("{}{design}", cr_note("E9", None, &base, &sha, "PASS").replace("cr/v2", "cr/v3")),
    );
}

// D3·E: the epic is the unit a design is coupled to, so an epic that carries a
// component design nobody built either builds it or says why it went. Check 6
// only forces the FILE out of the tree; a `git rm` with no record is the hole
// this closes.
#[test]
fn an_unbuilt_component_design_refuses_the_epic_until_the_drop_is_recorded() {
    let repo = epic_over_an_unbuilt_component_design();
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["components·unbuilt"], "{}", format_report(&report));
    assert!(report.failures[0].what.contains("StatusPill"), "the red names the component");

    epic_cr_with_drop(&repo, "  ");
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(
        checks_of(&report),
        vec!["components·unbuilt"],
        "a drop with no reason records nothing: {}",
        format_report(&report)
    );

    epic_cr_with_drop(&repo, "never built; the flow row renders its own status");
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "{}", format_report(&report));
    assert!(
        report.waived.iter().any(|w| w.contains("never built")),
        "the drop is billed as a waiver, not hidden beside one: {:?}",
        report.waived
    );
}

// The built case needs no record at all: the roster at the epic tip is the whole
// answer, and it is read from git rather than from the checkout.
#[test]
fn a_component_design_whose_component_landed_needs_no_drop_record() {
    let repo = epic_over_an_unbuilt_component_design();
    repo.write(
        "ui/src/components/StatusPill.tsx",
        "/**\n * StatusPill states one verdict.\n * @tier T1 — primitive\n */\nexport const StatusPill = () => null;\n",
    );
    repo.commit("build the pill");
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "{}", format_report(&report));
}

// The build side of the hand-over: the design landed in an earlier Working Unit,
// this one adds the component. The citation must resolve against the TIP, which
// is the only place the design still exists by then.
fn wu_building_a_component() -> Repo {
    let repo = Repo::new();
    repo.write(COMPONENT_DESIGN, "<html><body>the StatusPill contract</body></html>\n");
    repo.commit("an earlier WU landed the contract");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.write(
        "ui/src/components/StatusPill.tsx",
        "/**\n * StatusPill states one verdict.\n * @tier T1 — primitive\n */\nexport const StatusPill = () => null;\n",
    );
    repo.commit("build the pill");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo
}

#[test]
fn a_build_that_adds_a_component_must_cite_the_design_the_tip_ships() {
    let repo = wu_building_a_component();
    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    let v3 = |design: &str| {
        format!(
            "{}{design}",
            cr_note("E9", Some("E9.W1"), &base, &sha, "PASS").replace("cr/v2", "cr/v3")
        )
    };

    repo.note("cr", &tip, &v3(""));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(
        format_report(&report).contains("StatusPill.tsx"),
        "the red names the surface that owes a design"
    );

    let blob = repo.git(&["rev-parse", &format!("{tip}:{COMPONENT_DESIGN}")]);
    let cites = |pin: &str| {
        format!(
            "design:\n  implements:\n    - path: {COMPONENT_DESIGN}\n      blob: {pin}\n      built: [ui/src/components/StatusPill.tsx]\n"
        )
    };
    repo.note("cr", &tip, &v3(&cites(&blob[..7])));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "a resolvable citation lands it: {}", format_report(&report));

    // The comparison is against the tip's bytes, not the working tree's: editing
    // the design after the note was written must not silently stay green.
    repo.write(
        COMPONENT_DESIGN,
        "<html><body>the contract, revised after the note</body></html>\n",
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "an unstaged edit does not move the tip");
    repo.commit("revise the contract after the build cited it");
    let moved = repo.sha("wu/x/one");
    bless_wu(&repo, &moved);
    let moved_sha = diff_sha256(&repo.path, &base, &moved).unwrap();
    repo.note(
        "cr",
        &moved,
        &format!(
            "{}{}",
            cr_note("E9", Some("E9.W1"), &base, &moved_sha, "PASS").replace("cr/v2", "cr/v3"),
            cites(&blob[..7])
        ),
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"9·design"),
        "a stale blob is a citation of bytes the tip no longer ships: {}",
        format_report(&report)
    );
}

const PILL_WITH_CARRIERS: &str =
    "/**\n * StatusPill states one verdict.\n * @tier T1 — primitive\n\
 * @use one verdict inside a row\n * @avoid a sentence\n * @states rest · hover\n\
 * @madeof nothing\n */\nexport const StatusPill = () => null;\n";

// History B (DEC-28): the design landed on the epic, and the Working Unit that
// builds its component also retires the file - verify-doc's components·unretired
// refuses every turn while the two coexist, so the tip never ships the design the
// build read. With `amended`, the WU also edits the design before retiring it, so
// the last state is not the state the epic handed over.
fn wu_building_a_component_and_retiring_its_design(amended: bool) -> (Repo, String, String) {
    let repo = Repo::new();
    repo.git(&["checkout", "-q", "-b", "epic/x"]);
    repo.write(COMPONENT_DESIGN, "<html><body>the StatusPill contract</body></html>\n");
    let handed = repo.commit("the design WU landed the contract");
    let handed_blob = repo.git(&["rev-parse", &format!("{handed}:{COMPONENT_DESIGN}")]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    let mut last_blob = handed_blob.clone();
    if amended {
        repo.write(
            COMPONENT_DESIGN,
            "<html><body>the contract, amended before the build</body></html>\n",
        );
        let amend = repo.commit("amend the contract");
        last_blob = repo.git(&["rev-parse", &format!("{amend}:{COMPONENT_DESIGN}")]);
    }
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.write("ui/src/components/StatusPill.tsx", PILL_WITH_CARRIERS);
    std::fs::remove_file(repo.path.join(COMPONENT_DESIGN)).unwrap();
    let tip = repo.commit("build the pill and retire its design");
    bless_wu(&repo, &tip);
    let changes = format!(
        "{WU_CHANGES}  - path: {COMPONENT_DESIGN}\n    tier: T3\n    action: archived\n    distilled_to: \"the contract is carried verbatim by @states and @use\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    (repo, handed_blob, last_blob)
}

fn v3_note_with(repo: &Repo, design: &str) -> String {
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    cr_note_v3(&base, &sha, design)
}

fn implements(path: &str, pin: &str) -> String {
    format!("  implements:\n    - path: {path}\n      blob: {pin}\n      built: [ui/src/components/StatusPill.tsx]\n")
}

fn cites_the_design(repo: &Repo, path: &str, pin: &str) -> String {
    v3_note_with(repo, &format!("design:\n{}", implements(path, pin)))
}

/// The note a build writes when it also amended (or authored) the design it
/// retired: the design step accounting for the design's last state, and the
/// citation.
fn judges_and_cites_the_design(repo: &Repo, pin: &str) -> String {
    v3_note_with(
        repo,
        &format!(
            "{}{}",
            design_section(&[COMPONENT_DESIGN], "PASS"),
            implements(COMPONENT_DESIGN, pin)
        ),
    )
}

#[test]
fn a_build_that_retires_the_design_it_cites_lands_and_the_epic_walk_agrees() {
    let (repo, handed, _) = wu_building_a_component_and_retiring_its_design(false);
    let tip = repo.sha("wu/x/one");
    assert!(
        xtask::git::show_bytes(&repo.path, &tip, COMPONENT_DESIGN).is_err(),
        "the fixture is History B only if the tip does not ship the design"
    );

    repo.note("cr", &tip, &cites_the_design(&repo, COMPONENT_DESIGN, &handed[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        report.green(),
        "the citation resolves at the design's last state (DEC-28): {}",
        format_report(&report)
    );

    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
    std::fs::remove_file(repo.path.join(WU_RESULT)).unwrap();
    repo.commit("archive the wu result out of git");
    bless_epic(&repo, T2_ENTRY);
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "the walk re-asks the citation at the seam and history answers it: {}",
        format_report(&report)
    );
}

#[test]
fn a_citation_of_a_path_no_reachable_commit_ever_shipped_is_still_refused() {
    let (repo, handed, _) = wu_building_a_component_and_retiring_its_design(false);
    let tip = repo.sha("wu/x/one");
    let typo = "docs/work/components/staus-pill.html";
    repo.note("cr", &tip, &cites_the_design(&repo, typo, &handed[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    let said = format_report(&report);
    assert!(
        said.contains(&format!("{typo}, which no commit reachable from the tip ever shipped")),
        "the red names the path half, not the tip: {said}"
    );
}

#[test]
fn a_citation_of_bytes_a_retired_design_never_carried_names_its_last_state() {
    let (repo, handed, _) = wu_building_a_component_and_retiring_its_design(false);
    let tip = repo.sha("wu/x/one");
    repo.note("cr", &tip, &cites_the_design(&repo, COMPONENT_DESIGN, "0000000"));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    let said = format_report(&report);
    assert!(
        said.contains(&format!("was last shipped at {} (", &handed[..7])),
        "the red names the blob half - the last state to re-pin: {said}"
    );
    assert!(
        !said.contains("this tip ships"),
        "the tip ships nothing, and the red does not say it does: {said}"
    );
}

#[test]
fn a_design_amended_then_retired_in_one_wu_is_cited_at_its_last_state_not_its_first() {
    let (repo, handed, last) = wu_building_a_component_and_retiring_its_design(true);
    let tip = repo.sha("wu/x/one");
    assert_ne!(handed, last, "the amendment moved the bytes");

    repo.note("cr", &tip, &cites_the_design(&repo, COMPONENT_DESIGN, &handed[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"9·design"),
        "the build cites the bytes it was handed, not the bytes it left: {}",
        format_report(&report)
    );
    assert!(format_report(&report).contains(&format!("was last shipped at {}", &last[..7])));

    // Re-pinned, the citation resolves - and the amendment itself is still a design
    // artifact this WU carried, which nobody judged: DEC-25 D1a re-enters Tier-3 on
    // the merge that lands an amendment, whether or not the file also left the tree.
    repo.note("cr", &tip, &cites_the_design(&repo, COMPONENT_DESIGN, &last[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(checks_of(&report).iter().all(|c| *c == "9·design"), "{}", format_report(&report));
    assert!(
        format_report(&report).contains(&format!(
            "{COMPONENT_DESIGN} is a design artifact the design section does not name"
        )),
        "an amended-then-retired design is judged at its last state: {}",
        format_report(&report)
    );

    repo.note("cr", &tip, &judges_and_cites_the_design(&repo, &last[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        report.green(),
        "judged and re-pinned to the last state, it lands: {}",
        format_report(&report)
    );
}

// The one-WU design: authored, built and retired between the same base and tip,
// so the endpoint diff never shows the file at all. Nobody judged it before this
// merge, and this merge is the only one that can.
#[test]
fn a_design_authored_built_and_retired_in_one_wu_is_judged_by_that_wu() {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write(
        COMPONENT_DESIGN,
        "<html><body>the StatusPill contract, designed here</body></html>\n",
    );
    let designed = repo.commit("design the pill");
    let blob = repo.git(&["rev-parse", &format!("{designed}:{COMPONENT_DESIGN}")]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.write("ui/src/components/StatusPill.tsx", PILL_WITH_CARRIERS);
    std::fs::remove_file(repo.path.join(COMPONENT_DESIGN)).unwrap();
    let tip = repo.commit("build the pill and retire its design");
    bless_wu(&repo, &tip);
    assert!(
        !xtask::git::diff_names(&repo.path, &repo.sha("epic/x"), &tip)
            .unwrap()
            .contains(&COMPONENT_DESIGN.to_string()),
        "the endpoint diff nets the design out - only the range knows it existed"
    );

    // Judged and cited - and still refused: the design left the tree in this WU,
    // so DEC-16's retirement order is owed here too, and the default doc note
    // carries no archived entry for a file the endpoint diff never shows.
    repo.note("cr", &tip, &judges_and_cites_the_design(&repo, &blob[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"], "{}", format_report(&report));
    assert!(
        format_report(&report)
            .contains(&format!("{COMPONENT_DESIGN} changed in the diff but has no entry")),
        "the retirement is owed an entry: {}",
        format_report(&report)
    );

    let changes = format!(
        "{WU_CHANGES}  - path: {COMPONENT_DESIGN}\n    tier: T3\n    action: archived\n    distilled_to: \"the contract is carried verbatim by @states and @use\"\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &changes, None));
    repo.note("cr", &tip, &cites_the_design(&repo, COMPONENT_DESIGN, &blob[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(checks_of(&report).iter().all(|c| *c == "9·design"), "{}", format_report(&report));
    assert!(
        format_report(&report).contains("is a design artifact the design section does not name"),
        "a design nobody judged is refused even though the citation resolves: {}",
        format_report(&report)
    );

    repo.note("cr", &tip, &judges_and_cites_the_design(&repo, &blob[..7]));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        report.green(),
        "retired in order, judged at its last state and cited, it lands: {}",
        format_report(&report)
    );
}

// The reviewer's half of the same rule: design-scope names the retired design
// beside the ones the tip ships and says where its bytes are read - and only a
// design artifact counts, not every file that ever sat under the components dir.
#[test]
fn design_scope_names_a_design_retired_at_unjudged_bytes_and_where_to_read_it() {
    let (repo, _, last) = wu_building_a_component_and_retiring_its_design(true);
    repo.git(&["checkout", "-q", "wu/x/one"]);
    repo.write("docs/work/components/NOTES.md", "not a design\n");
    repo.write("docs/work/components/sub/draft.html", "<html>not a component design</html>\n");
    repo.commit("stray files under the components dir");
    std::fs::remove_file(repo.path.join("docs/work/components/NOTES.md")).unwrap();
    std::fs::remove_file(repo.path.join("docs/work/components/sub/draft.html")).unwrap();
    let tip = repo.commit("and gone again");
    let amend = repo.sha(&format!("{tip}~3"))[..10].to_string();

    let args: Vec<String> =
        ["design-scope", "epic/x", "wu/x/one"].iter().map(|s| (*s).to_string()).collect();
    let (code, out) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 design artifact(s) to judge"), "{out}");
    assert!(out.contains(COMPONENT_DESIGN), "{out}");
    assert!(
        out.contains("1 of them left the tree in this range"),
        "the count is of artifacts: {out}"
    );
    assert!(
        out.contains(&format!("git show {amend}:{COMPONENT_DESIGN}")),
        "the reviewer is told where the last state is read: {out}"
    );
    assert!(!out.contains("NOTES.md") && !out.contains("draft.html"), "not artifacts: {out}");
    assert_eq!(
        repo.git(&["rev-parse", &format!("{amend}:{COMPONENT_DESIGN}")])[..7],
        last[..7],
        "and that is the amended state the pin must match"
    );
}

// The case an added-only trigger can never see: a Working Unit that re-shapes a
// surface that already exists, which is where most UI work lives once the screens
// are built. No design is demanded of it - a gate that answered every edit with
// "go design it" would be routed around - but an answer is, and the answer is
// billed, so an epic can count what it shipped undesigned.
#[test]
fn a_build_that_only_edits_a_surface_owes_a_reason_and_the_reason_is_billed() {
    const SCREEN: &str = "ui/src/FlowDetail.tsx";
    let repo = Repo::new();
    repo.write(SCREEN, "export const FlowDetail = () => null;\n");
    repo.commit("a screen that landed before the design flow existed");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write(SCREEN, "export const FlowDetail = () => <section>rows</section>;\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.commit("re-shape the panel");

    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    bless_wu(&repo, &tip);
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    let v3 = |design: &str| {
        format!(
            "{}{design}",
            cr_note("E9", Some("E9.W1"), &base, &sha, "PASS").replace("cr/v2", "cr/v3")
        )
    };

    repo.note("cr", &tip, &v3(""));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(
        format_report(&report).contains(SCREEN),
        "the red names the file nothing accounts for: {}",
        format_report(&report)
    );

    repo.note(
        "cr",
        &tip,
        &v3(&format!(
            "design:\n  undesigned:\n    - path: {SCREEN}\n      why: \"a copy fix; the layout is untouched\"\n"
        )),
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    let text = format_report(&report);
    assert!(report.green(), "a stated reason answers for an edit: {text}");
    assert!(text.contains("WAIVE") && text.contains("a copy fix"), "and it is billed: {text}");
}

// Blesses the epic tip against the *merge-base* with main, not main's tip -
// once main and the epic diverge, base = merge-base(main, epic) != sha("main").
fn bless_epic_diverged(repo: &Repo, changes: &str) {
    let tip = repo.sha("epic/x");
    let base = repo.git(&["merge-base", "main", "epic/x"]);
    let sha = diff_sha256(&repo.path, &base, &tip).unwrap();
    repo.note(
        "test",
        &tip,
        &test_note("epic", "E9", None, "  e2e: { count: 1, covers: \"full flow\" }\n"),
    );
    repo.note("doc", &tip, &doc_note("E9", None, changes, None));
    repo.note("cr", &tip, &cr_note("E9", None, &base, &sha, "PASS"));
    allow_file(repo);
    repo.note("mr", &tip, &mr_note("epic", &tip));
}

// W1 / R1: a stale epic that back-merges an advanced main must walk clean. The
// case is divergent - main gained commits the epic lacks and vice-versa - so
// the existing `p2 ⊑ mainline` guard can't catch it; only target-reachability
// (`p2 ⊑ main`) does.
#[test]
fn back_merge_of_advanced_main_walks_clean() {
    let (repo, _) = epic_with_merged_wu();

    repo.git(&["checkout", "-q", "main"]);
    repo.write("src/mainlock.rs", "pub fn on_main() {}\n");
    repo.commit("main advances after the epic forked");

    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "back-merge main into epic", "main"]);
    bless_epic_diverged(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "a back-merge of landed history must not turn the walk red: {}",
        format_report(&report)
    );
}

// W1 / R2: the R2 guardrail - a genuinely ungated forward WU seam merged into
// the epic still turns the walk red. The target-reachability skip must not
// widen into a hole that swallows real seams.
#[test]
fn ungated_forward_wu_seam_still_walks_red() {
    let repo = wu_with_change();
    repo.git(&["checkout", "-q", "-b", "wu/x/forward", "epic/x"]);
    repo.write("src/forward.rs", "pub fn forward() {}\n");
    repo.commit("forward WU work, never gated");
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge ungated forward wu", "wu/x/forward"]);
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        checks_of(&report).iter().any(|c| c.starts_with("7·walk")),
        "an ungated forward WU seam must turn the walk red: {}",
        format_report(&report)
    );
}

// Rewrites the epic tip's cr/v2 note (already written by bless_epic) with a
// walk_waivers block appended, preserving the diff-bound base/hash/verdict.
fn cr_note_with_waivers(epic: &str, base: &str, sha: &str, waivers: &str) -> String {
    format!("{}walk_waivers:\n{waivers}", cr_note(epic, None, base, sha, "PASS"))
}

// Builds an epic whose walk has one genuinely ungated forward WU seam, returns
// (repo, that seam's merge-commit sha) so a test can waive exactly it.
fn epic_with_one_red_seam() -> (Repo, String) {
    let repo = wu_with_change();
    repo.git(&["checkout", "-q", "-b", "wu/x/forward", "epic/x"]);
    repo.write("src/forward.rs", "pub fn forward() {}\n");
    repo.commit("forward WU work, never gated");
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge ungated forward wu", "wu/x/forward"]);
    let seam = repo.sha("epic/x");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);
    (repo, seam)
}

// W2 / R5: a matching walk_waivers entry on the epic tip's cr/v2 note suppresses
// exactly that seam's walk failure - the gate goes green and reports it waived,
// while a blank why does not waive and the red output instructs the exact edit.
#[test]
fn walk_waiver_on_the_cr_note_suppresses_one_red_seam() {
    let (repo, seam) = epic_with_one_red_seam();
    let base = repo.sha("main");
    let sha = diff_sha256(&repo.path, &base, &repo.sha("epic/x")).unwrap();

    let entry = format!("  - seam: {seam}\n    why: \"waived: reviewed, safe to skip\"\n");
    repo.note("cr", &repo.sha("epic/x"), &cr_note_with_waivers("E9", &base, &sha, &entry));
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "a matching walk_waivers entry must make the gate green: {}",
        format_report(&report)
    );
    assert!(
        format_report(&report).contains("waived"),
        "the waived seam must be visible in the output: {}",
        format_report(&report)
    );
}

#[test]
fn walk_waiver_with_a_blank_why_does_not_suppress_the_seam() {
    let (repo, seam) = epic_with_one_red_seam();
    let base = repo.sha("main");
    let sha = diff_sha256(&repo.path, &base, &repo.sha("epic/x")).unwrap();

    let entry = format!("  - seam: {seam}\n    why: \"   \"\n");
    repo.note("cr", &repo.sha("epic/x"), &cr_note_with_waivers("E9", &base, &sha, &entry));
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        checks_of(&report).iter().any(|c| c.starts_with("7·walk")),
        "a blank why must not waive the seam: {}",
        format_report(&report)
    );
    let text = format_report(&report);
    assert!(text.contains("walk_waivers"), "the fix must instruct the waiver edit: {text}");
    assert!(text.contains("--no-verify"), "the fix must warn off --no-verify: {text}");
}

// The waiver is surgical: it suppresses the named walk seam only, never the
// tip's own non-walk checks. With the seam waived but the mr/v1 note removed,
// the gate stays red on manual-review - the waiver cannot widen into a hole.
#[test]
fn walk_waiver_leaves_the_tips_own_checks_running() {
    let (repo, seam) = epic_with_one_red_seam();
    let base = repo.sha("main");
    let sha = diff_sha256(&repo.path, &base, &repo.sha("epic/x")).unwrap();

    let entry = format!("  - seam: {seam}\n    why: \"waived: reviewed, safe to skip\"\n");
    repo.note("cr", &repo.sha("epic/x"), &cr_note_with_waivers("E9", &base, &sha, &entry));
    repo.git(&["notes", "--ref=mr", "remove", &repo.sha("epic/x")]);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(
        checks_of(&report),
        vec!["8·manual-review"],
        "the seam is waived but the tip's manual-review check still fires: {}",
        format_report(&report)
    );
}

// E-GATEBM epic e2e: the whole flow through one epic walk. A stale epic carries
// a gated WU seam, back-merges an advanced main, AND carries a genuinely ungated
// forward WU seam. Step 1 (W1): the back-merge integration seam walks clean.
// Step 2 (R2): the ungated forward seam turns the walk red. Step 3 (W2): a
// walk_waivers entry on the tip's cr note waives exactly that seam -> green;
// blanking its why -> red again with the instructing fix. One test, both WUs.
#[test]
fn e2e_backmerge_then_r2_guardrail_then_waiver_valve() {
    let (repo, _) = epic_with_merged_wu();

    repo.git(&["checkout", "-q", "main"]);
    repo.write("src/mainlock.rs", "pub fn on_main() {}\n");
    repo.commit("main advances after the epic forked");
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "back-merge advanced main into epic", "main"]);

    repo.git(&["checkout", "-q", "-b", "wu/x/forward", "epic/x"]);
    repo.write("src/forward.rs", "pub fn forward() {}\n");
    repo.commit("forward WU work, never gated");
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge ungated forward wu", "wu/x/forward"]);
    let forward_seam = repo.sha("epic/x");
    bless_epic_diverged(&repo, T2_ENTRY);

    // Steps 1 + 2: the back-merge seam is skipped (W1); only the ungated forward
    // seam is red (R2). Exactly one walk failure, on that seam.
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    let walk_failures: Vec<&str> =
        checks_of(&report).into_iter().filter(|c| c.starts_with("7·walk")).collect();
    assert!(
        !walk_failures.is_empty() && report.failures.iter().all(|f| f.check.starts_with("7·walk")),
        "the back-merge must walk clean while the ungated forward seam stays red: {}",
        format_report(&report)
    );

    // Step 3a (W2): waive that seam on the tip's cr note -> the epic gate is green.
    let base = repo.git(&["merge-base", "main", "epic/x"]);
    let sha = diff_sha256(&repo.path, &base, &repo.sha("epic/x")).unwrap();
    let entry = format!(
        "  - seam: {forward_seam}\n    why: \"live-integrated, gated on its own branch\"\n"
    );
    repo.note("cr", &repo.sha("epic/x"), &cr_note_with_waivers("E9", &base, &sha, &entry));
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green() && format_report(&report).contains("waived"),
        "waiving the forward seam must make the epic gate green with a visible waived line: {}",
        format_report(&report)
    );

    // Step 3b: blank the why -> the seam is red again, with the instructing fix.
    let entry = format!("  - seam: {forward_seam}\n    why: \"\"\n");
    repo.note("cr", &repo.sha("epic/x"), &cr_note_with_waivers("E9", &base, &sha, &entry));
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    let text = format_report(&report);
    assert!(
        checks_of(&report).iter().any(|c| c.starts_with("7·walk")) && text.contains("--no-verify"),
        "a blank why must leave the seam red with the instructing fix: {text}"
    );
}

#[test]
fn epic_gate_rejects_tracked_work_docs() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/work/plan.html", "<html>plan</html>\n");
    repo.commit("plan doc");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/work/plan.html\n    tier: T3\n    action: added\n    distills: \"in flight\"\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("still tracked"));
    assert!(format_report(&report).contains("docs/.archive/"));
}

#[test]
fn epic_gate_rejects_docs_in_unexpected_locations() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/scratch/notes.html", "<html>stray</html>\n");
    repo.commit("stray doc dir");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/scratch/notes.html\n    tier: T1\n    action: added\n    distills: \"stray\"\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("expected doc locations"));
}

// docs/backlog/ is the third T2 KB (living roadmap + deferred work): a tracked
// file there must pass both the epic tracked-doc layout check and the doc-note
// tier derivation (T2), exactly like use-cases/ and decisions/.
#[test]
fn epic_gate_accepts_a_tracked_backlog_kb_doc() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/backlog/roadmap.html", "<html>roadmap</html>\n");
    repo.commit("relocate roadmap into the backlog kb");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/backlog/roadmap.html\n    tier: T2\n    action: added\n    distills: \"roadmap relocated to the living backlog kb\"\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "docs/backlog/ is an expected tracked T2 location: {}",
        format_report(&report)
    );
}

// docs/design-rules/ is the fourth T2 KB (living design rulebook + vocabularies,
// E-DFLOW.W3): a tracked file there must pass both the epic tracked-doc layout
// check and the doc-note tier derivation (T2), like the other three KBs.
#[test]
fn epic_gate_accepts_a_tracked_design_rules_kb_doc() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/design-rules/design-rulebook.html", "<html>rulebook</html>\n");
    repo.commit("home the design rulebook in the design-rules kb");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/design-rules/design-rulebook.html\n    tier: T2\n    action: added\n    distills: \"design rulebook homed in the living design-rules kb\"\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "docs/design-rules/ is an expected tracked T2 location: {}",
        format_report(&report)
    );
}

#[test]
fn docs_only_epic_still_rejects_stray_tracked_work_docs() {
    let repo = Repo::new();
    allow_file(&repo);
    repo.git(&["checkout", "-q", "-b", "epic/x"]);
    repo.write("docs/work/plan.html", "<html>plan</html>\n");
    repo.commit("docs-only epic leaves a work doc");
    repo.note("mr", &repo.sha("epic/x"), &mr_note("epic", &repo.sha("epic/x")));

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(!report.exempt, "a dirty docs tree must not be exempt");
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);

    std::fs::remove_file(repo.path.join("docs/work/plan.html")).unwrap();
    repo.commit("archive the work doc out of git");
    repo.note("mr", &repo.sha("epic/x"), &mr_note("epic", &repo.sha("epic/x")));
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.exempt, "a clean docs-only epic with an mr note stays exempt");
}

#[test]
fn work_doc_archived_out_of_git_passes_the_epic_gate() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/work/plan.html", "<html>plan</html>\n");
    repo.commit("plan doc - t3 needs no trailer");
    std::fs::remove_file(repo.path.join("docs/work/plan.html")).unwrap();
    repo.commit("archive the plan out of git");
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

#[test]
fn archived_doc_note_entry_requires_distillation_lineage() {
    let repo = Repo::new();
    repo.write("docs/work/plan.html", "<html>plan</html>\n");
    repo.commit("work doc on main");
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    std::fs::remove_file(repo.path.join("docs/work/plan.html")).unwrap();
    let tip = repo.commit("wu change archives the plan");
    bless_wu(&repo, &tip);
    let bare =
        format!("{WU_CHANGES}  - path: docs/work/plan.html\n    tier: T3\n    action: archived\n");
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &bare, None));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("lineage"));

    let with_lineage = format!(
        "{WU_CHANGES}  - path: docs/work/plan.html\n    tier: T3\n    action: archived\n    distilled_to: [docs/use-cases/api.html]\n"
    );
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &with_lineage, None));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

fn close_epic(repo: &Repo) {
    repo.write("docs/use-cases/api.html", "<html>v3, closing</html>\n");
    repo.commit("docs-only closing commit");
}

#[test]
fn epic_walk_flags_a_noteless_standalone_code_commit() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("src/extra.rs", "pub fn stray() {}\n");
    repo.commit("stray code commit without manifest");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    let checks = checks_of(&report);
    assert!(
        checks.iter().any(|c| c.starts_with("7·walk·1·notes")),
        "expected a walk failure, got: {}",
        format_report(&report)
    );
}

#[test]
fn standalone_commit_gets_green_with_its_manifest() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("src/extra.rs", "pub fn covered() {}\n");
    let sha = repo.commit("standalone fix");
    repo.note(
        "test",
        &sha,
        "schema: test/v2\nlevel: commit\ntests:\n  unit: { count: 1, covers: \"covered fn\" }\n",
    );
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

fn second_wu(repo: &Repo) {
    repo.git(&["checkout", "-q", "-b", "wu/x/two", "epic/x"]);
    repo.write("src/other.rs", "pub fn other() {}\n");
    repo.commit("second wu change");
}

#[test]
fn epic_walk_rejects_octopus_merges() {
    let repo = wu_with_change();
    second_wu(&repo);
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "octopus merge", "wu/x/one", "wu/x/two"]);
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        format_report(&report).contains("octopus"),
        "expected an octopus rejection, got: {}",
        format_report(&report)
    );
}

#[test]
fn hook_mode_refuses_octopus_merges() {
    let repo = wu_with_change();
    second_wu(&repo);
    repo.git(&["checkout", "-q", "epic/x"]);
    let heads = format!("{}\n{}\n", repo.sha("wu/x/one"), repo.sha("wu/x/two"));
    std::fs::write(repo.path.join(".git/MERGE_HEAD"), heads).unwrap();

    let args: Vec<String> = ["verify-dod", "hook", "wu"].iter().map(|s| (*s).to_string()).collect();
    let (code, message) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 1);
    assert!(message.contains("octopus"), "got: {message}");
}

fn docs_only_wu(repo: &Repo) {
    repo.git(&["checkout", "-q", "-b", "wu/x/docs", "epic/x"]);
    repo.write("docs/use-cases/api.html", "<html>docs-only wu</html>\n");
    repo.commit("docs-only wu change");
}

#[test]
fn docs_only_wu_seam_is_exempt_in_the_epic_walk() {
    let (repo, _) = epic_with_merged_wu();
    docs_only_wu(&repo);
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge noteless docs-only wu", "wu/x/docs"]);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

#[test]
fn docs_only_wu_gate_requires_mr_then_is_exempt_without_a_receipt() {
    let repo = wu_with_change();
    docs_only_wu(&repo);
    allow_file(&repo);

    // Manual review is always required at a WU tip, docs-only or not — it runs
    // before the exemption, like the epic rule (DEC-10).
    let args: Vec<String> =
        ["verify-dod", "wu", "wu/x/docs"].iter().map(|s| (*s).to_string()).collect();
    let (code, message) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 1, "a docs-only WU still needs its mr note: {message}");
    assert!(message.contains("8·manual-review"), "got: {message}");

    repo.write(WU_RESULT, "<html>docs-only wu result</html>\n");
    let tip = repo.commit("wu result report");
    repo.note("mr", &tip, &wu_mr_note(&repo, &tip));
    let (code, message) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 0, "got: {message}");
    assert!(message.contains("exempt"), "got: {message}");
    assert!(
        xtask::git::git(&repo.path, &["notes", "--ref=dod", "show", &tip]).is_err(),
        "no receipt may be written for an exempt tip"
    );
}

// A design is a docs-only deliverable, so the exemption that lets incidental
// doc changes through would hand the design dimension exactly the diffs it
// exists to judge: E-DFLOW.W2 landed 14 component design files with no code
// review at all, on this rule (E-DFLOW.W7 ⑤).
#[test]
fn a_docs_only_wu_carrying_a_design_artifact_is_not_exempt() {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/design", "epic/x"]);
    repo.write(
        DESIGN_RUN,
        "<html><body><main data-artifact=\"design-run\" data-status=\"archived\">\
         <p>the run</p></main></body></html>\n",
    );
    repo.write(WU_RESULT, "<html>the design WU result</html>\n");
    let tip = repo.commit("a design run and nothing else");
    allow_file(&repo);
    repo.note("mr", &tip, &wu_mr_note(&repo, &tip));

    let report = repo.gate().verify(Level::Wu, "wu/x/design", "epic/x").unwrap();
    assert!(!report.exempt, "a design-bearing diff is the gate's subject, not its exception");
    assert!(
        checks_of(&report).contains(&"1·notes"),
        "the design deliverable needs the same evidence any other does: {}",
        format_report(&report)
    );
}

// ...and the exemption itself is untouched: a docs change that designs nothing
// still passes ungated, which is the whole point of having one.
#[test]
fn a_docs_only_wu_that_designs_nothing_stays_exempt() {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    docs_only_wu(&repo);
    repo.write(WU_RESULT, "<html>docs-only wu result</html>\n");
    let tip = repo.commit("wu result report");
    allow_file(&repo);
    repo.note("mr", &tip, &wu_mr_note(&repo, &tip));

    let report = repo.gate().verify(Level::Wu, "wu/x/docs", "epic/x").unwrap();
    assert!(report.exempt, "unexpected failures: {}", format_report(&report));
}

#[test]
fn docs_only_standalone_commit_is_exempt() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/use-cases/api.html", "<html>v2.5, interior</html>\n");
    repo.commit("docs-only t2 tweak, no manifest, no trailer");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

#[test]
fn every_epic_without_an_mr_note_fails_check_8() {
    let (repo, _) = epic_with_merged_wu();
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);
    repo.git(&["notes", "--ref=mr", "remove", &repo.sha("epic/x")]);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(format_report(&report).contains("MUST NOT self-authorise"));
}

#[test]
fn epic_mr_note_with_an_unlisted_signer_fails_check_8() {
    let (repo, _) = epic_with_merged_wu();
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);
    repo.write(".mr-allowed", "email: someone-else@x.co\n");

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
}

#[test]
fn every_wu_requires_an_mr_note() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.git(&["notes", "--ref=mr", "remove", &tip]);

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(
        format_report(&report).contains("every working-unit merge requires manual review"),
        "{}",
        format_report(&report)
    );

    repo.note("mr", &tip, &wu_mr_note(&repo, &tip));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        report.green(),
        "a listed mr note attesting the report clears check 8: {}",
        format_report(&report)
    );
}

#[test]
fn a_wu_mr_note_without_a_report_fails_check_8() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note(
        "mr",
        &tip,
        &format!("schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\n"),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(
        format_report(&report).contains("names no result report"),
        "{}",
        format_report(&report)
    );
}

#[test]
fn a_wu_mr_note_whose_report_hash_differs_fails_check_8() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note(
        "mr",
        &tip,
        &format!("schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: {WU_RESULT}\nreport_sha256: 0000tampered\n"),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(format_report(&report).contains("differs"), "{}", format_report(&report));
}

#[test]
fn a_wu_mr_note_naming_an_untracked_report_fails_check_8() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    repo.note(
        "mr",
        &tip,
        &format!("schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: docs/work/ghost-result.html\nreport_sha256: 0000\n"),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(
        format_report(&report).contains("not tracked at the tip"),
        "{}",
        format_report(&report)
    );
}

#[test]
fn an_epic_mr_note_without_a_report_fails_check_8() {
    let (repo, _) = epic_with_merged_wu();
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);
    let tip = repo.sha("epic/x");
    repo.note(
        "mr",
        &tip,
        &format!("schema: mr/v1\nlevel: epic\ntip: {tip}\ntier: email\nsigner: a@b.co\n"),
    );

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    assert!(
        format_report(&report).contains("names no result report"),
        "{}",
        format_report(&report)
    );
}

// Same boundary-only rule as doc commits (DEC-9, extended by DEC-10): mr/v1
// verifies against the mutable, local .mr-allowed, so the walk never re-judges
// merged WU tips — the WU merge hook gated the seam when it was made.
#[test]
fn epic_walk_never_rejudges_wu_manual_review() {
    let (repo, wu_tip) = epic_with_merged_wu();
    repo.git(&["notes", "--ref=mr", "remove", &wu_tip]);
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "a historical WU seam without an mr note must walk clean: {}",
        format_report(&report)
    );
}

#[test]
fn epic_walk_never_rejudges_doc_commits() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/standard.html", "<html>rule-bearing edit without a trailer</html>\n");
    repo.commit("t1 doc edit, no mr trailer - authorisation is boundary-only");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        !checks_of(&report).contains(&"8·manual-review"),
        "the walk must not re-judge immutable history against the mutable key list: {}",
        format_report(&report)
    );
}

#[test]
fn a_doc_commit_note_authorises_at_the_pre_push_seam_when_the_trailer_is_absent() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.write("docs/standard.html", "<html>edited without a trailer</html>\n");
    let doc_commit = repo.commit("t1 doc edit, no mr trailer");

    let (code, out) = range_run(&repo, &old, &doc_commit);
    assert_eq!(code, 1, "untrailered doc commit is red before a note: {out}");

    repo.note("mr", &doc_commit, &mr_note("commit", &doc_commit));
    let (code, out) = range_run(&repo, &old, &doc_commit);
    assert_eq!(code, 0, "an mr/v1 note on the doc commit authorises it: {out}");
}

fn range_run(repo: &Repo, old: &str, new: &str) -> (i32, String) {
    let args: Vec<String> = ["verify-dod", "range", old, new].map(String::from).into();
    xtask::run(&args, &repo.path).unwrap()
}

#[test]
fn range_rejects_an_unsigned_doc_commit_from_a_key_backed_identity() {
    let repo = Repo::new();
    let pubkey = repo.keygen();
    repo.write(".mr-allowed", &format!("email: a@b.co\nkey: vu {pubkey}\n"));
    let old = repo.sha("HEAD");
    repo.write("docs/standard.html", "<html>forged key trailer, no signature</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--cleanup=verbatim", "-m", "doc edit\n\nReviewed-By: key vu\n"]);
    let new = repo.sha("HEAD");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(
        code, 1,
        "a key-backed trailer without a verifying gpgsig must not authorise: {out}"
    );
    assert!(out.contains("8·manual-review"), "{out}");
}

#[test]
fn range_accepts_a_signed_doc_commit_from_a_key_backed_identity() {
    let repo = Repo::new();
    let pubkey = repo.keygen();
    repo.write(".mr-allowed", &format!("email: a@b.co\nkey: vu {pubkey}\n"));
    repo.git(&["config", "gpg.format", "ssh"]);
    repo.git(&["config", "user.signingkey", repo.path.join("signer").to_str().unwrap()]);
    let old = repo.sha("HEAD");
    repo.write("docs/standard.html", "<html>signed doc edit</html>\n");
    repo.git(&["add", "-A"]);
    repo.git(&[
        "commit",
        "-q",
        "-S",
        "--cleanup=verbatim",
        "-m",
        "doc edit\n\nReviewed-By: key vu\n",
    ]);
    let new = repo.sha("HEAD");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 0, "a doc commit signed by the listed key authorises: {out}");
}

#[test]
fn range_rejects_an_unauthorised_doc_commit_at_the_pre_push_seam() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.write("docs/standard.html", "<html>unauthorised edit</html>\n");
    let new = repo.commit("t1 doc edit, no trailer");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("8·manual-review"), "{out}");
}

#[test]
fn range_accepts_a_listed_identity_trailered_doc_commit() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.write("docs/standard.html", "<html>reviewed edit</html>\n");
    let new = repo.commit_reviewed("t1 doc edit");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn range_flags_a_doc_moved_out_of_docs_without_a_trailer() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.git(&["mv", "docs/standard.html", "notes.md"]);
    let new = repo.commit("move a t1 doc out of docs/, no trailer");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 1, "a rename out of docs/ must not bypass the trailer check: {out}");
    assert!(out.contains("8·manual-review"), "{out}");
}

// The DEC-10 narrowing: T2/T3 doc commits carry no trailer — their human
// coverage moved to the WU/epic boundary mr — while T0 gains the commit gate.
#[test]
fn range_ignores_untrailered_t2_and_t3_doc_commits() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.write("docs/use-cases/api.html", "<html>t2 edit, no trailer</html>\n");
    repo.write("docs/work/scratch.html", "<html>t3 scratch, no trailer</html>\n");
    let new = repo.commit("t2 + t3 doc edits without a trailer");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 0, "T2/T3 doc commits need no commit-time signature: {out}");
}

#[test]
fn range_rejects_an_untrailered_claude_md_commit() {
    let repo = Repo::new();
    allow_file(&repo);
    let old = repo.sha("HEAD");
    repo.write("CLAUDE.md", "# project rules\n");
    let new = repo.commit("t0 edit, no trailer");

    let (code, out) = range_run(&repo, &old, &new);
    assert_eq!(code, 1, "CLAUDE.md is rule-bearing (T0) and needs a trailer: {out}");
    assert!(out.contains("8·manual-review"), "{out}");

    repo.write("CLAUDE.md", "# project rules, reviewed\n");
    let trailered = repo.commit_reviewed("t0 edit");
    let (code, out) = range_run(&repo, &new, &trailered);
    assert_eq!(code, 0, "a listed trailer authorises the T0 edit: {out}");
}

// ---------------------------------------------------------------------------
// Check 8 names the cause it found: one sentence over every cause sends a
// reader to the causes it was not.
// ---------------------------------------------------------------------------

/// A key-backed `mr/v1` note over `epic/x..tip`, the shape `mr confirm` writes,
/// signed by `signer` with the repo's `signer` key.
fn signed_wu_mr_note(repo: &Repo, tip: &str, signer: &str) -> String {
    let base = repo.sha("epic/x");
    let hash = diff_sha256(&repo.path, &base, tip).unwrap();
    let bytes = xtask::git::show_bytes(&repo.path, tip, WU_RESULT).unwrap();
    let rsha = xtask::git::sha256_hex(&bytes).unwrap();
    let payload = format!("{hash}\n{rsha}");
    let sig = xtask::git::ssh_sign(
        "ssh-keygen",
        repo.path.join("signer").to_str().unwrap(),
        xtask::mr::NOTE_NAMESPACE,
        payload.as_bytes(),
        None,
    )
    .unwrap();
    let indented: String = sig.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n");
    format!(
        "schema: mr/v1\nlevel: working-unit\ntip: {tip}\nbase: {base}\ntier: key\nsigner: {signer}\nreport: {WU_RESULT}\nreport_sha256: {rsha}\ndate: 2026-09-03\nsig: |\n{indented}\n"
    )
}

/// A blessed WU whose `mr/v1` note is key-backed and whose roster lists the key.
fn wu_signed_by_a_listed_key() -> (Repo, String) {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let pubkey = repo.keygen();
    repo.write(".mr-allowed", &format!("email: a@b.co\nkey: vuluu {pubkey}\n"));
    repo.note("mr", &tip, &signed_wu_mr_note(&repo, &tip, "vuluu"));
    (repo, tip)
}

fn check_8_message(repo: &Repo) -> String {
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["8·manual-review"], "{}", format_report(&report));
    format_report(&report)
}

#[test]
fn a_signature_this_checkouts_roster_lists_no_key_for_names_the_signer_and_the_roster() {
    let (repo, _tip) = wu_signed_by_a_listed_key();
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "the roster lists the key: {}", format_report(&report));

    // The DEC-10 shape: a repo-wide note, a per-checkout roster without it.
    repo.write(".mr-allowed", "email: a@b.co\n");
    let out = check_8_message(&repo);

    // The roster's own absolute path, not the bare file name: NOT_AUTHORISED
    // already ends "must be listed in .mr-allowed", so asserting the name alone
    // passes on that boilerplate even when the message names no path at all.
    let roster = repo.path.join(".mr-allowed").display().to_string();
    assert!(out.contains("is signed by vuluu"), "the signer is named: {out}");
    assert!(out.contains(&roster), "and the roster this checkout read ({roster}): {out}");
    assert!(!out.contains("has no mr/v1 note"), "the note exists and is valid: {out}");
    assert!(!out.contains("does not verify"), "the signature is not the fault: {out}");
}

#[test]
fn a_signature_that_no_longer_covers_the_range_says_so_and_not_that_the_signer_is_unlisted() {
    let (repo, tip) = wu_signed_by_a_listed_key();
    // The tree moved after signing: same note, a diff it no longer covers.
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\npub fn later() {}\n");
    let moved = repo.commit("a change made after the signature");
    repo.note("test", &moved, &repo.git(&["notes", "--ref=test", "show", &tip]));
    repo.note("doc", &moved, &repo.git(&["notes", "--ref=doc", "show", &tip]));
    let base = repo.sha("epic/x");
    let sha = diff_sha256(&repo.path, &base, &moved).unwrap();
    repo.note("cr", &moved, &cr_note("E9", Some("E9.W1"), &base, &sha, "PASS"));
    repo.note("mr", &moved, &signed_wu_mr_note(&repo, &tip, "vuluu"));

    let out = check_8_message(&repo);

    assert!(out.contains("does not verify over base..tip"), "the cause is the signature: {out}");
    assert!(out.contains("vuluu"), "the signer is still named: {out}");
    assert!(!out.contains("does not list"), "the roster is not the fault: {out}");
}

#[test]
fn the_three_check_8_causes_print_three_different_sentences() {
    let (repo, tip) = wu_signed_by_a_listed_key();
    let listed = std::fs::read_to_string(repo.path.join(".mr-allowed")).unwrap();
    let note = repo.git(&["notes", "--ref=mr", "show", &tip]);

    repo.git(&["notes", "--ref=mr", "remove", &tip]);
    let absent = check_8_message(&repo);

    repo.note("mr", &tip, &note);
    repo.write(".mr-allowed", "email: a@b.co\n");
    let unlisted = check_8_message(&repo);

    repo.write(".mr-allowed", &listed);
    repo.note("mr", &tip, &note.replace("report_sha256: ", "report_sha256: 0"));
    let stale = check_8_message(&repo);

    let causes = [&absent, &unlisted, &stale];
    for (i, a) in causes.iter().enumerate() {
        for b in causes.iter().skip(i + 1) {
            assert_ne!(a, b, "each cause gets its own sentence");
        }
    }
    assert!(absent.contains("has no mr/v1 note"), "{absent}");
    assert!(unlisted.contains("does not list"), "{unlisted}");
    assert!(stale.contains("does not verify"), "{stale}");
}

#[test]
fn an_unreadable_note_says_that_rather_than_reporting_none() {
    let (repo, tip) = wu_signed_by_a_listed_key();

    repo.note("mr", &tip, "schema: mr/v2\nlevel: working-unit\ntip: x\ntier: key\nsigner: vuluu\n");
    let other_schema = check_8_message(&repo);
    assert!(other_schema.contains("declares schema mr/v2"), "{other_schema}");

    repo.note("mr", &tip, "schema: [not: a: note");
    let unparseable = check_8_message(&repo);
    assert!(unparseable.contains("does not parse"), "{unparseable}");
    assert!(!unparseable.contains("has no mr/v1 note"), "a note is present: {unparseable}");
}

#[test]
fn a_note_whose_base_is_not_in_this_history_says_the_range_cannot_be_recomputed() {
    let (repo, tip) = wu_signed_by_a_listed_key();
    let ghost = "0000000000000000000000000000000000000000";
    let note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    let base = repo.sha("epic/x");
    repo.note("mr", &tip, &note.replace(&format!("base: {base}"), &format!("base: {ghost}")));

    let out = check_8_message(&repo);

    assert!(out.contains("which this repo's history does not hold"), "{out}");
    assert!(!out.contains("does not verify"), "the signature was never reached: {out}");
}

#[test]
fn every_check_8_cause_keeps_the_check_id_and_the_receipts_shape() {
    // The id is what receipts record and what the epic walk filters at a seam;
    // five new sentences must not become five new checks.
    let (repo, tip) = wu_signed_by_a_listed_key();
    let note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    for body in [
        None,
        Some("schema: mr/v2\nlevel: working-unit\ntip: x\ntier: key\nsigner: vuluu\n".to_string()),
        Some(note.replace("report_sha256: ", "report_sha256: 0")),
    ] {
        if let Some(b) = body {
            repo.note("mr", &tip, &b);
        } else {
            repo.git(&["notes", "--ref=mr", "remove", &tip]);
        }
        let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
        assert_eq!(checks_of(&report), vec!["8·manual-review"]);
    }

    repo.note("mr", &tip, &note);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));
    let body = repo.gate().write_receipt(&report).unwrap();
    assert!(body.contains("manual-review"), "the receipt still lists the check: {body}");
    let receipt: DodReceipt = serde_yaml::from_str(&body).unwrap();
    assert_eq!(receipt.tip, tip);
}

// ─── W4: the review binds the code it judged ────────────────────────────────
//
// The review's subject, its fingerprint and its inputs are the code paths. The
// tests below are that promise: a document cannot void a PASS, a diff that
// changes no code owes no review, the code the review did judge still binds it,
// and the design verdict travels on the note W4 gave it.

/// What the review binds: the range's code paths, and only those.
fn code_sha(repo: &Repo, base: &str, tip: &str) -> String {
    let paths: Vec<String> = xtask::git::diff_names(&repo.path, base, tip)
        .unwrap()
        .into_iter()
        .filter(|p| xtask::notes::is_code(p))
        .collect();
    xtask::git::diff_sha256_paths(&repo.path, base, tip, &paths).unwrap()
}

fn cr_note_v4(base: &str, sha: &str) -> String {
    format!(
        "schema: cr/v4\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\nverdict: PASS\ndate: 2026-09-12\n",
        identity("E9", Some("E9.W1"))
    )
}

/// The design step's note: the section a `cr/v3` note carried, on the note W4
/// gave it, pinned to the design paths it read - the artifacts it judged and the
/// rendered files it accounts for.
fn design_note(repo: &Repo, artifacts: &[&str], verdict: &str, extra: &str) -> String {
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    let (_, owed) = repo.gate().surfaces(&tip, &base);
    let mut paths: Vec<String> = artifacts.iter().map(|a| (*a).to_string()).collect();
    paths.extend(owed);
    paths.sort();
    paths.dedup();
    let sha = xtask::git::diff_sha256_paths(&repo.path, &base, &tip, &paths).unwrap();
    format!(
        "schema: design/v1\n{}base: {base}\ndesign_sha256: {sha}\nverdict: {verdict}\nartifacts: [{}]\n{extra}date: 2026-09-12\n",
        identity("E9", Some("E9.W1")),
        artifacts.join(", ")
    )
}

const WU_TEST_NOTE: &str = "  integration: { count: 1, covers: \"the review's binding\" }\n";
const DOCS_ONLY_TEST_NOTE: &str =
    "  integration: { waived: \"the diff changes no code to test\" }\n";

#[test]
fn a_docs_only_commit_leaves_the_review_binding_where_it_was() {
    let repo = wu_with_change();
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    let sha = code_sha(&repo, &base, &tip);
    allow_file(&repo);
    let notes = |tip: &str, changes: &str| {
        repo.note("test", tip, &test_note("working-unit", "E9", Some("E9.W1"), WU_TEST_NOTE));
        repo.note("doc", tip, &doc_note("E9", Some("E9.W1"), changes, None));
        repo.note("mr", tip, &wu_mr_note(&repo, tip));
    };
    repo.note("cr", &tip, &cr_note_v4(&base, &sha));
    notes(&tip, WU_CHANGES);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));

    // A document lands after the verdict. The note copy that rides it is the same
    // one - its hash covers the code paths, which did not move - so no fleet runs.
    let plan = "docs/work/one-plan.html";
    repo.write(plan, "<html>the plan the review never reads</html>\n");
    let moved = repo.commit("the plan lands after the review");
    let changes = format!(
        "{WU_CHANGES}  - path: {plan}\n    tier: T3\n    action: added\n    distills: \"the plan\"\n"
    );
    repo.note("cr", &moved, &cr_note_v4(&base, &sha));
    notes(&moved, &changes);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "a document cannot void a PASS: {}", format_report(&report));

    // The same note, on a tip whose code moved: the binding is what refuses it.
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\npub fn also() {}\n");
    let code_moved = repo.commit("the code moves after the review");
    repo.note("cr", &code_moved, &cr_note_v4(&base, &sha));
    notes(&code_moved, &changes);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["5·diff-binding"], "{}", format_report(&report));
}

// A `cr/v4` note that binds the whole diff is binding bytes the review no longer
// judges: the schema says which subset its hash is, and the gate reads it that way.
#[test]
fn a_review_bound_to_the_whole_diff_is_refused() {
    let repo = wu_with_change();
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    let whole = diff_sha256(&repo.path, &base, &tip).unwrap();
    bless_wu(&repo, &tip);
    repo.note("cr", &tip, &cr_note_v4(&base, &whole));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["5·diff-binding"], "{}", format_report(&report));

    repo.note("cr", &tip, &cr_note_v4(&base, &code_sha(&repo, &base, &tip)));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));
}

/// A WU whose diff carries a closed design run and its result report, and no code.
fn design_only_wu() -> Repo {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write(
        DESIGN_RUN,
        "<html><body><main data-artifact=\"design-run\" data-status=\"archived\"><p>the run</p></main></body></html>\n",
    );
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    repo.commit("the fourth design run and its result");
    repo
}

fn design_only_notes(repo: &Repo, tip: &str) {
    let changes = format!(
        "  - path: {DESIGN_RUN}\n    tier: T3\n    action: added\n    distills: \"the fourth run\"\n  - path: {WU_RESULT}\n    tier: T3\n    action: added\n    distills: \"the result\"\n"
    );
    allow_file(repo);
    repo.note("test", tip, &test_note("working-unit", "E9", Some("E9.W1"), DOCS_ONLY_TEST_NOTE));
    repo.note(
        "doc",
        tip,
        &doc_note(
            "E9",
            Some("E9.W1"),
            &changes,
            Some("a design run and its result; no T2 contract moved"),
        ),
    );
    repo.note("mr", tip, &wu_mr_note(repo, tip));
}

#[test]
fn a_design_only_diff_owes_a_design_note_and_no_review() {
    let repo = design_only_wu();
    let tip = repo.sha("wu/x/one");
    design_only_notes(&repo, &tip);

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(
        !format_report(&report).contains("1·notes"),
        "a diff that changes no code owes no review: {}",
        format_report(&report)
    );

    repo.note("design", &tip, &design_note(&repo, &[DESIGN_RUN], "PASS", ""));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "the design verdict carries it: {}", format_report(&report));
}

#[test]
fn a_design_verdict_is_void_when_the_artifact_moves() {
    let repo = design_only_wu();
    let tip = repo.sha("wu/x/one");
    design_only_notes(&repo, &tip);
    let verdict = design_note(&repo, &[DESIGN_RUN], "PASS", "");
    repo.note("design", &tip, &verdict);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "{}", format_report(&report));

    repo.write(
        DESIGN_RUN,
        "<html><body><main data-artifact=\"design-run\" data-status=\"archived\"><p>the run, revised after the verdict</p></main></body></html>\n",
    );
    let moved = repo.commit("revise the run after the verdict");
    design_only_notes(&repo, &moved);
    repo.note("design", &moved, &verdict);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"9·design"),
        "a verdict pinned to other bytes is not a verdict about these: {}",
        format_report(&report)
    );
}

// The hand-off accounting moved with the verdict: a build that renders a file
// answers for it on the design note, and the review's own note says nothing.
#[test]
fn the_handoff_accounting_travels_on_the_design_note() {
    let repo = wu_building_a_component();
    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    bless_wu(&repo, &tip);
    repo.note("cr", &tip, &cr_note_v4(&base, &code_sha(&repo, &base, &tip)));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(format_report(&report).contains("StatusPill.tsx"), "the red names the surface");

    let blob = repo.git(&["rev-parse", &format!("{tip}:{COMPONENT_DESIGN}")]);
    let cites = format!(
        "implements:\n  - path: {COMPONENT_DESIGN}\n    blob: {}\n    built: [ui/src/components/StatusPill.tsx]\n",
        &blob[..7]
    );
    repo.note("design", &tip, &design_note(&repo, &[], "PASS", &cites));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "the citation lands on its own note: {}", format_report(&report));
}

// W4 round 2 - the pin covers every design path the range ships, and a build
// against a design it did not touch ships only rendered files. A pin read only
// where an artifact moved would leave exactly that case - the common one - bound
// to nothing, so a stale pin on a citation-only note reds here.
#[test]
fn a_citation_only_design_note_is_still_pinned_to_the_bytes_it_judged() {
    let repo = wu_building_a_component();
    let tip = repo.sha("wu/x/one");
    let base = repo.sha("epic/x");
    bless_wu(&repo, &tip);
    repo.note("cr", &tip, &cr_note_v4(&base, &code_sha(&repo, &base, &tip)));
    let blob = repo.git(&["rev-parse", &format!("{tip}:{COMPONENT_DESIGN}")]);
    let cites = format!(
        "implements:\n  - path: {COMPONENT_DESIGN}\n    blob: {}\n    built: [ui/src/components/StatusPill.tsx]\n",
        &blob[..7]
    );
    let honest = design_note(&repo, &[], "PASS", &cites);
    assert!(!honest.contains("design_sha256: \n"), "the diff ships a rendered file to pin");

    let stale = honest
        .lines()
        .map(|line| {
            if line.starts_with("design_sha256:") {
                format!("design_sha256: {}", "0".repeat(64))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    repo.note("design", &tip, &format!("{stale}\n"));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(
        format_report(&report).contains("pinned to different bytes"),
        "the red names the pin: {}",
        format_report(&report)
    );

    repo.note("design", &tip, &honest);
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "the honest pin greens: {}", format_report(&report));
}

// The other half of the same move: an epic's drop record travels with the verdict
// too. Reading it off the review note alone would refuse an epic for a drop it did
// record - so here a `cr/v4` epic with no design section greens on `design/v1`.
#[test]
fn an_epic_drop_record_is_read_from_the_design_note() {
    let repo = epic_over_an_unbuilt_component_design();
    let tip = repo.sha("epic/x");
    let base = repo.sha("main");
    // An epic tip's notes carry no unit, so both notes are written at that shape.
    let cr = format!(
        "schema: cr/v4\n{}base: {base}\nhead: x\ndiff_sha256: {}\nverdict: PASS\ndate: 2026-09-13\n",
        identity("E9", None),
        code_sha(&repo, &base, &tip)
    );
    repo.note("cr", &tip, &cr);
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["components·unbuilt"], "{}", format_report(&report));
    assert!(format_report(&report).contains("design/v1"), "the fix names the note that carries it");

    repo.note(
        "design",
        &tip,
        &format!(
            "schema: design/v1\n{}dropped:\n  - path: {COMPONENT_DESIGN}\n    why: \"the epic shipped no pill\"\ndate: 2026-09-13\n",
            identity("E9", None)
        ),
    );
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        report.green(),
        "the drop is recorded where the verdict lives: {}",
        format_report(&report)
    );
}

/// The note the review writes now: bound to the code paths, and carrying the
/// round's own bill under `fleet:`.
fn cr_note_v5(base: &str, sha: &str, extra: &str) -> String {
    format!(
        "schema: cr/v5\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\ndimensions: [correctness, reuse, simplification, efficiency, altitude]\nfleet:\n  finders: 5\n  verifiers: 5\n  by_finder:\n    correctness: {{ found: 3, verified: 1 }}\n    reuse: {{ found: 2, verified: 0 }}\n{extra}verdict: PASS\ndate: 2026-09-13\n",
        identity("E9", Some("E9.W1"))
    )
}

// W6: the bump changes what `dimensions:` means and adds the round's bill. What it
// must not change is the binding or the nit rule, so both are pinned at a real
// seam under the new contract.
#[test]
fn a_cr_v5_note_is_bound_to_the_code_paths_and_keeps_the_nit_rule() {
    let repo = wu_with_change();
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let code = code_sha(&repo, &base, &tip);

    repo.note("cr", &tip, &cr_note_v5(&base, &code, ""));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "a v5 PASS bound to the code paths greens: {}", format_report(&report));

    let whole = diff_sha256(&repo.path, &base, &tip).unwrap();
    repo.note("cr", &tip, &cr_note_v5(&base, &whole, ""));
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["5·diff-binding"], "{}", format_report(&report));
    assert!(
        format_report(&report).contains("reviewed code is not byte-identical"),
        "the red names the code-path binding: {}",
        format_report(&report)
    );

    repo.note(
        "cr",
        &tip,
        &cr_note_v5(&base, &code, "nits:\n  - finding: \"a loop variable named x\"\n"),
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["3·review-nits"], "{}", format_report(&report));

    repo.note(
        "cr",
        &tip,
        &cr_note_v5(
            &base,
            &code,
            "nits:\n  - finding: \"a loop variable named x\"\n    fixed: \"renamed to frame\"\n",
        ),
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "a dispositioned nit costs nothing: {}", format_report(&report));
}

/// The same contract, with the clock and the pool W8 added to the bill.
fn cr_note_v5_timed(base: &str, sha: &str, extra: &str) -> String {
    format!(
        "schema: cr/v5\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\ndimensions: [correctness, reuse, simplification, efficiency, altitude]\nfleet:\n  finders: 5\n  verifiers: 1\n  verifier_model: haiku-4.5\n  shards: 2\n  candidates: {{ blocker: 0, important: 1, nit: 6 }}\n  stage_seconds: {{ gate0: 58, finders: 145, verifiers: 110, total: 255 }}\n  by_finder:\n    correctness: {{ found: 3, verified: 1 }}\n    reuse: {{ found: 2, verified: 0 }}\n{extra}verdict: PASS\ndate: 2026-09-14\n",
        identity("E9", Some("E9.W1"))
    )
}

// W8: the bill grew a clock, a pool by severity and the model that judged it. No
// gate check reads any of them, so a round that counted itself and one that did
// not must green identically - and a nit that never bought a verifier is still
// refused while it is undecided.
#[test]
fn the_gate_reads_neither_the_clock_nor_the_pool_a_round_records() {
    let repo = wu_with_change();
    let base = repo.sha("epic/x");
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);
    let code = code_sha(&repo, &base, &tip);

    repo.note("cr", &tip, &cr_note_v5_timed(&base, &code, ""));
    let timed = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(timed.green(), "a timed round greens: {}", format_report(&timed));

    repo.note("cr", &tip, &cr_note_v5(&base, &code, ""));
    let untimed = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(
        checks_of(&timed),
        checks_of(&untimed),
        "the gate is indifferent to the reading: {}",
        format_report(&untimed)
    );

    repo.note(
        "cr",
        &tip,
        &cr_note_v5_timed(&base, &code, "nits:\n  - finding: \"a loop variable named x\"\n"),
    );
    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(
        checks_of(&report),
        vec!["3·review-nits"],
        "an unverified nit is still evidence that must be decided: {}",
        format_report(&report)
    );
}

/// An epic whose merged Working Units each shipped a different review contract -
/// one seam per schema, each bound the way its contract binds.
fn epic_over_every_review_contract() -> Repo {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    allow_file(&repo);
    for (n, (name, schema)) in
        [("one", "cr/v2"), ("two", "cr/v3"), ("three", "cr/v4"), ("four", "cr/v5")]
            .iter()
            .enumerate()
    {
        let wu = format!("E9.W{}", n + 1);
        let branch = format!("wu/x/{name}");
        repo.git(&["checkout", "-q", "-b", &branch, "epic/x"]);
        let base = repo.sha("epic/x");
        repo.write(&format!("src/{name}.rs"), "pub fn f() {}\n");
        let result = format!("docs/work/{name}-result.html");
        repo.write(&result, "<html>result</html>\n");
        let tip = repo.commit(&format!("{wu}: the code"));

        let identity = format!("epic: E9\nwu: {wu}\n");
        repo.note(
            "test",
            &tip,
            &format!(
                "schema: test/v2\nlevel: working-unit\n{identity}tests:\n  integration: {{ count: 1, covers: \"f\" }}\n"
            ),
        );
        repo.note(
            "doc",
            &tip,
            &format!(
                "schema: doc/v1\n{identity}changes:\n  - path: {result}\n    tier: T3\n    action: added\n    distills: \"the result\"\nt2_waived: \"a code-only unit; no contract moved\"\n"
            ),
        );
        let bound = if xtask::notes::binds_code(schema) {
            code_sha(&repo, &base, &tip)
        } else {
            diff_sha256(&repo.path, &base, &tip).unwrap()
        };
        let bill = if *schema == "cr/v5" {
            "fleet:\n  finders: 5\n  verifiers: 1\n  by_finder:\n    correctness: { found: 1, verified: 0 }\n    reuse: { found: 0, verified: 0 }\n"
        } else {
            ""
        };
        repo.note(
            "cr",
            &tip,
            &format!(
                "schema: {schema}\n{identity}base: {base}\nhead: {tip}\ndiff_sha256: {bound}\n{bill}verdict: PASS\ndate: 2026-09-13\n"
            ),
        );
        let bytes = xtask::git::show_bytes(&repo.path, &tip, &result).unwrap();
        repo.note(
            "mr",
            &tip,
            &format!(
                "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: {result}\nreport_sha256: {}\ndate: 2026-09-13\n",
                xtask::git::sha256_hex(&bytes).unwrap()
            ),
        );
        repo.git(&["checkout", "-q", "epic/x"]);
        repo.git(&["merge", "--no-ff", "-q", "-m", &format!("merge {branch}"), &branch]);
    }
    for name in ["one", "two", "three", "four"] {
        std::fs::remove_file(repo.path.join(format!("docs/work/{name}-result.html"))).unwrap();
    }
    repo.commit("archive the results out of git");
    let tip = repo.sha("epic/x");
    let base = repo.sha("main");
    repo.note(
        "test",
        &tip,
        &test_note("epic", "E9", None, "  e2e: { count: 1, covers: \"full flow\" }\n"),
    );
    repo.note(
        "doc",
        &tip,
        &format!("schema: doc/v1\n{}changes: []\nt2_waived: \"four code-only units; no contract moved\"\n", identity("E9", None)),
    );
    repo.note(
        "cr",
        &tip,
        &cr_note_v5(&base, &code_sha(&repo, &base, &tip), "").replace("wu: E9.W1\n", ""),
    );
    repo.note("mr", &tip, &mr_note("epic", &tip));
    repo
}

// Every contract the gate accepts is re-read by the epic walk at the binding it
// declared: a v2/v3 seam against its whole diff, a v4/v5 seam against its code
// paths. One epic over all four is what proves the bump orphans no merged seam.
#[test]
fn the_walk_reads_every_review_contract_at_the_binding_it_declared() {
    let repo = epic_over_every_review_contract();
    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
    assert_eq!(report.waived, Vec::<String>::new(), "no seam needed a waiver");
}
