#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

// E-SIMPWF.W2's contract: the review's preflight refuses while what the review is
// about to read is not settled - the tree Gate 0 proves, and the notes the finders
// judge - and refuses nothing else. The tree half is the real `gate0`, driven
// through its own cache so the suite never spawns a build: a checkout whose tree
// is already proven green is exactly the state a review normally starts in.

use std::path::PathBuf;

use tempfile::TempDir;
use xtask::gate0;
use xtask::git::worktree_key;

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

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        let ref_arg = format!("--ref={note_ref}");
        self.git(&["notes", &ref_arg, "add", "-f", "-m", body, tip]);
    }

    // Gate 0's own cache, written the way a green run writes it: the key of the
    // tree it vouched for. Inside the gitignored target/, so seeding it moves no
    // byte of the working tree the key hashes.
    fn prove_gate0_green(&self) {
        let cache = gate0::cache_path(&self.path);
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(cache, worktree_key(&self.path).unwrap()).unwrap();
    }

    fn preflight(&self, level: &str, tip: &str) -> (i32, String) {
        self.xtask(&["preflight", level, tip, "--target", "epic/x"])
    }

    fn preflight_deferred(&self, level: &str, tip: &str) -> (i32, String) {
        self.xtask(&["preflight", level, tip, "--target", "epic/x", "--defer-gate0"])
    }

    fn xtask(&self, args: &[&str]) -> (i32, String) {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        xtask::run(&args, &self.path).unwrap()
    }

    fn note_refs(&self) -> String {
        self.git(&["for-each-ref", "--format=%(refname)", "refs/notes"])
    }
}

fn test_note() -> String {
    "schema: test/v2\nlevel: working-unit\nepic: E9\nwu: E9.W1\nbranch: wu/x/one\ntests:\n  unit: { count: 3, covers: \"the preflight's rendering and its boundary\" }\n  integration: { count: 4, covers: \"the review's inputs, over a real repo\" }\n"
        .to_string()
}

fn doc_note() -> String {
    "schema: doc/v1\nepic: E9\nwu: E9.W1\nbranch: wu/x/one\nchanges:\n  - path: docs/use-cases/api.html\n    tier: T2\n    action: updated\n    distills: \"the preflight's contract\"\n"
        .to_string()
}

/// A WU branch off `epic/x` whose diff is code, so the review is asked for.
fn wu_branch() -> (Repo, String) {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.write("docs/use-cases/api.html", "<html>v2</html>\n");
    let tip = repo.commit("E9.W1: the code and the doc");
    (repo, tip)
}

#[test]
fn a_green_tree_and_the_notes_the_review_reads_pass() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 0, "a settled tip must not be refused: {said}");
    assert!(said.contains("preflight working-unit green"), "got: {said}");
    assert!(said.contains("spawn the finders"), "the review may start, and is told so: {said}");
}

#[test]
fn a_malformed_note_refuses_with_the_gates_own_block() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, "schema: test/v2\nlevel: [\n");
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 1, "a note the review cannot read stops it: {said}");
    assert!(said.contains("FAIL  1·notes"), "the blocker is named: {said}");
    assert!(said.contains("fix the note body to match the test/v2 contract"), "got: {said}");
    assert!(said.contains("no finder may spawn"), "got: {said}");
}

#[test]
fn an_absent_note_refuses_before_the_review_runs() {
    let (repo, tip) = wu_branch();
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 1, "got: {said}");
    assert!(said.contains("FAIL  1·notes"), "got: {said}");
    assert!(said.contains("no test/v2 note on tip"), "got: {said}");
}

#[test]
fn a_test_layer_nobody_wrote_yet_refuses_under_the_manifest() {
    let (repo, tip) = wu_branch();
    repo.note(
        "test",
        &tip,
        "schema: test/v2\nlevel: working-unit\nepic: E9\nwu: E9.W1\ntests:\n  unit: { count: 3 }\n  integration: { count: 0 }\n",
    );
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 1, "a missing layer is a test that does not exist yet: {said}");
    assert!(said.contains("FAIL  2·manifest"), "got: {said}");
    assert!(said.contains("needs integration >= 1 or a waiver"), "got: {said}");
}

// The tree half is really Gate 0, and a tree with no proof behind it is refused:
// the fixture repo is not a cargo project, so the three commands fail and the
// refusal names the check that owns that, not a note.
#[test]
fn an_unproven_tree_refuses_under_the_tree_check() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 1, "got: {said}");
    assert!(said.contains("FAIL  0·tree"), "the tree is named as the blocker: {said}");
    assert!(said.contains("before the review runs"), "the fix is stated: {said}");
}

// Gate 0 proves the working tree; the review binds base..tip. So a green read off
// a different commit, or off uncommitted work, would vouch for bytes the verdict
// is not about - the two ways a checkout can lie to a review.
#[test]
fn a_checkout_on_another_commit_does_not_vouch_for_the_reviewed_tip() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.write("src/other.rs", "pub fn elsewhere() {}\n");
    let head = repo.commit("a later commit on the branch");
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(
        code, 1,
        "the checked-out tree is not the tip's, so its green is not the tip's: {said}"
    );
    assert!(said.contains("FAIL  0·tree"), "got: {said}");
    assert!(said.contains("not the reviewed tip"), "got: {said}");
    assert!(said.contains("check out the reviewed tip"), "the fix is stated: {said}");
    assert!(
        said.contains(&head[..10]),
        "the checkout's own sha is named, so the block says what to look at: {said}"
    );
}

