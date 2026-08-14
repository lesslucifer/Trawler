use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::git::{diff_names, diff_sha256, git, note, write_note};
use crate::notes::{
    is_code, is_doc, is_expected_tracked_doc, is_rule_bearing, tier_of, CrNote, DistilledTo,
    DocChange, DocNote, TestNote, CR_SCHEMAS,
};

// What a fork card may be carried by when no decision record carries it: the TSDoc
// tags a contract compiles into, or a rulebook slug that outranks it. Both are
// things a reader can go and read - the whole difference between a dead end named
// and a dead end assumed (DEC-16).
const CARRIERS: &[&str] = &["@use", "@avoid", "@states", "@madeof", "@tier"];

// A slug counts only while the rulebook really holds it: a citation nobody can
// follow is the same silence as no citation, and the rulebook is the one place that
// can say whether `wrap-not-truncate` is a rule or a phrase.
fn names_a_carrier(repo: &Path, rev: &str, reason: &str) -> bool {
    if CARRIERS.iter().any(|tag| reason.contains(tag)) {
        return true;
    }
    crate::doc::rulebook_slugs(repo, rev).iter().any(|slug| reason.contains(slug.as_str()))
}

// A cr/v2 note carries no design section and cannot be made to carry one: the
// review that wrote it ran before the dimension existed, and editing it after
// the fact would claim a judgment nobody made. The epic walk therefore drops
// this one check at seams (immutable history), while every design check that
// only a cr/v3 note can fail keeps applying there.
const DESIGN_SCHEMA: &str = "9·design-schema";
const DESIGN: &str = "9·design";
const UNBUILT: &str = "components·unbuilt";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Commit,
    Wu,
    Epic,
}

impl Level {
    pub fn as_note_level(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Wu => "working-unit",
            Self::Epic => "epic",
        }
    }

    fn required_layer(self) -> &'static str {
        match self {
            Self::Commit => "unit",
            Self::Wu => "integration",
            Self::Epic => "e2e",
        }
    }
}

#[derive(Debug)]
pub struct Failure {
    pub check: String,
    pub what: String,
    pub fix: String,
}

impl Failure {
    fn new(check: &str, what: String, fix: String) -> Self {
        Self { check: check.to_string(), what, fix }
    }
}

pub struct Report {
    pub level: Level,
    pub tip_ref: String,
    pub tip: String,
    pub base: String,
    pub diff_sha256: String,
    pub epic: Option<String>,
    pub wu: Option<String>,
    pub exempt: bool,
    pub failures: Vec<Failure>,
    pub waived: Vec<String>,
}

impl Report {
    pub fn green(&self) -> bool {
        self.failures.is_empty()
    }
}

struct ParsedNotes {
    test: Option<TestNote>,
    cr: Option<CrNote>,
    doc: Option<DocNote>,
}

pub struct Gate<'a> {
    pub repo: &'a Path,
}

