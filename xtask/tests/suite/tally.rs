use std::path::PathBuf;

use tempfile::TempDir;
use xtask::gate::{format_report, Gate, Level};
use xtask::git::diff_sha256;
use xtask::tally::Entry;

const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const WU_RESULT: &str = "docs/work/one-result.html";

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
        repo.write("docs/standard.html", "<html>a T1 standard</html>\n");
        repo.write(RULEBOOK, "<html>the soft rules a design verdict is a claim about</html>\n");
        repo.write(".mr-allowed", "email: a@b.co\n");
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

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        let ref_arg = format!("--ref={note_ref}");
        self.git(&["notes", &ref_arg, "add", "-f", "-m", body, tip]);
    }

    fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev])
    }

    fn diff_sha(&self, base: &str, tip: &str) -> String {
        diff_sha256(&self.path, base, tip).unwrap()
    }

    fn xtask(&self, args: &[&str]) -> (i32, String) {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        xtask::run(&args, &self.path).unwrap()
    }

    fn tally(&self) -> Vec<Entry> {
        xtask::tally::entries(&self.path)
    }

    fn tally_path(&self) -> PathBuf {
        xtask::tally::path(&self.path)
    }

    fn note_refs(&self) -> String {
        self.git(&["for-each-ref", "--format=%(refname)", "refs/notes"])
    }
}

fn identity() -> &'static str {
    "epic: E9\nwu: E9.W1\nbranch: wu/x/one\n"
}

fn cr_note(base: &str, sha: &str, verdict: &str) -> String {
    format!(
        "schema: cr/v2\n{}base: {base}\nhead: x\ndiff_sha256: {sha}\nverdict: {verdict}\ndate: 2026-09-12\n",
        identity()
    )
}

/// A WU branch off `epic/x` whose first commit is the reviewed code.
fn wu_branch() -> (Repo, String) {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    let first = repo.commit("E9.W1: the code");
    (repo, first)
}

/// The second round: the fix, the docs, the result report, and every note a
/// green WU tip needs. `first` is the commit round 1 reviewed and failed on.
fn bless_second_round(repo: &Repo, first: &str) {
    let base = repo.sha("epic/x");
    let round_one = repo.diff_sha(&base, first);
    repo.note("cr", first, &cr_note(&base, &round_one, "FAIL"));

    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write(WU_RESULT, "<html>wu result</html>\n");
    let tip = repo.commit("E9.W1: the fix, the docs, the result");

    let sha = repo.diff_sha(&base, &tip);
    repo.note(
        "test",
        &tip,
        &format!(
            "schema: test/v2\nlevel: working-unit\n{}tests:\n  integration: {{ count: 2, covers: \"the tally's contract\" }}\n",
            identity()
        ),
    );
    repo.note(
        "doc",
        &tip,
        &format!(
            "schema: doc/v1\n{}changes:\n  - path: docs/use-cases/api.html\n    tier: T2\n    action: updated\n    distills: \"the measurement contract\"\n  - path: {WU_RESULT}\n    tier: T3\n    action: added\n    distills: \"wu result report\"\n",
            identity()
        ),
    );
    repo.note("cr", &tip, &cr_note(&base, &sha, "PASS"));
    let bytes = xtask::git::show_bytes(&repo.path, &tip, WU_RESULT).unwrap();
    repo.note(
        "mr",
        &tip,
        &format!(
            "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\nreport: {WU_RESULT}\nreport_sha256: {}\ndate: 2026-09-12\n",
            xtask::git::sha256_hex(&bytes).unwrap()
        ),
    );
}

