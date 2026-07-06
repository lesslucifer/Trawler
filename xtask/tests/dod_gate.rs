#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::PathBuf;

use tempfile::TempDir;
use xtask::gate::{format_report, Gate, Level};
use xtask::git::diff_sha256;
use xtask::notes::DodReceipt;

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

fn wu_with_change() -> Repo {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.commit("wu change");
    repo
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
    repo.note("doc", tip, &doc_note("E9", Some("E9.W1"), T2_ENTRY, None));
    repo.note("cr", tip, &cr_note("E9", Some("E9.W1"), &base, &sha, "PASS"));
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
    repo.note("doc", &tip, &format!("schema: doc/v1\nchanges:\n{T2_ENTRY}"));

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
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("docs/use-cases/api.html"));
}

fn epic_with_merged_wu() -> (Repo, String) {
    let repo = wu_with_change();
    let wu_tip = repo.sha("wu/x/one");
    bless_wu(&repo, &wu_tip);
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
    (repo, wu_tip)
}

fn mr_note(level: &str, tip: &str) -> String {
    format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\ntier: email\nsigner: a@b.co\ndate: 2026-07-06\n"
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
    repo.commit_reviewed("plan doc");
    std::fs::remove_file(repo.path.join("docs/work/plan.html")).unwrap();
    repo.commit_reviewed("archive the plan out of git");
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
    std::fs::remove_file(repo.path.join("docs/work/plan.html")).unwrap();
    let tip = repo.commit("wu change archives the plan");
    bless_wu(&repo, &tip);
    let bare =
        format!("{T2_ENTRY}  - path: docs/work/plan.html\n    tier: T3\n    action: archived\n");
    repo.note("doc", &tip, &doc_note("E9", Some("E9.W1"), &bare, None));

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("lineage"));

    let with_lineage = format!(
        "{T2_ENTRY}  - path: docs/work/plan.html\n    tier: T3\n    action: archived\n    distilled_to: [docs/use-cases/api.html]\n"
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
fn docs_only_wu_gate_is_exempt_and_writes_no_receipt() {
    let repo = wu_with_change();
    docs_only_wu(&repo);

    let args: Vec<String> =
        ["verify-dod", "wu", "wu/x/docs"].iter().map(|s| (*s).to_string()).collect();
    let (code, message) = xtask::run(&args, &repo.path).unwrap();
    assert_eq!(code, 0, "got: {message}");
    assert!(message.contains("exempt"), "got: {message}");
    let tip = repo.sha("wu/x/docs");
    assert!(
        xtask::git::git(&repo.path, &["notes", "--ref=dod", "show", &tip]).is_err(),
        "no receipt may be written for an exempt tip"
    );
}

#[test]
fn docs_only_standalone_commit_is_exempt() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/use-cases/api.html", "<html>v2.5, interior</html>\n");
    repo.commit_reviewed("docs-only tweak, no manifest");
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

fn ui_wu(repo: &Repo) -> String {
    repo.git(&["checkout", "-q", "-b", "wu/x/ui", "epic/x"]);
    repo.write("ui/app.tsx", "export const App = () => null;\n");
    let tip = repo.commit("ui work unit");
    repo.note(
        "test",
        &tip,
        &test_note("working-unit", "E9", Some("E9.W2"), "  integration: { count: 1 }\n"),
    );
    tip
}

#[test]
fn a_ui_wu_requires_an_mr_note() {
    let repo = wu_with_change();
    allow_file(&repo);
    let tip = ui_wu(&repo);

    let report = repo.gate().verify(Level::Wu, "wu/x/ui", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"8·manual-review"),
        "a UI WU must require manual review, got: {}",
        format_report(&report)
    );

    repo.note("mr", &tip, &mr_note("working-unit", &tip));
    let report = repo.gate().verify(Level::Wu, "wu/x/ui", "epic/x").unwrap();
    assert!(
        !checks_of(&report).contains(&"8·manual-review"),
        "a listed mr note clears check 8: {}",
        format_report(&report)
    );
}

#[test]
fn a_ui_file_renamed_out_of_ui_still_requires_an_mr_note() {
    let repo = wu_with_change();
    allow_file(&repo);
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.write("ui/widget.tsx", "export const Widget = () => null; // enough content\n");
    repo.commit("ui file exists at the epic base");
    repo.git(&["checkout", "-q", "-b", "wu/x/ui2", "epic/x"]);
    repo.git(&["mv", "ui/widget.tsx", "notes.md"]);
    let tip = repo.commit("move ui file out of ui/");
    repo.note(
        "test",
        &tip,
        &test_note("working-unit", "E9", Some("E9.W3"), "  integration: { count: 1 }\n"),
    );

    let report = repo.gate().verify(Level::Wu, "wu/x/ui2", "epic/x").unwrap();
    assert!(
        checks_of(&report).contains(&"8·manual-review"),
        "a rename out of ui/ must not bypass the UI manual-review requirement: {}",
        format_report(&report)
    );
}

#[test]
fn a_non_ui_wu_does_not_require_an_mr_note() {
    let repo = wu_with_change();
    let tip = repo.sha("wu/x/one");
    bless_wu(&repo, &tip);

    let report = repo.gate().verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(
        !checks_of(&report).contains(&"8·manual-review"),
        "a non-UI WU needs no manual review: {}",
        format_report(&report)
    );
}

#[test]
fn epic_walk_flags_an_untrailered_doc_commit() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/use-cases/api.html", "<html>edited without a trailer</html>\n");
    repo.commit("doc edit, no mr trailer");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        checks_of(&report).contains(&"8·manual-review"),
        "the walk must flag an untrailered doc commit: {}",
        format_report(&report)
    );
}

#[test]
fn epic_walk_flags_a_doc_moved_out_of_docs_without_a_trailer() {
    let (repo, _) = epic_with_merged_wu();
    repo.git(&["mv", "docs/use-cases/api.html", "src/moved.rs"]);
    repo.commit("move a doc out of docs/, no trailer");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(
        checks_of(&report).contains(&"8·manual-review"),
        "a rename out of docs/ must not bypass the trailer check: {}",
        format_report(&report)
    );
}