impl Gate<'_> {
    pub fn verify(&self, level: Level, tip_ref: &str, target_ref: &str) -> Result<Report, String> {
        let tip = git(self.repo, &["rev-parse", tip_ref])?;
        let base = git(self.repo, &["merge-base", target_ref, &tip])?;
        let names = diff_names(self.repo, &base, &tip)?;
        let sha = diff_sha256(self.repo, &base, &tip)?;
        let code_changed = names.iter().any(|p| is_code(p));
        // Three checks below read every doc the diff touches, at the same tip, for
        // the same immutable blobs. Fetched once per verify and threaded through:
        // the walk calls verify once per seam, so a per-site fetch multiplies whole
        // sweeps of `git show` by the number of seams.
        let docs = if level == Level::Commit { Vec::new() } else { self.doc_blobs(&tip, &names) };
        // A design is a docs-only deliverable, so the docs/infra exemption would
        // hand the design dimension exactly the diffs it exists to judge - the
        // most common design Working Unit writes a run and no code at all. The
        // merge that closes a design is answerable for it whether or not any
        // code moved, for the same reason `6·run-closed` already is.
        let designs = !design_artifacts_of(&docs).is_empty();
        let gated = code_changed || designs;
        let mut report = Report {
            level,
            tip_ref: tip_ref.to_string(),
            tip,
            base,
            diff_sha256: sha,
            epic: None,
            wu: None,
            exempt: !gated,
            failures: Vec::new(),
            waived: Vec::new(),
        };
        if level == Level::Epic {
            self.check_tracked_docs(&mut report);
            self.check_unbuilt_designs(&mut report);
        }
        if level != Level::Commit {
            check_runs_closed(&mut report, &docs);
        }
        self.check_manual_review(&mut report);
        report.exempt = report.exempt && report.failures.is_empty();
        if !gated {
            return Ok(report);
        }
        let parsed = self.load_notes(&mut report, &names);
        check_manifest(&mut report, parsed.test.as_ref());
        if level != Level::Commit {
            check_review(&mut report, parsed.cr.as_ref());
            self.check_design(&mut report, parsed.cr.as_ref(), &docs);
        }
        // The citation names a T3 design, which check 6 forces out of the tree by
        // the epic tip - so only the Working Unit that built the surface can
        // resolve it. The epic re-asks the question where it is answerable: at
        // every seam its walk re-verifies.
        if level == Level::Wu {
            self.check_handoff(&mut report, parsed.cr.as_ref());
        }
        if level == Level::Commit {
            Self::check_ui_outside_a_wu(&mut report, &names);
        }
        self.check_docs(&mut report, parsed.doc.as_ref(), &names);
        if level == Level::Epic {
            self.walk(&mut report, target_ref)?;
        }
        Ok(report)
    }

    fn load_notes(&self, report: &mut Report, names: &[String]) -> ParsedNotes {
        let tip = report.tip.clone();
        let doc_needed = report.level != Level::Commit || names.iter().any(|p| is_doc(p));
        let test = self
            .parse_note::<TestNote>(report, &tip, "test", &["test/v2"], true, |n| n.schema.clone());
        let cr = if report.level == Level::Commit {
            None
        } else {
            self.parse_note::<CrNote>(report, &tip, "cr", CR_SCHEMAS, true, |n| n.schema.clone())
        };
        let doc = self.parse_note::<DocNote>(report, &tip, "doc", &["doc/v1"], doc_needed, |n| {
            n.schema.clone()
        });
        if let Some(t) = &test {
            report.epic.clone_from(&t.epic);
            report.wu.clone_from(&t.wu);
        }
        check_identity(
            report,
            &[
                test.as_ref().map(|n| (n.epic.clone(), n.wu.clone())),
                cr.as_ref().map(|n| (n.epic.clone(), n.wu.clone())),
                doc.as_ref().map(|n| (n.epic.clone(), n.wu.clone())),
            ],
        );
        ParsedNotes { test, cr, doc }
    }

    // `accepted` is the contract history, oldest first: an older schema still
    // parses (its note is immutable evidence) while every message names the
    // current one, which is what a fix must be written to.
    fn parse_note<T: serde::de::DeserializeOwned>(
        &self,
        report: &mut Report,
        tip: &str,
        note_ref: &str,
        accepted: &[&str],
        required: bool,
        schema_of: impl Fn(&T) -> String,
    ) -> Option<T> {
        let schema = accepted.last().copied().unwrap_or_default();
        let Some(raw) = note(self.repo, note_ref, tip) else {
            if required {
                report.failures.push(Failure::new(
                    "1·notes",
                    format!("no {schema} note on tip {}", short(tip)),
                    match note_ref {
                        "cr" => format!(
                            "run the code-review skill on {} against the target until PASS lands on the tip",
                            report.tip_ref
                        ),
                        "doc" => "write the doc/v1 note accounting for every changed doc file"
                            .to_string(),
                        _ => format!(
                            "write the {schema} manifest: git notes --ref={note_ref} add -m '<yaml>' {}",
                            short(tip)
                        ),
                    },
                ));
            }
            return None;
        };
        match serde_yaml::from_str::<T>(&raw) {
            Ok(parsed) => {
                let found = schema_of(&parsed);
                if accepted.contains(&found.as_str()) {
                    Some(parsed)
                } else {
                    report.failures.push(Failure::new(
                        "1·notes",
                        format!("{note_ref} note declares schema {found}, expected {schema}"),
                        format!("rewrite the note to the {schema} contract"),
                    ));
                    None
                }
            }
            Err(e) => {
                report.failures.push(Failure::new(
                    "1·notes",
                    format!("{note_ref} note on {} is not valid {schema} YAML: {e}", short(tip)),
                    format!("fix the note body to match the {schema} contract"),
                ));
                None
            }
        }
    }

    fn check_docs(&self, report: &mut Report, doc: Option<&DocNote>, names: &[String]) {
        let doc_paths: Vec<&String> = names.iter().filter(|p| is_doc(p)).collect();
        if let Some(doc) = doc {
            let entries: BTreeMap<&str, &crate::notes::DocChange> =
                doc.changes.iter().map(|c| (c.path.as_str(), c)).collect();
            for path in &doc_paths {
                match entries.get(path.as_str()) {
                    None => report.failures.push(Failure::new(
                        "6·doc-note",
                        format!("{path} changed in the diff but has no entry in doc/v1"),
                        format!(
                            "add its entry (tier {}, action + distills/distilled_to) to the doc/v1 note",
                            tier_of(path).unwrap_or("?")
                        ),
                    )),
                    Some(entry) => {
                        let derived = tier_of(path).unwrap_or("?");
                        if entry.tier != derived {
                            report.failures.push(Failure::new(
                                "6·doc-note",
                                format!("{path} is tier {derived} by path, note says {}", entry.tier),
                                "correct the tier in the doc/v1 entry".to_string(),
                            ));
                        }
                        if entry.action == "archived"
                            && !entry
                                .distilled_to
                                .as_ref()
                                .is_some_and(crate::notes::DistilledTo::recorded)
                        {
                            report.failures.push(Failure::new(
                                "6·doc-note",
                                format!("{path} is archived without distillation lineage"),
                                "record distilled_to: [paths] or a dead-end reason".to_string(),
                            ));
                        }
                        self.check_retirement_order(report, path, entry);
                    }
                }
            }
            for entry in &doc.changes {
                if !doc_paths.iter().any(|p| p.as_str() == entry.path) {
                    report.failures.push(Failure::new(
                        "6·doc-note",
                        format!("doc/v1 lists {} but the diff does not touch it", entry.path),
                        "drop the phantom entry or commit the missing doc change".to_string(),
                    ));
                }
            }
            if report.level != Level::Commit {
                let t2_updated = doc.changes.iter().any(|c| c.tier == "T2");
                let t2_waived = doc.t2_waived.as_ref().is_some_and(|w| !w.trim().is_empty());
                if !t2_updated && !t2_waived {
                    report.failures.push(Failure::new(
                        "6·doc-note",
                        "code changed but no T2 doc was updated and no t2_waived reason given"
                            .to_string(),
                        "update docs/use-cases|decisions|backlog|design-rules, or record t2_waived: \"<why>\""
                            .to_string(),
                    ));
                }
            }
        }
    }

    // DEC-16's order, refused rather than remembered. Step ③ - removing a design
    // file - is the irreversible one, and step ① is the only one that can lose
    // data, so the gate holds ③ until ① has landed.
    //
    // Two lineages are accepted, and the second exists because the first was too
    // narrow: W7 ⑥ measured it over the sixteen files W7 retired and 2 of them had
    // NO surviving record, correctly - every fork was carried by the contract or
    // superseded by a rule. DEC-16 says so itself ("most are local and die with the
    // file"), so demanding a record refused the case that record calls the
    // majority. What the check still refuses is the reason that names nothing:
    // "none", "nothing survives", "local" - a dead end ASSUMED, which is the loss
    // DEC-16 exists to prevent. A dead end NAMED cites what carries the fork.
    fn check_retirement_order(&self, report: &mut Report, path: &str, entry: &DocChange) {
        if !path.starts_with(crate::doc::COMPONENTS)
            || crate::git::show_bytes(self.repo, &report.tip, path).is_ok()
        {
            return;
        }
        let distilled = match &entry.distilled_to {
            Some(DistilledTo::Paths(paths)) => paths.iter().any(|to| {
                to.starts_with("docs/decisions/")
                    && crate::git::show_bytes(self.repo, &report.tip, to).is_ok()
            }),
            Some(DistilledTo::DeadEnd(reason)) => names_a_carrier(self.repo, &report.tip, reason),
            None => false,
        };
        if !distilled {
            report.failures.push(Failure::new(
                "6·retirement-order",
                format!("{path} is removed with its forks distilled nowhere"),
                format!("DEC-16: distil the fork cards to docs/decisions/ and record distilled_to: [that record] - or, where every fork really is a dead end, say what carries it: a TSDoc tag ({}) or a rulebook slug. A reason that names nothing is a dead end assumed, not named", CARRIERS.join(" ")),
            ));
        }
    }

    fn doc_blobs(&self, tip: &str, names: &[String]) -> Vec<(String, String)> {
        names
            .iter()
            .filter(|p| is_doc(p))
            .filter_map(|path| {
                let bytes = crate::git::show_bytes(self.repo, tip, path).ok()?;
                Some((path.clone(), String::from_utf8(bytes).ok()?))
            })
            .collect()
    }

    pub fn design_artifacts(&self, tip: &str, names: &[String]) -> Vec<String> {
        design_artifacts_of(&self.doc_blobs(tip, names))
    }

    // Tier-3, enforced where Tier-2 could not be: the design dimension of the
    // code review is an independent agent's judgment, so the only thing a gate
    // can hold is that it ran, over every design artifact the diff carries, and
    // against the rulebook that diff ships. Everything below refuses a silence -
    // a design-bearing PASS that never mentions the design is exactly the
    // failure the schema bump exists to make visible.
    fn check_design(&self, report: &mut Report, cr: Option<&CrNote>, docs: &[(String, String)]) {
        let Some(cr) = cr else { return };
        let artifacts = design_artifacts_of(docs);
        // Nothing judged, nothing claimed: a build's note carries a design
        // section for its citation and its drops alone, and a verdict or a
        // rulebook pin over an empty judgment would be a claim about no design.
        if artifacts.is_empty() {
            return;
        }
        let Some(design) = &cr.design else {
            let check = if cr.schema == "cr/v3" { DESIGN } else { DESIGN_SCHEMA };
            report.failures.push(Failure::new(
                check,
                format!(
                    "{} design artifact(s) in the diff and the {} note carries no design section",
                    artifacts.len(),
                    cr.schema
                ),
                format!(
                    "re-run the code-review skill on {}: a diff carrying a design artifact activates the design finder, and its verdict is the cr/v3 note's design section",
                    report.tip_ref
                ),
            ));
            return;
        };
        for path in artifacts.iter().filter(|p| !design.judged(p)) {
            report.failures.push(Failure::new(
                DESIGN,
                format!("{path} is a design artifact the design section does not name"),
                "the design dimension judges every design artifact in the diff - review it and list it under design.artifacts, or the PASS covers a design nobody read".to_string(),
            ));
        }
        if design.verdict != "PASS" {
            let stated =
                if design.verdict.trim().is_empty() { "absent" } else { design.verdict.as_str() };
            report.failures.push(Failure::new(
                DESIGN,
                format!("design verdict is {stated}"),
                "fix the verified design findings and re-run the review until its design dimension passes".to_string(),
            ));
        }
        self.check_design_pin(report, design);
    }

    // The design verdict is a claim against a specific rulebook - the same claim
    // the run's own receipt makes, and the same way it went unchecked for three
    // Working Units. Read at the TIP, never the working tree: a seam is judged
    // against the rulebook that seam shipped.
    fn check_design_pin(&self, report: &mut Report, design: &crate::notes::Design) {
        let Some(blob) = crate::doc::rulebook_blob(self.repo, &report.tip) else {
            report.failures.push(Failure::new(
                DESIGN,
                "the tip carries no readable design rulebook to check the design pin against"
                    .to_string(),
                "run the gate from a checkout where the tip's docs/design-rules/design-rules-index.html resolves".to_string(),
            ));
            return;
        };
        let short_blob = crate::doc::pin_of(&blob);
        if design.rulebook.len() < crate::doc::PIN_MIN {
            report.failures.push(Failure::new(
                DESIGN,
                format!(
                    "the design section pins \"{}\", which names no rulebook blob",
                    design.rulebook
                ),
                format!("pin what the design dimension read: design.rulebook: {short_blob}"),
            ));
        } else if !crate::doc::pin_matches(&design.rulebook, &blob) {
            report.failures.push(Failure::new(
                DESIGN,
                format!(
                    "the design dimension judged against rulebook {}; this tip ships {short_blob}",
                    design.rulebook
                ),
                format!("the rulebook moved under the review - re-run it against this tip's rulebook and re-pin: design.rulebook: {short_blob}"),
            ));
        }
    }

    // The surfaces a diff adds that owe a design - the same function
    // `design-scope` prints for the reviewer, so the note and the gate cannot
    // disagree about what a build must cite.
    pub fn new_surfaces(&self, tip: &str, base: &str) -> Vec<String> {
        let (added, renamed) =
            crate::git::added_and_renamed(self.repo, base, tip).unwrap_or_default();
        // A rename brings its design with it only when what it renamed was itself
        // a surface. Git pairs on similarity and knows nothing of surfaces, so a
        // pairing whose source was never one is a new surface wearing a recycled
        // file - and it owes a design like any other.
        let mut surfaces: Vec<String> = added
            .into_iter()
            .chain(
                renamed
                    .into_iter()
                    .filter(|(from, _)| !crate::doc::surface_at(self.repo, base, from))
                    .map(|(_, to)| to),
            )
            .filter(|path| crate::doc::surface_at(self.repo, tip, path))
            .collect();
        surfaces.sort();
        surfaces.dedup();
        surfaces
    }

    // The accounting lives on a `cr` note, and a standalone commit has none - so
    // a commit that renders something outside a Working Unit cannot be asked the
    // question at all. Refusing it keeps the WU seam the only door UI work comes
    // through. Deletions count here though they are owed nothing at the WU seam:
    // there the question is which design a file came from, unanswerable for a file
    // that is gone; here it is whether anyone reviewed the change, and removing a
    // screen is as unreviewed as adding one.
    fn check_ui_outside_a_wu(report: &mut Report, names: &[String]) {
        let rendered: Vec<&str> =
            names.iter().filter(|p| crate::doc::ui_surface(p)).map(String::as_str).collect();
        if rendered.is_empty() {
            return;
        }
        report.failures.push(Failure::new(
            DESIGN,
            format!(
                "{} rendered file(s) in a standalone commit: {}",
                rendered.len(),
                rendered.join(", ")
            ),
            "UI work happens inside a Working Unit - the design accounting lives on its cr/v3 note, and a standalone commit has none".to_string(),
        ));
    }

    // What a diff owes the design flow: the surfaces it ADDS, which only a
    // citation answers for, and every rendered file it leaves standing, which a
    // citation or a stated reason answers for. The added set is not a subset of
    // the second - an `@tier` `.ts` is a registry entry that renders nothing - so
    // the two are unioned rather than nested.
    pub fn surfaces(&self, tip: &str, base: &str) -> (Vec<String>, Vec<String>) {
        let added = self.new_surfaces(tip, base);
        let mut owed: Vec<String> = crate::git::changed_names(self.repo, base, tip)
            .unwrap_or_default()
            .into_iter()
            .filter(|path| crate::doc::ui_surface(path))
            .chain(added.iter().cloned())
            .collect();
        owed.sort();
        owed.dedup();
        (added, owed)
    }

    // The other direction of the hand-over: a build answers for everything it
    // renders. An added surface names the design it was built from; anything else
    // names that design or the reason it needed none. A `cr/v2` note carries
    // neither field and cannot be given one after the fact, so - exactly as for
    // the design section - its silence is reported under the schema check the walk
    // drops at seams. This epic's own W5 seams added twenty components under that
    // schema.
    fn check_handoff(&self, report: &mut Report, cr: Option<&CrNote>) {
        let Some(cr) = cr else { return };
        let (added, owed) = self.surfaces(&report.tip, &report.base);
        if owed.is_empty() {
            return;
        }
        let check = if cr.schema == "cr/v3" { DESIGN } else { DESIGN_SCHEMA };
        let fix = format!(
            "account for each on the {} note: design.implements: [{{ path: <the design>, blob: <git rev-parse {}:<the design>>, built: [<the files it produced>] }}], or design.undesigned: [{{ path, why }}] for a file that needed no design - an added surface takes only the citation. `cargo xtask design-scope` lists what the gate is asking about",
            cr.schema,
            short(&report.tip)
        );
        let design = cr.design.as_ref();
        let cited_any = design.and_then(|d| d.implements.as_ref()).is_some_and(|c| !c.is_empty());
        if !cited_any && design.is_none_or(|d| d.undesigned.is_empty()) {
            // DEC-25's distinction, widened: an absent field is a silence and an
            // empty list is a stated nothing, and the red says which happened.
            let stated_nothing = design.is_some_and(|d| d.implements.is_some());
            let (what, hint) = if stated_nothing {
                ("design.implements is empty", "an empty list is a stated nothing")
            } else {
                ("the note accounts for none of them", "a silence is not an answer")
            };
            report.failures.push(Failure::new(
                check,
                format!(
                    "{} rendered file(s) in the diff and {what}: {}",
                    owed.len(),
                    owed.join(", ")
                ),
                format!("{hint} - {fix}"),
            ));
            return;
        }
        for path in &owed {
            let cited = design.is_some_and(|d| d.built().any(|built| built == path));
            let claimed = design.is_some_and(|d| d.claims(path));
            if cited && claimed {
                report.failures.push(Failure::new(
                    check,
                    format!("{path} is cited to a design and declared undesigned"),
                    "one file, one answer - drop whichever of the two is not true".to_string(),
                ));
            } else if added.contains(path) && !cited {
                report.failures.push(Failure::new(
                    check,
                    format!("{path} is a new surface this diff adds, and no citation names it"),
                    format!("only a design answers for a new surface - a reason does not. {fix}"),
                ));
            } else if !cited {
                match design.and_then(|d| d.undesigned_why(path)) {
                    Some(why) => report.waived.push(format!("{check} {path} undesigned: {why}")),
                    None if claimed => report.failures.push(Failure::new(
                        check,
                        format!("{path} is declared undesigned with no reason"),
                        "an empty reason records that a file went undesigned, not why - say what made a design unnecessary".to_string(),
                    )),
                    None => report.failures.push(Failure::new(
                        check,
                        format!("{path} is rendered by this diff and nothing accounts for it"),
                        fix.clone(),
                    )),
                }
            }
        }
        let untouched = |path: &String| !owed.iter().any(|owed| owed == path);
        for citation in design.and_then(|d| d.implements.as_ref()).into_iter().flatten() {
            self.check_citation(report, check, citation);
            for built in citation.built.iter().filter(|b| untouched(b)) {
                report.failures.push(Failure::new(
                    check,
                    format!("{} is cited as built from {}, and this diff does not render it", built, citation.path),
                    "a citation names what this diff produced - `cargo xtask design-scope` lists it".to_string(),
                ));
            }
        }
        for record in design.iter().flat_map(|d| d.undesigned.iter()).filter(|r| untouched(&r.path))
        {
            report.failures.push(Failure::new(
                check,
                format!("{} is declared undesigned, and this diff does not render it", record.path),
                "declare the files the diff actually touches - `cargo xtask design-scope` lists them"
                    .to_string(),
            ));
        }
    }

    // A citation is checked, never trusted: the path resolves at the tip, the
    // blob is the bytes the tip ships, and what it names is a finished design.
    // `check` is the caller's, not this function's: a hand-written `cr/v2` note
    // carrying a citation predates the field either way, and its refusals must be
    // the ones the walk drops at a seam like every other refusal on that note.
    fn check_citation(&self, report: &mut Report, check: &str, cited: &crate::notes::Cited) {
        let (Ok(bytes), Some(blob)) = (
            crate::git::show_bytes(self.repo, &report.tip, &cited.path),
            crate::git::blob_of(self.repo, &report.tip, &cited.path),
        ) else {
            report.failures.push(Failure::new(
                check,
                format!("design.implements names {}, which does not resolve at the tip", cited.path),
                "cite a design the tip actually ships - a path that resolves nowhere is a citation nobody can follow".to_string(),
            ));
            return;
        };
        let short_blob = crate::doc::pin_of(&blob);
        if cited.blob.len() < crate::doc::PIN_MIN {
            report.failures.push(Failure::new(
                check,
                format!(
                    "the citation of {} pins \"{}\", which names no blob",
                    cited.path, cited.blob
                ),
                format!("pin the bytes the build read: blob: {short_blob}"),
            ));
        } else if !crate::doc::pin_matches(&cited.blob, &blob) {
            report.failures.push(Failure::new(
                check,
                format!(
                    "the build cites {} at blob {}; this tip ships {short_blob}",
                    cited.path, cited.blob
                ),
                format!("the design moved after the note was written - re-read it and re-pin: blob: {short_blob}"),
            ));
        }
        // Bytes that do not decode are not a design either: returning here would
        // accept the one citation nobody can read as though Tier-3 had judged it.
        let Ok(html) = String::from_utf8(bytes) else {
            report.failures.push(Failure::new(
                check,
                format!("{} does not decode as text, so nothing can say it is a design", cited.path),
                "cite a design artifact the review could read - a doc that is not UTF-8 was never judged"
                    .to_string(),
            ));
            return;
        };
        if crate::doc::design_artifact(&cited.path, &html).is_some() {
            return;
        }
        let (what, fix) = if crate::doc::design_run_status(&html).is_some() {
            (
                format!("{} is a design run that is not archived", cited.path),
                "an open run cannot be handed to a build - close it (data-status=\"archived\") in the merge that finishes it, then cite it",
            )
        } else {
            (
                format!("{} is not a design artifact", cited.path),
                "cite a closed design run or a component design file - a doc that is neither was never judged by Tier-3",
            )
        };
        report.failures.push(Failure::new(check, what, fix.to_string()));
    }

    fn check_manual_review(&self, report: &mut Report) {
        let why = match report.level {
            Level::Epic => "every epic merge requires manual review",
            Level::Wu => "every working-unit merge requires manual review",
            Level::Commit => return,
        };
        if !crate::mr::tip_authorised(self.repo, &report.tip) {
            report.failures.push(Failure::new(
                "8·manual-review",
                format!("{why} - tip {} has no valid mr/v1 note", short(&report.tip)),
                crate::mr::NOT_AUTHORISED.to_string(),
            ));
            return;
        }
        self.check_attested_report(report);
    }

    // Sign-what-you-read (DEC-10): the mr/v1 note must name the level's result
    // report; at WU level the named bytes must be tracked at the tip,
    // hash-identical. An epic report is archived out of git by gate time
    // (DEC-3), so there its hash is attested by the note's signature alone.
    fn check_attested_report(&self, report: &mut Report) {
        let Some(mr) = note(self.repo, "mr", &report.tip)
            .and_then(|raw| serde_yaml::from_str::<crate::notes::MrNote>(&raw).ok())
        else {
            return;
        };
        let (Some(path), Some(sha)) = (&mr.report, &mr.report_sha256) else {
            report.failures.push(Failure::new(
                "8·manual-review",
                format!("the mr/v1 note on {} names no result report", short(&report.tip)),
                "re-run `cargo xtask mr confirm` with --report <the level's result report>"
                    .to_string(),
            ));
            return;
        };
        if report.level != Level::Wu {
            return;
        }
        match crate::git::show_bytes(self.repo, &report.tip, path) {
            Ok(bytes) if crate::git::sha256_hex(&bytes).ok().as_deref() == Some(sha.as_str()) => {}
            Ok(_) => report.failures.push(Failure::new(
                "8·manual-review",
                format!("{path} at the tip differs from the bytes the mr/v1 note attests"),
                "re-run `cargo xtask mr confirm` against the tip's actual result report"
                    .to_string(),
            )),
            Err(_) => report.failures.push(Failure::new(
                "8·manual-review",
                format!("{path} (the attested result report) is not tracked at the tip"),
                "commit the WU result report on the branch, then re-run `cargo xtask mr confirm`"
                    .to_string(),
            )),
        }
    }

    // No unbuilt design survives its epic (DEC-16's survival clause superseded).
    // `check_retirement` refuses a design file whose component EXISTS; this
    // refuses one whose component never arrived, and between them a component
    // design is never both written and unaccounted for.
    //
    // Read over the range's commits rather than its endpoints: check 6 forces
    // every docs/work/ file out of the tree by the epic tip, so an add and a
    // silent `git rm` net out of an endpoint diff - which is the hole. The
    // roster is read at the tip, because "was it built" is a question about the
    // epic's result, not about any one seam.
    fn check_unbuilt_designs(&self, report: &mut Report) {
        let designed =
            crate::git::added_under(self.repo, &report.base, &report.tip, crate::doc::COMPONENTS);
        if designed.is_empty() {
            return;
        }
        let roster = crate::doc::roster_at(self.repo, &report.tip);
        let cr = self.tip_cr_note(&report.tip);
        let drop_record = |path: &str| {
            cr.as_ref().and_then(|c| c.design.as_ref()).and_then(|d| d.dropped_why(path))
        };
        for path in designed {
            let Some(name) = crate::doc::component_designed(&path) else { continue };
            if roster.contains(&name) {
                continue;
            }
            if let Some(why) = drop_record(&path) {
                report.waived.push(format!("{UNBUILT} {path} dropped: {why}"));
                continue;
            }
            report.failures.push(Failure::new(
                UNBUILT,
                format!("{path} designs {name}, which no component in the roster carries at the epic tip"),
                format!("a design outlives its epic nowhere: build {name} in this epic, or record the drop on the epic's cr note - design.dropped: [{{ path: {path}, why: \"<why it went>\" }}]"),
            ));
        }
    }

    fn check_tracked_docs(&self, report: &mut Report) {
        let tracked = git(self.repo, &["ls-tree", "-r", "--name-only", &report.tip, "--", "docs"])
            .unwrap_or_default();
        for path in tracked.lines() {
            if is_expected_tracked_doc(path) {
                continue;
            }
            let (what, fix) = if path.starts_with("docs/work/") {
                (
                    format!("{path} is an in-flight work doc still tracked at the epic tip"),
                    "distill it, then git rm it and move the file to docs/.archive/ (gitignored)",
                )
            } else {
                (
                    format!("{path} is tracked outside the expected doc locations"),
                    "move it to docs/, docs/use-cases/, docs/decisions/, docs/backlog/ or docs/design-rules/, or archive it to docs/.archive/ (gitignored)",
                )
            };
            report.failures.push(Failure::new("6·doc-note", what, fix.to_string()));
        }
    }

    pub(crate) fn check_commit_trailer(
        &self,
        report: &mut Report,
        commit: &str,
        parent: &str,
    ) -> Result<(), String> {
        let names = diff_names(self.repo, parent, commit)?;
        if !names.iter().any(|p| is_rule_bearing(p)) {
            return Ok(());
        }
        let message = git(self.repo, &["log", "-1", "--format=%B", commit]).unwrap_or_default();
        // A doc commit is authorised by an in-message Reviewed-By trailer — which a
        // key-backed identity must seal with a verifying gpgsig — or, for commits
        // that predate the trailer mechanism, an mr/v1 note added to it.
        let trailer_ok = crate::mr::message_authorised(self.repo, &message)
            && (!crate::mr::trailer_key_backed(self.repo, &message)
                || crate::mr::commit_signed(self.repo, commit));
        if !trailer_ok && !crate::mr::tip_authorised(self.repo, commit) {
            report.exempt = false;
            report.failures.push(Failure::new(
                "8·manual-review",
                format!(
                    "rule-bearing doc commit {} (T0 CLAUDE.md / T1 docs) has no valid {} trailer (a key-backed identity must also sign the commit) or mr/v1 note",
                    short(commit),
                    crate::mr::TRAILER_KEY
                ),
                crate::mr::NOT_AUTHORISED.to_string(),
            ));
        }
        Ok(())
    }

    fn is_ancestor(&self, commit: &str, of: &str) -> bool {
        git(self.repo, &["merge-base", "--is-ancestor", commit, of]).is_ok()
    }

    fn tip_cr_note(&self, tip: &str) -> Option<CrNote> {
        note(self.repo, "cr", tip).and_then(|raw| serde_yaml::from_str::<CrNote>(&raw).ok())
    }

    // A walk seam either fails or, if the tip's cr/v2 note carries a matching
    // walk_waivers entry, is recorded as a visible waiver instead. The waiver is
    // per-seam (keyed by sha) and never touches the tip's own non-walk checks; a
    // still-red seam's fix names the exact, on-the-record waiver edit and warns
    // off --no-verify, which would skip every gate rather than this one seam.
    fn record_seam(
        report: &mut Report,
        seam: &str,
        failures: Vec<Failure>,
        waivers: Option<&CrNote>,
    ) {
        if failures.is_empty() {
            return;
        }
        if let Some(why) = waivers.and_then(|cr| cr.walk_waiver_for(seam)) {
            report.waived.push(format!("7·walk seam {} waived: {why}", short(seam)));
            return;
        }
        let hint = format!(
            "or, if this walk seam is a misfire the walk can't classify, record it on the tip's \
             cr/v2 note: git notes --ref=cr edit {} then add a walk_waivers entry \
             {{ seam: {}, why: \"<why safe>\" }} - do NOT use --no-verify, which skips every gate",
            short(&report.tip),
            short(seam)
        );
        for mut f in failures {
            f.fix = format!("{}; {hint}", f.fix);
            report.failures.push(f);
        }
    }

    fn is_landed_history(&self, wu_tip: &str, mainline: &str, target: &str) -> bool {
        self.is_ancestor(wu_tip, mainline) || self.is_ancestor(wu_tip, target)
    }

    fn walk(&self, report: &mut Report, target: &str) -> Result<(), String> {
        let waivers = self.tip_cr_note(&report.tip);
        let range = format!("{}..{}", report.base, report.tip);
        let lines = git(self.repo, &["rev-list", "--first-parent", "--parents", &range])?;
        for line in lines.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let (seam, seam_failures) = match parts.as_slice() {
                [merge, mainline, wu_tip] => {
                    // --first-parent ordering: parent one is the epic mainline, parent two the
                    // merged-in WU tip. Skip when wu_tip is already landed history rather than a
                    // net-new seam: reachable from the mainline (this epic's own re-merge) or from
                    // the gate target (a back-merge of main / a sibling already on main). A forward
                    // seam always carries commits absent from the target, so this never gates it.
                    if self.is_landed_history(wu_tip, mainline, target) {
                        continue;
                    }
                    let inner = self.verify(Level::Wu, wu_tip, mainline)?;
                    let level = inner.level.as_note_level();
                    // A seam's own waivers are the epic's ledger: every file a
                    // Working Unit declared undesigned surfaces here, so an epic
                    // that shipped UI without designing any of it says so in its
                    // own receipt rather than in nine notes nobody re-reads.
                    report
                        .waived
                        .extend(inner.waived.iter().map(|w| format!("[{level} {}] {w}", short(merge))));
                    // Manual review is boundary-only at seams too (DEC-9/DEC-10):
                    // mr/v1 verifies against the mutable, local .mr-allowed, so
                    // re-judging immutable WU tips would redden authorised history
                    // after key rotation or on a fresh clone. The WU merge hook
                    // gated the seam when it was made; the epic tip's own check 8
                    // still runs.
                    let failures = inner
                        .failures
                        .into_iter()
                        .filter(|f| f.check != "8·manual-review" && f.check != DESIGN_SCHEMA)
                        .map(|f| {
                            Failure::new(
                                &format!("7·walk·{}", f.check),
                                format!("[{level} {}] {}", short(merge), f.what),
                                f.fix,
                            )
                        })
                        .collect();
                    (*merge, failures)
                }
                [commit, first] => {
                    if *commit == report.tip {
                        continue;
                    }
                    // Doc-commit authorisation is boundary-only (commit-msg + pre-push
                    // range): the walk never re-judges immutable history against the
                    // mutable key list, or key rotation would redden authorised commits.
                    let inner = self.verify(Level::Commit, commit, first)?;
                    let level = inner.level.as_note_level();
                    let failures = inner
                        .failures
                        .into_iter()
                        .map(|f| {
                            Failure::new(
                                &format!("7·walk·{}", f.check),
                                format!("[{level} {}] {}", short(commit), f.what),
                                f.fix,
                            )
                        })
                        .collect();
                    (*commit, failures)
                }
                [merge, _, _, _, ..] => (
                    *merge,
                    vec![Failure::new(
                        "7·walk",
                        format!(
                            "{} is an octopus merge - parents beyond the second were never gated",
                            short(merge)
                        ),
                        "merge one WU per merge commit; rebuild this seam as sequential --no-ff merges"
                            .to_string(),
                    )],
                ),
                _ => continue,
            };
            Self::record_seam(report, seam, seam_failures, waivers.as_ref());
        }
        Ok(())
    }

    pub fn write_receipt(&self, report: &Report) -> Result<String, String> {
        let mut body = String::from("schema: dod/v1\n");
        let mut line = |text: String| {
            body.push_str(&text);
            body.push('\n');
        };
        line(format!("level: {}", report.level.as_note_level()));
        if let Some(epic) = &report.epic {
            line(format!("epic: {epic}"));
        }
        if let Some(wu) = &report.wu {
            line(format!("wu: {wu}"));
        }
        if report.tip_ref != report.tip {
            line(format!("branch: {}", report.tip_ref));
        }
        line(format!("base: {}", report.base));
        line(format!("tip: {}", report.tip));
        line(format!("diff_sha256: {}", report.diff_sha256));
        let checks = match report.level {
            Level::Epic => "[notes, manifest, review, review-base, diff-binding, doc-note, design, components-unbuilt, manual-review, walk]",
            _ => "[notes, manifest, review, review-base, diff-binding, doc-note, design, manual-review]",
        };
        line(format!("checks: {checks}"));
        line(format!("date: {}", today()));
        write_note(self.repo, "dod", &report.tip, &body)?;
        Ok(body)
    }
}

