use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::gate::{short, Report};

// The gate's own measurement, in the shape of Gate 0's cache: a local file under
// target/, appended on every gate run, read only by `cargo xtask tally`. It is
// never a git note and never syncs, because a measurement that ships with the
// repository is a claim about a machine rather than a reading of one (DEC-2's
// recompute-don't-trust, applied to a tally instead of a receipt). Nothing in
// the gate reads it back: delete the file and every verdict is what it was.
const FILE_NAME: &str = ".gate-tally";
const LEVELS: [&str; 3] = ["commit", "working-unit", "epic"];
const SEVERITY: [&str; 3] = ["exempt", "green", "red"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub date: String,
    pub level: String,
    pub status: String,
    pub tip: String,
    pub base: String,
    pub rounds: usize,
    pub epic: Option<String>,
    pub wu: Option<String>,
    pub checks: Vec<String>,
}

impl Entry {
    fn of(repo: &Path, report: &Report) -> Self {
        Self {
            date: crate::gate::today(),
            level: report.level.as_note_level().to_string(),
            status: status_of(report).to_string(),
            tip: report.tip.clone(),
            base: report.base.clone(),
            rounds: review_rounds(repo, &report.base, &report.tip),
            epic: report.epic.clone(),
            wu: report.wu.clone(),
            checks: distinct(report.failures.iter().map(|f| f.check.clone())),
        }
    }

    fn line(&self) -> String {
        let fields = [
            format!("date={}", self.date),
            format!("level={}", self.level),
            format!("status={}", self.status),
            format!("tip={}", self.tip),
            format!("base={}", self.base),
            format!("rounds={}", self.rounds),
            format!("epic={}", self.epic.as_deref().unwrap_or("-")),
            format!("wu={}", self.wu.as_deref().unwrap_or("-")),
            format!("checks={}", self.checks.join(",")),
        ];
        fields.join("\t")
    }

    fn parse(line: &str) -> Option<Self> {
        let mut entry = Self {
            date: String::new(),
            level: String::new(),
            status: String::new(),
            tip: String::new(),
            base: String::new(),
            rounds: 0,
            epic: None,
            wu: None,
            checks: Vec::new(),
        };
        for field in line.split('\t') {
            let (key, value) = field.split_once('=')?;
            match key {
                "date" => entry.date = value.to_string(),
                "level" => entry.level = value.to_string(),
                "status" => entry.status = value.to_string(),
                "tip" => entry.tip = value.to_string(),
                "base" => entry.base = value.to_string(),
                "rounds" => entry.rounds = value.parse().ok()?,
                "epic" => entry.epic = named(value),
                "wu" => entry.wu = named(value),
                "checks" => {
                    entry.checks =
                        value.split(',').filter(|c| !c.is_empty()).map(str::to_string).collect();
                }
                _ => {}
            }
        }
        (!entry.tip.is_empty() && LEVELS.contains(&entry.level.as_str())).then_some(entry)
    }
}

fn named(value: &str) -> Option<String> {
    (value != "-").then(|| value.to_string())
}

fn status_of(report: &Report) -> &'static str {
    if report.exempt {
        "exempt"
    } else if report.green() {
        "green"
    } else {
        "red"
    }
}

fn distinct(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items.filter(|item| seen.insert(item.clone())).collect()
}

pub fn path(repo: &Path) -> PathBuf {
    repo.join("target").join(FILE_NAME)
}

/// Review rounds the work behind a tip paid: one `cr` note is written per review
/// run — PASS or FAIL — on the tip that run judged, so the notes attached to
/// commits in the gate's range are the rounds the range paid for.
pub fn review_rounds(repo: &Path, base: &str, tip: &str) -> usize {
    let annotated: BTreeSet<String> = crate::git::git(repo, &["notes", "--ref=cr", "list"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1).map(str::to_string))
        .collect();
    let range = format!("{base}..{tip}");
    crate::git::git(repo, &["rev-list", &range])
        .unwrap_or_default()
        .lines()
        .filter(|commit| annotated.contains(*commit))
        .count()
}

