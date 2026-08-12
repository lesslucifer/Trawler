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
    assert!(format_report(&report).contains("design finder"), "the fix names how to get one");
}

// The reviewer and the merge gate must never disagree about what is
// design-bearing, so they call one function - this is the reviewer's door to it.
#[test]
fn design_scope_reports_what_the_review_must_judge_and_the_pin_it_writes() {
    let repo = wu_carrying_a_design_run("archived");
    let args: Vec<String> =
        ["design-scope", "epic/x", "wu/x/one"].iter().map(|s| (*s).to_string()).collect();
    let (code, out) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 0);
    assert!(out.contains(DESIGN_RUN), "{out}");
    assert!(out.contains(&pin_of(&repo, &repo.sha("wu/x/one"))), "the pin to write: {out}");

    let plain = wu_with_change();
    let (code, out) = xtask::run(&args, &plain.path).unwrap();
    assert_eq!(code, 0);
    assert!(out.contains("no design artifact"), "{out}");
}

fn pin_of(repo: &Repo, tip: &str) -> String {
    let blob = repo.git(&["rev-parse", &format!("{tip}:{RULEBOOK}")]);
    blob[..7].to_string()
}

fn design_section(artifacts: &[&str], pin: &str, verdict: &str) -> String {
    format!(
        "design:\n  artifacts: [{}]\n  rulebook: {pin}\n  findings: {{ found: 1, verified: 0, waived: 0 }}\n  verdict: {verdict}\n",
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
    let (repo, report) = verify_design_bearing(&|repo, tip| {
        design_section(&[DESIGN_RUN], &pin_of(repo, tip), "PASS")
    });
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
    let design = design_section(&[DESIGN_RUN], &pin_of(&repo, &tip), "PASS");
    repo.note("cr", &tip, &cr_note_v3(&base, &sha, &design));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(format_report(&report).contains(component), "the red names the artifact nobody read");
}

#[test]
fn a_failing_design_verdict_is_refused() {
    let (_repo, report) = verify_design_bearing(&|repo, tip| {
        design_section(&[DESIGN_RUN], &pin_of(repo, tip), "FAIL")
    });
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
}

// The same claim the run's own receipt makes, and the same way it went unchecked
// for three Working Units: a design verdict is a judgment against one rulebook,
// and the tip's rulebook is the only one that can be checked against.
#[test]
fn a_design_pin_that_names_no_blob_or_another_one_is_refused() {
    let (_repo, report) =
        verify_design_bearing(&|_, _| design_section(&[DESIGN_RUN], "796c5", "PASS"));
    assert_eq!(checks_of(&report), vec!["9·design"], "{}", format_report(&report));
    assert!(format_report(&report).contains("names no rulebook blob"));

    let (repo, report) =
        verify_design_bearing(&|_, _| design_section(&[DESIGN_RUN], "28f5437", "PASS"));
    let text = format_report(&report);
    assert_eq!(checks_of(&report), vec!["9·design"], "{text}");
    let tip = repo.sha("wu/x/one");
    assert!(
        text.contains("28f5437") && text.contains(&pin_of(&repo, &tip)),
        "both versions: {text}"
    );
}

// A rulebook that moves in a LATER Working Unit must not redden this seam: the
// pin is read at the tip, so a review stays true to the tree it judged.
#[test]
fn a_rulebook_that_moves_after_the_review_leaves_the_reviewed_tip_green() {
    let (repo, report) = verify_design_bearing(&|repo, tip| {
        design_section(&[DESIGN_RUN], &pin_of(repo, tip), "PASS")
    });
    assert!(report.green(), "{}", format_report(&report));
    let tip = repo.sha("wu/x/one");
    repo.write(RULEBOOK, "<html>the rulebook, revised by the next Working Unit</html>\n");
    repo.commit("revise the rulebook");
    let after = repo.gate().verify(Level::Wu, &tip, "epic/x").unwrap();
    assert!(after.green(), "{}", format_report(&after));
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