fn design_artifacts_of(docs: &[(String, String)]) -> Vec<String> {
    docs.iter()
        .filter(|(path, html)| crate::doc::design_artifact(path, html).is_some())
        .map(|(path, _)| path.clone())
        .collect()
}

// Closing a design run was nobody's step: a run was archived as a side effect
// of the NEXT run being written, so run #3 - which had no next run - stayed
// open and red in the tree for two Working Units. The merge is where that
// stops being invisible, because a run the diff touched is a run this seam is
// answerable for. A commit mid-design is not: a run is legitimately open, and
// red, for as long as it is being designed.
fn check_runs_closed(report: &mut Report, docs: &[(String, String)]) {
    for (path, html) in docs {
        let Some(status) = crate::doc::design_run_status(html) else { continue };
        if status.is_empty() {
            report.failures.push(Failure::new(
                "6·run-closed",
                format!("{path} is a design run left open at the tip"),
                "close the run when it finishes: archive it (data-status=\"archived\" beside its data-artifact), which is what keeps a later rule from re-opening it - a run still mid-design across a merge declares data-status=\"draft\" instead".to_string(),
            ));
        }
    }
}

fn check_manifest(report: &mut Report, test: Option<&TestNote>) {
    let Some(test) = test else { return };
    let expected = report.level.as_note_level();
    if test.level != expected {
        report.failures.push(Failure::new(
            "2·manifest",
            format!("test note level is {}, this gate needs {expected}", test.level),
            format!("set level: {expected} in the test/v2 note, or run the matching gate"),
        ));
        return;
    }
    let layer = report.level.required_layer();
    let ok = test.tests.layer(layer).is_some_and(super::notes::Layer::satisfied);
    if !ok {
        let found = test
            .tests
            .layer(layer)
            .map_or_else(|| "layer absent".to_string(), super::notes::Layer::describe);
        report.failures.push(Failure::new(
            "2·manifest",
            format!("level {expected} needs {layer} >= 1 or a waiver - found {found}"),
            format!(
                "add {layer} test(s) and refresh the note, or record {layer}: {{ waived: \"<why>\" }}"
            ),
        ));
    }
}

