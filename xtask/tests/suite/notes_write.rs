// E-SIMPWF.W3's contract: one command derives the range, the identity, the doc
// paths and their tiers and the per-layer counts, writes the tip's test/v2 and
// doc/v1 from one judgment, and refuses - writing nothing - when the judgment and
// the range disagree. The gate's own checks are read back over the result, because
// "passes the gate first time" is the promise.

use std::path::PathBuf;

use tempfile::TempDir;
use xtask::gate::{Gate, Level};

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
        repo.git(&["config", "user.email", "notes@test"]);
        repo.git(&["config", "user.name", "notes"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write(".gitignore", "/target\n");
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

    /// The judgment is handed over as a file, the way an agent writes it once and
    /// pipes it back - the command reads it, and writes nothing of it.
    fn judgment(&self, body: &str) -> String {
        let path = self.path.join("judgment.yaml");
        std::fs::write(&path, body).unwrap();
        path.to_str().unwrap().to_string()
    }

    fn xtask(&self, args: &[&str]) -> (i32, String) {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        xtask::run(&args, &self.path).unwrap()
    }

    fn write_notes(&self, level: &str, tip: &str, judgment: &str) -> (i32, String) {
        let path = self.judgment(judgment);
        self.xtask(&["notes", "write", level, tip, "--target", "epic/x", "--judgment", &path])
    }

    fn note(&self, note_ref: &str, tip: &str) -> Option<String> {
        xtask::git::note(&self.path, note_ref, tip)
    }

    fn note_refs(&self) -> String {
        self.git(&["for-each-ref", "--format=%(refname)", "refs/notes"])
    }

    /// The gate's own reading of the tip, without the receipt a green run writes:
    /// the written notes are judged by the same code the merge uses.
    fn gate_failures(&self, tip: &str) -> Vec<(String, String)> {
        let report = Gate { repo: &self.path }.verify(Level::Wu, tip, "epic/x").unwrap();
        report.failures.iter().map(|f| (f.check.clone(), f.what.clone())).collect()
    }
}

/// A WU branch off `epic/x` whose range changes code, a T2 doc and a T3 work doc:
/// every kind of doc the note has to account for, and the layers a WU is asked for.
fn wu_branch() -> (Repo, String) {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write(
        "src/lib.rs",
        "pub fn base() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn one() {}\n    #[test]\n    fn two() {}\n    #[test]\n    fn three() {}\n}\n",
    );
    repo.write("tests/wu.rs", "#[test]\nfn the_boundary() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    repo.write("docs/work/2026-09-12-plan.html", "<html>the plan</html>\n");
    let tip = repo.commit("E-X.W1: the code and the docs");
    (repo, tip)
}

const JUDGMENT: &str = "\
wu: E-X.W1
tests:
  unit:
    files: [src/lib.rs]
    covers: \"the branch's own arithmetic\"
  integration:
    files: [tests/wu.rs]
    covers: \"the WU's boundary\"
docs:
  - path: docs/work/2026-09-12-plan.html
    action: added
    distills: \"the WU plan - archived with the epic\"
  - path: docs/use-cases/api.html
    action: updated
    distills: \"the contract this WU settles\"
";

#[test]
fn one_command_writes_both_notes_and_the_gate_reads_them_green() {
    let (repo, tip) = wu_branch();

    let (code, said) = repo.write_notes("wu", &tip, JUDGMENT);
    assert_eq!(code, 0, "a judgment the range agrees with is written: {said}");
    assert!(said.contains("test/v2 + doc/v1"), "both notes, one command: {said}");

    let test = repo.note("test", &tip).expect("the test note is on the tip");
    assert!(test.contains("schema: test/v2"));
    assert!(test.contains("level: working-unit"));
    assert!(test.contains("epic: E-X\n"), "the epic is derived from the wu id: {test}");
    assert!(test.contains("wu: E-X.W1"));
    assert!(
        test.contains("count: 3"),
        "the unit count is the declarations in src/lib.rs, not a typed number: {test}"
    );
    assert!(test.contains("count: 1"), "the integration count came from tests/wu.rs: {test}");

    let doc = repo.note("doc", &tip).expect("the doc note is on the tip");
    assert!(doc.contains("tier: T3"), "the work doc's tier is derived: {doc}");
    assert!(doc.contains("tier: T2"), "the contract's tier is derived: {doc}");
    assert!(doc.contains("path: \"docs/use-cases/api.html\""));

    // The promise: the notes this writes are the notes the gate accepts. What is
    // still red is the review's own note and the signature, which land after this.
    let failures = repo.gate_failures(&tip);
    for check in ["2·manifest", "6·doc-note"] {
        assert!(
            !failures.iter().any(|(named, _)| named == check),
            "the derived notes leave {check} green: {failures:?}"
        );
    }
    for owed in ["test/v2", "doc/v1", "identity"] {
        assert!(
            !failures.iter().any(|(_, what)| what.contains(owed)),
            "nothing about {owed} is still owed: {failures:?}"
        );
    }
    assert!(
        failures.iter().any(|(_, what)| what.contains("no cr/v5 note")),
        "the review's own note is the one thing left to write: {failures:?}"
    );
}

#[test]
fn the_printed_range_is_the_one_the_gate_recomputes() {
    let (repo, tip) = wu_branch();
    let (code, said) = repo.write_notes("wu", &tip, JUDGMENT);
    assert_eq!(code, 0, "{said}");

    let base = repo.git(&["merge-base", "epic/x", &tip]);
    let diff = xtask::git::diff_sha256(&repo.path, &base, &tip).unwrap();
    assert!(said.contains(&format!("base: {base}")), "the review pins this base: {said}");
    assert!(said.contains(&format!("head: {tip}")), "got: {said}");
    assert!(said.contains(&format!("diff_sha256: {diff}")), "got: {said}");
    // The review binds the code paths, so the writer prints that hash too: a note
    // that pinned the whole diff would be refused at the merge.
    let code_paths: Vec<String> = xtask::git::diff_names(&repo.path, &base, &tip)
        .unwrap()
        .into_iter()
        .filter(|p| xtask::notes::is_code(p))
        .collect();
    let code = xtask::git::diff_sha256_paths(&repo.path, &base, &tip, &code_paths).unwrap();
    assert!(said.contains(&format!("code_sha256: {code}")), "got: {said}");
}

#[test]
fn a_changed_doc_the_judgment_omits_is_refused_in_the_gates_own_block() {
    let (repo, tip) = wu_branch();
    let judgment = JUDGMENT.replace(
        "  - path: docs/use-cases/api.html\n    action: updated\n    distills: \"the contract this WU settles\"\n",
        "",
    );
    let before = repo.note_refs();

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "an unaccounted doc is not written past: {said}");
    assert!(said.contains("FAIL  6·doc-note"), "the gate's own id: {said}");
    assert!(
        said.contains("docs/use-cases/api.html changed in the diff but has no entry in doc/v1"),
        "the gate's own words: {said}"
    );
    assert!(said.contains("nothing was written"), "got: {said}");
    assert_eq!(repo.note_refs(), before, "a refusal writes no note");
}

#[test]
fn a_doc_entry_the_range_does_not_touch_is_refused_as_a_phantom() {
    let (repo, tip) = wu_branch();
    let judgment = format!(
        "{JUDGMENT}  - path: docs/decisions/dec-9-the-thing.html\n    action: added\n    distills: \"a record the range never wrote\"\n"
    );

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("FAIL  6·doc-note"), "{said}");
    assert!(said.contains("the diff does not touch it"), "got: {said}");
    assert!(repo.note("test", &tip).is_none(), "no note lands on a refusal");
}