pub fn record(repo: &Path, report: &Report) -> Result<(), String> {
    let file = path(repo);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(|e| format!("open {}: {e}", file.display()))?;
    writeln!(handle, "{}", Entry::of(repo, report).line())
        .map_err(|e| format!("append {}: {e}", file.display()))
}

pub fn entries(repo: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path(repo))
        .map(|raw| raw.lines().filter_map(Entry::parse).collect())
        .unwrap_or_default()
}

#[derive(Clone, Debug)]
struct Row {
    level: String,
    sha: String,
    epic: Option<String>,
    wu: Option<String>,
    status: String,
    rounds: usize,
    checks: Vec<String>,
}

impl Row {
    fn identity(&self) -> &str {
        self.wu.as_deref().or(self.epic.as_deref()).unwrap_or("-")
    }

    fn merge(&mut self, entry: &Entry) {
        if severity(&entry.status) > severity(&self.status) {
            self.status.clone_from(&entry.status);
        }
        self.rounds = self.rounds.max(entry.rounds);
        if self.epic.is_none() {
            self.epic.clone_from(&entry.epic);
        }
        if self.wu.is_none() {
            self.wu.clone_from(&entry.wu);
        }
        for check in &entry.checks {
            if !self.checks.contains(check) {
                self.checks.push(check.clone());
            }
        }
    }
}

fn severity(status: &str) -> usize {
    SEVERITY.iter().position(|s| *s == status).unwrap_or(SEVERITY.len())
}

// One row per (level, tip): the gate can be run many times on a tip, and what a
// reader wants is the union of what refused there, not the number of attempts.
fn collapse(entries: &[Entry]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    for entry in entries {
        let key = (entry.level.clone(), entry.tip.clone());
        if let Some(at) = index.get(&key) {
            rows[*at].merge(entry);
            continue;
        }
        index.insert(key, rows.len());
        rows.push(Row {
            level: entry.level.clone(),
            sha: entry.tip.clone(),
            epic: entry.epic.clone(),
            wu: entry.wu.clone(),
            status: entry.status.clone(),
            rounds: entry.rounds,
            checks: entry.checks.clone(),
        });
    }
    rows
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn refusal_rows<'a>(rows: &[&'a Row]) -> Vec<(&'a str, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows {
        for check in &row.checks {
            *counts.entry(check.as_str()).or_default() += 1;
        }
    }
    let mut rows: Vec<(&str, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    rows
}

