use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const DOCTEST_KINDS: [&str; 4] = ["lib", "rlib", "dylib", "proc-macro"];
const RUST_TAGS: [&str; 6] =
    ["rust", "should_panic", "no_run", "compile_fail", "test_harness", "standalone_crate"];

fn rustdoc_compiles(info: &str) -> bool {
    let mut rust = false;
    let mut other = false;
    for tag in info.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()) {
        let tag = tag.trim_matches(|c| c == '{' || c == '}');
        if tag == "ignore" || tag.starts_with("ignore-") {
            return false;
        }
        let error_code =
            tag.len() == 5 && tag.starts_with('E') && tag[1..].bytes().all(|b| b.is_ascii_digit());
        if RUST_TAGS.contains(&tag) || tag.starts_with("edition") || error_code {
            rust = true;
        } else if !tag.is_empty() {
            other = true;
        }
    }
    rust || !other
}

fn doc_text(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let rest = line
        .strip_prefix("///")
        .filter(|r| !r.starts_with('/'))
        .or_else(|| line.strip_prefix("//!"))?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

fn compiled_examples(source: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut open_fence: Option<&str> = None;
    for (index, line) in source.lines().enumerate() {
        let Some(text) = doc_text(line) else {
            open_fence = None;
            continue;
        };
        let text = text.trim_start();
        let fence = ["```", "~~~"].into_iter().find(|f| text.starts_with(f));
        match (open_fence, fence) {
            (Some(open), Some(f)) if open == f => open_fence = None,
            (None, Some(f)) => {
                open_fence = Some(f);
                if rustdoc_compiles(text.trim_start_matches(f.chars().next().unwrap_or('`'))) {
                    found.push(index + 1);
                }
            }
            _ => {}
        }
    }
    found
}

fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|e| e == "rs") {
            into.push(path);
        }
    }
}

fn workspace_libs() -> Vec<(String, bool, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--offline", "--format-version", "1"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let metadata: Value = serde_json::from_slice(&out.stdout).unwrap();
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["targets"].as_array().unwrap())
        .filter(|t| {
            t["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| DOCTEST_KINDS.contains(&k.as_str().unwrap()))
        })
        .map(|t| {
            let src = PathBuf::from(t["src_path"].as_str().unwrap());
            (
                t["name"].as_str().unwrap().to_string(),
                t["doctest"].as_bool().unwrap(),
                src.parent().unwrap().to_path_buf(),
            )
        })
        .collect()
}

#[test]
fn rustdoc_compiles_bare_and_rust_tagged_fences() {
    for info in [
        "",
        "rust",
        "no_run",
        "should_panic",
        "compile_fail",
        "edition2021",
        "rust,no_run",
        "E0499",
    ] {
        assert!(rustdoc_compiles(info), "{info:?}");
    }
}

#[test]
fn rustdoc_skips_ignored_and_foreign_fences() {
    for info in ["ignore", "rust,ignore", "ignore-windows", "text", "toml", "sh", "json"] {
        assert!(!rustdoc_compiles(info), "{info:?}");
    }
}

#[test]
fn only_doc_comment_fences_are_examples() {
    let source = "\
/// Adds.
///
/// ```
/// assert_eq!(add(1, 1), 2);
/// ```
//! ~~~no_run
//! run();
//! ~~~
// ```
// not a doc comment
// ```
//// ```
const FENCE: &str = \"```\";
/// ```text
/// $ trawler
/// ```
";
    assert_eq!(compiled_examples(source), [3, 6]);
}

#[test]
fn a_fence_never_spans_two_doc_blocks() {
    let source = "/// ```text\nfn a() {}\n/// ```\n/// body\n/// ```\n";
    assert_eq!(compiled_examples(source), [3]);
}

#[test]
fn every_workspace_lib_skips_the_doc_test_pass() {
    let libs = workspace_libs();
    assert!(!libs.is_empty());
    let running: Vec<_> =
        libs.iter().filter(|(_, doctest, _)| *doctest).map(|(name, ..)| name).collect();
    assert!(
        running.is_empty(),
        "Gate 0 runs nextest, which never runs doc-tests — set `doctest = false` on {running:?} and test through the suite"
    );
}

#[test]
fn no_doc_test_free_lib_holds_an_example_rustdoc_would_compile() {
    let mut hidden = Vec::new();
    for (_, _, src) in workspace_libs() {
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        for file in files {
            let source = fs::read_to_string(&file).unwrap();
            hidden.extend(
                compiled_examples(&source)
                    .into_iter()
                    .map(|line| format!("{}:{line}", file.display())),
            );
        }
    }
    assert!(
        hidden.is_empty(),
        "doc examples that would run as doc-tests, which no gate runs — mark the fence `text` or `ignore`, or make it a test: {hidden:#?}"
    );
}
