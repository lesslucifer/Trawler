use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::gate::{
    doc_paths, format_failures, is_gated, judged_docs, short, Failure, Gate, Level, Report,
};
use crate::git::{blob_of, read_utf8_at, write_note};
use crate::notes::{tier_of, DistilledTo, DocChange, DocNote, TestNote};

// The authoring half of the evidence the gate reads. It exists because a note
// typed by hand can disagree with the tree in ways git already answers - a tier
// guessed, a doc path forgotten, a count retyped - and every one of those is a
// finding the merge gate prints after the expensive judgment has run. Deriving
// them here moves the repair in front of the review, and leaves the agent only
// the sentences no command can write.

/// This command's own check id: what the judgment document is refused for, before
/// the gate's own blocks apply to the notes it would become. Named like the
/// preflight's `0·tree` - the one input this step owns.
const JUDGMENT: &str = "0·judgment";

/// `doc/v1`'s closed action vocabulary, per the documentation system. Refused
/// here because the gate reads only `archived` and would let a typo through.
const ACTIONS: &[&str] = &["added", "updated", "archived", "deleted"];

const LAYERS: [&str; 3] = ["unit", "integration", "e2e"];

pub const USAGE: &str = "\
usage:
  cargo xtask notes write <wu|epic|commit> <tip> [--target <ref>] [--judgment <path>]
  cargo xtask notes write <wu|epic|commit> <tip> [--target <ref>] --template
      (the judgment is the two notes minus every derived field; read it from a file with
       --judgment, or from stdin; --template prints the skeleton and writes nothing)";

/// The judgment document: `test/v2` and `doc/v1` with every field git can answer
/// removed. One document, because the two notes share one identity block, one
/// range and one refusal - and an author who fills two documents is the author
/// who lets them disagree.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Judgment {
    #[serde(default)]
    epic: Option<String>,
    #[serde(default)]
    wu: Option<String>,
    #[serde(default)]
    tests: Tests,
    #[serde(default)]
    docs: Vec<DocEntry>,
    #[serde(default)]
    t2_waived: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Tests {
    #[serde(default)]
    unit: Option<Layer>,
    #[serde(default)]
    integration: Option<Layer>,
    #[serde(default)]
    e2e: Option<Layer>,
}

impl Tests {
    fn named(&self, layer: &str) -> Option<&Layer> {
        match layer {
            "unit" => self.unit.as_ref(),
            "integration" => self.integration.as_ref(),
            _ => self.e2e.as_ref(),
        }
    }
}

/// One layer's judgment: the files that hold its tests, plus the sentence saying
/// what they establish - or the reason this layer owes none. The count is not
/// here, because it is not the agent's to type.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    covers: Option<String>,
    #[serde(default)]
    waived: Option<String>,
}

/// One doc's judgment: its action and what it carries. The path is the join key
/// back to the range - a path the range does not touch, or a touched path with no
/// entry, is refused by the gate's own `6·doc-note`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocEntry {
    path: String,
    action: String,
    /// Printed by `--template` so the derived tier is never typed. Checked against
    /// the path when it is present: the field is a courtesy to the reader, and a
    /// judgment that contradicts the path is refused rather than trusted.
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    distills: Option<String>,
    #[serde(default)]
    distilled_to: Option<DistilledTo>,
}

/// A doc path the range touches, and whether it left the tree: a component design
/// authored and retired inside the range nets out of an endpoint diff, and the
/// gate holds its note entry anyway (DEC-29).
struct TouchedDoc {
    path: String,
    tier: String,
    /// The range retired it: the path does not resolve at the tip, so its entry is
    /// owed despite the endpoint diff, and `archived` owes its lineage.
    absent: bool,
}

struct Range {
    tip_ref: String,
    tip: String,
    base: String,
    diff_sha256: String,
    code_sha256: String,
    names: Vec<String>,
    branch: Option<String>,
}

impl Range {
    // The range is resolved exactly as the gate resolves it - merge-base, not the
    // target's tip - so the notes and the verdict are written about one diff.
    fn resolve(repo: &Path, level: Level, tip_ref: &str, rest: &[&str]) -> Result<Self, String> {
        let target = crate::scope_target(level, tip_ref, rest)?;
        let (tip, base, names, diff_sha256) = Gate { repo }.range(tip_ref, &target)?;
        // What the review binds: the code paths, not the documents this tool is
        // about to write a note for. Empty when the range changes no code.
        let code_sha256 = Gate { repo }.code_sha256(&base, &tip, &names)?;
        // The branch label rides on a named ref, exactly as the receipt's does: a
        // tip named by sha carries none, because there is no branch it names.
        let branch = (tip_ref != tip).then(|| tip_ref.to_string());
        Ok(Self {
            tip_ref: tip_ref.to_string(),
            tip,
            base,
            diff_sha256,
            code_sha256,
            names,
            branch,
        })
    }

