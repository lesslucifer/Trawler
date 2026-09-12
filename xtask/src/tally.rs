use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::gate::{short, Report};

// The gate's own measurement, in the shape of Gate 0's cache: a local file under
// target/, appended on every gate run that computes a report, read only by
// `cargo xtask tally`. It is never a git note and never syncs, because a
// measurement that ships with the repository is a claim about a machine rather
// than a reading of one (DEC-2's recompute-don't-trust, applied to a tally
// instead of a receipt). Nothing in the gate reads it back: delete the file and
// every verdict is what it was.
const FILE_NAME: &str = ".gate-tally";
const LEVELS: [&str; 3] = ["commit", "working-unit", "epic"];
const STATUSES: [&str; 3] = ["exempt", "green", "red"];
// A reading, not a history: past this the oldest half goes, so the file stays
// bounded on a checkout that gates for years (P1).
const KEEP: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub date: String,
    pub level: String,
    pub status: String,
    pub tip: String,
    pub base: String,
    pub branch: Option<String>,
    /// Rounds the tip's own line paid, or `None` when the range could not be
    /// read - an unreadable reading, never a zero.
    pub rounds: Option<usize>,
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
            branch: branch_of(report),
            rounds: review_rounds(repo, &report.base, &report.tip),
            epic: report.epic.clone(),
            wu: report.wu.clone(),
            checks: distinct(report.failures.iter().map(|f| f.check.clone())),
        }
    }

    fn identity(&self) -> &str {
        self.wu.as_deref().or(self.epic.as_deref()).or(self.branch.as_deref()).unwrap_or("-")
    }

    fn line(&self) -> String {
        let rounds = self.rounds.map_or_else(String::new, |r| r.to_string());
        let fields = [
            format!("date={}", self.date),
            format!("level={}", self.level),
            format!("status={}", self.status),
            format!("tip={}", self.tip),
            format!("base={}", self.base),
            format!("branch={}", self.branch.as_deref().unwrap_or("-")),
            format!("rounds={rounds}"),
            format!("epic={}", self.epic.as_deref().unwrap_or("-")),
            format!("wu={}", self.wu.as_deref().unwrap_or("-")),
            format!("checks={}", self.checks.join(",")),
        ];
        fields.join("\t")
    }

    // Strict on the fields a record cannot work without, so a half-written line
    // from an interrupted append is dropped rather than counted: the last field
    // (`checks`) has to be there, and the level and status have to be members.
    fn parse(line: &str) -> Option<Self> {
        let fields: BTreeMap<&str, &str> =
            line.split('\t').filter_map(|field| field.split_once('=')).collect();
        let field = |key: &str| fields.get(key).copied();
        let rounds = match field("rounds") {
            Some("") | None => None,
            Some(text) => Some(text.parse().ok()?),
        };
        let entry = Self {
            date: field("date").unwrap_or_default().to_string(),
            level: field("level")?.to_string(),
            status: field("status")?.to_string(),
            tip: field("tip")?.to_string(),
            base: field("base").unwrap_or_default().to_string(),
            branch: named(field("branch").unwrap_or("-")),
            rounds,
            epic: named(field("epic").unwrap_or("-")),
            wu: named(field("wu").unwrap_or("-")),
            checks: field("checks")?
                .split(',')
                .filter(|check| !check.is_empty())
                .map(str::to_string)
                .collect(),
        };
        let known = LEVELS.contains(&entry.level.as_str())
            && STATUSES.contains(&entry.status.as_str())
            && !entry.tip.is_empty();
        known.then_some(entry)
    }
}

fn named(value: &str) -> Option<String> {
    (value != "-").then(|| value.to_string())
}

fn branch_of(report: &Report) -> Option<String> {
    (report.tip_ref != report.tip).then(|| report.tip_ref.clone())
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

/// Rounds the tip's own line paid. The code review writes one `cr` note per run
/// (PASS or FAIL) on the tip that run judged, so the notes on the first-parent
/// chain from `base` to `tip` are the rounds this line paid; a range that merged
/// other branches' work in contributes none of theirs.
fn review_rounds(repo: &Path, base: &str, tip: &str) -> Option<usize> {
    let range = format!("{base}..{tip}");
    let commits = crate::git::git(repo, &["rev-list", "--first-parent", &range]).ok()?;
    let annotated: BTreeSet<String> = crate::git::git(repo, &["notes", "--ref=cr", "list"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1).map(str::to_string))
        .collect();
    Some(commits.lines().filter(|commit| annotated.contains(*commit)).count())
}

pub fn write(repo: &Path, report: &Report) -> Result<(), String> {
    let file = path(repo);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(|e| format!("open {}: {e}", file.display()))?;
    let mut line = Entry::of(repo, report).line();
    line.push('\n');
    // One write, so two runs appending at once cannot interleave mid-record.
    handle.write_all(line.as_bytes()).map_err(|e| format!("append {}: {e}", file.display()))?;
    drop(handle);
    bound(&file)
}

// Half the cap goes, oldest first, when the file outgrows it. A pure function
// because the truncation is the part worth testing, not the file handling.
fn truncated(text: &str, keep: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= keep {
        return None;
    }
    Some(format!("{}\n", lines[lines.len() - keep / 2..].join("\n")))
}

fn bound(file: &Path) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Ok(()) };
    let Some(kept) = truncated(&text, KEEP) else { return Ok(()) };
    std::fs::write(file, kept).map_err(|e| format!("bound {}: {e}", file.display()))
}

