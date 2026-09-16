//! E-CODEX epic e2e — the session a Codex agent actually has, walked end to end.
//!
//! The doors are read where they live, because the symlinks *are* the artifact
//! (steps 1-2); the hooks are then run in a hermetic temp repo standing in for a
//! second worktree (steps 3-5), driven by the command strings deserialized out of
//! the committed `.codex/hooks.json` rather than restated here — so a change to
//! the shipped hook changes what this test runs.
//!
//! Every payload case runs **both** harnesses' shapes over the same file and
//! compares exit code and stderr, so a future change that reddens one harness
//! differently from the other fails even while both stay red.
#![cfg(all(unix, feature = "codex-e2e"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde::Deserialize;

const RED_READY_DOC: &str = "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
     <main data-artifact=\"wu-result\"><p data-item=\"outcome\">only this</p></main>\
     <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
     </body></html>";

const GREEN_READY_DOC: &str = "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
     <main data-artifact=\"wu-result\">\
     <p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
     <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-52</p>\
     <p data-item=\"follow-ups\">none</p></main>\
     <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
     </body></html>";

#[derive(Deserialize)]
struct SharedConfig {
    hooks: HookEvents,
}

#[derive(Deserialize)]
struct HookEvents {
    #[serde(rename = "PostToolUse")]
    post_tool_use: Vec<HookMatcher>,
    #[serde(rename = "Stop")]
    stop: Vec<HookMatcher>,
}

#[derive(Deserialize)]
struct HookMatcher {
    hooks: Vec<HookCommand>,
}

#[derive(Deserialize)]
struct HookCommand {
    command: String,
}

/// The definitions as Codex loads them: through its own door, not Claude's.
/// `serde_yaml` reads JSON, so the shipped file is parsed rather than re-typed.
fn committed_commands() -> (String, String) {
    let door = repo_root().join(".codex/hooks.json");
    let config: SharedConfig = serde_yaml::from_str(&fs::read_to_string(&door).unwrap())
        .unwrap_or_else(|e| panic!("{} must be loadable hook definitions: {e}", door.display()));
    let only = |event: &[HookMatcher], name: &str| {
        let commands: Vec<&HookCommand> = event.iter().flat_map(|m| &m.hooks).collect();
        assert_eq!(commands.len(), 1, "{name} must define exactly one command for both harnesses");
        commands[0].command.clone()
    };
    (only(&config.hooks.post_tool_use, "PostToolUse"), only(&config.hooks.stop, "Stop"))
}

fn repo_root() -> PathBuf {
    xtask::repo_root().unwrap_or_else(|e| panic!("{e}"))
}

fn git(repo: &Path, args: &[&str]) -> String {
    xtask::git::git(repo, args).unwrap_or_else(|e| panic!("{e}"))
}

fn tracked(repo: &Path, path: &str) -> Vec<u8> {
    xtask::git::show_bytes(repo, "", path).unwrap_or_else(|e| panic!("{e}"))
}

/// A repo that is not this one but shares its `.claude/` — what a second
/// worktree is to the hook, without a worktree's coupling to this checkout.
struct Session {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Session {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let session = Self { _dir: dir, path };
        git(&session.path, &["init", "-q", "-b", "main"]);
        git(&session.path, &["config", "user.email", "codex@test"]);
        git(&session.path, &["config", "user.name", "codex"]);
        git(&session.path, &["config", "commit.gpgsign", "false"]);
        symlink(repo_root().join(".claude"), session.path.join(".claude")).unwrap();
        session
    }