    /// The judgment skeleton: the derived half, so no path or tier is typed. It
    /// writes nothing - the agent fills it in and pipes it back.
    fn template(&self, level: Level, docs: &[TouchedDoc]) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# cargo xtask notes write {} {} - fill this in, then pipe it back",
            level.as_note_level(),
            self.tip_ref
        );
        let _ = writeln!(out, "# range: base {} tip {}", self.base, self.tip);
        let _ = writeln!(out, "# diff_sha256: {}", self.diff_sha256);
        if !self.code_sha256.is_empty() {
            let _ = writeln!(out, "# code_sha256: {}", self.code_sha256);
        }
        match level {
            Level::Wu => {
                let _ = writeln!(
                    out,
                    "wu: <E-CODE.Wn>                  # the epic half of this id is derived"
                );
            }
            Level::Epic => {
                let _ = writeln!(out, "epic: <E-CODE>                   # the epic's own id");
            }
            Level::Commit => {
                let _ = writeln!(out, "# a standalone commit carries no identity block");
            }
        }
        let required = level.required_layer();
        let _ = writeln!(
            out,
            "tests:                           # {} needs {}: a count >= 1, or a waiver",
            level.as_note_level(),
            required
        );
        for layer in LAYERS {
            let note = if layer == required { "  # required at this level" } else { "" };
            let _ = writeln!(out, "  {layer}:{note}");
            let _ = writeln!(out, "    files: [<{layer} test file at the tip>]");
            let _ = writeln!(out, "    covers: \"<what those tests establish>\"");
        }
        let _ = writeln!(
            out,
            "docs:                            # every path the range touches; the tier is derived"
        );
        if docs.is_empty() {
            let _ = writeln!(out, "  []");
        }
        for doc in docs {
            let _ = writeln!(out, "  - path: {}", scalar(&doc.path));
            let _ = writeln!(out, "    tier: {}", doc.tier);
            if doc.absent {
                let _ = writeln!(
                    out,
                    "    action: <archived|deleted>  # the range retired this; `archived` needs its lineage"
                );
                let _ = writeln!(out, "    distills: \"<what it carried>\"");
                let _ = writeln!(
                    out,
                    "    distilled_to: []            # archived: the records that carry its forks, or \"none: <why nothing survives>\""
                );
            } else {
                let _ = writeln!(out, "    action: <{}>", ACTIONS.join("|"));
                let _ = writeln!(out, "    distills: \"<what it now carries>\"");
            }
        }
        if !docs.iter().any(|d| d.tier == "T2") {
            let _ = writeln!(
                out,
                "# no T2 doc is in this range: record t2_waived: \"<why>\" - a code change with no T2 entry is refused"
            );
        }
        out.trim_end().to_string() + "\n"
    }
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let (level, tip_ref, rest) = match args {
        [level, tip, rest @ ..] => (level_of(level)?, *tip, rest),
        _ => return Err(USAGE.to_string()),
    };
    let range = Range::resolve(repo, level, tip_ref, rest)?;
    let gate = Gate { repo };
    // A component design authored and retired inside the range nets out of the
    // endpoint diff, and the seam's note still owes it its entry (DEC-29).
    let retired_designs = if level == Level::Wu {
        gate.retired_designs(&range.base, &range.tip, &range.names)
    } else {
        Vec::new()
    };
    let retired: Vec<String> = retired_designs.iter().map(|d| d.path.clone()).collect();
    let touched = touched_docs(repo, &range, &retired);
    if rest.contains(&"--template") {
        return Ok((0, range.template(level, &touched)));
    }

    // A diff the merge exempts owes no note at all: the gate returns before it
    // reads one, and the review is not asked for. Refusing it here would invent
    // work the gate does not owe, so the writer reports the same exemption the
    // gate and the preflight report - and writes nothing.
    let docs =
        if level == Level::Commit { Vec::new() } else { gate.doc_blobs(&range.tip, &range.names) };
    if !is_gated(&range.names, &judged_docs(&docs, &retired_designs)) {
        return Ok((0, exempt(level, &range)));
    }

    let text = read_judgment(crate::flag_value(rest, "--judgment"))?;
    let mut failures = Vec::new();
    let judgment: Option<Judgment> = match serde_yaml::from_str(&text) {
        Ok(judgment) => Some(judgment),
        Err(e) => {
            failures.push(Failure::new(
                JUDGMENT,
                format!("the judgment is not valid YAML: {e}"),
                format!("fix the judgment document (`cargo xtask notes write {} {} --template` prints the shape)", level.as_note_level(), range.tip_ref),
            ));
            None
        }
    };
    let Some(judgment) = judgment else {
        return Ok((1, refused(level, &range.tip, &failures)));
    };

    let (epic, wu) = identity(level, &judgment, &mut failures);
    let changes = doc_changes(&judgment, &mut failures);
    let layers = test_layers(repo, &range.tip, &judgment, &mut failures);
    if !failures.is_empty() {
        return Ok((1, refused(level, &range.tip, &failures)));
    }

    let test_body =
        render_test(level, epic.as_deref(), wu.as_deref(), range.branch.as_deref(), &layers);
    let doc_body = render_doc(
        epic.as_deref(),
        wu.as_deref(),
        range.branch.as_deref(),
        &changes,
        judgment.t2_waived.as_deref(),
    );
    let test_note: TestNote = serde_yaml::from_str(&test_body)
        .map_err(|e| format!("the rendered test/v2 note does not parse: {e}"))?;
    let doc_note: DocNote = serde_yaml::from_str(&doc_body)
        .map_err(|e| format!("the rendered doc/v1 note does not parse: {e}"))?;

    // The gate's own checks, over the bytes about to be written: what it reads and
    // this command writes is one contract, so a derived note is refused here with
    // the merge's own ids and fix lines rather than a second opinion about them.
    let mut report = Report {
        level,
        tip_ref: range.tip_ref.clone(),
        tip: range.tip.clone(),
        base: range.base.clone(),
        diff_sha256: range.diff_sha256.clone(),
        epic: epic.clone(),
        wu: wu.clone(),
        exempt: false,
        failures: Vec::new(),
        waived: Vec::new(),
    };
    crate::gate::check_manifest(&mut report, Some(&test_note));
    gate.check_docs(&mut report, Some(&doc_note), &range.names, &retired);
    if !report.green() {
        return Ok((1, refused(level, &range.tip, &report.failures)));
    }

    write_note(repo, "test", &range.tip, &test_body)?;
    let mut wrote = vec!["test/v2"];
    // A standalone commit is owed the doc note only when it touches docs; every
    // other level carries one whether or not the range changed a doc, because the
    // T2 answer (`t2_waived`) lives on it.
    if level != Level::Commit || !touched.is_empty() {
        write_note(repo, "doc", &range.tip, &doc_body)?;
        wrote.push("doc/v1");
    }
    Ok((0, written(level, &range, &wrote)))
}

