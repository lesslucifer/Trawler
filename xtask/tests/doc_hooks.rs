#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn hooks_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.claude/hooks")
}

fn run_hook(script: &str, project_dir: &Path, stdin_json: &str) -> Output {
    let mut child = Command::new("bash")
        .arg(hooks_dir().join(script))
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("XTASK_BIN", env!("CARGO_BIN_EXE_xtask"))
        .env("CLAUDE_PROJECT_DIR", project_dir)
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(stdin_json.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn write_json(path: &Path) -> String {
    format!("{{\"tool_name\":\"Write\",\"tool_input\":{{\"file_path\":\"{}\"}}}}", path.display())
}

fn red_ready_doc() -> &'static str {
    "<html><body><main data-artifact=\"wu-result\"><p data-item=\"outcome\">only this</p></main></body></html>"
}

fn green_ready_doc() -> &'static str {
    "<html><body><main data-artifact=\"wu-result\">\
     <p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
     <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-9</p>\
     <p data-item=\"follow-ups\">none</p></main></body></html>"
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

mod post_tool_use {
    use super::*;

    fn run_on_doc(content: &str, name: &str) -> Output {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        run_hook("verify-doc-post.sh", dir.path(), &write_json(&path))
    }

    #[test]
    fn red_ready_doc_feeds_problems_back_with_exit_two() {
        let out = run_on_doc(red_ready_doc(), "result.html");
        assert_eq!(out.status.code(), Some(2));
        let err = stderr(&out);
        assert!(err.contains("item·deviations") && err.contains("fix ->"), "{err}");
    }

    #[test]
    fn green_ready_doc_is_silent() {
        let out = run_on_doc(green_ready_doc(), "result.html");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
        assert!(stderr(&out).is_empty());
    }

    #[test]
    fn draft_doc_is_silent() {
        let draft = red_ready_doc().replace(
            "data-artifact=\"wu-result\"",
            "data-artifact=\"wu-result\" data-status=\"draft\"",
        );
        let out = run_on_doc(&draft, "result.html");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    }

    #[test]
    fn ungoverned_doc_and_non_html_paths_are_silent() {
        let out = run_on_doc("<html><body><p>pre-standard</p></body></html>", "brainstorm.html");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("main.rs");
        std::fs::write(&path, "fn main() {}").unwrap();
        let out = run_hook("verify-doc-post.sh", dir.path(), &write_json(&path));
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    }
}

mod stop {
    use super::*;

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
            repo.git(&["config", "user.email", "hook@test"]);
            repo.git(&["config", "user.name", "hook"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo
        }

        fn git(&self, args: &[&str]) {
            let out = Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(args)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        }

        fn write(&self, rel: &str, content: &str) {
            let full = self.path.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, content).unwrap();
        }
    }

    #[test]
    fn red_ready_tracked_doc_blocks_the_turn_with_the_problems() {
        let repo = Repo::new();
        repo.write("docs/result.html", red_ready_doc());
        repo.git(&["add", "-A"]);
        let out = run_hook("verify-doc-stop.sh", &repo.path, "{}");
        assert_eq!(out.status.code(), Some(2));
        let err = stderr(&out);
        assert!(err.contains("docs/result.html") && err.contains("item·deviations"), "{err}");
    }

    #[test]
    fn red_untracked_doc_is_caught_too() {
        let repo = Repo::new();
        repo.write("docs/work/2026-07-05-new-result.html", red_ready_doc());
        let out = run_hook("verify-doc-stop.sh", &repo.path, "{}");
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    }

    #[test]
    fn drafts_and_green_docs_allow_the_turn_to_end() {
        let repo = Repo::new();
        repo.write("docs/green.html", green_ready_doc());
        let draft = red_ready_doc().replace(
            "data-artifact=\"wu-result\"",
            "data-artifact=\"wu-result\" data-status=\"draft\"",
        );
        repo.write("docs/draft.html", &draft);
        repo.write("docs/plain.html", "<html><body><p>ungoverned</p></body></html>");
        repo.git(&["add", "-A"]);
        let out = run_hook("verify-doc-stop.sh", &repo.path, "{}");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    }

    #[test]
    fn no_docs_at_all_allows_the_turn_to_end() {
        let repo = Repo::new();
        let out = run_hook("verify-doc-stop.sh", &repo.path, "{}");
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    }
}
