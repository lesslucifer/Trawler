use std::path::{Path, PathBuf};
use std::process::Command;

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
        let out = Command::new("git").arg("-C").arg(&self.path).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
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
}

#[test]
fn green_epic_walk_verifies_the_merged_wu() {
    let (repo, _) = epic_with_merged_wu();
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}

#[test]
fn epic_gate_rejects_unarchived_work_docs() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/work/plan.html", "<html>plan</html>\n");
    repo.commit("plan doc");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/work/plan.html\n    tier: T3\n    action: added\n    distills: \"in flight\"\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert_eq!(checks_of(&report), vec!["6·doc-note"]);
    assert!(format_report(&report).contains("still not archived"));
}

#[test]
fn archived_work_doc_with_lineage_passes_the_epic_gate() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/work/archive/plan.html", "<html>plan</html>\n");
    repo.commit("archived plan doc");
    let entry = format!(
        "{T2_ENTRY}  - path: docs/work/archive/plan.html\n    tier: T3\n    action: archived\n    distilled_to: [docs/use-cases/api.html]\n"
    );
    bless_epic(&repo, &entry);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
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
    let out = Command::new("git")
        .args(["-C", repo.path.to_str().unwrap(), "notes", "--ref=dod", "show", &tip])
        .output()
        .unwrap();
    assert!(!out.status.success(), "no receipt may be written for an exempt tip");
}

#[test]
fn docs_only_standalone_commit_is_exempt() {
    let (repo, _) = epic_with_merged_wu();
    repo.write("docs/use-cases/api.html", "<html>v2.5, interior</html>\n");
    repo.commit("docs-only tweak, no manifest");
    close_epic(&repo);
    bless_epic(&repo, T2_ENTRY);

    let report = repo.gate().verify(Level::Epic, "epic/x", "main").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));
}