fn level_of(name: &str) -> Result<Level, String> {
    match name {
        "wu" => Ok(Level::Wu),
        "epic" => Ok(Level::Epic),
        "commit" => Ok(Level::Commit),
        _ => Err(USAGE.to_string()),
    }
}

fn read_judgment(path: Option<String>) -> Result<String, String> {
    if let Some(path) = path {
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read --judgment {path}: {e}"))
    } else {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read the judgment from stdin: {e}"))?;
        Ok(text)
    }
}

fn touched_docs(repo: &Path, range: &Range, retired: &[String]) -> Vec<TouchedDoc> {
    doc_paths(&range.names, retired)
        .into_iter()
        .map(|path| {
            let absent = blob_of(repo, &range.tip, &path).is_none();
            TouchedDoc { tier: tier_of(&path).unwrap_or("?").to_string(), path, absent }
        })
        .collect()
}

// The identity is declared once and split here: a WU id carries its epic, so the
// two halves of the block cannot disagree - and `1·notes` compares the notes'
// blocks, which is exactly the disagreement this removes.
fn identity(
    level: Level,
    judgment: &Judgment,
    failures: &mut Vec<Failure>,
) -> (Option<String>, Option<String>) {
    let epic = judgment.epic.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let wu = judgment.wu.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match level {
        Level::Commit => {
            if epic.is_some() || wu.is_some() {
                failures.push(Failure::new(
                    JUDGMENT,
                    "a standalone commit carries no epic/wu block, but the judgment declares one"
                        .to_string(),
                    "drop the identity from the judgment - a commit-level note is identified by its level alone".to_string(),
                ));
            }
            (None, None)
        }
        Level::Wu => {
            let Some(wu) = wu else {
                failures.push(Failure::new(
                    JUDGMENT,
                    "no wu id in the judgment".to_string(),
                    "declare `wu: E-CODE.Wn` - a working-unit note carries the identity block the merge compares across the tip's notes".to_string(),
                ));
                return (None, None);
            };
            let Some(derived) = epic_of_wu(wu) else {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("wu id {wu} is not E-CODE.Wn"),
                    "write it as the epic's id plus `.W` and the unit's number, e.g. E-SIMPWF.W3"
                        .to_string(),
                ));
                return (None, None);
            };
            if epic.is_some_and(|e| e != derived) {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("the judgment's epic {} is not the epic its wu id names ({derived})", epic.unwrap_or_default()),
                    format!("drop the separate epic line, or correct one of them - {derived} is what the wu id says"),
                ));
                return (None, None);
            }
            (Some(derived.to_string()), Some(wu.to_string()))
        }
        Level::Epic => {
            let Some(epic) = epic else {
                failures.push(Failure::new(
                    JUDGMENT,
                    "no epic id in the judgment".to_string(),
                    "declare `epic: E-CODE`".to_string(),
                ));
                return (None, None);
            };
            if !is_epic_id(epic) {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("epic id {epic} is not E-CODE"),
                    "use the epic's id from its plan, e.g. E-SIMPWF".to_string(),
                ));
                return (None, None);
            }
            if wu.is_some() {
                failures.push(Failure::new(
                    JUDGMENT,
                    "an epic-level judgment carries a wu id".to_string(),
                    "drop it - the epic's notes carry the epic's id and nothing below it"
                        .to_string(),
                ));
                return (None, None);
            }
            (Some(epic.to_string()), None)
        }
    }
}