fn check_review(report: &mut Report, cr: Option<&CrNote>) {
    let Some(cr) = cr else { return };
    if cr.verdict != "PASS" {
        report.failures.push(Failure::new(
            "3·review",
            format!("review verdict is {}", cr.verdict),
            "fix the verified findings and re-run the review until PASS".to_string(),
        ));
    }
    if cr.base != report.base {
        report.failures.push(Failure::new(
            "4·review-base",
            format!(
                "review base {} != current merge-base {}",
                short(&cr.base),
                short(&report.base)
            ),
            "the target moved since the review - re-run the review against the current base"
                .to_string(),
        ));
    }
    if cr.diff_sha256 != report.diff_sha256 {
        report.failures.push(Failure::new(
            "5·diff-binding",
            "reviewed diff is not byte-identical to the diff being gated".to_string(),
            "the tip changed after review - re-run the review on this exact tip".to_string(),
        ));
    }
}

fn check_identity(report: &mut Report, ids: &[Option<(Option<String>, Option<String>)>]) {
    let present: Vec<&(Option<String>, Option<String>)> = ids.iter().flatten().collect();
    if present.windows(2).any(|w| w[0] != w[1]) {
        report.failures.push(Failure::new(
            "1·notes",
            "identity block disagrees across the tip's notes".to_string(),
            "make epic/wu identical in test/v2, cr/v2 and doc/v1".to_string(),
        ));
    }
}