/// The records on disk. A missing file is an empty reading; a file that is not
/// UTF-8 is read lossily so one stray byte cannot blank the whole tally.
pub fn entries(repo: &Path) -> Vec<Entry> {
    match std::fs::read(path(repo)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).lines().filter_map(Entry::parse).collect(),
        Err(_) => Vec::new(),
    }
}

#[derive(Clone, Debug)]
struct Row {
    level: String,
    sha: String,
    identity: String,
    status: String,
    rounds: Option<usize>,
    checks: Vec<String>,
}

impl Row {
    fn of(entry: &Entry) -> Self {
        Self {
            level: entry.level.clone(),
            sha: entry.tip.clone(),
            identity: entry.identity().to_string(),
            status: entry.status.clone(),
            rounds: entry.rounds,
            checks: entry.checks.clone(),
        }
    }

    fn merge(&mut self, entry: &Entry) {
        if severity(&entry.status) > severity(&self.status) {
            self.status.clone_from(&entry.status);
        }
        self.rounds = match (self.rounds, entry.rounds) {
            (Some(seen), Some(now)) => Some(seen.max(now)),
            (seen, None) => seen,
            (None, now) => now,
        };
        if self.identity == "-" && entry.identity() != "-" {
            self.identity = entry.identity().to_string();
        }
        for check in &entry.checks {
            if !self.checks.contains(check) {
                self.checks.push(check.clone());
            }
        }
    }
}

fn severity(status: &str) -> usize {
    STATUSES.iter().position(|s| *s == status).unwrap_or(0)
}

// One row per (level, tip): the gate can be run many times on a tip, and what a
// reader wants is the union of what refused there and the largest round count it
// showed, not the number of attempts.
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
        rows.push(Row::of(entry));
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