fn epic_of_wu(wu: &str) -> Option<&str> {
    let (epic, number) = wu.rsplit_once(".W")?;
    let numbered = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
    (numbered && is_epic_id(epic)).then_some(epic)
}

fn is_epic_id(id: &str) -> bool {
    id.strip_prefix("E-").is_some_and(|code| {
        !code.is_empty()
            && code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    })
}

fn doc_changes(judgment: &Judgment, failures: &mut Vec<Failure>) -> Vec<DocChange> {
    let mut changes = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for entry in &judgment.docs {
        let path = entry.path.trim();
        if seen.contains(&path) {
            failures.push(Failure::new(
                JUDGMENT,
                format!("the judgment lists {path} twice"),
                "keep one entry per doc path - the note is refused for a phantom entry, and two entries for one path is the same as a phantom".to_string(),
            ));
            continue;
        }
        seen.push(path);
        let Some(tier) = tier_of(path) else {
            failures.push(Failure::new(
                JUDGMENT,
                format!("{path} is not a docs/ path"),
                "a doc/v1 entry accounts for a document under docs/; code and infra are not doc changes".to_string(),
            ));
            continue;
        };
        if !ACTIONS.contains(&entry.action.as_str()) {
            failures.push(Failure::new(
                JUDGMENT,
                format!(
                    "{path} names action {}, which is not one of {}",
                    entry.action,
                    ACTIONS.join(" | ")
                ),
                format!("use one of {}", ACTIONS.join(" | ")),
            ));
            continue;
        }
        if entry.tier.as_deref().map(str::trim).is_some_and(|said| said != tier) {
            failures.push(Failure::new(
                JUDGMENT,
                format!(
                    "{path} is tier {tier} by path, and the judgment says {}",
                    entry.tier.as_deref().unwrap_or_default().trim()
                ),
                format!("drop the tier line - {tier} is what the path is, and the derived note writes it"),
            ));
            continue;
        }
        let distills = entry.distills.as_deref().map_or("", str::trim);
        if entry.action != "archived" && distills.is_empty() {
            failures.push(Failure::new(
                JUDGMENT,
                format!("{path} carries no distills sentence"),
                "say what the doc now carries, or what it stopped carrying - the sentence is what makes the accounting a distillation rather than a list".to_string(),
            ));
            continue;
        }
        changes.push(DocChange {
            path: path.to_string(),
            tier: tier.to_string(),
            action: entry.action.clone(),
            distills: (!distills.is_empty()).then(|| distills.to_string()),
            distilled_to: entry.distilled_to.clone(),
        });
    }
    changes
}

enum Rendered {
    Counted { count: u32, covers: String },
    Waived { why: String },
}