pub fn today() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(String::new, |o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn short(sha: &str) -> &str {
    sha.get(..10).unwrap_or(sha)
}

pub fn format_report(report: &Report) -> String {
    use std::fmt::Write as _;
    if report.exempt {
        return format!(
            "verify-dod {} exempt on {} - docs/infra-only, gate not required",
            report.level.as_note_level(),
            short(&report.tip)
        );
    }
    let mut out = String::new();
    for w in &report.waived {
        let _ = writeln!(out, "WAIVE {w}");
    }
    if report.green() {
        let _ = write!(
            out,
            "verify-dod {} green on {}",
            report.level.as_note_level(),
            short(&report.tip)
        );
        return out;
    }
    for f in &report.failures {
        let _ = writeln!(out, "FAIL  {:<24} {}", f.check, f.what);
        let _ = writeln!(out, "      fix -> {}", f.fix);
    }
    let _ = write!(
        out,
        "{} failed -> no receipt written; the merge gate will refuse this tip",
        report.failures.len()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{format_report, short, CrNote, Failure, Gate, Level, Report};

    fn report(failures: Vec<Failure>) -> Report {
        Report {
            level: Level::Wu,
            tip_ref: "wu/x/y".to_string(),
            tip: "abcdef012345".to_string(),
            base: "0123456789ab".to_string(),
            diff_sha256: "cafe".to_string(),
            epic: None,
            wu: None,
            exempt: false,
            failures,
            waived: Vec::new(),
        }
    }

    #[test]
    fn exempt_report_says_so_and_never_shows_failures() {
        let mut r = report(vec![]);
        r.exempt = true;
        assert!(format_report(&r).contains("exempt"));
        assert!(format_report(&r).contains("gate not required"));
    }

    #[test]
    fn green_report_is_one_line() {
        let r = report(vec![]);
        assert!(r.green());
        assert_eq!(format_report(&r), "verify-dod working-unit green on abcdef0123");
    }

    #[test]
    fn failures_render_check_what_and_fix() {
        let r = report(vec![Failure {
            check: "2·manifest".to_string(),
            what: "level working-unit needs integration >= 1".to_string(),
            fix: "add the test".to_string(),
        }]);
        let text = format_report(&r);
        assert!(text.contains("FAIL  2·manifest"));
        assert!(text.contains("fix -> add the test"));
        assert!(text.contains("1 failed"));
    }

    #[test]
    fn short_handles_small_ids() {
        assert_eq!(short("abc"), "abc");
        assert_eq!(short("0123456789abcdef"), "0123456789");
    }

    const PILL: &str = "ui/src/components/StatusPill.tsx";
    const PILL_DESIGN: &str = "docs/work/components/status-pill.html";

    struct Built {
        _dir: tempfile::TempDir,
        repo: std::path::PathBuf,
        base: String,
        tip: String,
        later: String,
        blob: String,
    }

    // One repo, two ranges, because the rule has two halves. `base..tip` ADDS the
    // pill, with a test sibling, a `.ts` sibling and an untouched component beside
    // it; `tip..later` only re-shapes it, and deletes another surface outright.
    fn a_repo_that_builds_then_re_shapes_a_pill() -> Built {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().to_path_buf();
        let g = |args: &[&str]| super::git(&repo, args).unwrap();
        let write = |rel: &str, body: &str| {
            let full = repo.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, body).unwrap();
        };
        let commit = |msg: &str| {
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", msg]);
            g(&["rev-parse", "HEAD"])
        };

        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        write("src/lib.rs", "pub fn base() {}\n");
        write("ui/src/components/Section.tsx", "/**\n * Section.\n * @tier T2\n */\n");
        let base = commit("base");

        write(PILL_DESIGN, "<html>the StatusPill contract</html>\n");
        write("docs/work/plan.html", "<html>a plan, not a design</html>\n");
        write(
            "docs/work/run-5.html",
            "<html><body><main data-artifact=\"design-run\">open</main></body></html>\n",
        );
        write(
            PILL,
            "/**\n * StatusPill.\n * @tier T1\n */\nexport const StatusPill = () => null;\n",
        );
        write("ui/src/components/StatusPill.test.tsx", "test('x', () => {});\n");
        write("ui/src/Section.notes.ts", "export const notes = 1;\n");
        let tip = commit("build the pill");
        let blob = g(&["rev-parse", &format!("{tip}:{PILL_DESIGN}")]);

        write(PILL, "/**\n * StatusPill, revised.\n * @tier T1\n */\n");
        std::fs::remove_file(repo.join("ui/src/components/Section.tsx")).unwrap();
        let later = commit("revise the pill, retire Section");

        Built { _dir: dir, repo, base, tip, later, blob }
    }

    fn note_yaml(schema: &str, base: &str, design: &str) -> String {
        format!("schema: {schema}\nbase: {base}\ndiff_sha256: x\nverdict: PASS\n{design}")
    }

    fn implements(path: &str, blob: &str, built: &str) -> String {
        format!(
            "design:\n  verdict: PASS\n  implements:\n    - path: {path}\n      blob: {blob}\n      built: [{built}]\n"
        )
    }

    fn undesigned(path: &str, why: &str) -> String {
        format!(
            "design:\n  verdict: PASS\n  undesigned:\n    - path: {path}\n      why: \"{why}\"\n"
        )
    }

    fn handoff(built: &Built, from: &str, to: &str, note: &str) -> (Vec<Failure>, Vec<String>) {
        let cr: CrNote = serde_yaml::from_str(note).expect("the note parses");
        let mut r = report(vec![]);
        r.base = from.to_string();
        r.tip = to.to_string();
        Gate { repo: &built.repo }.check_handoff(&mut r, Some(&cr));
        (r.failures, r.waived)
    }

    // Every way a build can fail to name what it built from, and the one way it
    // succeeds. The surface is a real added file at a real tip, so the trigger and
    // the citation checks are driven together.
    #[test]
    fn a_build_that_adds_a_surface_cites_a_design_the_tip_ships_or_reds() {
        let built = a_repo_that_builds_then_re_shapes_a_pill();
        let (base, tip, blob) = (&built.base, &built.tip, &built.blob);
        let gate = Gate { repo: &built.repo };
        assert_eq!(
            gate.new_surfaces(tip, base),
            vec![PILL.to_string()],
            "a @tier component added; its test, a .ts sibling and the untouched Section are not surfaces"
        );
        let checked = |section: &str| handoff(&built, base, tip, section).0;
        let v3 = |section: &str| note_yaml("cr/v3", base, section);

        let silent = checked(&v3(""));
        assert_eq!(silent.len(), 1);
        assert_eq!(silent[0].check, super::DESIGN, "a v3 note has no excuse");
        assert_eq!(
            checked(&note_yaml("cr/v2", base, ""))[0].check,
            super::DESIGN_SCHEMA,
            "a v2 note predates the field, so the walk can drop it at a seam"
        );

        // The silence names the surface; a stated nothing is not that silence;
        // then a path nobody can follow, bytes the tip does not ship, a pin too
        // short to be a blob, a doc Tier-3 never judged, and an open run. Then the
        // three ways an accounting contradicts itself.
        let cite = |path: &str, pin: &str| implements(path, pin, PILL);
        for (section, red) in [
            (String::new(), "StatusPill.tsx"),
            ("design:\n  verdict: PASS\n  implements: []\n".to_string(), "empty"),
            (cite("docs/work/gone.html", blob), "does not resolve"),
            (cite(PILL_DESIGN, "0000000"), "this tip ships"),
            (cite(PILL_DESIGN, "abc"), "names no blob"),
            (cite("docs/work/plan.html", blob), "not a design artifact"),
            (cite("docs/work/run-5.html", blob), "not archived"),
            (undesigned(PILL, "not this time"), "new surface this diff adds"),
            (implements(PILL_DESIGN, blob, "ui/src/components/Ghost.tsx"), "does not render it"),
            (
                format!(
                    "{}  undesigned:\n    - path: {PILL}\n      why: also\n",
                    cite(PILL_DESIGN, blob)
                ),
                "cited to a design and declared undesigned",
            ),
        ] {
            let what: Vec<String> = checked(&v3(&section)).iter().map(|f| f.what.clone()).collect();
            assert!(what.iter().any(|w| w.contains(red)), "expected {red}, got {what:?}");
        }

        assert!(
            checked(&v3(&cite(PILL_DESIGN, &blob[..7]))).is_empty(),
            "path resolves, blob matches, it is a design, and the citation names what it built"
        );
    }

    // The half an added-only trigger could never see: the same file, modified
    // rather than added. A design is never demanded of it, an answer always is,
    // and the file this commit deleted is owed neither.
    #[test]
    fn a_build_that_only_re_shapes_a_surface_states_why_it_needed_no_design() {
        let built = a_repo_that_builds_then_re_shapes_a_pill();
        let (tip, later) = (&built.tip, &built.later);
        assert!(
            Gate { repo: &built.repo }.new_surfaces(later, tip).is_empty(),
            "the old trigger is about adding a surface - an edit to one and a deletion are not"
        );
        let edited = |section: &str| handoff(&built, tip, later, &note_yaml("cr/v3", tip, section));

        let (reds, _) = edited("");
        assert_eq!(reds.len(), 1, "one rendered file left standing, one red: {reds:?}");
        assert!(reds[0].what.contains(PILL) && !reds[0].what.contains("Section.tsx"));

        let (reds, waived) = edited(&undesigned(PILL, "a doc-comment fix, nothing rendered moved"));
        assert!(reds.is_empty(), "a stated reason answers for an edit: {reds:?}");
        assert_eq!(waived.len(), 1, "and it is billed, so the epic sees what shipped undesigned");
        assert!(waived[0].contains("undesigned: a doc-comment fix"), "{waived:?}");

        let (reds, _) = edited(&undesigned(PILL, ""));
        assert!(reds[0].what.contains("no reason"), "an empty why is not a reason: {reds:?}");

        let what: Vec<String> = edited(&undesigned("ui/src/components/Ghost.tsx", "never touched"))
            .0
            .iter()
            .map(|f| f.what.clone())
            .collect();
        assert!(what.iter().any(|w| w.contains("does not render it")), "{what:?}");
        assert!(what.iter().any(|w| w.contains(PILL)), "and the real file is still owed: {what:?}");
    }

    // The other door UI work could come through, and the one the accounting
    // cannot reach: a commit with no Working Unit, and so no cr note to answer on.
    #[test]
    fn a_standalone_commit_may_not_render_anything() {
        let mut r = report(vec![]);
        Gate::check_ui_outside_a_wu(
            &mut r,
            &["src/lib.rs".to_string(), "ui/src/api.ts".to_string()],
        );
        assert!(r.failures.is_empty(), "logic and Rust are not rendered files");

        let mut r = report(vec![]);
        Gate::check_ui_outside_a_wu(&mut r, &["ui/src/FlowDetail.tsx".to_string()]);
        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].check, super::DESIGN);
        assert!(r.failures[0].fix.contains("inside a Working Unit"));
    }

    // The topology an epic walk actually sees, and the one this check exists for:
    // a design added AND removed inside one Working Unit branch, then merged. That
    // merge is TREESAME for the path, so git's default pathspec simplification
    // follows the first parent straight past it and never visits the add - the
    // range walk goes quiet on exactly the case an endpoint diff was rejected for.
    #[test]
    fn a_design_added_and_removed_inside_one_branch_still_reds_the_epic() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::create_dir_all(repo.join("docs/work/components")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn base() {}\n").ok();
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "base"]);
        let base = g(&["rev-parse", "HEAD"]);

        g(&["branch", "epic/x"]);
        g(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
        let design = "docs/work/components/status-pill.html";
        std::fs::write(repo.join(design), "<html>the StatusPill contract</html>\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "design the pill"]);
        g(&["rm", "-q", design]);
        g(&["commit", "-q", "-m", "archive it out of git"]);
        g(&["checkout", "-q", "epic/x"]);
        g(&["merge", "-q", "--no-ff", "wu/x/one", "-m", "Merge the WU"]);

        let mut r = report(vec![]);
        r.base = base;
        r.tip = g(&["rev-parse", "HEAD"]);
        Gate { repo }.check_unbuilt_designs(&mut r);
        assert_eq!(
            r.failures.iter().map(|f| f.check.as_str()).collect::<Vec<_>>(),
            vec![super::UNBUILT],
            "a design the range added and buried is still a design the epic must answer for"
        );
        assert!(r.failures[0].what.contains("StatusPill"), "{}", r.failures[0].what);
    }

    // Membership at a revision, over a real tree. Only a component pays for its
    // bytes, and nothing that renders may escape by being unreadable or nested.
    #[test]
    fn a_screen_is_a_surface_by_its_path_alone_however_its_bytes_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        let write = |rel: &str, body: &[u8]| {
            let full = repo.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, body).unwrap();
        };
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        write("src/lib.rs", b"pub fn base() {}\n");
        let base = {
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", "base"]);
            g(&["rev-parse", "HEAD"])
        };

        write("ui/src/Onboarding.tsx", b"export const Onboarding = () => null;\n");
        // A screen carrying one byte no UTF-8 decoder accepts. Membership must not
        // depend on the read: a decode that fails reads as "not a surface", which
        // would let a stray byte buy a new screen its way out of citing a design.
        write("ui/src/Mojibake.tsx", b"export const M = () => null; // \xe9\n");
        write("ui/src/components/status/StatusPill.tsx", b"/**\n * @tier T1\n */\n");
        write("ui/src/components/Button.tsx", b"/**\n * @tier T1\n */\n");
        write("ui/src/components/helper.tsx", b"export const helper = () => null;\n");
        write("ui/src/components/index.ts", b"export * from './Button';\n");
        write("ui/src/api.ts", b"export const get = () => null;\n");
        write("ui/src/Onboarding.test.tsx", b"test('x', () => {});\n");
        let tip = {
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", "one of everything"]);
            g(&["rev-parse", "HEAD"])
        };

        assert_eq!(
            Gate { repo }.new_surfaces(&tip, &base),
            vec![
                "ui/src/Mojibake.tsx".to_string(),
                "ui/src/Onboarding.tsx".to_string(),
                "ui/src/components/Button.tsx".to_string(),
                "ui/src/components/status/StatusPill.tsx".to_string(),
            ],
            "screens by path - nested or undecodable - and only the tagged component"
        );
        assert_eq!(
            crate::doc::roster_at(repo, &tip),
            ["Button", "Mojibake", "Onboarding", "StatusPill"]
                .into_iter()
                .map(String::from)
                .collect(),
            "the roster answers the same question, through the same function"
        );
    }

    // The refusal only the citation path can produce, and the schema tagging that
    // decides whether the walk may drop it. A design whose bytes do not decode was
    // being accepted in silence - the one citation nobody can read, passing as
    // though Tier-3 had judged it.
    #[test]
    fn a_citation_nobody_can_read_is_refused_under_its_own_note_schema() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::create_dir_all(repo.join("docs/work/components")).unwrap();
        std::fs::create_dir_all(repo.join("ui/src/components")).unwrap();
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "base"]);
        let base = g(&["rev-parse", "HEAD"]);

        std::fs::write(repo.join(PILL_DESIGN), b"<html>the contract \xff</html>\n").unwrap();
        std::fs::write(repo.join(PILL), "/**\n * StatusPill.\n * @tier T1\n */\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "build it against an undecodable contract"]);
        let tip = g(&["rev-parse", "HEAD"]);
        let blob = g(&["rev-parse", &format!("{tip}:{PILL_DESIGN}")]);

        let judged = |schema: &str| {
            let cr: CrNote = serde_yaml::from_str(&note_yaml(
                schema,
                &base,
                &implements(PILL_DESIGN, &blob, PILL),
            ))
            .unwrap();
            let mut r = report(vec![]);
            r.base.clone_from(&base);
            r.tip.clone_from(&tip);
            Gate { repo }.check_handoff(&mut r, Some(&cr));
            r.failures
        };

        let reds = judged("cr/v3");
        assert_eq!(reds.len(), 1, "{reds:?}");
        assert!(reds[0].what.contains("does not decode"), "{}", reds[0].what);
        assert_eq!(reds[0].check, super::DESIGN);
        assert_eq!(
            judged("cr/v2")[0].check,
            super::DESIGN_SCHEMA,
            "a note that predates the field is grandfathered through every refusal it can raise, \
             not through all but this one"
        );
    }

    // A rename is not a new surface. The component it renames is already built,
    // so DEC-16 has retired the design a citation would have to name - treating
    // the destination as added would demand an artifact the rules guarantee is
    // gone, and no honest note could be written. It belongs in the touched tier,
    // where a reason is the answer.
    #[test]
    fn renaming_a_built_component_is_answerable_rather_than_unmergeable() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::create_dir_all(repo.join("ui/src/components")).unwrap();
        std::fs::write(
            repo.join(PILL),
            "/**\n * StatusPill.\n * @tier T1\n */\nexport const StatusPill = () => null;\n",
        )
        .unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "the pill, built and its design long retired"]);
        let base = g(&["rev-parse", "HEAD"]);

        let renamed = "ui/src/components/StatusBadge.tsx";
        g(&["mv", PILL, renamed]);
        g(&["commit", "-q", "-m", "rename it"]);
        let tip = g(&["rev-parse", "HEAD"]);

        // The other half of the same question. Git pairs on similarity alone, so
        // a new screen scaffolded from a deleted file it happens to resemble is
        // reported as a rename too - and that one owes a design like any other.
        let padding = "one line of boilerplate padding\n".repeat(60);
        std::fs::write(repo.join("filler.txt"), &padding).unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "an unrelated file"]);
        let before_scaffold = g(&["rev-parse", "HEAD"]);
        g(&["rm", "-q", "filler.txt"]);
        std::fs::write(
            repo.join("ui/src/NewScreen.tsx"),
            format!("export const NewScreen = () => null;\n/*\n{padding}*/\n"),
        )
        .unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "scaffold a screen out of it"]);
        let scaffolded = g(&["rev-parse", "HEAD"]);
        assert_eq!(
            Gate { repo }.new_surfaces(&scaffolded, &before_scaffold),
            vec!["ui/src/NewScreen.tsx".to_string()],
            "git calls it a rename; it renames nothing that was ever a surface, so it is added"
        );

        let gate = Gate { repo };
        assert!(
            gate.new_surfaces(&tip, &base).is_empty(),
            "a rename adds nothing: {:?}",
            gate.new_surfaces(&tip, &base)
        );
        let (added, owed) = gate.surfaces(&tip, &base);
        assert!(added.is_empty());
        assert_eq!(owed, vec![renamed.to_string()], "and the new name is still owed an answer");

        let note: CrNote = serde_yaml::from_str(&note_yaml(
            "cr/v3",
            &base,
            &undesigned(renamed, "renamed; the component and its contract are unchanged"),
        ))
        .unwrap();
        let mut r = report(vec![]);
        r.base.clone_from(&base);
        r.tip.clone_from(&tip);
        gate.check_handoff(&mut r, Some(&note));
        assert!(r.failures.is_empty(), "a stated reason lands the rename: {:?}", r.failures);
    }

    // The four ways an epic can answer for a component design it wrote: built,
    // unbuilt, dropped with a reason, dropped with none. The design file is
    // archived out of the tree first, as check 6 demands - so a check reading
    // the endpoints instead of the range would see nothing at all here.
    #[test]
    fn an_unbuilt_component_design_reds_the_epic_unless_a_reason_is_recorded() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        let write = |rel: &str, body: &str| {
            let full = repo.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, body).unwrap();
        };
        let commit = |msg: &str| {
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", msg]);
            g(&["rev-parse", "HEAD"])
        };

        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        write("src/lib.rs", "pub fn base() {}\n");
        let base = commit("base");

        let design = "docs/work/components/status-pill.html";
        write(design, "<html>the StatusPill contract</html>\n");
        commit("design the pill");
        std::fs::remove_file(repo.join(design)).unwrap();
        let unbuilt = commit("archive the design out of git");

        let gate = Gate { repo };
        let checked = |tip: &str| {
            let mut r = report(vec![]);
            r.base.clone_from(&base);
            r.tip = tip.to_string();
            gate.check_unbuilt_designs(&mut r);
            r
        };

        let red = checked(&unbuilt);
        assert_eq!(
            red.failures.iter().map(|f| f.check.as_str()).collect::<Vec<_>>(),
            vec![super::UNBUILT],
            "a design whose component never arrived refuses the epic"
        );
        assert!(red.failures[0].what.contains("StatusPill"), "{}", red.failures[0].what);

        let note = |body: &str| {
            g(&["notes", "--ref=cr", "add", "-f", "-m", body, &unbuilt]);
        };
        let cr = |why: &str| {
            format!(
                "schema: cr/v3\nbase: {base}\ndiff_sha256: x\nverdict: PASS\ndesign:\n  \
                 rulebook: 796c547\n  verdict: PASS\n  dropped:\n    - path: {design}\n      why: \"{why}\"\n"
            )
        };

        note(&cr("  "));
        assert_eq!(checked(&unbuilt).failures.len(), 1, "a drop with no reason is not a drop");

        note(&cr("never built; the row renders its own status"));
        let dropped = checked(&unbuilt);
        assert!(dropped.failures.is_empty(), "a recorded drop greens it");
        assert!(dropped.waived[0].contains("never built"), "and lands in the waiver bill");

        write(
            "ui/src/components/StatusPill.tsx",
            "/**\n * StatusPill.\n * @tier T1 — primitive\n */\nexport const StatusPill = () => null;\n",
        );
        let built = commit("build the pill");
        assert!(checked(&built).failures.is_empty(), "a built component needs no drop record");

        // A screen design resolves where a screen was built. Before the roster
        // counted screens, designing one cost a drop record while designing
        // nothing cost nothing - the epic gate paid you to skip the design.
        let screen_design = "docs/work/components/body-viewer.html";
        write(screen_design, "<html>the BodyViewer contract</html>\n");
        commit("design the viewer");
        std::fs::remove_file(repo.join(screen_design)).unwrap();
        let designed = commit("archive the viewer design");
        assert!(
            checked(&designed).failures[0].what.contains("BodyViewer"),
            "a screen design with no screen is as unbuilt as any other"
        );

        write("ui/src/BodyViewer.tsx", "export const BodyViewer = () => null;\n");
        let screen = commit("build the viewer as a screen, outside the collection");
        assert!(
            checked(&screen).failures.is_empty(),
            "the screen carries the contract; no @tier tag, and none is owed"
        );
    }

    // The discriminator's whole point is the divergent case: main and the epic
    // each advanced past their common base, so neither tip contains the other.
    // A back-merged main tip is still landed history (⊑ target); a forward WU
    // tip carrying epic-only work is not.
    #[test]
    fn is_landed_history_recognises_target_reachable_seams_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        let commit = |name: &str, msg: &str| {
            std::fs::write(repo.join(name), msg).unwrap();
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", msg]);
            g(&["rev-parse", "HEAD"])
        };

        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        commit("base", "base");
        g(&["branch", "epic"]);

        let main_tip = commit("m", "main advances past the fork");

        g(&["checkout", "-q", "epic"]);
        let epic_mainline = g(&["rev-parse", "HEAD"]);
        let forward_tip = commit("e", "epic-only work, absent from main");

        let gate = Gate { repo };
        assert!(
            gate.is_landed_history(&main_tip, &epic_mainline, "main"),
            "a diverged main tip back-merged in is landed history (⊑ target)"
        );
        assert!(
            !gate.is_landed_history(&forward_tip, &epic_mainline, "main"),
            "a forward WU tip carrying epic-only work is a real seam, never landed history"
        );
    }
}