#[test]
fn a_green_wu_tip_records_the_review_rounds_it_paid() {
    let (repo, first) = wu_branch();
    bless_second_round(&repo, &first);
    let tip = repo.sha("HEAD");

    let report = Gate { repo: &repo.path }.verify(Level::Wu, "wu/x/one", "epic/x").unwrap();
    assert!(report.green(), "unexpected failures: {}", format_report(&report));

    let (code, message) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    assert_eq!(code, 0, "{message}");

    let entries = repo.tally();
    assert_eq!(entries.len(), 1, "one gate run, one record: {entries:?}");
    let entry = &entries[0];
    assert_eq!(entry.level, "working-unit");
    assert_eq!(entry.status, "green");
    assert_eq!(entry.tip, tip);
    assert_eq!(entry.epic.as_deref(), Some("E9"));
    assert_eq!(entry.wu.as_deref(), Some("E9.W1"));
    assert!(entry.checks.is_empty());
    assert_eq!(entry.branch.as_deref(), Some("wu/x/one"));
    assert_eq!(entry.rounds, Some(2), "a FAIL round and a PASS round were paid: {entry:?}");
}

#[test]
fn a_red_wu_gate_records_the_check_ids_that_refused() {
    let (repo, _first) = wu_branch();

    let (code, message) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    assert_eq!(code, 1, "{message}");
    assert!(message.contains("1·notes"), "{message}");

    let entries = repo.tally();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.level, "working-unit");
    assert_eq!(entry.status, "red");
    assert!(entry.checks.contains(&"1·notes".to_string()), "{entry:?}");
    assert!(entry.checks.contains(&"8·manual-review".to_string()), "{entry:?}");
    assert_eq!(entry.rounds, Some(0), "nothing was reviewed: {entry:?}");
}

#[test]
fn the_rounds_are_the_tips_own_line_not_everything_it_merged() {
    let (repo, first) = wu_branch();
    let base = repo.sha("epic/x");

    // Another branch's reviewed work, merged into this one: its cr note is
    // reachable from the tip, but it is not a round this line paid for.
    repo.git(&["checkout", "-q", "-b", "wu/x/other", &base]);
    repo.write("src/other.rs", "pub fn other() {}\n");
    let other = repo.commit("E9.W9: another unit's work");
    repo.note("cr", &other, &cr_note(&base, &repo.diff_sha(&base, &other), "PASS"));
    repo.git(&["checkout", "-q", "wu/x/one"]);
    repo.git(&["merge", "-q", "--no-ff", "-m", "back-merge", "wu/x/other"]);

    bless_second_round(&repo, &first);
    let (code, message) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    assert_eq!(code, 0, "{message}");

    let entries = repo.tally();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].rounds,
        Some(2),
        "the two rounds this line paid, not the one it merged: {:?}",
        entries[0]
    );
}

#[test]
fn the_tally_lives_in_the_git_common_dir_and_never_becomes_a_note() {
    let (repo, _first) = wu_branch();
    let before = repo.note_refs();

    repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);

    let file = repo.tally_path();
    let common = std::fs::canonicalize(repo.path.join(".git")).unwrap();
    assert_eq!(file.parent(), Some(common.as_path()), "{file:?}");
    assert!(file.exists(), "the gate appends its record to {file:?}");
    let tracked = repo.git(&["ls-files", "--", file.to_str().unwrap()]);
    assert!(tracked.is_empty(), "the tally is not tracked: {tracked}");
    assert_eq!(repo.note_refs(), before, "a gate run writes no note");
}

#[test]
fn the_tally_command_prints_refusals_per_level_and_rounds_per_tip() {
    let (repo, first) = wu_branch();
    bless_second_round(&repo, &first);
    repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);

    let (code, out) = repo.xtask(&["tally"]);
    assert_eq!(code, 0);
    assert!(out.contains("working-unit - 0 of 1 tips refused"), "{out}");
    assert!(out.contains("2 review rounds"), "{out}");
    assert!(out.contains("E9.W1"), "{out}");
    assert!(out.contains("review rounds per tip"), "{out}");
}

#[test]
fn repeated_runs_on_one_tip_collapse_to_one_row() {
    let (repo, _first) = wu_branch();
    repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);

    assert_eq!(repo.tally().len(), 2, "each run is recorded");

    let (_, out) = repo.xtask(&["tally"]);
    assert!(out.contains("1 tip - 1 refused"), "{out}");
    assert!(out.contains("working-unit - 1 of 1 tips refused"), "{out}");
}

