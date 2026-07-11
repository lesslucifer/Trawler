use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::git::{diff_names, diff_sha256, git, note, write_note};
use crate::notes::{
    is_code, is_doc, is_expected_tracked_doc, is_ui, tier_of, CrNote, DocNote, TestNote,
};

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
        let mut report = Report {
            level,
            tip_ref: tip_ref.to_string(),
            tip,
            base,
            diff_sha256: sha,
            epic: None,
            wu: None,
            exempt: !code_changed,
            failures: Vec::new(),
            waived: Vec::new(),
        };
        if level == Level::Epic {
            self.check_tracked_docs(&mut report);
        }
        self.check_manual_review(&mut report, &names);
        report.exempt = report.exempt && report.failures.is_empty();
        if !code_changed {
            return Ok(report);
        }
        let parsed = self.load_notes(&mut report, &names);
        check_manifest(&mut report, parsed.test.as_ref());
        if level != Level::Commit {
            check_review(&mut report, parsed.cr.as_ref());
        }
        Self::check_docs(&mut report, parsed.doc.as_ref(), &names);
        if level == Level::Epic {
            self.walk(&mut report, target_ref)?;
        }
        Ok(report)
    }

    fn load_notes(&self, report: &mut Report, names: &[String]) -> ParsedNotes {
        let tip = report.tip.clone();
        let doc_needed = report.level != Level::Commit || names.iter().any(|p| is_doc(p));
        let test = self
            .parse_note::<TestNote>(report, &tip, "test", "test/v2", true, |n| n.schema.clone());
        let cr = if report.level == Level::Commit {
            None
        } else {
            self.parse_note::<CrNote>(report, &tip, "cr", "cr/v2", true, |n| n.schema.clone())
        };
        let doc = self
            .parse_note::<DocNote>(report, &tip, "doc", "doc/v1", doc_needed, |n| n.schema.clone());
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

    fn parse_note<T: serde::de::DeserializeOwned>(
        &self,
        report: &mut Report,
        tip: &str,
        note_ref: &str,
        schema: &str,
        required: bool,
        schema_of: impl Fn(&T) -> String,
    ) -> Option<T> {
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
                if found == schema {
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

    fn check_docs(report: &mut Report, doc: Option<&DocNote>, names: &[String]) {
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
                        "update docs/use-cases|decisions|backlog, or record t2_waived: \"<why>\""
                            .to_string(),
                    ));
                }
            }
        }
    }

    fn check_manual_review(&self, report: &mut Report, names: &[String]) {
        let why = match report.level {
            Level::Epic => "every epic merge requires manual review",
            Level::Wu if names.iter().any(|p| is_ui(p)) => {
                "this WU touches UI and requires manual review"
            }
            _ => return,
        };
        if !crate::mr::tip_authorised(self.repo, &report.tip) {
            report.failures.push(Failure::new(
                "8·manual-review",
                format!("{why} - tip {} has no valid mr/v1 note", short(&report.tip)),
                crate::mr::NOT_AUTHORISED.to_string(),
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
                    "move it to docs/, docs/use-cases/, docs/decisions/ or docs/backlog/, or archive it to docs/.archive/ (gitignored)",
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
        if !names.iter().any(|p| is_doc(p)) {
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
                    "doc commit {} has no valid {} trailer (a key-backed identity must also sign the commit) or mr/v1 note",
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

    fn tip_walk_waivers(&self, tip: &str) -> Option<CrNote> {
        note(self.repo, "cr", tip).and_then(|raw| serde_yaml::from_str::<CrNote>(&raw).ok())
    }

    // A walk seam either fails or, if the tip's cr/v2 note carries a matching
    // walk_waivers entry, is recorded as a visible waiver instead. The waiver is
    // per-seam (keyed by sha) and never touches the tip's own non-walk checks; a
    // still-red seam's fix names the exact, on-the-record waiver edit and warns
    // off --no-verify, which would skip every gate rather than this one seam.
    fn record_seam(
        &self,
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
        let waivers = self.tip_walk_waivers(&report.tip);
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
                    let failures = inner
                        .failures
                        .into_iter()
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
                    self.check_commit_trailer(report, commit, first)?;
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
            self.record_seam(report, seam, seam_failures, waivers.as_ref());
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
            Level::Epic => {
                "[notes, manifest, review, review-base, diff-binding, doc-note, manual-review, walk]"
            }
            _ => "[notes, manifest, review, review-base, diff-binding, doc-note, manual-review]",
        };
        line(format!("checks: {checks}"));
        line(format!("date: {}", today()));
        write_note(self.repo, "dod", &report.tip, &body)?;
        Ok(body)
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
    use super::{format_report, short, Failure, Gate, Level, Report};

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
