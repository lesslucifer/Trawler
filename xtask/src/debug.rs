use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use crate::git::{git, note};
use crate::notes::{CrNote, DocNote, DodReceipt, TestNote};

const DEFAULT_PORT: u16 = 48027;
const NOTE_REFS: [&str; 4] = ["test", "cr", "doc", "dod"];

pub fn serve(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let port = port_flag(args).unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot bind 127.0.0.1:{port}: {e}"))?;
    let repo = repo.to_path_buf();
    let url = format!("http://127.0.0.1:{port}/");
    // ponytail: single-threaded blocking loop; a debug view for one dev needs no concurrency.
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = handle(&mut stream, &repo, &url);
    }
    Ok((0, String::new()))
}

fn handle(stream: &mut TcpStream, repo: &Path, url: &str) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(&*stream).read_line(&mut line)?;
    // drain the rest of the request head so the client accepts our response
    let body = render(repo, url);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn port_flag(args: &[&str]) -> Option<u16> {
    args.iter()
        .position(|a| *a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
}

struct Commit {
    sha: String,
    subject: String,
    notes: BTreeMap<&'static str, String>,
}

pub fn render(repo: &Path, url: &str) -> String {
    let model = collect(repo);
    page(&model, url)
}

struct Model {
    epics: BTreeMap<String, BTreeMap<String, Vec<Commit>>>,
    invalid: Vec<Commit>,
}

fn collect(repo: &Path) -> Model {
    let commits = commits_with_notes(repo);
    let mut epics: BTreeMap<String, BTreeMap<String, Vec<Commit>>> = BTreeMap::new();
    let mut invalid = Vec::new();
    for commit in commits {
        match rollup_key(&commit) {
            Some((epic, wu)) => epics.entry(epic).or_default().entry(wu).or_default().push(commit),
            None => invalid.push(commit),
        }
    }
    Model { epics, invalid }
}

fn commits_with_notes(repo: &Path) -> Vec<Commit> {
    let mut shas: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for note_ref in NOTE_REFS {
        let ref_arg = format!("--ref={note_ref}");
        let listing = git(repo, &["notes", &ref_arg, "list"]).unwrap_or_default();
        for line in listing.lines() {
            if let Some(commit) = line.split_whitespace().nth(1) {
                if seen.insert(commit.to_string()) {
                    shas.push(commit.to_string());
                }
            }
        }
    }
    let mut commits: Vec<Commit> = shas
        .into_iter()
        .map(|sha| {
            let subject = git(repo, &["log", "-1", "--format=%s", &sha])
                .unwrap_or_else(|_| "(unknown commit)".to_string());
            let mut notes = BTreeMap::new();
            for note_ref in NOTE_REFS {
                if let Some(body) = note(repo, note_ref, &sha) {
                    notes.insert(note_ref, body);
                }
            }
            Commit { sha, subject, notes }
        })
        .collect();
    // newest first, by commit time
    commits.sort_by_key(|c| std::cmp::Reverse(commit_time(repo, &c.sha)));
    commits
}

fn commit_time(repo: &Path, sha: &str) -> i64 {
    git(repo, &["log", "-1", "--format=%ct", sha]).ok().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn rollup_key(commit: &Commit) -> Option<(String, String)> {
    for body in commit.notes.values() {
        if let Some(pair) = epic_wu(body) {
            return Some(pair);
        }
    }
    None
}

fn epic_wu(body: &str) -> Option<(String, String)> {
    let epic: Option<String> = serde_yaml::from_str::<TestNote>(body)
        .ok()
        .and_then(|n| n.epic)
        .or_else(|| serde_yaml::from_str::<CrNote>(body).ok().and_then(|n| n.epic))
        .or_else(|| serde_yaml::from_str::<DocNote>(body).ok().and_then(|n| n.epic))
        .or_else(|| serde_yaml::from_str::<DodReceipt>(body).ok().and_then(|n| n.epic));
    let wu: Option<String> = serde_yaml::from_str::<TestNote>(body)
        .ok()
        .and_then(|n| n.wu)
        .or_else(|| serde_yaml::from_str::<CrNote>(body).ok().and_then(|n| n.wu))
        .or_else(|| serde_yaml::from_str::<DocNote>(body).ok().and_then(|n| n.wu))
        .or_else(|| serde_yaml::from_str::<DodReceipt>(body).ok().and_then(|n| n.wu));
    match (epic, wu) {
        (Some(e), Some(w)) => Some((e, w)),
        _ => None,
    }
}

fn chip(note_ref: &str, body: Option<&str>, key: &str) -> String {
    let (summary, cls) = match (note_ref, body) {
        (_, None) => return format!(r#"<span class="chip none">{note_ref} —</span>"#),
        ("test", Some(b)) => test_chip(b),
        ("cr", Some(b)) => cr_chip(b),
        ("doc", Some(b)) => (doc_chip(b), ""),
        ("dod", Some(b)) => dod_chip(b),
        (_, Some(_)) => (note_ref.to_string(), ""),
    };
    format!(r#"<span class="chip {cls}" data-note="{key}">{summary}</span>"#)
}

fn layer_cell(layer: Option<&crate::notes::Layer>) -> String {
    use crate::notes::Layer;
    match layer {
        None => "–".to_string(),
        Some(Layer::Counted { count, .. }) => count.to_string(),
        Some(Layer::Waived { .. }) => "w".to_string(),
    }
}

fn test_chip(body: &str) -> (String, &'static str) {
    match serde_yaml::from_str::<TestNote>(body) {
        Ok(n) => {
            let u = layer_cell(n.tests.layer("unit"));
            let i = layer_cell(n.tests.layer("integration"));
            let e = layer_cell(n.tests.layer("e2e"));
            (format!("test({u}/{i}/{e})"), "")
        }
        Err(_) => ("test(?)".to_string(), "fail"),
    }
}

fn cr_chip(body: &str) -> (String, &'static str) {
    match serde_yaml::from_str::<CrNote>(body) {
        Ok(n) => {
            let flag = findings_count(body).map(|c| format!(" ⚑{c}")).unwrap_or_default();
            let cls = if n.verdict.eq_ignore_ascii_case("PASS") { "pass" } else { "fail" };
            (format!("cr{flag}"), cls)
        }
        Err(_) => ("cr(?)".to_string(), "fail"),
    }
}

fn findings_count(body: &str) -> Option<u64> {
    let val: serde_yaml::Value = serde_yaml::from_str(body).ok()?;
    val.get("findings")?.get("found")?.as_u64()
}

fn doc_chip(body: &str) -> String {
    match serde_yaml::from_str::<DocNote>(body) {
        Ok(n) => format!("doc({})", n.changes.len()),
        Err(_) => "doc(?)".to_string(),
    }
}

fn dod_chip(body: &str) -> (String, &'static str) {
    let n = gate_check_count(body);
    (format!("dod ✓{n}"), "pass")
}

fn gate_check_count(body: &str) -> usize {
    serde_yaml::from_str::<serde_yaml::Value>(body)
        .ok()
        .and_then(|v| v.get("checks").and_then(|c| c.as_sequence()).map(Vec::len))
        .unwrap_or(0)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn js_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${")
}

fn page(model: &Model, url: &str) -> String {
    use std::fmt::Write as _;
    let mut body = String::new();
    let mut note_map = String::new();

    for (epic, wus) in &model.epics {
        let _ = writeln!(body, "<h2>{}</h2>", esc(epic));
        for (wu, commits) in wus {
            let _ = write!(
                body,
                r#"<p class="wu"><strong>{} › {}</strong></p><div class="tl">"#,
                esc(epic),
                esc(wu)
            );
            render_nodes(commits, &mut body, &mut note_map);
            body.push_str("</div>\n");
        }
    }
    if !model.invalid.is_empty() {
        body.push_str(r#"<h2><span class="badge inv">invalid</span> — notes with no epic/wu</h2><div class="tl">"#);
        render_nodes(&model.invalid, &mut body, &mut note_map);
        body.push_str("</div>\n");
    }
    if model.epics.is_empty() && model.invalid.is_empty() {
        body.push_str("<p class=\"empty\">No workflow notes found in this repository.</p>");
    }

    template(&body, &note_map, url)
}

fn render_nodes(commits: &[Commit], body: &mut String, note_map: &mut String) {
    use std::fmt::Write as _;
    for commit in commits {
        let short = &commit.sha[..commit.sha.len().min(7)];
        let _ = write!(
            body,
            r#"<div class="node"><div class="row"><span class="sha">{}</span> {}</div><div class="row">"#,
            short,
            esc(&commit.subject)
        );
        for note_ref in NOTE_REFS {
            let stored = commit.notes.get(note_ref).map(String::as_str);
            let key = format!("{note_ref}-{short}");
            body.push_str(&chip(note_ref, stored, &key));
            if let Some(raw) = stored {
                let _ = writeln!(
                    note_map,
                    "  '{}': ['{} · {}', `{}`],",
                    js_str(&key),
                    note_ref,
                    short,
                    js_str(raw)
                );
            }
        }
        body.push_str("</div></div>\n");
    }
}

fn template(body: &str, note_map: &str, url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Trawler — workflow notes</title>
<style>
:root {{ --bg:#12151b; --paper:#181b22; --ink:#d7dbe4; --muted:#7c8496; --line:#2b3140; --accent:#6ea8fe; --ok:#5fd08a; --warn:#e88; }}
* {{ box-sizing: border-box; }}
body {{ background: var(--bg); color: var(--ink); font-family: -apple-system, system-ui, sans-serif; margin: 0; padding: 2rem; line-height: 1.5; }}
main {{ max-width: 900px; margin: 0 auto; }}
h1 {{ font-size: 1.4rem; }} h2 {{ font-size: 1.05rem; border-bottom: 1px solid var(--line); padding-bottom: .3rem; margin-top: 2rem; }}
.meta {{ color: var(--muted); font-size: .8rem; }}
.wu {{ margin: .6rem 0 .3rem; }}
.tl {{ border-left: 2px solid var(--line); margin-left: 1rem; padding-left: 1.25rem; }}
.tl .node {{ position: relative; margin: .75rem 0; }}
.tl .node::before {{ content: "●"; position: absolute; left: -1.72rem; color: var(--accent); }}
.tl .node .row {{ font-family: ui-monospace, monospace; font-size: .82rem; }}
.sha {{ color: var(--muted); font-family: ui-monospace, monospace; }}
.chip {{ display: inline-block; font-family: ui-monospace, monospace; font-size: .76rem; padding: .1rem .5rem; margin: .25rem .25rem 0 0; border: 1px solid var(--line); border-radius: 999px; cursor: pointer; background: var(--bg); color: var(--ink); }}
.chip:hover {{ border-color: var(--accent); }}
.chip.pass {{ border-color: var(--ok); color: var(--ok); }}
.chip.fail {{ border-color: var(--warn); color: var(--warn); }}
.chip.none {{ color: var(--muted); border-style: dashed; cursor: default; }}
.badge.inv {{ background: var(--warn); color: #000; border-radius: 4px; padding: 0 .4rem; font-size: .8rem; }}
.empty {{ color: var(--muted); }}
dialog {{ background: var(--paper); color: var(--ink); border: 1px solid var(--line); border-radius: 8px; max-width: 640px; width: 90%; padding: 0; }}
dialog::backdrop {{ background: rgba(0,0,0,.6); }}
dialog header {{ display: flex; justify-content: space-between; align-items: center; padding: .6rem 1rem; border-bottom: 1px solid var(--line); font-family: ui-monospace, monospace; }}
dialog pre {{ margin: 0; padding: 1rem; white-space: pre-wrap; font-size: .8rem; max-height: 60vh; overflow: auto; }}
dialog button.x {{ background: none; border: none; color: var(--muted); font-size: 1.2rem; cursor: pointer; }}
</style>
</head>
<body>
<main>
<h1>Workflow notes</h1>
<p class="meta">Live from git notes (test · cr · doc · dod) · {url} · reload to refresh</p>
{body}
</main>
<dialog id="note-modal">
<header><span id="note-modal-title">note</span><button class="x" onclick="document.getElementById('note-modal').close()">×</button></header>
<pre id="note-modal-body"></pre>
</dialog>
<script type="module">
const NOTES = {{
{note_map}}};
const dlg = document.getElementById('note-modal');
document.querySelectorAll('.chip[data-note]').forEach((c) => c.addEventListener('click', () => {{
  const [title, body] = NOTES[c.dataset.note] || ['note', '(no data)'];
  document.getElementById('note-modal-title').textContent = title;
  document.getElementById('note-modal-body').textContent = body;
  dlg.showModal();
}}));
dlg.addEventListener('click', (e) => {{ if (e.target === dlg) dlg.close(); }});
</script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str, subject: &str, notes: &[(&'static str, &str)]) -> Commit {
        Commit {
            sha: sha.to_string(),
            subject: subject.to_string(),
            notes: notes.iter().map(|(r, b)| (*r, (*b).to_string())).collect(),
        }
    }

    #[test]
    fn rollup_groups_by_epic_and_wu() {
        let c = commit(
            "abc1234",
            "x",
            &[("test", "schema: test/v2\nepic: E2\nwu: E2.W1\ntests: {}\nlevel: commit")],
        );
        assert_eq!(rollup_key(&c), Some(("E2".to_string(), "E2.W1".to_string())));
    }

    #[test]
    fn note_without_epic_wu_is_invalid() {
        let c = commit("abc1234", "x", &[("test", "schema: test/v2\nlevel: commit\ntests: {}")]);
        assert_eq!(rollup_key(&c), None);
    }

    #[test]
    fn epic_wu_read_from_any_ref() {
        let c = commit("abc1234", "x", &[("dod", "schema: dod/v1\nlevel: working-unit\nepic: E9\nwu: E9.W2\nbase: a\ntip: b\ndiff_sha256: c")]);
        assert_eq!(rollup_key(&c), Some(("E9".to_string(), "E9.W2".to_string())));
    }

    #[test]
    fn test_chip_shows_layer_cells() {
        let (s, cls) = test_chip("schema: test/v2\nlevel: commit\ntests:\n  unit: { count: 3 }\n  integration: { waived: \"none\" }");
        assert_eq!(s, "test(3/w/–)");
        assert_eq!(cls, "");
    }

    #[test]
    fn cr_chip_color_by_verdict_and_findings_flag() {
        let (s, cls) = cr_chip(
            "schema: cr/v2\nbase: a\ndiff_sha256: c\nverdict: PASS\nfindings: { found: 3 }",
        );
        assert_eq!(s, "cr ⚑3");
        assert_eq!(cls, "pass");
        let (_, cls_fail) = cr_chip("schema: cr/v2\nbase: a\ndiff_sha256: c\nverdict: FAIL");
        assert_eq!(cls_fail, "fail");
    }

    #[test]
    fn doc_chip_counts_changes() {
        let s = doc_chip("schema: doc/v1\nchanges:\n  - { path: a, tier: T1, action: updated }\n  - { path: b, tier: T2, action: updated }");
        assert_eq!(s, "doc(2)");
    }

    #[test]
    fn dod_chip_counts_gate_checks() {
        let (s, cls) = dod_chip("schema: dod/v1\nlevel: wu\nbase: a\ntip: b\ndiff_sha256: c\nchecks: [notes, manifest, review]");
        assert_eq!(s, "dod ✓3");
        assert_eq!(cls, "pass");
    }

    #[test]
    fn absent_note_renders_dashed_non_clickable() {
        let html = chip("cr", None, "cr-abc1234");
        assert!(html.contains("chip none"));
        assert!(!html.contains("data-note"));
    }

    #[test]
    fn page_embeds_note_body_and_escapes_html() {
        let model = collect_from(vec![commit(
            "abc1234",
            "subject <script>",
            &[("test", "schema: test/v2\nepic: E1\nwu: E1.W1\ntests: {}\nlevel: commit")],
        )]);
        let html = page(&model, "http://x/");
        assert!(html.contains("subject &lt;script&gt;"));
        assert!(html.contains("'test-abc1234'"));
        assert!(html.contains("E1.W1"));
    }

    fn collect_from(commits: Vec<Commit>) -> Model {
        let mut epics: BTreeMap<String, BTreeMap<String, Vec<Commit>>> = BTreeMap::new();
        let mut invalid = Vec::new();
        for c in commits {
            match rollup_key(&c) {
                Some((e, w)) => epics.entry(e).or_default().entry(w).or_default().push(c),
                None => invalid.push(c),
            }
        }
        Model { epics, invalid }
    }
}
