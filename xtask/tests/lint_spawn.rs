#![cfg(unix)]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

// What `cargo xtask lint` actually spawns. The unit test beside CLIPPY pins the
// constant's contents; only this pins that `run_lints` forwards it to a real
// process - the guarantee the pre-commit hook used to carry in its own text,
// where a test could read the executed command verbatim. Dropping --workspace
// inside run_lints while leaving CLIPPY untouched fails here and nowhere else.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

struct Env {
    _dir: TempDir,
    repo: std::path::PathBuf,
    home: std::path::PathBuf,
    log: std::path::PathBuf,
    git_env_log: std::path::PathBuf,
    cwd_log: std::path::PathBuf,
    rustc: std::path::PathBuf,
    rustc_log: std::path::PathBuf,
    path: String,
}

struct Machine {
    nextest: bool,
    first_launch_secs: u32,
}

const FAST: Machine = Machine { nextest: true, first_launch_secs: 0 };

impl Env {
    // `cargo_exit` is what the shim reports: 0 stands in for a green fmt/clippy,
    // non-zero for a red one.
    fn new(cargo_exit: i32) -> Self {
        Self::on(&FAST, cargo_exit)
    }

    // `first_launch_secs` is how long the program the rustc shim links runs, so
    // the launch probe sees that machine's first-launch wait.
    fn on(machine: &Machine, cargo_exit: i32) -> Self {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        let bin = dir.path().join("bin");
        let home = dir.path().join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&home).unwrap();
        let log = dir.path().join("cargo-calls.log");
        let git_env_log = dir.path().join("cargo-git-env.log");
        let cwd_log = dir.path().join("cargo-cwd.log");

        let nextest_exit = i32::from(!machine.nextest);
        let shim = bin.join("cargo");
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\n[ \"$1 $2\" = 'nextest --version' ] && exit {nextest_exit}\necho \"$@\" >> \"{}\"\nenv | grep '^GIT_' >> \"{}\"\npwd -P >> \"{}\"\nexit {cargo_exit}\n",
                log.display(),
                git_env_log.display(),
                cwd_log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        let pnpm = bin.join("pnpm");
        fs::write(
            &pnpm,
            format!("#!/bin/sh\necho \"pnpm $@ in $(pwd -P)\" >> \"{}\"\nexit 0\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&pnpm, fs::Permissions::from_mode(0o755)).unwrap();
        let rustc_log = dir.path().join("rustc-calls.log");
        let rustc = bin.join("rustc-shim");
        fs::write(
            &rustc,
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\nwhile [ $# -gt 0 ]; do [ \"$1\" = -o ] && out=$2; shift; done\n\
                 printf '#!/bin/sh\\nsleep {}\\n' > \"$out\"\nchmod +x \"$out\"\n",
                rustc_log.display(),
                machine.first_launch_secs
            ),
        )
        .unwrap();
        fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755)).unwrap();

        let env = Self {
            _dir: dir,
            repo,
            home,
            log,
            git_env_log,
            cwd_log,
            rustc,
            rustc_log,
            path: format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
        };
        env.git(&["init", "-q", "-b", "main"]);
        env.git(&["config", "user.email", "lint@test"]);
        env.git(&["config", "user.name", "lint"]);
        // Belt and braces with the scrubbed HOME above: a contributor with
        // commit.gpgsign on would otherwise have this suite's commits block on a
        // Touch ID prompt that never comes.
        env.git(&["config", "commit.gpgsign", "false"]);
        // The marker lands under target/, which the key would otherwise hash as
        // an untracked build input - writing the proof would invalidate the very
        // tree it vouches for. Every cargo repo ignores target/; this one must
        // too, or the skip can never fire.
        fs::write(env.repo.join(".gitignore"), "/target\n").unwrap();
        fs::write(env.repo.join("src.rs"), "pub fn a() {}\n").unwrap();
        env.git(&["add", "-A"]);
        env.git(&["commit", "-q", "-m", "init"]);
        env
    }