#[test]
fn a_required_layer_with_no_tests_in_the_named_file_is_refused_under_the_manifest() {
    let (repo, _) = wu_branch();
    repo.write("src/empty.rs", "pub fn nothing() {}\n");
    let tip = repo.commit("E-X.W1: a file that declares no test");
    let judgment = JUDGMENT.replace("files: [tests/wu.rs]", "files: [src/empty.rs]");

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "a zero count is a layer nobody has written yet: {said}");
    assert!(said.contains("FAIL  2·manifest"), "{said}");
    assert!(said.contains("needs integration >= 1 or a waiver"), "got: {said}");
}

#[test]
fn a_named_file_the_tip_does_not_hold_is_refused_rather_than_counted_as_zero() {
    let (repo, tip) = wu_branch();
    let judgment = JUDGMENT.replace("files: [src/lib.rs]", "files: [src/never_written.rs]");

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("FAIL  0·judgment"), "{said}");
    assert!(said.contains("src/never_written.rs"), "the file is named: {said}");
    assert!(repo.note("test", &tip).is_none());
}

#[test]
fn a_file_kind_the_counter_does_not_know_is_refused() {
    let (repo, _) = wu_branch();
    repo.write("scripts/fixture.py", "def test_x():\n    pass\n");
    let tip = repo.commit("E-X.W1: a python fixture");
    let judgment = JUDGMENT.replace("files: [src/lib.rs]", "files: [scripts/fixture.py]");

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("FAIL  0·judgment"), "{said}");
    assert!(said.contains("the counter does not know"), "got: {said}");
}