    fn write(&self, rel: &str, content: &str) -> PathBuf {
        let full = self.path.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, content).unwrap();
        full
    }

    fn run(&self, command: &str, payload: &str) -> Output {
        let mut child = Command::new("bash")
            .arg("-c")
            .arg(command)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("CLAUDE_PROJECT_DIR")
            .env("XTASK_BIN", env!("CARGO_BIN_EXE_xtask"))
            .current_dir(&self.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(payload.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    }

    /// The same edit announced the way each harness announces it. Returns the
    /// shared verdict, and fails if the two harnesses disagree on any of it.
    fn edit_verdict(&self, command: &str, target: &Path) -> (i32, String) {
        let claude = self.run(command, &claude_payload(target));
        let codex = self.run(command, &codex_payload(target));
        assert_eq!(
            (claude.status.code(), stderr(&claude)),
            (codex.status.code(), stderr(&codex)),
            "the two harnesses must reach one verdict for {}:\nclaude: {}\ncodex: {}",
            target.display(),
            stderr(&claude),
            stderr(&codex)
        );
        (claude.status.code().unwrap(), stderr(&claude))
    }
}

fn claude_payload(target: &Path) -> String {
    format!("{{\"tool_name\":\"Write\",\"tool_input\":{{\"file_path\":\"{}\"}}}}", target.display())
}

fn codex_payload(target: &Path) -> String {
    let patch = format!(
        "*** Begin Patch\n*** Update File: {}\n@@\n-old\n+new\n*** End Patch\n",
        target.display()
    );
    let escaped = patch.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    format!("{{\"tool_name\":\"apply_patch\",\"tool_input\":{{\"command\":\"{escaped}\"}}}}")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn a_codex_session_reads_the_rules_finds_the_reviewer_and_gets_claudes_verdict() {
    let root = repo_root();

    // 1. The T0 door: the rules a Codex session loads are the tracked blob.
    let staged = git(&root, &["ls-files", "-s", "AGENTS.md"]);
    let mode = staged.split_whitespace().next().unwrap_or_default();
    assert_eq!(mode, "120000", "AGENTS.md must be a symlink, never a second copy of the rules");
    // A symlink's own blob is the path it names, so the door is read twice: what
    // it points at, and what a session opening it gets versus the blob git holds.
    assert_eq!(
        tracked(&root, "AGENTS.md"),
        b"CLAUDE.md",
        "the door must name the canonical T0 file"
    );
    assert_eq!(
        fs::read(root.join("AGENTS.md")).unwrap(),
        tracked(&root, "CLAUDE.md"),
        "the rules a Codex session loads must be the tracked T0 blob, not a copy of it"
    );

    // 2. The skills door: every skill, and by name the reviewer a cr/v3 note claims.
    let skills = root.join(".claude/skills");
    let through_codex = root.join(".agents/skills");
    let mut resolved = Vec::new();
    for entry in fs::read_dir(&skills).unwrap() {
        let name = entry.unwrap().file_name();
        let canonical = skills.join(&name).join("SKILL.md");
        if !canonical.is_file() {
            continue;
        }
        let codex_path = through_codex.join(&name).join("SKILL.md");
        assert_eq!(
            fs::read(&codex_path).unwrap(),
            fs::read(&canonical).unwrap(),
            "{} must resolve to the tracked skill",
            codex_path.display()
        );
        let text = fs::read_to_string(&codex_path).unwrap();
        let frontmatter: Vec<&str> =
            text.lines().skip(1).take_while(|line| line.trim() != "---").collect();
        for key in ["name:", "description:"] {
            assert!(
                frontmatter.iter().any(|line| line.starts_with(key)),
                "{} must declare {key} for Codex to load it",
                codex_path.display()
            );
        }
        resolved.push(name.to_string_lossy().into_owned());
    }
    assert!(
        resolved.iter().any(|name| name == "code-review"),
        "the reviewer every cr/v3 note names must be a skill in a Codex session: {resolved:?}"
    );

    // 3-5. The hooks, as shipped, from a session that is not this checkout.
    let (post, stop) = committed_commands();
    let session = Session::new();

    let red = session.write("docs/work/result.html", RED_READY_DOC);
    let (code, problems) = session.edit_verdict(&post, &red);
    assert_eq!(code, 2, "a red doc must be fed back to both harnesses: {problems}");
    assert!(
        problems.contains("item·deviations") && problems.contains("fix ->"),
        "the feedback must name what is owed: {problems}"
    );

    let green = session.write("docs/work/green.html", GREEN_READY_DOC);
    assert_eq!(session.edit_verdict(&post, &green), (0, String::new()), "a green doc says nothing");

    let code_file = session.write("src/lib.rs", "pub fn base() {}\n");
    assert_eq!(session.edit_verdict(&post, &code_file), (0, String::new()), "code is not a doc");

    fs::remove_file(&green).unwrap();
    git(&session.path, &["add", "-A"]);
    let turn_end = session.run(&stop, "{}");
    assert_eq!(turn_end.status.code(), Some(2), "the backstop survives the shared config");
    assert!(
        stderr(&turn_end).contains("docs/work/result.html"),
        "the refusal must name the doc holding the turn: {}",
        stderr(&turn_end)
    );
}