    fn git(&self, args: &[&str]) {
        let out = self.hermetic("git").args(args).current_dir(&self.repo).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A command that cannot see the developer's git identity, signing setup or
    /// hooks: `scrubbed` drops inherited `GIT_*`, and these put the ambient
    /// config out of reach too. Without them a globally-configured
    /// `commit.gpgsign` reaches this suite's commits.
    fn hermetic(&self, program: impl AsRef<Path>) -> Command {
        let mut cmd = scrubbed(program);
        cmd.env("HOME", &self.home)
            .env("RUSTC", &self.rustc)
            .env_remove("XDG_CACHE_HOME")
            .env_remove(xtask::launch::ACCEPT_SLOW)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "lint")
            .env("GIT_AUTHOR_EMAIL", "lint@test.invalid")
            .env("GIT_COMMITTER_NAME", "lint")
            .env("GIT_COMMITTER_EMAIL", "lint@test.invalid");
        cmd
    }

    fn lint(&self) -> (bool, String) {
        let out = self
            .hermetic(env!("CARGO_BIN_EXE_xtask"))
            .arg("lint")
            .current_dir(&self.repo)
            .env("PATH", &self.path)
            .output()
            .unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn gate0(&self, accept_slow: bool) -> (Option<i32>, String) {
        let mut cmd = self.hermetic(env!("CARGO_BIN_EXE_xtask"));
        cmd.arg("gate0").current_dir(&self.repo).env("PATH", &self.path);
        if accept_slow {
            cmd.env(xtask::launch::ACCEPT_SLOW, "1");
        }
        let out = cmd.output().unwrap();
        (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn probes_linked(&self) -> usize {
        fs::read_to_string(&self.rustc_log).unwrap_or_default().lines().count()
    }

    fn gate0_merge_inside_a_hook(&self) -> (bool, String) {
        let out = self
            .hermetic(env!("CARGO_BIN_EXE_xtask"))
            .args(["gate0", "merge"])
            .current_dir(&self.repo)
            .env("PATH", &self.path)
            .env("GIT_INDEX_FILE", ".git/index")
            .env("GIT_REFLOG_ACTION", "merge epic/x")
            .env("GIT_EDITOR", ":")
            .output()
            .unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn git_stdout(&self, args: &[&str]) -> String {
        let out = self.hermetic("git").args(args).current_dir(&self.repo).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn gate0_push(&self, refspecs: &str) -> (bool, String) {
        use std::io::Write as _;
        let mut child = self
            .hermetic(env!("CARGO_BIN_EXE_xtask"))
            .args(["gate0", "push"])
            .current_dir(&self.repo)
            .env("PATH", &self.path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(refspecs.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn cargo_dirs(&self) -> Vec<String> {
        fs::read_to_string(&self.cwd_log).unwrap_or_default().lines().map(str::to_string).collect()
    }

    fn scratch_root(&self) -> std::path::PathBuf {
        self.repo.join("target").join("push-proof").canonicalize().unwrap()
    }

    fn assert_no_scratch_left(&self) {
        let root = self.repo.join("target").join("push-proof");
        let left = fs::read_dir(&root).map_or(0, Iterator::count);
        assert_eq!(left, 0, "the scratch worktree must be removed");
        assert_eq!(self.git_stdout(&["worktree", "list"]).lines().count(), 1);
    }

    fn git_env_seen_by_cargo(&self) -> String {
        fs::read_to_string(&self.git_env_log).unwrap_or_default()
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log).unwrap_or_default().lines().map(str::to_string).collect()
    }

    fn clippy_call(&self) -> String {
        self.calls()
            .into_iter()
            .find(|c| c.starts_with("clippy"))
            .expect("lint must spawn clippy on a cache miss")
    }
}

fn push_line(sha: &str) -> String {
    format!("refs/heads/epic/x {sha} refs/heads/epic/x 0000000000000000000000000000000000000000\n")
}

// Git exports GIT_* to a test process running under a hook; inherited, GIT_DIR
// overrides current_dir and redirects every "hermetic" call at the real repo.
fn scrubbed(program: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new(program.as_ref());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd
}

// The whole point of this file: the scoping reaches the spawned process. Without
// --workspace the root package is linted and xtask - where every gate, hook and
// design check lives - is not.
#[test]
fn the_spawned_clippy_lints_every_workspace_member() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let clippy = env.clippy_call();
    for flag in ["--workspace", "--all-targets", "--all-features", "-D warnings"] {
        assert!(clippy.contains(flag), "the executed clippy dropped {flag}: cargo {clippy}");
    }
}

#[test]
fn fmt_runs_before_clippy() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let calls = env.calls();
    let fmt = calls.iter().position(|c| c.starts_with("fmt")).expect("lint must spawn fmt");
    let clippy =
        calls.iter().position(|c| c.starts_with("clippy")).expect("lint must spawn clippy");
    assert!(fmt < clippy, "fmt is the cheaper refusal and goes first: {calls:?}");
}

#[test]
fn a_green_run_caches_and_the_next_run_spawns_nothing() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let after_first = env.calls().len();
    let (ok, said) = env.lint();
    assert!(ok);
    assert!(said.contains("skipped"), "the second run must skip: {said}");
    assert_eq!(env.calls().len(), after_first, "a skip spawns no cargo process");
}

#[test]
fn a_red_run_refuses_and_caches_nothing() {
    let env = Env::new(1);
    let (ok, said) = env.lint();
    assert!(!ok, "a red lint must exit non-zero: {said}");
    assert!(
        !xtask::gate0::lint_cache_path(&env.repo).exists(),
        "a red run must not vouch for the tree"
    );
    let (still_red, _) = env.lint();
    assert!(!still_red, "nothing was cached, so the next run must re-run and refuse again");
}

// The marker stores the key it vouches for, never a bare verdict (DEC-11), so a
// tree that moved after the green re-runs rather than riding the old proof.
#[test]
fn a_tree_change_after_the_green_re_runs_the_pair() {
    let env = Env::new(0);
    assert!(env.lint().0);
    let after_first = env.calls().len();
    fs::write(env.repo.join("src.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    assert!(env.lint().0);
    assert!(env.calls().len() > after_first, "an edited tree must re-run fmt and clippy");
}

// A merge hook exports its own GIT_* (probed: GIT_INDEX_FILE, GIT_REFLOG_ACTION,
// GIT_EDITOR, ...); a suite that inherits them reads the hook's merge as its own.
#[test]
fn checks_spawned_inside_a_hook_see_none_of_its_git_environment() {
    let env = Env::new(0);
    let (ok, said) = env.gate0_merge_inside_a_hook();
    assert!(ok, "{said}");
    assert!(
        env.calls().iter().any(|c| c.contains("test")),
        "the suite must run: {:?}",
        env.calls()
    );
    assert_eq!(env.git_env_seen_by_cargo(), "", "no spawned check may inherit GIT_*");
}

#[test]
fn a_red_merged_tree_names_the_abort_and_vouches_for_nothing() {
    let env = Env::new(1);
    let (ok, said) = env.gate0_merge_inside_a_hook();
    assert!(!ok, "{said}");
    assert!(said.contains(xtask::gate0::MERGE_REFUSED), "{said}");
    assert!(!xtask::gate0::cache_path(&env.repo).exists(), "a red tree must not be cached");
}

#[test]
fn a_green_merged_tree_is_cached_and_the_next_merge_of_it_spawns_nothing() {
    let env = Env::new(0);
    assert!(env.gate0_merge_inside_a_hook().0);
    let after_first = env.calls().len();
    let (ok, said) = env.gate0_merge_inside_a_hook();
    assert!(ok);
    assert!(said.contains("skipped"), "{said}");
    assert_eq!(env.calls().len(), after_first, "a proven tree spawns no cargo process");
}

#[test]
fn a_push_of_the_proven_clean_head_spawns_nothing() {
    let env = Env::new(0);
    let (ok, said) = env.gate0_push(&push_line(&env.git_stdout(&["rev-parse", "HEAD"])));
    assert!(ok, "{said}");
    let after_first = env.calls().len();
    let (ok, said) = env.gate0_push(&push_line(&env.git_stdout(&["rev-parse", "HEAD"])));
    assert!(ok);
    assert!(said.contains("gate0 skipped"), "the push reads the marker: {said}");
    assert_eq!(env.calls().len(), after_first, "a proven tree spawns no cargo process");
}

#[test]
fn a_push_of_another_tree_is_proven_in_a_scratch_worktree_then_removed() {
    let env = Env::new(0);
    let older = env.git_stdout(&["rev-parse", "HEAD"]);
    fs::write(env.repo.join("src.rs"), "pub fn b() {}\n").unwrap();
    env.git(&["commit", "-q", "-am", "next"]);

    let (ok, said) = env.gate0_push(&push_line(&older));
    assert!(ok, "{said}");
    assert!(said.contains(&format!("pushed {}", &older[..10])), "{said}");
    let scratch = env.scratch_root();
    assert!(
        env.cargo_dirs().iter().any(|dir| dir.starts_with(scratch.to_str().unwrap())),
        "the suite must run in the scratch worktree: {:?}",
        env.cargo_dirs()
    );
    env.assert_no_scratch_left();
}

#[test]
fn a_dirty_worktree_never_vouches_for_the_pushed_head() {
    let env = Env::new(0);
    let head = env.git_stdout(&["rev-parse", "HEAD"]);
    fs::write(env.repo.join("src.rs"), "pub fn dirty() {}\n").unwrap();

    let (ok, said) = env.gate0_push(&push_line(&head));
    assert!(ok, "{said}");
    let scratch = env.scratch_root();
    let dirs = env.cargo_dirs();
    assert!(
        !dirs.is_empty() && dirs.iter().all(|dir| dir.starts_with(scratch.to_str().unwrap())),
        "{dirs:?}"
    );
    env.assert_no_scratch_left();
}

#[test]
fn a_red_pushed_tree_refuses_and_leaves_no_scratch() {
    let env = Env::new(1);
    let head = env.git_stdout(&["rev-parse", "HEAD"]);
    fs::write(env.repo.join("src.rs"), "pub fn dirty() {}\n").unwrap();

    let (ok, said) = env.gate0_push(&push_line(&head));
    assert!(!ok, "{said}");
    assert!(said.contains(xtask::gate0::PUSH_REFUSED), "{said}");
    env.assert_no_scratch_left();
}

#[test]
fn a_push_of_only_notes_and_deletions_runs_no_gate0() {
    let env = Env::new(0);
    let zero = "0000000000000000000000000000000000000000";
    let head = env.git_stdout(&["rev-parse", "HEAD"]);
    let refspecs = format!(
        "refs/notes/test {head} refs/notes/test {zero}\n(delete) {zero} refs/heads/gone {head}\n"
    );
    let (ok, said) = env.gate0_push(&refspecs);
    assert!(ok, "{said}");
    assert!(env.calls().is_empty(), "{:?}", env.calls());
}

#[test]
fn a_scratch_worktree_gets_the_ui_packages_before_the_suite_builds() {
    let env = Env::new(0);
    fs::create_dir_all(env.repo.join("ui")).unwrap();
    fs::write(env.repo.join("ui/package.json"), "{}\n").unwrap();
    env.git(&["add", "-A"]);
    env.git(&["commit", "-q", "-m", "ui"]);
    let head = env.git_stdout(&["rev-parse", "HEAD"]);
    fs::write(env.repo.join("src.rs"), "pub fn dirty() {}\n").unwrap();

    let (ok, said) = env.gate0_push(&push_line(&head));
    assert!(ok, "{said}");
    let calls = env.calls();
    let install = calls
        .iter()
        .position(|c| c.starts_with("pnpm install --frozen-lockfile"))
        .expect("the scratch must install the UI packages");
    let scratch = env.scratch_root();
    assert!(
        calls[install].ends_with(scratch.join(&head[..10]).join("ui").to_str().unwrap()),
        "{calls:?}"
    );
    let suite = calls.iter().position(|c| c.starts_with("fmt")).expect("the suite runs");
    assert!(install < suite, "packages before the build: {calls:?}");
    env.assert_no_scratch_left();
}

const REFUSED: Option<i32> = Some(xtask::gate0::MACHINE_REFUSED);

#[test]
fn a_machine_without_nextest_is_refused_before_any_check() {
    let env = Env::on(&Machine { nextest: false, ..FAST }, 0);
    let (code, said) = env.gate0(false);
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("cargo install cargo-nextest --locked"), "{said}");
    assert!(env.calls().is_empty(), "no check may run: {:?}", env.calls());
    assert_eq!(env.probes_linked(), 0);
    assert!(!xtask::gate0::cache_path(&env.repo).exists());
}

#[test]
fn a_slow_first_launch_is_refused_at_the_threshold_and_measured_again_next_time() {
    let env = Env::on(&Machine { first_launch_secs: 30, ..FAST }, 0);
    let started = std::time::Instant::now();
    let (code, said) = env.gate0(false);
    assert!(started.elapsed().as_secs() < 15, "the refusal must not await the launch");
    assert_eq!(code, REFUSED, "{said}");
    assert!(said.contains("Developer Tools"), "{said}");
    assert!(said.contains("TRAWLER_ACCEPT_SLOW_LAUNCH=1"), "{said}");
    assert!(env.calls().is_empty(), "no check may run: {:?}", env.calls());

    assert_eq!(env.gate0(false).0, REFUSED);
    assert_eq!(env.probes_linked(), 2, "a slow verdict is never cached");
}

#[test]
fn the_override_skips_the_probe_and_runs_the_checks() {
    let env = Env::on(&Machine { first_launch_secs: 30, ..FAST }, 0);
    let (code, said) = env.gate0(true);
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(env.probes_linked(), 0);
    assert!(env.calls().iter().any(|c| c == "nextest run --all"), "{:?}", env.calls());
}

#[test]
fn a_fast_machine_pays_the_probe_once_and_the_suite_runs_through_nextest() {
    let env = Env::new(0);
    assert_eq!(env.gate0(false).0, Some(0));
    fs::write(env.repo.join("src.rs"), "pub fn moved() {}\n").unwrap();
    assert_eq!(env.gate0(false).0, Some(0));
    let (code, said) = env.gate0(false);
    assert_eq!(code, Some(0));
    assert!(said.contains("gate0 skipped"), "{said}");

    assert_eq!(env.probes_linked(), 1, "the fast verdict is cached for the boot");
    let calls = env.calls();
    assert_eq!(calls.iter().filter(|c| *c == "nextest run --all").count(), 2, "{calls:?}");
    assert!(!calls.iter().any(|c| c.starts_with("test")), "no serial fallback: {calls:?}");
}

#[test]
fn an_unwritable_cache_dir_costs_the_cache_never_the_fast_verdict() {
    let env = Env::new(0);
    fs::write(env.home.join(".cache"), "a file where the cache dir would go").unwrap();
    let (code, said) = env.gate0(false);
    assert_eq!(code, Some(0), "{said}");
    assert!(env.calls().iter().any(|c| c == "nextest run --all"), "{:?}", env.calls());
}
