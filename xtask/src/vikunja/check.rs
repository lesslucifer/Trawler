use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::{id_of, state, tasks_matching, Client, ROUTER_PROJECT};
use crate::git::git;

#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub task_id: u64,
    pub title: String,
    pub branch: String,
    pub open: bool,
}

pub fn worktrees(porcelain: &str) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    for block in porcelain.split("\n\n") {
        let field = |name: &str| block.lines().find_map(|l| l.strip_prefix(name));
        if field("prunable").is_some() {
            continue;
        }
        if let (Some(path), Some(branch)) = (field("worktree "), field("branch refs/heads/")) {
            found.insert(branch.to_string(), PathBuf::from(path));
        }
    }
    found
}

const INLINE_TAGS: [&str; 10] = ["a", "b", "code", "em", "i", "mark", "s", "span", "strong", "u"];

/// The web editor stores descriptions as HTML: block tags end a line, inline
/// tags (`<strong>Branch:</strong>`, `<code>wu/x/y</code>`) vanish. A `<` opens
/// a tag only when a letter or `/` follows and its `>` comes before any newline
/// or other `<`: plain text keeps `x < 3`, and the editor escapes a typed `<`.
fn plain_text(description: &str) -> String {
    let mut text = String::new();
    let mut rest = description;
    while let Some((before, after)) = rest.split_once('<') {
        text.push_str(before);
        let tag = after.split_once('>').filter(|(tag, _)| {
            tag.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/')
                && !tag.contains(['\n', '<'])
        });
        let Some((tag, after_tag)) = tag else {
            text.push('<');
            rest = after;
            continue;
        };
        let name = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or_default();
        if !INLINE_TAGS.contains(&name.to_ascii_lowercase().as_str()) {
            text.push('\n');
        }
        rest = after_tag;
    }
    text.push_str(rest);
    text.replace("&nbsp;", " ")
}

pub fn branch_of(description: &str) -> Option<String> {
    plain_text(description).lines().find_map(|line| {
        line.trim_start().strip_prefix("Branch:")?.split_whitespace().next().map(str::to_string)
    })
}

fn reference(task: &Value) -> Option<Reference> {
    let branch = branch_of(task["description"].as_str()?)?;
    Some(Reference {
        task_id: id_of(task),
        title: task["title"].as_str().unwrap_or_default().to_string(),
        branch,
        open: state(task) == "open",
    })
}

/// A work worktree no task names is an orphan; done tasks count, because a
/// merged WU's worktree outlives its task until it is removed. An open task
/// whose branch has no worktree is a missing worktree.
pub fn findings(trees: &BTreeMap<String, PathBuf>, references: &[Reference]) -> Vec<Value> {
    let orphans = trees
        .iter()
        .filter(|(branch, _)| {
            crate::mr::infer_level(branch).is_some()
                && !references.iter().any(|r| &r.branch == *branch)
        })
        .map(|(branch, tree)| json!({ "event": "orphan", "branch": branch, "worktree": tree }));
    let missing = references.iter().filter(|r| r.open && !trees.contains_key(&r.branch)).map(|r| {
        json!({ "event": "missing-worktree", "task_id": r.task_id, "title": r.title, "branch": r.branch })
    });
    orphans.chain(missing).collect()
}