#[test]
fn a_deleted_tally_changes_no_verdict() {
    let (repo, _first) = wu_branch();
    let (code, first) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    std::fs::remove_file(repo.tally_path()).unwrap();
    let (again, second) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);

    assert_eq!((code, &first), (again, &second), "the gate re-derives everything it decides");
}

#[test]
fn an_empty_tally_prints_that_nothing_was_recorded() {
    let repo = Repo::new();
    let (code, out) = repo.xtask(&["tally"]);
    assert_eq!(code, 0);
    assert!(out.contains("no gate runs recorded yet"), "{out}");
}

/// What the review binds under `cr/v4` and later: the range's code paths only.
fn code_sha(repo: &Repo, base: &str, tip: &str) -> String {
    let paths: Vec<String> = xtask::git::diff_names(&repo.path, base, tip)
        .unwrap()
        .into_iter()
        .filter(|p| xtask::notes::is_code(p))
        .collect();
    xtask::git::diff_sha256_paths(&repo.path, base, tip, &paths).unwrap()
}

fn cr_note_v5(base: &str, code_sha: &str, bill: &str) -> String {
    format!(
        "schema: cr/v5\n{}base: {base}\nhead: x\ndiff_sha256: {code_sha}\ndimensions: [correctness, reuse, simplification, efficiency, altitude]\n{bill}verdict: PASS\ndate: 2026-09-13\n",
        identity()
    )
}

const BILL: &str = "fleet:\n  finders: 5\n  verifiers: 15\n  by_finder:\n    correctness: { found: 9, verified: 5 }\n    reuse: { found: 6, verified: 2 }\n";

// W6: the fleet reading walks the same ranges the tally already knows and reads
// each round's own bill off its note. The FAIL round predates the bill and is
// counted as unmeasured; the PASS round that counted itself is what the per-finder
// rows add up.
#[test]
fn the_fleet_reading_bills_each_round_that_counted_itself_and_names_the_rest() {
    let (repo, first) = wu_branch();
    bless_second_round(&repo, &first);
    let tip = repo.sha("HEAD");
    let base = repo.sha("epic/x");
    repo.note("cr", &tip, &cr_note_v5(&base, &code_sha(&repo, &base, &tip), BILL));

    let (code, message) = repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);
    assert_eq!(code, 0, "{message}");

    let (code, out) = repo.xtask(&["tally", "--fleet"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("2 review rounds on the ranges this tally knows - 1 measured, 1 unmeasured"),
        "{out}"
    );
    assert!(
        out.contains("agents per measured round: 20.0 (5.0 finders + 15.0 verifiers)"),
        "{out}"
    );
    let correctness =
        out.lines().find(|l| l.trim_start().starts_with("correctness")).unwrap_or_default();
    assert_eq!(
        correctness.split_whitespace().collect::<Vec<_>>(),
        ["correctness", "1", "9", "5", "55%"],
        "{out}"
    );
    let reuse = out.lines().find(|l| l.trim_start().starts_with("reuse")).unwrap_or_default();
    assert_eq!(
        reuse.split_whitespace().collect::<Vec<_>>(),
        ["reuse", "1", "6", "2", "33%"],
        "{out}"
    );
}

#[test]
fn a_fleet_reading_over_no_gate_runs_says_so() {
    let repo = Repo::new();
    let (code, out) = repo.xtask(&["tally", "--fleet"]);
    assert_eq!(code, 0);
    assert!(out.contains("no review round on the ranges this tally knows"), "{out}");
}

#[test]
fn rounds_without_a_bill_are_unmeasured_not_zero_agents() {
    let (repo, first) = wu_branch();
    bless_second_round(&repo, &first);
    repo.xtask(&["verify-dod", "wu", "wu/x/one", "--target", "epic/x"]);

    let (code, out) = repo.xtask(&["tally", "--fleet"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("2 review rounds on the ranges this tally knows - 0 measured, 2 unmeasured"),
        "{out}"
    );
    assert!(out.contains("no round has counted itself yet"), "{out}");
    assert!(!out.contains("agents per measured round"), "{out}");
}