fn test_layers(
    repo: &Path,
    tip: &str,
    judgment: &Judgment,
    failures: &mut Vec<Failure>,
) -> Vec<(&'static str, Rendered)> {
    let mut layers = Vec::new();
    for name in LAYERS {
        let Some(layer) = judgment.tests.named(name) else { continue };
        let waived = layer.waived.as_deref().map_or("", str::trim);
        if !waived.is_empty() && !layer.files.is_empty() {
            failures.push(Failure::new(
                JUDGMENT,
                format!("tests.{name} carries both files and a waiver"),
                format!("give tests.{name} either the files that hold its tests, or a waived reason - not both"),
            ));
            continue;
        }
        if !waived.is_empty() {
            layers.push((name, Rendered::Waived { why: waived.to_string() }));
            continue;
        }
        if layer.files.is_empty() {
            failures.push(Failure::new(
                JUDGMENT,
                format!("tests.{name} names no test file"),
                format!("list the files that hold the {name} tests at the tip, or record tests.{name}.waived"),
            ));
            continue;
        }
        let covers = layer.covers.as_deref().map_or("", str::trim).to_string();
        if covers.is_empty() {
            failures.push(Failure::new(
                JUDGMENT,
                format!("tests.{name} carries no covers sentence"),
                format!("say what the {name} tests establish - the count is derived, the sentence is the judgment"),
            ));
            continue;
        }
        let mut count: u32 = 0;
        let mut named: Vec<&str> = Vec::new();
        for file in &layer.files {
            let file = file.trim();
            if named.contains(&file) {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("tests.{name} names {file} twice"),
                    "list each test file once - a file counted twice is a count nobody can explain"
                        .to_string(),
                ));
                continue;
            }
            named.push(file);
            let Some(body) = read_utf8_at(repo, tip, file) else {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("tests.{name} names {file}, which the tip's tree does not hold as text"),
                    "name a file at the tip - the count is read from the tip's bytes, never from the working tree".to_string(),
                ));
                continue;
            };
            let Some(declared) = declared_tests(file, &body) else {
                failures.push(Failure::new(
                    JUDGMENT,
                    format!("tests.{name} names {file}, whose kind the counter does not know"),
                    "name a Rust or TypeScript test file - an unrecognised file would be counted as zero, which is the lie this command exists to prevent".to_string(),
                ));
                continue;
            };
            count = count.saturating_add(declared);
        }
        layers.push((name, Rendered::Counted { count, covers }));
    }
    layers
}

/// The tests a file declares, by the declaration its language writes: the
/// `#[test]`/`#[tokio::test]` attributes of Rust - which is what a `proptest!`
/// block's properties carry in this tree - and the `it(`/`test(` cases of
/// TypeScript, `.each(` tables included. It is a line rule over the file's text,
/// and that is stated rather than implied: a declaration written in another form
/// is not counted, and text that spells one is. `None` for a kind with no rule -
/// refused rather than counted as zero.
fn declared_tests(path: &str, body: &str) -> Option<u32> {
    match path.rsplit('.').next()? {
        "rs" => Some(rust_tests(body)),
        "ts" | "tsx" | "js" | "jsx" => Some(ts_tests(body)),
        _ => None,
    }
}

fn rust_tests(body: &str) -> u32 {
    let mut count: u32 = 0;
    for line in body.lines() {
        let line = line.trim_start();
        if line.starts_with("#[test]") || line.starts_with("#[tokio::test") {
            count = count.saturating_add(1);
        }
    }
    count
}

fn ts_tests(body: &str) -> u32 {
    let mut count: u32 = 0;
    for line in body.lines() {
        let line = line.trim_start();
        for form in ["it(", "test(", "it.each(", "test.each("] {
            if line.starts_with(form) {
                count = count.saturating_add(1);
                break;
            }
        }
    }
    count
}

fn render_test(
    level: Level,
    epic: Option<&str>,
    wu: Option<&str>,
    branch: Option<&str>,
    layers: &[(&str, Rendered)],
) -> String {
    use std::fmt::Write as _;
    let mut out = format!("schema: test/v2\nlevel: {}\n", level.as_note_level());
    if let Some(epic) = epic {
        let _ = writeln!(out, "epic: {epic}");
    }
    if let Some(wu) = wu {
        let _ = writeln!(out, "wu: {wu}");
    }
    if let Some(branch) = branch {
        let _ = writeln!(out, "branch: {branch}");
    }
    if layers.is_empty() {
        out.push_str("tests: {}\n");
        return out;
    }
    out.push_str("tests:\n");
    for (name, layer) in layers {
        match layer {
            Rendered::Counted { count, covers } => {
                let _ = writeln!(out, "  {name}: {{ count: {count}, covers: {} }}", scalar(covers));
            }
            Rendered::Waived { why } => {
                let _ = writeln!(out, "  {name}: {{ waived: {} }}", scalar(why));
            }
        }
    }
    out
}

