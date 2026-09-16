//! A hermetic git repo driven through the real `xtask` binary — the substrate
//! the gate e2es share. Fixtures and hooks stay with each test: what lives here
//! is only what "a temp repo, real git, real binary" means, which is identical
//! wherever it is asked for.
#![allow(dead_code)] // each e2e uses a subset; the harness is the union

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The `xtask` built for this test run, beside the test deps — called directly
/// so a hook installed from it is hermetic (no cargo, no PATH assumptions).
pub fn xtask_bin() -> PathBuf {
    let mut dir = std::env::current_exe().unwrap();
    dir.pop(); // deps/
    dir.pop(); // debug/
    let bin = dir.join("xtask");
    assert!(bin.exists(), "build xtask first: {}", bin.display());
    bin
}

pub struct GitRepo {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl GitRepo {
    /// An initialised repo on `main` with committer identity and signing off —
    /// no fixture, no commit. The caller writes what its own story needs.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Self { _dir: dir, path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Git with the ambient environment stripped: an inherited `GIT_*` (the
    /// harness may itself run inside a hook) would silently retarget the repo.
    pub fn git_raw(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.args(args)
            .current_dir(&self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.invalid")
            .output()
            .expect("spawn git")
    }

    /// [`GitRepo::git_raw`] asserting success, returning trimmed stdout.
    pub fn git(&self, args: &[&str]) -> String {
        let out = self.git_raw(args);
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }

    pub fn xtask(&self, args: &[&str]) -> Output {
        Command::new(xtask_bin()).args(args).current_dir(&self.path).output().expect("spawn xtask")
    }

    pub fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev])
    }

    pub fn blob(&self, rev: &str, path: &str) -> String {
        self.git(&["rev-parse", &format!("{rev}:{path}")])
    }

    pub fn note(&self, note_ref: &str, tip: &str, body: &str) {
        self.git(&["notes", &format!("--ref={note_ref}"), "add", "-f", "-m", body, tip]);
    }

    /// Install a git hook that execs the built binary.
    pub fn install_hook(&self, name: &str, body: &str) {
        let hooks = self.path.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join(name);
        fs::write(&hook, body).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The sha256 of `base..tip`'s diff, matching what `cargo xtask verify-dod`
    /// recomputes — the flags and the tool selection must both agree or the
    /// binding a `cr` note carries will not verify.
    pub fn diff_sha(&self, base: &str, tip: &str) -> String {
        let range = format!("{base}..{tip}");
        let diff = self.git_raw(&["diff", "--no-color", "--no-ext-diff", &range]).stdout;
        sha256(&diff)
    }

    /// The sha256 of `base..tip` restricted to `paths` — the subset a
    /// `design/v1` note pins, and the shape `code_sha` narrows to the code paths.
    pub fn paths_sha(&self, base: &str, tip: &str, paths: &[&str]) -> String {
        let range = format!("{base}..{tip}");
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", range.as_str(), "--"];
        args.extend_from_slice(paths);
        sha256(&self.git_raw(&args).stdout)
    }

    /// The sha256 a `cr/v5` note binds: the code paths' half of `base..tip`
    /// (DEC-45), matching `cargo xtask verify-dod`'s recomputation.
    pub fn code_sha(&self, base: &str, tip: &str) -> String {
        self.paths_sha(base, tip, &["src", "xtask", "tests", "ui/src", "ui/e2e"])
    }

    /// The sha256 of a path's bytes at a revision — what an `mr/v1` note
    /// attests its result report at.
    pub fn report_sha(&self, rev: &str, path: &str) -> String {
        sha256(&self.git_raw(&["show", &format!("{rev}:{path}")]).stdout)
    }
}

/// Match xtask's tool selection: shasum (macOS) or sha256sum (Linux).
fn sha256(bytes: &[u8]) -> String {
    for tool in [["shasum", "-a", "256"].as_slice(), ["sha256sum"].as_slice()] {
        let Ok(mut child) = Command::new(tool[0])
            .args(&tool[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        let out = child.wait_with_output().unwrap().stdout;
        if let Some(hex) = String::from_utf8_lossy(&out).split_whitespace().next() {
            return hex.to_string();
        }
    }
    panic!("no sha256 tool (shasum or sha256sum) on PATH");
}
