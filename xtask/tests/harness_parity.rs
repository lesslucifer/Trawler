#![cfg(unix)]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

//! The Codex doors: every path a Codex session reads links to the one tracked
//! artifact a Claude session reads, and the shared hook config resolves through
//! the active worktree rather than one hard-coded tree.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

const POST_COMMAND: &str = r#"root=$(git rev-parse --show-toplevel 2>/dev/null || echo "$CLAUDE_PROJECT_DIR"); export TRAWLER_HOOK_ROOT="$root"; bash "$root/.claude/hooks/verify-doc-post.sh""#;
const STOP_COMMAND: &str = r#"root=$(git rev-parse --show-toplevel 2>/dev/null || echo "$CLAUDE_PROJECT_DIR"); export TRAWLER_HOOK_ROOT="$root"; bash "$root/.claude/hooks/verify-doc-stop.sh""#;

const RED_READY_DOC: &str = "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
     <main data-artifact=\"wu-result\"><p data-item=\"outcome\">only this</p></main>\
     <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
     </body></html>";

fn repo_root() -> PathBuf {
    xtask::repo_root().unwrap_or_else(|e| panic!("{e}"))
}

fn link_target(path: &Path) -> PathBuf {
    let meta =
        fs::symlink_metadata(path).unwrap_or_else(|e| panic!("{} must exist: {e}", path.display()));
    assert!(
        meta.file_type().is_symlink(),
        "{} must be a symlink to the tracked source, never a copy",
        path.display()
    );
    fs::read_link(path).unwrap()
}

fn expected_config() -> String {
    format!(
        r#"{{
  "hooks": {{
    "PostToolUse": [
      {{
        "matcher": "Write|Edit",
        "hooks": [
          {{
            "type": "command",
            "command": "{post}"
          }}
        ]
      }}
    ],
    "Stop": [
      {{
        "hooks": [
          {{
            "type": "command",
            "command": "{stop}"
          }}
        ]
      }}
    ]
  }}
}}"#,
        post = POST_COMMAND.replace('"', "\\\""),
        stop = STOP_COMMAND.replace('"', "\\\"")
    )
}

#[test]
fn agents_md_is_a_symlink_to_the_t0_doc() {
    let root = repo_root();
    assert_eq!(link_target(&root.join("AGENTS.md")), Path::new("CLAUDE.md"));
    assert_eq!(
        fs::read(root.join("AGENTS.md")).unwrap(),
        fs::read(root.join("CLAUDE.md")).unwrap()
    );
}

#[test]
fn the_codex_skill_root_reaches_every_claude_skill() {
    let root = repo_root();
    assert_eq!(link_target(&root.join(".agents/skills")), Path::new("../.claude/skills"));

    let skills = root.join(".claude/skills");
    let mut found = 0;
    for entry in fs::read_dir(&skills).unwrap() {
        let name = entry.unwrap().file_name();
        let claude = skills.join(&name).join("SKILL.md");
        if !claude.is_file() {
            continue;
        }
        found += 1;

        let through_codex = root.join(".agents/skills").join(&name).join("SKILL.md");
        assert_eq!(
            fs::read(&through_codex).unwrap(),
            fs::read(&claude).unwrap(),
            "{} must resolve to the tracked skill",
            through_codex.display()
        );

        let text = fs::read_to_string(&claude).unwrap();
        let frontmatter: Vec<&str> =
            text.lines().skip(1).take_while(|line| line.trim() != "---").collect();
        assert!(!frontmatter.is_empty(), "{} must open with frontmatter", claude.display());
        for key in ["name:", "description:"] {
            assert!(
                frontmatter.iter().any(|line| line.starts_with(key)),
                "{} must declare {key} for Codex to load it",
                claude.display()
            );
        }
    }
    assert!(found > 0, "the repository has at least one skill");
}

#[test]
fn both_harnesses_load_one_hook_definition_file() {
    let root = repo_root();
    assert_eq!(link_target(&root.join(".codex/hooks.json")), Path::new("../.claude/settings.json"));
    assert_eq!(
        fs::canonicalize(root.join(".codex/hooks.json")).unwrap(),
        fs::canonicalize(root.join(".claude/settings.json")).unwrap()
    );

    let shared = fs::read_to_string(root.join(".claude/settings.json")).unwrap();
    assert_eq!(
        shared.trim_end(),
        expected_config().trim_end(),
        "the shared file is the contract: hooks only, no absolute path. Update this test in the \
         same commit as any legitimate change, and split the file if a Claude-only setting appears"
    );
}

#[test]
fn the_codex_hook_dir_is_the_shared_script_tree() {
    let root = repo_root();
    assert_eq!(link_target(&root.join(".codex/hooks")), Path::new("../.claude/hooks"));
    for script in ["verify-doc-post.sh", "verify-doc-stop.sh"] {
        assert_eq!(
            fs::read(root.join(".codex/hooks").join(script)).unwrap(),
            fs::read(root.join(".claude/hooks").join(script)).unwrap()
        );
    }
}

#[test]
fn the_shared_command_finds_the_script_from_a_subdirectory_and_a_linked_worktree() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main");
    fs::create_dir_all(main.join("sub")).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(&main, &["config", "user.email", "harness@test"]);
    git(&main, &["config", "user.name", "harness"]);
    git(&main, &["config", "commit.gpgsign", "false"]);
    symlink(repo_root().join(".claude"), main.join(".claude")).unwrap();

    fs::write(main.join("sub/red.html"), RED_READY_DOC).unwrap();
    let out = run(POST_COMMAND, &main.join("sub"), &codex_payload(Path::new("red.html")));
    assert_eq!(out.status.code(), Some(2), "from a subdirectory: {}", stderr(&out));

    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-qm", "seed"]);
    let linked = dir.path().join("linked");
    git(&main, &["worktree", "add", "-q", linked.to_str().unwrap()]);
    assert!(fs::symlink_metadata(linked.join(".claude")).unwrap().file_type().is_symlink());
    fs::write(linked.join("red.html"), RED_READY_DOC).unwrap();
    let out = run(POST_COMMAND, &linked, &codex_payload(Path::new("red.html")));
    assert_eq!(out.status.code(), Some(2), "from a linked worktree: {}", stderr(&out));
}

fn git(repo: &Path, args: &[&str]) {
    xtask::git::git(repo, args).unwrap_or_else(|e| panic!("{e}"));
}

fn run(command: &str, cwd: &Path, payload: &str) -> Output {
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(command)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("CLAUDE_PROJECT_DIR")
        .env("XTASK_BIN", env!("CARGO_BIN_EXE_xtask"))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(payload.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
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
