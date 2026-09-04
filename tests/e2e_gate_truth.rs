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
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DESIGN: &str = "docs/work/components/flow-badge.html";
const SURFACE: &str = "ui/src/components/FlowBadge.tsx";
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const REPORT: &str = "docs/work/plan.html";

fn xtask_bin() -> PathBuf {
    let mut dir = std::env::current_exe().unwrap();
    dir.pop(); // deps/
    dir.pop(); // debug/
    let bin = dir.join("xtask");
    assert!(bin.exists(), "build xtask first: {}", bin.display());
    bin
}

struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Self {
        let repo = Self { dir: tempfile::tempdir().unwrap() };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
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
        repo.install_merge_hook();
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git_raw(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new("git");
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(k);
            }
        }
        cmd.args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.invalid")
            .output()
            .expect("spawn git")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.git_raw(args);
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }

    fn xtask(&self, args: &[&str]) -> Output {
        Command::new(xtask_bin()).args(args).current_dir(self.path()).output().expect("spawn xtask")
    }

    /// The real dispatch rule with the built binary in place of cargo: a merge
    /// into `epic/*` runs the WU gate, into `main` the epic gate.
    fn install_merge_hook(&self) {
        let hooks = self.path().join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-merge-commit");
        fs::write(
            &hook,
            format!(
                "#!/bin/sh\nset -e\nbranch=$(git symbolic-ref --short -q HEAD || true)\n\
                 case \"$branch\" in\nmain) gate=epic ;;\nepic/*) gate=wu ;;\n*) exit 0 ;;\nesac\n\
                 exec \"{}\" verify-dod hook \"$gate\"\n",
                xtask_bin().display()
            ),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev])
    }

    fn blob(&self, rev: &str, path: &str) -> String {
        self.git(&["rev-parse", &format!("{rev}:{path}")])
    }

    fn diff_sha(&self, base: &str, tip: &str) -> String {
        use std::io::Write as _;
        let diff = self
            .git_raw(&["diff", "--no-color", "--no-ext-diff", &format!("{base}..{tip}")])
            .stdout;
        // Match xtask's tool selection: shasum (macOS) or sha256sum (Linux).
        for tool in [["shasum", "-a", "256"].as_slice(), ["sha256sum"].as_slice()] {
            let Ok(mut child) = Command::new(tool[0])
                .args(&tool[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
            else {
                continue;
            };
            child.stdin.take().unwrap().write_all(&diff).unwrap();
            let out = child.wait_with_output().unwrap().stdout;
            if let Some(hex) = String::from_utf8_lossy(&out).split_whitespace().next() {
                return hex.to_string();
            }
        }
        panic!("no sha256 tool (shasum or sha256sum) on PATH");
    }

    /// The hash an `mr/v1` note attests its result report at, over the bytes the
    /// tip ships - the same input `mr confirm` would hash.
    fn report_sha(&self, tip: &str, path: &str) -> String {
        use std::io::Write as _;
        let bytes = self.git_raw(&["show", &format!("{tip}:{path}")]).stdout;
        for tool in [["shasum", "-a", "256"].as_slice(), ["sha256sum"].as_slice()] {
            let Ok(mut child) = Command::new(tool[0])
                .args(&tool[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
            else {
                continue;
            };
            child.stdin.take().unwrap().write_all(&bytes).unwrap();
            let out = child.wait_with_output().unwrap().stdout;
            if let Some(hex) = String::from_utf8_lossy(&out).split_whitespace().next() {
                return hex.to_string();
            }
        }
        panic!("no sha256 tool (shasum or sha256sum) on PATH");
    }

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        self.git(&["notes", &format!("--ref={note_ref}"), "add", "-f", "-m", body, tip]);
    }

    fn merge(&self, target: &str, branch: &str) -> (bool, String) {
        self.git(&["checkout", "-q", target]);
        let out = self.git_raw(&["merge", "--no-ff", "-m", &format!("merge {branch}"), branch]);
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            self.git_raw(&["merge", "--abort"]);
        }
        (out.status.success(), said)
    }
}

fn design_html(body: &str) -> String {
    format!("<html><body><h1>FlowBadge</h1><p>{body}</p></body></html>\n")
}

/// P2b then P2a, over the doc the rest of the test then leans on: the link
/// check reds a dead `href` and goes green once it resolves, and the `mr commit`
/// that lands the result reports landing rather than `error:`.
fn land_the_report(repo: &Repo) {
    repo.write(REPORT, "<html><a href=\"nowhere.html\">gone</a></html>\n");
    let dead = repo.xtask(&["verify-doc", REPORT]);
    let dead_said = String::from_utf8_lossy(&dead.stdout).to_string();
    assert!(!dead.status.success(), "a dead relative href is red: {dead_said}");
    assert!(dead_said.contains("nowhere.html"), "the red names the link: {dead_said}");

    repo.write(REPORT, "<html><a href=\"../target.html\">the target</a></html>\n");
    let live = repo.xtask(&["verify-doc", REPORT]);
    assert!(
        live.status.success(),
        "the same doc is green once the href resolves: {}",
        String::from_utf8_lossy(&live.stdout)
    );

    repo.git(&["add", "-A"]);
    let before = repo.sha("HEAD");
    let committed = repo.xtask(&["mr", "commit", "-m", "docs: the plan", "--as", "a@b.co"]);
    let said = String::from_utf8_lossy(&committed.stdout).to_string();
    assert!(committed.status.success(), "the commit lands: {said}");
    assert!(!said.contains("error"), "a landed commit reports no error: {said}");
    assert_ne!(repo.sha("HEAD"), before, "and HEAD actually moved");
}

/// The history every design→build WU produces under DEC-16: the design lands,
/// the component is built from it, and the design is retired with it - so the
/// path resolves nowhere at the tip its citation is judged at. Returns the blob
/// the build read, the only pin DEC-28 accepts.
fn build_the_badge_and_retire_its_design(repo: &Repo) -> String {
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
    let repo = Repo::new();
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
    let (merged, said) = repo.merge("epic/x", "wu/x/one");
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
    let (merged, said) = repo.merge("epic/x", "wu/x/one");
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
    let (merged, said) = repo.merge("epic/x", "wu/x/one");
    assert!(
        merged,
        "a retired design cited at its last state, signed by a listed signer, merges: {said}"
    );

    // The citation resolved through history at a real merge — the whole point of
    // DEC-28, and the state that was unreachable before this epic.
    assert_eq!(repo.sha("HEAD"), repo.sha("epic/x"), "the seam is on the epic mainline");
    assert!(
        repo.git_raw(&["cat-file", "-e", &format!("{}:{DESIGN}", repo.sha("HEAD"))])
            .status
            .success()
            .eq(&false),
        "and the design stayed retired - the merge did not resurrect it"
    );
}