#[test]
fn a_judgment_that_is_not_yaml_is_refused_with_nothing_written() {
    let (repo, tip) = wu_branch();
    let before = repo.note_refs();

    let (code, said) = repo.write_notes("wu", &tip, "wu: [E9.W1\n");
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("FAIL  0·judgment"), "{said}");
    assert!(said.contains("not valid YAML"), "got: {said}");
    assert_eq!(repo.note_refs(), before);
}

#[test]
fn a_wu_id_the_identity_convention_refuses_is_refused() {
    let (repo, tip) = wu_branch();
    let judgment = JUDGMENT.replace("wu: E-X.W1", "wu: E-X");

    let (code, said) = repo.write_notes("wu", &tip, &judgment);
    assert_eq!(code, 1, "{said}");
    assert!(said.contains("FAIL  0·judgment"), "{said}");
    assert!(said.contains("is not E-CODE.Wn"), "got: {said}");
}

#[test]
fn the_template_derives_the_paths_and_tiers_and_writes_nothing() {
    let (repo, tip) = wu_branch();
    let before = repo.note_refs();
    let (code, said) = repo.xtask(&["notes", "write", "wu", "wu/x/one", "--template"]);

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("path: \"docs/use-cases/api.html\""), "{said}");
    assert!(said.contains("tier: T2"), "the tier is handed over, not typed: {said}");
    assert!(said.contains("path: \"docs/work/2026-09-12-plan.html\""));
    assert!(said.contains("integration:  # required at this level"), "{said}");
    assert!(said.contains(&tip), "the range is pinned in the skeleton: {said}");
    assert_eq!(repo.note_refs(), before, "a template writes nothing");
    assert!(repo.note("test", &tip).is_none());
}

#[test]
fn a_run_writes_the_two_notes_and_nothing_else() {
    let (repo, tip) = wu_branch();
    let (code, said) = repo.write_notes("wu", &tip, JUDGMENT);
    assert_eq!(code, 0, "{said}");

    let refs = repo.note_refs();
    assert!(refs.contains("refs/notes/test"), "{refs}");
    assert!(refs.contains("refs/notes/doc"), "{refs}");
    for other in ["dod", "cr", "mr"] {
        assert!(!refs.contains(other), "no {other} note is this command's to write: {refs}");
    }
    assert!(
        !xtask::tally::path(&repo.path).exists(),
        "a note is not a gate run, and writes no tally"
    );
}

#[test]
fn a_standalone_commit_is_written_without_an_identity_block() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "pub fn base() {}\n\n#[test]\nfn one() {}\n");
    let tip = repo.commit("a standalone change");
    let judgment = "\
tests:
  unit:
    files: [src/lib.rs]
    covers: \"the standalone change\"
";
    let (code, said) = repo.write_notes("commit", &tip, judgment);
    assert_eq!(code, 0, "{said}");
    let test = repo.note("test", &tip).expect("the test note is on the tip");
    assert!(test.contains("level: commit"), "{test}");
    assert!(!test.contains("epic:"), "a standalone commit carries no identity block: {test}");
    assert!(!test.contains("wu:"));
    assert!(repo.note("doc", &tip).is_none(), "a commit that touches no doc owes no doc note");
}

#[test]
fn a_standalone_commit_that_touches_a_doc_carries_one() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "pub fn base() {}\n\n#[test]\nfn one() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    let tip = repo.commit("a standalone doc-touching change");
    let judgment = "\
tests:
  unit:
    files: [src/lib.rs]
    covers: \"the standalone change\"
docs:
  - path: docs/use-cases/api.html
    action: updated
    distills: \"the sentence this commit settles\"
";
    let (code, said) = repo.write_notes("commit", &tip, judgment);
    assert_eq!(code, 0, "{said}");
    let doc = repo.note("doc", &tip).expect("the doc note is on the tip");
    assert!(doc.contains("tier: T2"), "{doc}");
    assert!(!doc.contains("epic:"), "still no identity block: {doc}");
}

// A docs-only diff is exempt: the gate returns before it reads a note, and the
// review is not asked for. Demanding one would be the writer inventing work.
#[test]
fn a_diff_the_gate_exempts_is_reported_and_writes_nothing() {
    let repo = Repo::new();
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    let tip = repo.commit("a docs-only change");
    let before = repo.note_refs();

    let (code, said) = repo.write_notes("commit", &tip, "docs: []\n");
    assert_eq!(code, 0, "an exempt diff is not refused: {said}");
    assert!(said.contains("no review is asked for"), "got: {said}");
    assert!(said.contains("nothing was written"), "got: {said}");
    assert_eq!(repo.note_refs(), before);
}