fn render_doc(
    epic: Option<&str>,
    wu: Option<&str>,
    branch: Option<&str>,
    changes: &[DocChange],
    t2_waived: Option<&str>,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("schema: doc/v1\n");
    if let Some(epic) = epic {
        let _ = writeln!(out, "epic: {epic}");
    }
    if let Some(wu) = wu {
        let _ = writeln!(out, "wu: {wu}");
    }
    if let Some(branch) = branch {
        let _ = writeln!(out, "branch: {branch}");
    }
    if changes.is_empty() {
        out.push_str("changes: []\n");
    } else {
        out.push_str("changes:\n");
        for change in changes {
            let _ = writeln!(out, "  - path: {}", scalar(&change.path));
            let _ = writeln!(out, "    tier: {}", change.tier);
            let _ = writeln!(out, "    action: {}", change.action);
            if let Some(distills) = &change.distills {
                let _ = writeln!(out, "    distills: {}", scalar(distills));
            }
            if let Some(distilled_to) = &change.distilled_to {
                let _ = writeln!(out, "    distilled_to: {}", distilled_to_yaml(distilled_to));
            }
        }
    }
    if let Some(why) = t2_waived.map(str::trim).filter(|why| !why.is_empty()) {
        let _ = writeln!(out, "t2_waived: {}", scalar(why));
    }
    out
}

fn distilled_to_yaml(distilled_to: &DistilledTo) -> String {
    match distilled_to {
        DistilledTo::Paths(paths) => {
            let list = paths.iter().map(|p| scalar(p)).collect::<Vec<_>>().join(", ");
            format!("[{list}]")
        }
        DistilledTo::DeadEnd(reason) => scalar(reason),
    }
}

