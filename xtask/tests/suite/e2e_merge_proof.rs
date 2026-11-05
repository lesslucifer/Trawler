#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;
use xtask::gate0::{slow_launch_refusal, MERGE_REFUSED};
use xtask::hooks::TRACKED_DIR;

use crate::hermetic::{executable, git, hermetic, said, write, AUTHOR_EMAIL};

const NEXTEST_RAN: &str = "Starting";

fn part(value: u32) -> String {
    format!("pub fn part() -> u32 {{\n    {value}\n}}\n")
}

struct Flow {
    dir: TempDir,
    repo: PathBuf,
}

impl Flow {
    // The app lives under `app/`, outside the gate's code paths, so the notes
    // gate exempts it and the merge's only owed note is check 8's sign-off.
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        let flow = Self { dir, repo };
        executable(
            &flow.bin().join("cargo-xtask"),
            &format!("#!/bin/sh\nshift\nexec \"{}\" \"$@\"\n", env!("CARGO_BIN_EXE_xtask")),
        );
        write(
            &flow.repo.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\"]\nresolver = \"2\"\n",
        );
        write(
            &flow.repo.join("app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        write(
            &flow.repo.join("app/src/lib.rs"),
            "pub mod a;\npub mod b;\n\n#[test]\nfn the_parts_stay_small() {\n    assert!(a::part() + b::part() < 4);\n}\n",
        );
        write(&flow.repo.join("app/src/a.rs"), &part(1));
        write(&flow.repo.join("app/src/b.rs"), &part(1));
        write(
            &flow.repo.join(".gitignore"),
            "/target\n/Cargo.lock\n/.mr-allowed\n/docs/.archive/\n",
        );
        write(&flow.repo.join(".mr-allowed"), &format!("email: {AUTHOR_EMAIL}\n"));
        let source_hooks =
            Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join(TRACKED_DIR);
        for hook in fs::read_dir(&source_hooks).unwrap().flatten() {
            let text = fs::read_to_string(hook.path()).unwrap();
            executable(&flow.repo.join(TRACKED_DIR).join(hook.file_name()), &text);
        }
        flow.probe_launches_in("0");
        git(&flow.repo, &["init", "-q", "-b", "main"]);
        git(&flow.repo, &["add", "-A"]);
        git(&flow.repo, &["commit", "-q", "--no-verify", "-m", "init"]);
        let installed = flow.run(env!("CARGO_BIN_EXE_xtask"), &["hooks", "install"]);
        assert!(installed.status.success(), "{}", said(&installed));
        git(dir_of(&flow.repo), &["init", "-q", "--bare", "origin.git"]);
        git(&flow.repo, &["remote", "add", "origin", "../origin.git"]);
        flow
    }

    fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    // Named `rustc` because clippy-driver drops its first argument only under
    // that name; kept off PATH so its own `exec rustc` reaches the real one.
    fn rustc_shim(&self) -> PathBuf {
        self.dir.path().join("rustc-shim").join("rustc")
    }

    fn probe_log(&self) -> PathBuf {
        self.dir.path().join("probes.log")
    }

    // Stands in for rustc only when Gate 0 links its launch probe, writing a
    // program whose first launch takes `seconds`; every other compile is real.
    fn probe_launches_in(&self, seconds: &str) {
        executable(
            &self.rustc_shim(),
            &format!(
                "#!/bin/sh\nif [ \"$1\" = -o ] && [ \"${{3##*/}}\" = probe.rs ]; then\n  echo \"$2\" >> \"{}\"\n  printf '#!/bin/sh\\nsleep {seconds}\\n' > \"$2\"\n  chmod +x \"$2\"\n  exit 0\nfi\nexec rustc \"$@\"\n",
                self.probe_log().display()
            ),
        );
    }

    fn probes_linked(&self) -> usize {
        fs::read_to_string(self.probe_log()).unwrap_or_default().lines().count()
    }

    fn command(&self, program: impl AsRef<Path>) -> Command {
        let mut path = OsString::from(self.bin());
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        let mut cmd = hermetic(program, &self.repo);
        cmd.env("PATH", path)
            .env("RUSTC", self.rustc_shim())
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"));
        cmd
    }

    fn run(&self, program: impl AsRef<Path>, args: &[&str]) -> Output {
        self.command(program).args(args).output().unwrap()
    }