// Rounds the reader can add up: a row whose range could not be read has none to
// add, and the summary says so instead of contributing a zero.
fn total_rounds(rows: &[Row]) -> String {
    let known: Vec<usize> = rows.iter().filter_map(|row| row.rounds).collect();
    let unknown = rows.len() - known.len();
    let total = count(known.iter().sum(), "review round");
    if unknown == 0 {
        total
    } else {
        format!("{total} ({unknown} tip(s) with an unreadable range)")
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
    let refused = rows.iter().filter(|row| row.status == "red").count();
    let _ =
        writeln!(out, "{} - {refused} refused - {}", count(rows.len(), "tip"), total_rounds(&rows));
    let _ = writeln!(out);
    let _ = writeln!(out, "refusals by level (tips where the check id appeared)");
    for level in LEVELS {
        let at_level: Vec<&Row> = rows.iter().filter(|row| row.level == level).collect();
        if at_level.is_empty() {
            continue;
        }
        let refused_here = at_level.iter().filter(|row| row.status == "red").count();
        let _ = writeln!(out, "  {level} - {refused_here} of {} tips refused", at_level.len());
        for (check, n) in refusal_rows(&at_level) {
            let _ = writeln!(out, "    {check:<36} {n}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "review rounds per tip");
    for level in LEVELS {
        for row in rows.iter().filter(|row| row.level == level) {
            let rounds = row.rounds.map_or_else(|| "-".to_string(), |r| count(r, "round"));
            let checks =
                if row.checks.is_empty() { "-".to_string() } else { row.checks.join(", ") };
            let _ = writeln!(
                out,
                "  {:<14} {:<11} {:<16} {:<12} {checks}",
                level,
                short(&row.sha),
                row.identity,
                rounds
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
    use super::{collapse, entries, summarise, truncated, Entry, FILE_NAME};
    use std::path::Path;

    fn entry(
        level: &str,
        tip: &str,
        status: &str,
        rounds: Option<usize>,
        checks: &[&str],
    ) -> Entry {
        Entry {
            date: "2026-09-12".to_string(),
            level: level.to_string(),
            status: status.to_string(),
            tip: tip.to_string(),
            base: "base".to_string(),
            branch: Some("wu/x/one".to_string()),
            rounds,
            epic: Some("E-X".to_string()),
            wu: Some("E-X.W1".to_string()),
            checks: checks.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn a_record_round_trips_through_its_line() {
        let original =
            entry("working-unit", "abc123", "red", Some(2), &["2·manifest", "6·doc-note"]);
        assert_eq!(Entry::parse(&original.line()), Some(original));
    }

    #[test]
    fn an_unknown_round_count_and_no_identity_round_trip() {
        let mut original = entry("epic", "abc123", "green", None, &[]);
        original.epic = None;
        original.wu = None;
        original.branch = None;
        assert_eq!(Entry::parse(&original.line()), Some(original));
    }

    #[test]
    fn a_malformed_line_parses_as_nothing() {
        assert_eq!(Entry::parse("this is not a record"), None);
        assert_eq!(Entry::parse("level=working-unit\trounds=2"), None);
        assert_eq!(Entry::parse("tip=abc\tlevel=not-a-level\tchecks="), None);
        assert_eq!(Entry::parse("tip=abc\tlevel=epic\tstatus=red\trounds=two\tchecks="), None);
        assert_eq!(Entry::parse("tip=abc\tlevel=epic\tstatus=green"), None);
    }

    #[test]
    fn a_torn_record_is_dropped_rather_than_counted() {
        let whole = entry("working-unit", "abc123", "red", Some(1), &["1·notes"]).line();
        let torn = &whole[..whole.find("\tchecks=").unwrap()];
        assert_eq!(Entry::parse(torn), None, "a half-written line is not a record");
    }

    #[test]
    fn a_malformed_line_never_poisons_the_others() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = super::path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let good = entry("working-unit", "abc123", "red", Some(1), &["1·notes"]);
        std::fs::write(&file, format!("garbage\n{}\n\n", good.line())).unwrap();
        assert_eq!(entries(dir.path()), vec![good]);
    }

    #[test]
    fn one_stray_byte_does_not_blank_the_reading() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = super::path(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let good = entry("working-unit", "abc123", "red", Some(1), &["1·notes"]);
        let mut bytes = format!("{}\n", good.line()).into_bytes();
        bytes.push(0xFF);
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(entries(dir.path()), vec![good]);
    }

    #[test]
    fn repeated_runs_on_one_tip_collapse_to_its_union_and_worst_state() {
        let entries = vec![
            entry("working-unit", "abc123", "green", Some(1), &[]),
            entry("working-unit", "abc123", "red", Some(3), &["2·manifest"]),
            entry("working-unit", "abc123", "red", Some(2), &["6·doc-note", "2·manifest"]),
        ];
        let rows = collapse(&entries);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "red");
        assert_eq!(rows[0].rounds, Some(3));
        assert_eq!(rows[0].checks, vec!["2·manifest", "6·doc-note"]);
    }

    #[test]
    fn a_reading_that_could_not_be_taken_is_not_a_zero() {
        let rows = collapse(&[entry("working-unit", "abc123", "green", None, &[])]);
        assert_eq!(rows[0].rounds, None);
        let text =
            summarise(&[entry("working-unit", "abc123", "green", None, &[])], Path::new("/t"));
        assert!(text.contains("unreadable range"), "{text}");
    }

    #[test]
    fn an_exempt_run_is_not_a_refusal() {
        let rows = collapse(&[entry("epic", "abc123", "exempt", Some(0), &[])]);
        assert_eq!(rows[0].status, "exempt");
    }

    #[test]
    fn the_summary_names_each_refused_check_under_its_level() {
        let entries = vec![
            entry("commit", "c1", "red", Some(0), &["8·manual-review"]),
            entry("working-unit", "w1", "red", Some(2), &["2·manifest"]),
            entry("working-unit", "w2", "green", Some(1), &[]),
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
    fn the_file_is_bounded_when_it_outgrows_its_cap() {
        let lines: Vec<String> = (0..10).map(|n| format!("line {n}")).collect();
        let text = format!("{}\n", lines.join("\n"));
        assert_eq!(truncated(&text, 10), None, "at the cap nothing goes");
        let kept = truncated(&text, 4).unwrap();
        assert!(kept.starts_with("line 8\n"), "the oldest half goes: {kept}");
        assert_eq!(kept.lines().count(), 2, "half the cap is kept: {kept}");
    }

    #[test]
    fn an_empty_tally_says_so_and_names_its_file() {
        let text = summarise(&[], &Path::new("/repo").join("target").join(FILE_NAME));
        assert!(text.contains("no gate runs recorded yet"), "{text}");
        assert!(text.contains(&format!("target/{FILE_NAME}")), "{text}");
    }
}