pub fn summarise(entries: &[Entry], file: &Path) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "gate tally - {} (local measurement on this checkout, never gate evidence)",
        file.display()
    );
    if entries.is_empty() {
        let _ =
            write!(out, "no gate runs recorded yet - the next gate run writes the first record");
        return out;
    }
    let rows = collapse(entries);
    let refused = rows.iter().filter(|r| r.status == "red").count();
    let rounds: usize = rows.iter().map(|r| r.rounds).sum();
    let _ = writeln!(
        out,
        "{} - {refused} refused - {}",
        count(rows.len(), "tip"),
        count(rounds, "review round")
    );
    let _ = writeln!(out);
    for level in LEVELS {
        let at_level: Vec<&Row> = rows.iter().filter(|r| r.level == level).collect();
        if at_level.is_empty() {
            continue;
        }
        let refused_here = at_level.iter().filter(|r| r.status == "red").count();
        let _ = writeln!(out, "{level} - {refused_here} of {} tips refused", at_level.len());
        for (check, n) in refusal_rows(&at_level) {
            let _ = writeln!(out, "    {check:<36} {n}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "review rounds per tip");
    for level in LEVELS {
        for row in rows.iter().filter(|r| r.level == level) {
            let checks =
                if row.checks.is_empty() { "-".to_string() } else { row.checks.join(", ") };
            let _ = writeln!(
                out,
                "  {:<14} {:<11} {:<16} {:<12} {checks}",
                level,
                short(&row.sha),
                row.identity(),
                count(row.rounds, "round")
            );
        }
    }
    out.trim_end().to_string()
}

pub fn run(repo: &Path) -> Result<(i32, String), String> {
    Ok((0, summarise(&entries(repo), &path(repo))))
}

#[cfg(test)]
mod tests {
    use super::{collapse, entries, summarise, Entry, FILE_NAME};
    use std::path::Path;

    fn entry(level: &str, tip: &str, status: &str, rounds: usize, checks: &[&str]) -> Entry {
        Entry {
            date: "2026-09-12".to_string(),
            level: level.to_string(),
            status: status.to_string(),
            tip: tip.to_string(),
            base: "base".to_string(),
            rounds,
            epic: Some("E-X".to_string()),
            wu: Some("E-X.W1".to_string()),
            checks: checks.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn a_record_round_trips_through_its_line() {
        let original = entry("working-unit", "abc123", "red", 2, &["2·manifest", "6·doc-note"]);
        assert_eq!(Entry::parse(&original.line()), Some(original));
    }

    #[test]
    fn an_absent_identity_and_no_checks_round_trip() {
        let mut original = entry("epic", "abc123", "green", 0, &[]);
        original.epic = None;
        original.wu = None;
        assert_eq!(Entry::parse(&original.line()), Some(original));
    }

    #[test]
    fn a_malformed_line_parses_as_nothing() {
        assert_eq!(Entry::parse("this is not a record"), None);
        assert_eq!(Entry::parse("level=working-unit\trounds=2"), None);
        assert_eq!(Entry::parse("tip=abc\tlevel=not-a-level"), None);
        assert_eq!(Entry::parse("tip=abc\tlevel=epic\trounds=two"), None);
    }

    #[test]
    fn a_malformed_line_never_poisons_the_others() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = super::path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let good = entry("working-unit", "abc123", "red", 1, &["1·notes"]);
        std::fs::write(&file, format!("garbage\n{}\n\n", good.line())).unwrap();
        assert_eq!(entries(dir.path()), vec![good]);
    }

    #[test]
    fn repeated_runs_on_one_tip_collapse_to_its_union_and_worst_state() {
        let entries = vec![
            entry("working-unit", "abc123", "green", 1, &[]),
            entry("working-unit", "abc123", "red", 3, &["2·manifest"]),
            entry("working-unit", "abc123", "red", 2, &["6·doc-note", "2·manifest"]),
        ];
        let tips = collapse(&entries);
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0].status, "red");
        assert_eq!(tips[0].rounds, 3);
        assert_eq!(tips[0].checks, vec!["2·manifest", "6·doc-note"]);
    }

    #[test]
    fn an_exempt_run_is_not_a_refusal() {
        let tips = collapse(&[entry("epic", "abc123", "exempt", 0, &[])]);
        assert_eq!(tips[0].status, "exempt");
    }

    #[test]
    fn the_summary_names_each_refused_check_under_its_level() {
        let entries = vec![
            entry("commit", "c1", "red", 0, &["8·manual-review"]),
            entry("working-unit", "w1", "red", 2, &["2·manifest"]),
            entry("working-unit", "w2", "green", 1, &[]),
        ];
        let text = summarise(&entries, Path::new("/repo/target/.gate-tally"));
        assert!(text.contains("3 tips - 2 refused - 3 review rounds"), "{text}");
        assert!(text.contains("commit - 1 of 1 tips refused"), "{text}");
        assert!(text.contains("8·manual-review"), "{text}");
        assert!(text.contains("working-unit - 1 of 2 tips refused"), "{text}");
        assert!(text.contains("2·manifest"), "{text}");
        assert!(text.contains("E-X.W1"), "{text}");
    }

    #[test]
    fn an_empty_tally_says_so_and_names_its_file() {
        let text = summarise(&[], &Path::new("/repo").join("target").join(FILE_NAME));
        assert!(text.contains("no gate runs recorded yet"), "{text}");
        assert!(text.contains(&format!("target/{FILE_NAME}")), "{text}");
    }
}