#[test]
fn uncommitted_work_refuses_before_the_review() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\npub fn mid_edit() {}\n");

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 1, "a diff that is about to move cannot be reviewed yet: {said}");
    assert!(said.contains("FAIL  0·tree"), "got: {said}");
    assert!(said.contains("uncommitted changes"), "got: {said}");
}

#[test]
fn a_diff_that_asks_for_no_review_is_never_refused_and_demands_no_note() {
    let repo = Repo::new();
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("docs/work/plan.html", "<html>a plan, and nothing to review</html>\n");
    let tip = repo.commit("E9.W1: docs only");

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 0, "no review is asked for, so nothing refuses it: {said}");
    assert!(said.contains("no review is asked for"), "got: {said}");
    assert!(
        !said.contains("1·notes"),
        "no note is demanded of a diff with nothing to review: {said}"
    );
}

// W4: the review reads no doc note, and a doc note can no longer void a PASS - so
// a malformed one is not a blocker in front of the fleet, while the note the
// finders do read still stops it.
#[test]
fn a_malformed_doc_note_no_longer_stops_the_review() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, "schema: doc/v1\nchanges: [\n");
    repo.prove_gate0_green();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 0, "a note the review does not read cannot stop it: {said}");

    let (gate_code, gate) = repo.xtask(&["verify-dod", "wu", &tip, "--target", "epic/x"]);
    assert_eq!(gate_code, 1, "the merge gate still reads it: {gate}");
    assert!(gate.contains("doc/v1"), "got: {gate}");
}

// The preflight is not a second gate: it asks for the notes the review reads and
// never for the evidence the review writes. The gate, on the same tip, asks for
// both - which is the one difference that makes the preflight a preflight.
#[test]
fn the_preflight_never_asks_for_the_note_the_review_writes() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();

    let (preflight_code, preflight) = repo.preflight("wu", &tip);
    assert_eq!(preflight_code, 0, "the exam's own evidence is not a precondition: {preflight}");
    assert!(!preflight.contains("cr/v5"), "got: {preflight}");

    let (gate_code, gate) = repo.xtask(&["verify-dod", "wu", &tip, "--target", "epic/x"]);
    assert_eq!(gate_code, 1, "the merge gate still demands it: {gate}");
    assert!(gate.contains("no cr/v5 note on tip"), "got: {gate}");
}

#[test]
fn running_the_preflight_writes_nothing() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.prove_gate0_green();
    let before = repo.note_refs();

    let (code, said) = repo.preflight("wu", &tip);
    assert_eq!(code, 0, "got: {said}");
    assert_eq!(before, repo.note_refs(), "a reading writes no note");
    assert!(
        !xtask::tally::path(&repo.path).exists(),
        "a preflight is not a gate run and is never tallied (DEC-42)"
    );
    assert!(
        !xtask::git::git(&repo.path, &["notes", "--ref=dod", "list"]).unwrap().contains(&tip),
        "no receipt is written by a preflight"
    );
}

// E-SIMPWF.W8 · L2. Gate 0 is the minute the preflight costs; deferring it lets
// the finder stage start beside it. What must NOT move is the checkout identity:
// Gate 0 measures a working tree, so a deferred run still has to prove that tree
// is the reviewed tip's before it lets a finder read it.

// The fixture repo is not a cargo project, so an undeferred preflight reds under
// 0·tree (see an_unproven_tree_refuses_under_the_tree_check). Deferring greens the
// same tip without ever spawning the suite - that difference is the whole lever.
#[test]
fn a_deferred_preflight_greens_a_tree_it_never_measured() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());

    let (undeferred, _) = repo.preflight("wu", &tip);
    assert_eq!(undeferred, 1, "the unmeasured tree reds when the preflight measures it");

    let (code, said) = repo.preflight_deferred("wu", &tip);
    assert_eq!(code, 0, "deferring the suite must not refuse on it: {said}");
    assert!(said.contains("the tree is NOT measured here"), "the gap is stated: {said}");
    assert!(said.contains("cargo xtask gate0"), "the caller is told to run it: {said}");
    assert!(said.contains("discards the round"), "a red Gate 0 costs the round: {said}");
}

#[test]
fn a_deferred_preflight_still_refuses_a_checkout_that_is_not_the_reviewed_tip() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.write("src/other.rs", "pub fn elsewhere() {}\n");
    repo.commit("a later commit on the branch");

    let (code, said) = repo.preflight_deferred("wu", &tip);
    assert_eq!(code, 1, "a concurrent Gate 0 would measure the wrong tree: {said}");
    assert!(said.contains("FAIL  0·tree"), "got: {said}");
    assert!(said.contains("not the reviewed tip"), "got: {said}");
}

#[test]
fn a_deferred_preflight_still_refuses_uncommitted_work() {
    let (repo, tip) = wu_branch();
    repo.note("test", &tip, &test_note());
    repo.note("doc", &tip, &doc_note());
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\npub fn mid_edit() {}\n");

    let (code, said) = repo.preflight_deferred("wu", &tip);
    assert_eq!(code, 1, "got: {said}");
    assert!(said.contains("uncommitted changes"), "got: {said}");
}

// Deferring the tree must not silently drop the paperwork the finders read.
#[test]
fn a_deferred_preflight_still_refuses_a_note_the_review_needs() {
    let (repo, tip) = wu_branch();
    repo.note("doc", &tip, &doc_note());

    let (code, said) = repo.preflight_deferred("wu", &tip);
    assert_eq!(code, 1, "an absent test/v2 still stops the round: {said}");
    assert!(!said.contains("0·tree"), "the tree is not what refused it: {said}");
}
