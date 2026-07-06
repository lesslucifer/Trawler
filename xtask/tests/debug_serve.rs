#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::PathBuf;

use tempfile::TempDir;
use xtask::debug::render;

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
        repo.git(&["config", "user.email", "nv@test"]);
        repo.git(&["config", "user.name", "nv"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        xtask::git::git(&self.path, args).unwrap_or_else(|e| panic!("{e}"))
    }

    fn commit(&self, msg: &str) -> String {
        std::fs::write(self.path.join("f"), msg).unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        let ref_arg = format!("--ref={note_ref}");
        self.git(&["notes", &ref_arg, "add", "-f", "-m", body, tip]);
    }
}

fn short(sha: &str) -> String {
    sha[..7].to_string()
}

#[test]
fn full_render_groups_by_epic_wu_with_chips_invalid_and_modal_data() {
    let repo = Repo::new();

    let c1 = repo.commit("first WU commit");
    repo.note(
        "test",
        &c1,
        "schema: test/v2\nlevel: working-unit\nepic: E1\nwu: E1.W1\ntests:\n  unit: { count: 3 }\n  integration: { waived: \"no boundary\" }",
    );
    repo.note(
        "cr",
        &c1,
        "schema: cr/v2\nepic: E1\nwu: E1.W1\nbase: aaaa\ndiff_sha256: dddd\nverdict: PASS\nfindings: { found: 2, verified: 0, waived: 0 }",
    );
    repo.note(
        "doc",
        &c1,
        "schema: doc/v1\nepic: E1\nwu: E1.W1\nchanges:\n  - { path: docs/workflow.html, tier: T1, action: updated }",
    );
    repo.note(
        "dod",
        &c1,
        "schema: dod/v1\nlevel: working-unit\nepic: E1\nwu: E1.W1\nbase: aaaa\ntip: bbbb\ndiff_sha256: dddd\nchecks: [notes, manifest, review]",
    );

    let c2 = repo.commit("second epic partial commit");
    repo.note(
        "test",
        &c2,
        "schema: test/v2\nlevel: commit\nepic: E2\nwu: E2.W1\ntests:\n  unit: { count: 5 }",
    );

    let c3 = repo.commit("stray commit-level note");
    repo.note("test", &c3, "schema: test/v2\nlevel: commit\ntests:\n  unit: { count: 1 }");

    let html = render(&repo.path, "http://127.0.0.1:48027/");

    assert!(html.contains("<h2>E1</h2>"), "epic E1 heading");
    assert!(html.contains("<h2>E2</h2>"), "epic E2 heading");
    assert!(html.contains("E1 › E1.W1"), "WU grouping under epic");

    assert!(html.contains("test(3/w/–)"), "test chip: unit count, waived integration, absent e2e");
    assert!(html.contains("cr ⚑2"), "cr chip: findings flag, color by class");
    assert!(html.contains(r#"class="chip pass" data-note="cr-"#), "cr PASS colored");
    assert!(html.contains("doc(1)"), "doc chip: change count");
    assert!(html.contains("dod ✓3"), "dod chip: gate-check count");

    assert!(
        html.contains(r#"<span class="chip none">cr —</span>"#),
        "absent cr is dashed, non-clickable"
    );

    assert!(html.contains(r#"class="badge inv">invalid"#), "invalid bucket rendered");
    assert!(html.contains(&format!("test-{}", short(&c3))), "stray note appears in invalid bucket");

    assert!(
        html.contains(&format!("'test-{}':", short(&c1))),
        "modal data map carries the raw note"
    );
    assert!(html.contains("no boundary"), "raw YAML body embedded verbatim for the modal");
}