pub fn run(client: &Client, repo: &Path) -> Result<(i32, String), String> {
    let trees = worktrees(&git(repo, &["worktree", "list", "--porcelain"])?);
    let tasks = tasks_matching(client, &format!("project = {ROUTER_PROJECT}"))?;
    let references: Vec<Reference> = tasks.iter().filter_map(reference).collect();
    let found = findings(&trees, &references);
    let lines: Vec<String> = found.iter().map(Value::to_string).collect();
    Ok((i32::from(!found.is_empty()), lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trees(pairs: &[(&str, &str)]) -> BTreeMap<String, PathBuf> {
        pairs.iter().map(|(b, p)| ((*b).to_string(), PathBuf::from(p))).collect()
    }

    fn named(task_id: u64, branch: &str, open: bool) -> Reference {
        Reference { task_id, title: format!("t{task_id}"), branch: branch.into(), open }
    }

    #[test]
    fn worktree_list_maps_branches_to_paths_and_skips_detached_ones() {
        let porcelain = "worktree /r/main\nHEAD abc\nbranch refs/heads/main\n\nworktree /r/w2\nHEAD def\nbranch refs/heads/wu/x/y\n\nworktree /r/old\nHEAD 123\ndetached\n";
        assert_eq!(worktrees(porcelain), trees(&[("main", "/r/main"), ("wu/x/y", "/r/w2")]));
    }

    #[test]
    fn a_worktree_whose_directory_was_deleted_outside_git_is_no_worktree() {
        let porcelain = "worktree /r/x\nHEAD abc\nbranch refs/heads/epic/x\n\nworktree /r/gone\nHEAD def\nbranch refs/heads/wu/x/gone\nprunable gitdir file points to non-existent location\n";
        assert_eq!(worktrees(porcelain), trees(&[("epic/x", "/r/x")]));
    }

    #[test]
    fn the_branch_line_is_read_from_plain_text_or_the_web_editor_s_html() {
        let branch = branch_of;
        assert_eq!(branch("Branch: wu/x/y\nintent").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("intent\n  Branch:epic/x  trailing").as_deref(), Some("epic/x"));
        assert_eq!(branch("<p>Branch: epic/x</p><p>plan</p>").as_deref(), Some("epic/x"));
        assert_eq!(branch("<p>intent</p><p>Branch: epic/x</p>").as_deref(), Some("epic/x"));
        assert_eq!(branch("intent<br/>Branch: epic/x").as_deref(), Some("epic/x"));
        assert_eq!(branch("the Branch: wu/x/y is cut"), None);
        assert_eq!(branch("Branch:"), None);
        assert_eq!(branch(""), None);
    }

    #[test]
    fn inline_markup_around_the_branch_line_or_an_empty_one_before_it_still_names_it() {
        let branch = branch_of;
        assert_eq!(branch("<p><strong>Branch:</strong> wu/x/y</p>").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("<p>Branch: <code>wu/x/y</code></p>").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("<p>Branch:&nbsp;wu/x/y</p>").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("Branch:\nBranch: wu/x/y").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("a < b\nBranch: wu/x/y").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("x < 3\nBranch: wu/x/y\nA -> B").as_deref(), Some("wu/x/y"));
        assert_eq!(branch("x<3 and\nBranch: wu/x/y\nA -> B").as_deref(), Some("wu/x/y"));
    }

    #[test]
    fn a_task_without_a_description_or_branch_line_names_nothing() {
        assert_eq!(reference(&json!({"id": 4, "title": "t", "description": null})), None);
        assert_eq!(
            reference(
                &json!({"id": 4, "title": "t", "done": true, "description": "Branch: wu/x/y"})
            ),
            Some(Reference { task_id: 4, title: "t".into(), branch: "wu/x/y".into(), open: false })
        );
    }

    #[test]
    fn a_work_worktree_no_task_names_is_an_orphan_and_other_branches_are_ignored() {
        let found = findings(
            &trees(&[
                ("main", "/r/main"),
                ("scratch", "/r/s"),
                ("epic/x", "/r/x"),
                ("wu/x/a", "/r/xa"),
                ("wu/x/b", "/r/xb"),
            ]),
            &[named(1, "epic/x", true), named(2, "wu/x/a", false)],
        );
        assert_eq!(found, [json!({"event": "orphan", "branch": "wu/x/b", "worktree": "/r/xb"})]);
    }

    #[test]
    fn an_open_task_whose_worktree_is_gone_is_flagged_but_a_done_one_is_not() {
        let found = findings(
            &trees(&[("epic/x", "/r/x")]),
            &[
                named(1, "epic/x", true),
                named(2, "wu/x/gone", true),
                named(3, "wu/x/merged", false),
            ],
        );
        assert_eq!(
            found,
            [
                json!({"event": "missing-worktree", "task_id": 2, "title": "t2", "branch": "wu/x/gone"})
            ]
        );
    }

    #[test]
    fn orphans_come_before_missing_worktrees_and_nothing_found_is_empty() {
        let found = findings(&trees(&[("epic/y", "/r/y")]), &[named(9, "epic/z", true)]);
        let events: Vec<&str> = found.iter().filter_map(|f| f["event"].as_str()).collect();
        assert_eq!(events, ["orphan", "missing-worktree"]);
        assert_eq!(
            findings(&trees(&[("epic/y", "/r/y")]), &[named(1, "epic/y", true)]),
            Vec::<Value>::new()
        );
        assert_eq!(findings(&BTreeMap::new(), &[]), Vec::<Value>::new());
    }
}