/// A double-quoted YAML scalar. Quoted rather than plain because every one of
/// these strings is a sentence an agent wrote: a colon, a `#` or a newline must
/// survive into the note rather than change what the YAML means.
fn scalar(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ =
                    std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn refused(level: Level, tip: &str, failures: &[Failure]) -> String {
    format!(
        "notes write {} refused on {} - nothing was written; the judgment and the range disagree\n{}{} blocker(s) -> fix the judgment, then run it again",
        level.as_note_level(),
        short(tip),
        format_failures(failures),
        failures.len()
    )
}

fn exempt(level: Level, range: &Range) -> String {
    format!(
        "notes write {} on {}: no review is asked for on this diff - the gate is exempt (no code and no design artifact in the range), so no test/v2 or doc/v1 is demanded and nothing was written",
        level.as_note_level(),
        short(&range.tip)
    )
}

fn written(level: Level, range: &Range, wrote: &[&str]) -> String {
    let binding = if range.code_sha256.is_empty() {
        "this range changes no code, so no review binds it: the cr note is not asked for"
            .to_string()
    } else if crate::gate::runs_at(crate::gate::REVIEW, level) {
        format!(
            "code_sha256: {}\nthe review binds the code paths in its cr/v5 note, and the design step pins what it judged in design/v1",
            range.code_sha256
        )
    } else {
        format!(
            "code_sha256: {}\nthis level asks for no review of its own diff - every seam carries its own (DEC-51); the design step pins what it judged in design/v1",
            range.code_sha256
        )
    };
    format!(
        "notes write {} on {}: {} written\nbase: {}\nhead: {}\ndiff_sha256: {}\n{binding}",
        level.as_note_level(),
        short(&range.tip),
        wrote.join(" + "),
        range.base,
        range.tip,
        range.diff_sha256
    )
}

#[cfg(test)]
mod tests {
    use super::{
        declared_tests, epic_of_wu, is_epic_id, render_doc, render_test, scalar, Rendered, ACTIONS,
    };
    use crate::gate::Level;
    use crate::notes::{DocChange, DocNote, TestNote};

    fn range_over_code(code_sha256: &str) -> super::Range {
        super::Range {
            tip_ref: "HEAD".to_string(),
            tip: "c4b1e31e4020e6718146971b7feecfbc9dd76bf8".to_string(),
            base: "f6948c1b8f79a63bc8b436049d73729d515aec67".to_string(),
            diff_sha256: "cef8b5b0".to_string(),
            code_sha256: code_sha256.to_string(),
            names: vec!["src/lib.rs".to_string()],
            branch: None,
        }
    }

    // What the tool prints is what an agent does next, so it may not name a note
    // the level does not owe: an epic's seams carry their reviews and the epic
    // itself carries none (DEC-51). The rule is asked of the gate, never restated.
    #[test]
    fn the_written_message_names_a_review_only_where_the_level_runs_one() {
        let wu = super::written(Level::Wu, &range_over_code("eed5194d"), &["test/v2"]);
        assert!(wu.contains("the review binds the code paths in its cr/v5 note"), "{wu}");

        let epic = super::written(Level::Epic, &range_over_code("eed5194d"), &["test/v2"]);
        assert!(epic.contains("code_sha256: eed5194d"), "the hash is still printed: {epic}");
        assert!(!epic.contains("cr/v5"), "an epic owes no review of its own diff: {epic}");
        assert!(epic.contains("every seam carries its own"), "{epic}");
        assert!(epic.contains("design/v1"), "the design step is unaffected: {epic}");

        let no_code = super::written(Level::Wu, &range_over_code(""), &["test/v2"]);
        assert!(no_code.contains("no review binds it"), "{no_code}");
    }

    // The count is the whole reason this command exists: it is derived from the
    // bytes at the tip, so it can be neither mistyped nor guessed.
    #[test]
    fn a_count_is_the_declarations_the_named_file_holds() {
        let rust = "\
#[test]
fn one() {}
#[tokio::test]
async fn two() {}
#[tokio::test(flavor = \"multi_thread\", worker_threads = 2)]
async fn three() {}
proptest! {
    #[test]
    fn four(x in 0u8..2) {}
}
";
        // Four declarations: a property keeps its own `#[test]`, which is what the
        // tree writes, and an attribute written with arguments is still the same
        // declaration.
        assert_eq!(declared_tests("xtask/src/notes.rs", rust), Some(4));
        let ts = "\
it('lands cold', () => {});
  test('keeps the order', () => {});
test.each([[1], [2]])('a row', () => {});
describe('a group', () => {});
";
        assert_eq!(declared_tests("ui/src/flow.test.ts", ts), Some(3));
    }

    // An unrecognised file counted as zero would write a number the tree never
    // said - refused, so the count is always read rather than assumed.
    #[test]
    fn a_kind_with_no_rule_is_refused_rather_than_counted_as_zero() {
        assert_eq!(declared_tests("tests/fixture.py", "def test_x():\n"), None);
        assert_eq!(declared_tests("no-extension", "#[test]\n"), None);
    }

    // One declared id, two halves: the epic is derived from the wu id, which is
    // what makes the tip's identity block impossible to disagree with itself.
    #[test]
    fn a_wu_id_yields_its_epic_and_a_malformed_one_yields_nothing() {
        assert_eq!(epic_of_wu("E-SIMPWF.W3"), Some("E-SIMPWF"));
        assert_eq!(epic_of_wu("E-UIFOUND.W12"), Some("E-UIFOUND"));
        assert_eq!(epic_of_wu("E-SIMPWF"), None, "no unit number");
        assert_eq!(epic_of_wu("E-SIMPWF.Wthree"), None, "a word is not a unit number");
        assert_eq!(epic_of_wu("SIMPWF.W3"), None, "the id convention is E-CODE");
        assert_eq!(epic_of_wu(".W3"), None, "no epic");

        assert!(is_epic_id("E-SIMPWF"));
        assert!(is_epic_id("E-UI"));
        assert!(!is_epic_id("simpwf"));
        assert!(!is_epic_id("E-"));
        assert!(!is_epic_id("E-simpwf"));
    }

    // Both notes are rendered from one identity, so they cannot be the two
    // documents that make `1·notes` red.
    #[test]
    fn both_notes_carry_one_identical_identity_block() {
        let epic = Some("E-SIMPWF".to_string());
        let wu = Some("E-SIMPWF.W3".to_string());
        let branch = Some("wu/simpwf/note-tool".to_string());
        let test = render_test(
            Level::Wu,
            epic.as_deref(),
            wu.as_deref(),
            branch.as_deref(),
            &[("integration", Rendered::Waived { why: "nothing owed".to_string() })],
        );
        let doc = render_doc(epic.as_deref(), wu.as_deref(), branch.as_deref(), &[], None);

        let test: TestNote = serde_yaml::from_str(&test).expect("the test note parses");
        let doc: DocNote = serde_yaml::from_str(&doc).expect("the doc note parses");
        assert_eq!(test.level, "working-unit");
        assert_eq!(test.epic, doc.epic);
        assert_eq!(test.wu, doc.wu);
        assert_eq!(test.epic.as_deref(), Some("E-SIMPWF"));
        assert_eq!(test.wu.as_deref(), Some("E-SIMPWF.W3"));
    }

    #[test]
    fn a_counted_layer_renders_its_count_and_its_sentence() {
        let test = render_test(
            Level::Commit,
            None,
            None,
            None,
            &[(
                "unit",
                Rendered::Counted {
                    count: 12,
                    covers: "the rule, and its edges: 1 -> 2".to_string(),
                },
            )],
        );
        let note: TestNote = serde_yaml::from_str(&test).expect("parses");
        let unit = note.tests.layer("unit").expect("unit");
        assert_eq!(unit.describe(), "count 12");
        assert!(unit.satisfied());
    }

    // A doc entry is the one place a path is still typed - and the tier, which the
    // gate compares against the path, is the field this command takes away.
    #[test]
    fn a_doc_entry_renders_the_derived_tier_and_the_decided_action() {
        let doc = render_doc(
            Some("E-SIMPWF"),
            Some("E-SIMPWF.W3"),
            None,
            &[DocChange {
                path: "docs/decisions/dec-44-x.html".to_string(),
                tier: "T2".to_string(),
                action: "added".to_string(),
                distills: Some("what the note tool derives".to_string()),
                distilled_to: None,
            }],
            Some("no T2 doc was touched"),
        );
        let note: DocNote = serde_yaml::from_str(&doc).expect("parses");
        assert_eq!(note.changes[0].tier, "T2");
        assert_eq!(note.changes[0].action, "added");
        assert_eq!(note.t2_waived.as_deref(), Some("no T2 doc was touched"));
    }

    // The judgment `--template` prints carries the derived tier, so it has to be
    // one this command accepts back: the field is read, and a judgment that
    // contradicts the path is refused rather than quietly overwritten.
    #[test]
    fn a_tier_that_contradicts_the_path_is_refused_and_the_derived_one_is_accepted() {
        let judgment = |tier: Option<&str>| super::Judgment {
            docs: vec![super::DocEntry {
                path: "docs/use-cases/api.html".to_string(),
                action: "updated".to_string(),
                tier: tier.map(str::to_string),
                distills: Some("the contract".to_string()),
                distilled_to: None,
            }],
            ..super::Judgment::default()
        };

        let mut failures = Vec::new();
        let changes = super::doc_changes(&judgment(Some("T2")), &mut failures);
        assert!(failures.is_empty(), "the derived tier is accepted: {failures:?}");
        assert_eq!(changes[0].tier, "T2");

        let mut failures = Vec::new();
        super::doc_changes(&judgment(Some("T3")), &mut failures);
        assert_eq!(failures.len(), 1, "a tier the path denies is refused: {failures:?}");
        assert_eq!(failures[0].check, super::JUDGMENT);
        assert!(failures[0].what.contains("is tier T2 by path"), "got: {}", failures[0].what);
    }

    #[test]
    fn a_distilled_to_may_be_paths_or_a_named_dead_end() {
        let change = |distilled_to| DocChange {
            path: "docs/work/2026-09-12-simpwf-w3-plan.html".to_string(),
            tier: "T3".to_string(),
            action: "archived".to_string(),
            distills: None,
            distilled_to,
        };
        let paths = render_doc(
            None,
            None,
            None,
            &[change(Some(crate::notes::DistilledTo::Paths(vec![
                "docs/workflow.html".to_string()
            ])))],
            None,
        );
        let dead_end = render_doc(
            None,
            None,
            None,
            &[change(Some(crate::notes::DistilledTo::DeadEnd(
                "none: the run was a dead end".to_string(),
            )))],
            None,
        );
        let paths: DocNote = serde_yaml::from_str(&paths).expect("parses");
        let dead_end: DocNote = serde_yaml::from_str(&dead_end).expect("parses");
        assert!(paths.changes[0].distilled_to.as_ref().expect("paths").recorded());
        assert!(dead_end.changes[0].distilled_to.as_ref().expect("dead end").recorded());
    }

    // Sentences, not tokens: a covers line carries colons, hashes and quotes, and
    // the note has to mean the same thing after it round-trips.
    #[test]
    fn a_sentence_survives_the_scalar_it_is_written_as() {
        let sentences = [
            "the rule: 1 -> 2",
            "a # in the middle",
            "quotes \"and\" more",
            "two\nlines",
            "a tab\there",
        ];
        let covers = sentences.join("; ");
        let test = render_test(
            Level::Commit,
            None,
            None,
            None,
            &[("unit", Rendered::Counted { count: 1, covers: covers.clone() })],
        );
        let note: TestNote = serde_yaml::from_str(&test).expect("parses");
        match note.tests.layer("unit").expect("unit") {
            crate::notes::Layer::Counted { covers: read, .. } => {
                assert_eq!(read.as_deref(), Some(covers.as_str()), "got: {test}");
            }
            crate::notes::Layer::Waived { .. } => {
                panic!("a counted layer stays counted: {test}")
            }
        }
        assert_eq!(scalar("plain"), "\"plain\"");
    }

    #[test]
    fn the_action_vocabulary_is_the_documented_one() {
        assert_eq!(ACTIONS, ["added", "updated", "archived", "deleted"]);
    }
}
