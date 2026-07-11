//! E-NMWF epic e2e — the full manual-review flow over a hermetic temp repo,
//! driven through the real `xtask` binary and real git (no shims).
//!
//! Covers the rules end to end (DEC-7/DEC-9/DEC-10): a rule-bearing doc commit
//! (T0 CLAUDE.md / T1 flat docs/) needs a listed `Reviewed-By` trailer at
//! commit time while T2/T3 doc commits pass unsigned; an epic tip always needs
//! a listed `mr/v1` note attesting its result report; unlisted identities fail.
#![cfg(all(unix, feature = "hook-tests"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn xtask_bin() -> PathBuf {
    // tests run with the workspace built; the binary sits beside the test deps.
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
        let dir = tempfile::tempdir().unwrap();
        let r = Self { dir };
        r.git(&["init", "-q", "-b", "main"]);
        r.git(&["config", "user.email", "gate@test"]);
        r.git(&["config", "user.name", "gate"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r.write(".mr-allowed", "email: a@b.co\nusername: vu\n");
        r.write("src/lib.rs", "pub fn base() {}\n");
        r.write("docs/use-cases/api.html", "<html>v1</html>\n");
        r.git(&["add", "-A"]);
        r.git(&["commit", "-q", "-m", "init"]);
        r
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

    fn git(&self, args: &[&str]) {
        let out = self.git_raw(args);
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }

    fn xtask(&self, args: &[&str]) -> Output {
        Command::new(xtask_bin()).args(args).current_dir(self.path()).output().expect("spawn xtask")
    }

    fn install_commit_msg_hook(&self) {
        let hooks = self.path().join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let dest = hooks.join("commit-msg");
        let bin = xtask_bin();
        // Call the built binary directly so the hook is hermetic (no cargo).
        fs::write(
            &dest,
            format!("#!/bin/sh\nexec \"{}\" verify-dod commit-msg \"$1\"\n", bin.display()),
        )
        .unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn sha(&self, rev: &str) -> String {
        String::from_utf8_lossy(&self.git_raw(&["rev-parse", rev]).stdout).trim().to_string()
    }

    fn diff_sha(&self, base: &str, tip: &str) -> String {
        use std::io::Write as _;
        let range = format!("{base}..{tip}");
        let diff = self.git_raw(&["diff", "--no-color", "--no-ext-diff", &range]).stdout;
        // Match xtask's tool selection: shasum (macOS) or sha256sum (Linux).
        for tool in [["shasum", "-a", "256"].as_slice(), ["sha256sum"].as_slice()] {
            let Ok(mut c) = Command::new(tool[0])
                .args(&tool[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
            else {
                continue;
            };
            c.stdin.take().unwrap().write_all(&diff).unwrap();
            let out = c.wait_with_output().unwrap().stdout;
            if let Some(hex) = String::from_utf8_lossy(&out).split_whitespace().next() {
                return hex.to_string();
            }
        }
        panic!("no sha256 tool (shasum or sha256sum) on PATH");
    }

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        self.git(&["notes", &format!("--ref={note_ref}"), "add", "-f", "-m", body, tip]);
    }
}

#[test]
fn full_manual_review_flow() {
    let repo = Repo::new();
    repo.install_commit_msg_hook();

    // (a) a rule-bearing (T1) doc commit with no trailer is refused by the
    // commit-msg hook — while a T2 doc commit passes unsigned (DEC-10).
    repo.write("docs/use-cases/api.html", "<html>v2 edit</html>\n");
    repo.git(&["add", "-A"]);
    let t2_ok = repo.git_raw(&["commit", "-q", "-m", "docs: t2 edit, no trailer"]);
    assert!(t2_ok.status.success(), "a T2 doc commit needs no trailer");

    repo.write("docs/standard.html", "<html>t1 standard</html>\n");
    repo.git(&["add", "-A"]);
    let refused = repo.git_raw(&["commit", "-q", "-m", "docs: t1 edit without trailer"]);
    assert!(!refused.status.success(), "a T1 doc commit without a trailer must be refused");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("self-authorise")
            || String::from_utf8_lossy(&refused.stdout).contains("self-authorise"),
        "the refusal explains a human must confirm"
    );

    // (b) an unlisted identity is refused by `mr commit`.
    let unlisted = repo.xtask(&["mr", "commit", "-m", "docs: edit", "--as", "ghost@x.co"]);
    assert!(
        String::from_utf8_lossy(&unlisted.stdout).contains("not listed in .mr-allowed"),
        "an unlisted identity is refused"
    );

    // (c) a listed identity via `mr commit` lands the doc commit with a trailer.
    let ok = repo.xtask(&["mr", "commit", "-m", "docs: edit", "--as", "a@b.co"]);
    assert!(
        ok.status.success(),
        "listed identity commits: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let msg = String::from_utf8_lossy(&repo.git_raw(&["log", "-1", "--format=%B", "HEAD"]).stdout)
        .to_string();
    assert!(msg.contains("Reviewed-By: email a@b.co"), "the trailer is recorded");

    // Build an epic with one merged WU so the epic gate has a real seam.
    repo.git(&["branch", "epic/x"]);
    repo.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
    repo.write("src/lib.rs", "pub fn base() {}\npub fn added() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "wu code"]);
    let wu_tip = repo.sha("HEAD");
    let wu_base = repo.sha("epic/x");
    let wu_diff = repo.diff_sha(&wu_base, &wu_tip);
    repo.note("test", &wu_tip, "schema: test/v2\nlevel: working-unit\nepic: E9\nwu: E9.W1\ntests:\n  integration: { count: 1 }\n");
    repo.note(
        "doc",
        &wu_tip,
        "schema: doc/v1\nepic: E9\nwu: E9.W1\nchanges: []\nt2_waived: \"n/a\"\n",
    );
    repo.note("cr", &wu_tip, &format!("schema: cr/v2\nepic: E9\nwu: E9.W1\nbase: {wu_base}\nhead: {wu_tip}\ndiff_sha256: {wu_diff}\nverdict: PASS\n"));
    repo.git(&["checkout", "-q", "epic/x"]);
    repo.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);

    // Bless the epic tip fully — everything EXCEPT the mr note.
    let epic_tip = repo.sha("epic/x");
    let ebase = repo.sha("main");
    let ediff = repo.diff_sha(&ebase, &epic_tip);
    repo.note(
        "test",
        &epic_tip,
        "schema: test/v2\nlevel: epic\nepic: E9\ntests:\n  e2e: { count: 1 }\n",
    );
    repo.note("doc", &epic_tip, "schema: doc/v1\nepic: E9\nchanges: []\nt2_waived: \"n/a\"\n");
    repo.note("cr", &epic_tip, &format!("schema: cr/v2\nepic: E9\nbase: {ebase}\nhead: {epic_tip}\ndiff_sha256: {ediff}\nverdict: PASS\n"));

    // (d) with every other note present, the epic is red for ONE reason: no mr note.
    let red = repo.xtask(&["verify-dod", "epic", "epic/x", "--target", "main"]);
    let red_out = String::from_utf8_lossy(&red.stdout);
    assert!(!red.status.success(), "an otherwise-blessed epic without an mr note is red");
    assert!(red_out.contains("8·manual-review"), "the red is the manual-review check: {red_out}");
    assert_eq!(
        red_out.matches("FAIL").count(),
        1,
        "manual review is the SOLE remaining failure: {red_out}"
    );

    // (e) signing the epic — attesting its (archived) result report — clears
    // the last check and the gate goes green.
    repo.write("docs/.archive/e9-result.html", "<html>epic result</html>\n");
    let signed = repo.xtask(&[
        "mr",
        "confirm",
        "epic",
        "epic/x",
        "--as",
        "a@b.co",
        "--report",
        "docs/.archive/e9-result.html",
    ]);
    assert!(signed.status.success(), "epic sign-off: {}", String::from_utf8_lossy(&signed.stderr));

    let green = repo.xtask(&["verify-dod", "epic", "epic/x", "--target", "main"]);
    assert!(
        green.status.success(),
        "a fully-blessed, signed epic is green: {}",
        String::from_utf8_lossy(&green.stdout)
    );
}