    fn git(&self, args: &[&str]) -> Output {
        let out = self.run("git", args);
        assert!(out.status.success(), "git {args:?}: {}", said(&out));
        out
    }

    fn rev(&self, rev: &str) -> String {
        String::from_utf8_lossy(&self.git(&["rev-parse", rev]).stdout).trim().to_string()
    }

    fn signed_epic(&self, name: &str) {
        let branch = format!("epic/{name}");
        self.git(&["checkout", "-q", "-b", &branch, "main"]);
        write(&self.repo.join(format!("app/src/{name}.rs")), &part(2));
        self.git(&["commit", "-q", "-am", &format!("{name} doubles its part")]);
        self.git(&["checkout", "-q", "main"]);
        let report = format!("docs/.archive/{name}-result.html");
        write(&self.repo.join(&report), &format!("<html>{name} result</html>\n"));
        let signed = self.run(
            env!("CARGO_BIN_EXE_xtask"),
            &[
                "mr",
                "confirm",
                "epic",
                &branch,
                "--as",
                AUTHOR_EMAIL,
                "--target",
                "main",
                "--report",
                &report,
            ],
        );
        assert!(signed.status.success(), "{}", said(&signed));
    }

    fn merge(&self, branch: &str) -> Output {
        self.run("git", &["merge", "--no-ff", "-m", &format!("merge {branch}"), branch])
    }
}

fn dir_of(path: &Path) -> &Path {
    path.parent().unwrap()
}

#[test]
fn the_merge_into_main_proves_its_tree_and_the_push_reads_that_proof() {
    let flow = Flow::new();
    flow.signed_epic("a");
    flow.signed_epic("b");
    let root = flow.rev("main");

    flow.probe_launches_in("5");
    let slow = flow.merge("epic/a");
    let slow_said = said(&slow);
    assert!(!slow.status.success(), "a slow-launch machine refuses: {slow_said}");
    assert!(slow_said.contains(&slow_launch_refusal()), "{slow_said}");
    assert!(!slow_said.contains(NEXTEST_RAN), "refused before any suite: {slow_said}");
    assert!(!slow_said.contains(MERGE_REFUSED), "the machine is blamed, not the tree");
    assert_eq!(flow.rev("main"), root);
    flow.git(&["merge", "--abort"]);

    flow.probe_launches_in("0");
    let landed = flow.merge("epic/a");
    assert!(landed.status.success(), "{}", said(&landed));
    assert!(said(&landed).contains("gate0 green"), "{}", said(&landed));
    assert_eq!(flow.rev("main^2"), flow.rev("epic/a"));
    assert_eq!(flow.probes_linked(), 2, "one slow probe, then one fast probe");
    let merged_a = flow.rev("main");

    let combined = flow.merge("epic/b");
    let combined_said = said(&combined);
    assert!(!combined.status.success(), "each epic is green alone, red together");
    assert!(combined_said.contains("gate0 red"), "{combined_said}");
    assert!(combined_said.contains(MERGE_REFUSED), "{combined_said}");
    assert_eq!(flow.rev("main"), merged_a, "no red merge commit reaches main");
    assert_eq!(flow.probes_linked(), 2, "the fast verdict is cached for the machine");
    flow.git(&["merge", "--abort"]);

    let pushed_main = flow.run("git", &["push", "origin", "main"]);
    let pushed_main_said = said(&pushed_main);
    assert!(pushed_main.status.success(), "{pushed_main_said}");
    assert!(pushed_main_said.contains("gate0 skipped"), "{pushed_main_said}");
    assert!(!pushed_main_said.contains(NEXTEST_RAN), "{pushed_main_said}");

    let epic_b = flow.rev("epic/b");
    let pushed_b = flow.run("git", &["push", "origin", "epic/b"]);
    let pushed_b_said = said(&pushed_b);
    assert!(pushed_b.status.success(), "{pushed_b_said}");
    assert_eq!(pushed_b_said.matches("gate0 green").count(), 1, "{pushed_b_said}");
    assert!(
        pushed_b_said.contains(&format!("pushed {}: gate0 green", &epic_b[..10])),
        "{pushed_b_said}"
    );
    let remote = dir_of(&flow.repo).join("origin.git");
    let landed_b = git(&remote, &["rev-parse", "epic/b"]);
    assert_eq!(String::from_utf8_lossy(&landed_b.stdout).trim(), epic_b);
}
