use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct TestNote {
    pub schema: String,
    pub level: String,
    #[serde(default)]
    pub epic: Option<String>,
    #[serde(default)]
    pub wu: Option<String>,
    pub tests: Tests,
}

#[derive(Deserialize, Default)]
pub struct Tests {
    #[serde(default)]
    pub unit: Option<Layer>,
    #[serde(default)]
    pub integration: Option<Layer>,
    #[serde(default)]
    pub e2e: Option<Layer>,
}

impl Tests {
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        match name {
            "unit" => self.unit.as_ref(),
            "integration" => self.integration.as_ref(),
            "e2e" => self.e2e.as_ref(),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Layer {
    Counted {
        count: u32,
        #[serde(default)]
        covers: Option<String>,
    },
    Waived {
        waived: String,
    },
}

impl Layer {
    pub fn satisfied(&self) -> bool {
        match self {
            Self::Counted { count, .. } => *count >= 1,
            Self::Waived { waived } => !waived.trim().is_empty(),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Counted { count, .. } => format!("count {count}"),
            Self::Waived { waived } if waived.trim().is_empty() => "empty waiver".to_string(),
            Self::Waived { .. } => "waived".to_string(),
        }
    }
}

/// The review contracts the gate reads. `cr/v3` added the design dimension's
/// section, `cr/v4` binds the code paths the review judged rather than the whole
/// diff - so a document cannot void a PASS - and `cr/v5` records what the round
/// spent under `fleet:`; an older note is immutable evidence, and the tip that
/// declared one is still judged against what it declared.
pub const CR_SCHEMAS: &[&str] = &["cr/v2", "cr/v3", "cr/v4", "cr/v5"];

/// The contract a review written today declares — the newest `CR_SCHEMAS` entry.
/// Anything telling a reader which note to go and write names it from here, so a
/// schema bump moves one constant instead of every sentence quoting it.
pub fn cr_schema() -> &'static str {
    CR_SCHEMAS.last().copied().unwrap_or_default()
}

/// Whether a review contract binds the code paths the diff changes, or the whole
/// diff. The whole-diff contracts are a closed pair: every bump since `cr/v4`
/// keeps the code-path binding, so a newer schema is read as code-bound without
/// this function learning its name.
pub fn binds_code(schema: &str) -> bool {
    !matches!(schema, "cr/v2" | "cr/v3")
}

/// The design dimension's own contract, now that its verdict travels on a note of
/// its own: a diff can carry a design artifact and change no code, and such a diff
/// owes the verdict without owing a review.
pub const DESIGN_SCHEMAS: &[&str] = &["design/v1"];

#[derive(Deserialize)]
pub struct CrNote {
    pub schema: String,
    #[serde(default)]
    pub epic: Option<String>,
    #[serde(default)]
    pub wu: Option<String>,
    pub base: String,
    pub diff_sha256: String,
    pub verdict: String,
    /// The verified findings the verdict no longer fails on, each with what was
    /// decided about it. Absent on every note written before the severity rule,
    /// which is exactly what an empty list means.
    #[serde(default)]
    pub nits: Vec<Nit>,
    #[serde(default)]
    pub walk_waivers: Vec<WalkWaiver>,
    #[serde(default)]
    pub design: Option<Design>,
    /// The round's own bill. Absent on every note written before `cr/v5`, and
    /// read as a round that did not count itself - never as one that cost nothing.
    #[serde(default)]
    pub fleet: Option<Fleet>,
}

/// What a round spent, as its note recorded it: agents spawned per role, the
/// clock each stage took, the pool it spawned them for, and what each finder
/// returned against what the verifier confirmed. Read back by
/// `cargo xtask tally --fleet` and by no gate check.
#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Fleet {
    #[serde(default)]
    pub finders: u32,
    #[serde(default)]
    pub verifiers: u32,
    /// What judged the round's candidates. A verdict is only comparable across
    /// rounds when the note says what bought it.
    #[serde(default)]
    pub verifier_model: Option<String>,
    #[serde(default)]
    pub shards: Option<u32>,
    #[serde(default)]
    pub candidates: Option<Candidates>,
    #[serde(default)]
    pub stage_seconds: Option<StageSeconds>,
    #[serde(default)]
    pub by_finder: BTreeMap<String, FinderCount>,
}

/// The pool the finders returned, by the severity they proposed - the reading a
/// round needs to say what the candidates that never bought a verifier cost it.
/// The severity vocabulary is the rubric's closed set, so an unknown row is
/// refused rather than read as a silent zero on the row it was meant to be.
#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidates {
    #[serde(default)]
    pub blocker: u32,
    #[serde(default)]
    pub important: u32,
    #[serde(default)]
    pub nit: u32,
}

/// The clock the orchestrator kept: when it spawned each stage, and when the
/// last result came back. Every field is a reading in its own right, so an
/// absent one is a stage that was not timed rather than a stage that took no
/// time - the distinction the whole block exists to preserve.
#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StageSeconds {
    #[serde(default)]
    pub gate0: Option<u32>,
    #[serde(default)]
    pub finders: Option<u32>,
    #[serde(default)]
    pub verifiers: Option<u32>,
    /// End to end, and the only field the five-minute target reads.
    #[serde(default)]
    pub total: Option<u32>,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct FinderCount {
    #[serde(default)]
    pub found: u32,
    #[serde(default)]
    pub verified: u32,
}

impl Candidates {
    pub fn total(&self) -> u32 {
        self.blocker + self.important + self.nit
    }

    /// The candidates that buy a verifier: a nit is recorded and dispositioned
    /// without one, because it cannot move the verdict.
    pub fn verdict_bearing(&self) -> u32 {
        self.blocker + self.important
    }
}

/// Tier-3's verdict: what the design dimension judged, the rulebook blob it
/// judged against, and the outcome. Its absence on a design-bearing diff is the
/// silence the schema bump exists to make loud — a PASS where nobody looked at
/// the design must not read like one where somebody did.
#[derive(Deserialize)]
pub struct Design {
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub rulebook: String,
    /// Absent when the section carries no judgment — a build's citation or an
    /// epic's drop record. A design-bearing diff is refused without it.
    #[serde(default)]
    pub verdict: String,
    /// The designs this diff's new surfaces were built from, pinned by blob. An
    /// absent field is a silence and an empty list is a stated nothing; both
    /// refuse a build that added a surface, and the two stay distinguishable so
    /// the red can say which one happened.
    #[serde(default)]
    pub implements: Option<Vec<Cited>>,
    /// The designs this epic gave up on, each with the reason it went — the
    /// `walk_waivers` shape, so a drop lands in the waiver bill the epic already
    /// reports rather than beside it.
    #[serde(default)]
    pub dropped: Vec<Excused>,
    /// The rendered files this diff touched that no design covers, each with why
    /// it needed none. The edit case: a build re-shapes far more often than it
    /// adds, and a gate that answered every edit with "go design it" would be
    /// routed around — so the rule asks for the sentence, and refuses only the
    /// silence.
    #[serde(default)]
    pub undesigned: Vec<Excused>,
}

#[derive(Deserialize)]
pub struct Cited {
    pub path: String,
    #[serde(default)]
    pub blob: String,
    /// The rendered files this design produced. A citation with no `built` list
    /// covers nothing: one design naming one path used to satisfy a diff that
    /// added five surfaces.
    #[serde(default)]
    pub built: Vec<String>,
}

/// A path, and why it is not where the rule would otherwise put it: a design the
/// epic gave up on, or a rendered file no design covers. One shape because it is
/// one rule — an empty reason records that something happened, not why, so it
/// records nothing.
#[derive(Deserialize)]
pub struct Excused {
    pub path: String,
    #[serde(default)]
    pub why: String,
}

fn why_for<'a>(records: &'a [Excused], path: &str) -> Option<&'a str> {
    records.iter().find_map(|r| {
        let why = r.why.trim();
        (r.path == path && !why.is_empty()).then_some(why)
    })
}

/// The design dimension's verdict on a note of its own: the section a `cr/v3`
/// note used to carry, moved off the review's note because the review no longer
/// binds - and so can no longer certify - the documents it judged.
#[derive(Deserialize)]
pub struct DesignNote {
    pub schema: String,
    #[serde(default)]
    pub epic: Option<String>,
    #[serde(default)]
    pub wu: Option<String>,
    // Absent on a note that carries only a citation or a drop record - the same
    // reason a `cr/v3` section's verdict is optional: a build's hand-over and an
    // epic's drop are sections over nothing judged.
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub design_sha256: String,
    /// The verdict itself, in the shape a `cr/v3` section already carried it —
    /// flattened, so the note's YAML stays flat and the two carriers cannot
    /// drift into two shapes.
    #[serde(flatten)]
    pub design: Design,
}

/// The verdict, whichever note carries it: the `design:` section of a note merged
/// before W4 (re-read at every seam the walk re-verifies), or a `design/v1` note.
/// One shape, so the checks that read it cannot drift between the two.
#[derive(Clone, Copy)]
pub enum DesignCarrier<'a> {
    Section(&'a Design),
    Note(&'a DesignNote),
}

// The accessors hand back references with the carrier's own lifetime, not the
// borrow of the carrier: a check that holds a `DesignCarrier` by value still reads
// the verdict it points at.
impl<'a> DesignCarrier<'a> {
    // Both carriers hold the same shape, so the match happens once and every
    // field reads through it.
    fn design(&self) -> &'a Design {
        match self {
            Self::Section(d) => d,
            Self::Note(n) => &n.design,
        }
    }

    pub fn artifacts(&self) -> &'a [String] {
        self.design().artifacts.as_slice()
    }

    pub fn rulebook(&self) -> &'a str {
        &self.design().rulebook
    }

    pub fn verdict(&self) -> &'a str {
        &self.design().verdict
    }

    pub fn dropped(&self) -> &'a [Excused] {
        &self.design().dropped
    }

    pub fn implements(&self) -> Option<&'a Vec<Cited>> {
        self.design().implements.as_ref()
    }

    pub fn undesigned(&self) -> &'a [Excused] {
        &self.design().undesigned
    }

    pub fn judged(&self, path: &str) -> bool {
        self.artifacts().iter().any(|a| a == path)
    }

    // An empty reason is not a drop, for the same reason an empty waiver is not
    // a waiver: it records that something went, not why.
    pub fn dropped_why(&self, path: &str) -> Option<&'a str> {
        why_for(self.dropped(), path)
    }

    pub fn undesigned_why(&self, path: &str) -> Option<&'a str> {
        why_for(self.undesigned(), path)
    }

    pub fn claims(&self, path: &str) -> bool {
        self.undesigned().iter().any(|r| r.path == path)
    }

    pub fn built(&self) -> impl Iterator<Item = &'a str> {
        self.implements().into_iter().flatten().flat_map(|c| c.built.iter()).map(String::as_str)
    }
}

/// A verified nit and what was decided about it. Recording one costs the verdict
/// nothing; leaving it undecided is what the gate refuses, so the severity rule
/// buys a cheaper pass without buying an unread list. `fixed` names a change
/// already inside the range the note binds - a fix landing after the verdict
/// moves the code hash and orphans the note, which is the binding's job.
#[derive(Deserialize)]
pub struct Nit {
    pub finding: String,
    #[serde(default)]
    pub fixed: Option<String>,
    #[serde(default)]
    pub waived: Option<String>,
}

impl Nit {
    pub fn decided(&self) -> bool {
        self.fault().is_none()
    }

    /// What stops this entry being a decision - an empty sentence records that
    /// something happened rather than what was decided, and two dispositions
    /// record a contradiction, so neither is one.
    pub fn fault(&self) -> Option<&'static str> {
        let said =
            |sentence: &Option<String>| sentence.as_ref().is_some_and(|s| !s.trim().is_empty());
        if self.finding.trim().is_empty() {
            return Some("no finding");
        }
        match (said(&self.fixed), said(&self.waived)) {
            (true, false) | (false, true) => None,
            (true, true) => Some("both fixed and waived"),
            (false, false) if self.fixed.is_some() || self.waived.is_some() => {
                Some("an empty disposition")
            }
            (false, false) => Some("no disposition"),
        }
    }
}

#[derive(Deserialize)]
pub struct WalkWaiver {
    pub seam: String,
    #[serde(default)]
    pub why: String,
}

impl CrNote {
    // A walk seam failure is waived only by an entry whose seam sha matches and
    // whose why is a non-empty reason - mirroring the waived-with-reason rule
    // used for test-layer waivers. Sha comparison is prefix-tolerant so a short
    // sha in the note matches the full seam sha the walk holds, and vice-versa.
    pub fn walk_waiver_for(&self, seam: &str) -> Option<&str> {
        self.walk_waivers.iter().find_map(|w| {
            let reason = w.why.trim();
            let matches =
                !w.seam.is_empty() && (seam.starts_with(&w.seam) || w.seam.starts_with(seam));
            (matches && !reason.is_empty()).then_some(reason)
        })
    }
}

#[derive(Deserialize)]
pub struct DocNote {
    pub schema: String,
    #[serde(default)]
    pub epic: Option<String>,
    #[serde(default)]
    pub wu: Option<String>,
    #[serde(default)]
    pub changes: Vec<DocChange>,
    #[serde(default)]
    pub t2_waived: Option<String>,
}

#[derive(Deserialize)]
pub struct DocChange {
    pub path: String,
    pub tier: String,
    pub action: String,
    #[serde(default)]
    pub distills: Option<String>,
    #[serde(default)]
    pub distilled_to: Option<DistilledTo>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum DistilledTo {
    Paths(Vec<String>),
    DeadEnd(String),
}

impl DistilledTo {
    pub fn recorded(&self) -> bool {
        match self {
            Self::Paths(paths) => !paths.is_empty(),
            Self::DeadEnd(reason) => !reason.trim().is_empty(),
        }
    }
}

#[derive(Deserialize)]
pub struct MrNote {
    pub schema: String,
    pub level: String,
    pub tip: String,
    pub tier: String,
    pub signer: String,
    /// The `base` of the signed range; present with `sig` for `key`-backed notes.
    #[serde(default)]
    pub base: Option<String>,
    /// The SSH `SIGNATURE` blob over the change-hash payload (`sha256(git diff
    /// base..tip)`, plus `report_sha256` on its own line when a report is named),
    /// `key` entries only.
    #[serde(default)]
    pub sig: Option<String>,
    /// The result report this sign-off attests (WU/epic levels): path + sha256
    /// of the exact bytes the human read.
    #[serde(default)]
    pub report: Option<String>,
    #[serde(default)]
    pub report_sha256: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct DodReceipt {
    pub schema: String,
    pub level: String,
    #[serde(default)]
    pub epic: Option<String>,
    #[serde(default)]
    pub wu: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    pub base: String,
    pub tip: String,
    pub diff_sha256: String,
    #[serde(default)]
    pub date: Option<String>,
}

pub fn tier_of(path: &str) -> Option<&'static str> {
    if !path.starts_with("docs/") {
        return None;
    }
    if path.starts_with("docs/work/") {
        Some("T3")
    } else if path.starts_with("docs/use-cases/")
        || path.starts_with("docs/decisions/")
        || path.starts_with("docs/backlog/")
        || path.starts_with("docs/design-rules/")
    {
        Some("T2")
    } else {
        Some("T1")
    }
}

// The frontend (`ui/src`, `ui/e2e`) is code the gate must enforce, same as the
// Rust crates — `ui/`-root config and git-ignored `ui/dist`/`ui/node_modules`
// stay infra.
pub fn is_code(path: &str) -> bool {
    path.starts_with("src/")
        || path.starts_with("xtask/")
        || path.starts_with("tests/")
        || path.starts_with("ui/src/")
        || path.starts_with("ui/e2e/")
}

pub fn is_doc(path: &str) -> bool {
    path.starts_with("docs/")
}

// Rule-bearing files configure the workflow itself, so their commits need a
// human signature at commit time (DEC-10): T0 (CLAUDE.md) and T1 (flat docs/).
pub fn is_rule_bearing(path: &str) -> bool {
    path == "CLAUDE.md" || tier_of(path) == Some("T1")
}

pub fn is_expected_tracked_doc(path: &str) -> bool {
    path.strip_prefix("docs/").is_some_and(|rest| {
        !rest.contains('/')
            || rest.starts_with("use-cases/")
            || rest.starts_with("decisions/")
            || rest.starts_with("backlog/")
            || rest.starts_with("design-rules/")
    })
}

#[cfg(test)]
mod tests {
    use super::{
        binds_code, is_code, is_doc, is_expected_tracked_doc, is_rule_bearing, tier_of, CrNote,
        DesignCarrier, DesignNote, DocNote, FinderCount, Nit, TestNote, CR_SCHEMAS,
    };

    #[test]
    fn parses_test_note_with_counts_waiver_and_identity() {
        let yaml = "\
schema: test/v2
level: working-unit
epic: E1
wu: E1.W2
branch: wu/capture/pagination
tests:
  unit: { count: 3, covers: \"cursor arithmetic\" }
  integration:
    waived: \"no new boundary\"
";
        let note: TestNote = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(note.schema, "test/v2");
        assert_eq!(note.epic.as_deref(), Some("E1"));
        assert!(note.tests.layer("unit").expect("unit").satisfied());
        assert!(note.tests.layer("integration").expect("int").satisfied());
        assert!(note.tests.layer("e2e").is_none());
    }

    #[test]
    fn zero_count_and_blank_waiver_are_unsatisfied() {
        let yaml = "\
schema: test/v2
level: commit
tests:
  unit: { count: 0 }
  integration: { waived: \"  \" }
";
        let note: TestNote = serde_yaml::from_str(yaml).expect("parses");
        assert!(!note.tests.layer("unit").expect("unit").satisfied());
        assert!(!note.tests.layer("integration").expect("int").satisfied());
    }

    #[test]
    fn parses_cr_note_ignoring_extra_fields() {
        let yaml = "\
schema: cr/v2
scope: \"WU pagination -> epic/capture\"
epic: E1
base: aaaa
head: bbbb
diff_sha256: cccc
dimensions: [correctness, reuse]
findings: { found: 2, verified: 0, waived: 0 }
waivers: []
verdict: PASS
date: 2026-07-05
";
        let note: CrNote = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(note.verdict, "PASS");
        assert_eq!(note.base, "aaaa");
        assert!(note.walk_waivers.is_empty(), "absent walk_waivers defaults empty");
    }

    // The severity rule's whole surface: a nit costs the verdict nothing, so the
    // only thing left to get wrong is recording one nobody decided about.
    #[test]
    fn a_nit_is_decided_by_a_non_empty_fixed_or_waiver() {
        let note: CrNote = serde_yaml::from_str(
            "\
schema: cr/v4
base: aaaa
diff_sha256: cccc
verdict: PASS
nits:
  - finding: \"loop variable named x\"
    fixed: \"renamed to frame\"
  - finding: \"two branches could share a helper\"
    waived: \"the helper would take three flags\"
  - finding: \"stale doc comment\"
    waived: \"   \"
  - finding: \"nobody decided\"
",
        )
        .expect("parses");
        let decided: Vec<bool> = note.nits.iter().map(Nit::decided).collect();
        assert_eq!(decided, vec![true, true, false, false]);
        assert_eq!(note.nits[2].fault(), Some("an empty disposition"));
        assert_eq!(note.nits[3].fault(), Some("no disposition"));
    }

    // Both faults this review found in its own first shape: a nit naming nothing
    // was a decision about nothing, and one carrying two dispositions recorded a
    // contradiction as though it were an answer.
    #[test]
    fn a_nit_naming_nothing_or_deciding_twice_is_no_decision() {
        let nit = |entry: &str| -> CrNote {
            serde_yaml::from_str(&format!(
                "schema: cr/v4\nbase: a\ndiff_sha256: c\nverdict: PASS\nnits:\n  - {entry}"
            ))
            .expect("parses")
        };
        assert_eq!(
            nit("finding: \"\"\n    waived: \"a reason for nothing\"\n").nits[0].fault(),
            Some("no finding")
        );
        assert_eq!(
            nit("finding: \"  \"\n    fixed: \"renamed\"\n").nits[0].fault(),
            Some("no finding")
        );
        assert_eq!(
            nit("finding: f\n    fixed: \"renamed\"\n    waived: \"left alone\"\n").nits[0].fault(),
            Some("both fixed and waived")
        );
        assert!(nit("finding: f\n    fixed: \"renamed\"\n").nits[0].decided());
    }

    #[test]
    fn absent_nits_is_an_empty_list_on_every_review_contract() {
        for schema in CR_SCHEMAS {
            let note: CrNote = serde_yaml::from_str(&format!(
                "schema: {schema}\nbase: a\ndiff_sha256: c\nverdict: PASS\n"
            ))
            .expect("parses");
            assert!(note.nits.is_empty(), "{schema} defaults to no nits");
        }
    }

    // W6: the round's bill travels on the note, and a note without one is a round
    // that did not count itself - the two readings must stay distinguishable, so
    // the block is optional and never defaulted to zeros.
    #[test]
    fn a_cr_v5_note_carries_its_fleet_bill_and_a_note_without_one_carries_no_reading() {
        let yaml = "\
schema: cr/v5
base: aaaa
diff_sha256: cccc
dimensions: [correctness, reuse, simplification, efficiency, altitude]
fleet:
  finders: 5
  verifiers: 15
  by_finder:
    correctness:    { found: 9, verified: 5 }
    simplification: { found: 6, verified: 2 }
findings: { found: 15, verified: 7, waived: 1 }
verdict: PASS
";
        let note: CrNote = serde_yaml::from_str(yaml).expect("parses");
        let fleet = note.fleet.expect("the bill");
        assert_eq!((fleet.finders, fleet.verifiers), (5, 15));
        assert_eq!(fleet.by_finder["correctness"], FinderCount { found: 9, verified: 5 });
        assert_eq!(fleet.by_finder["simplification"], FinderCount { found: 6, verified: 2 });

        for schema in CR_SCHEMAS {
            let uncounted: CrNote = serde_yaml::from_str(&format!(
                "schema: {schema}\nbase: a\ndiff_sha256: c\nverdict: PASS\n"
            ))
            .expect("parses");
            assert!(uncounted.fleet.is_none(), "{schema}: no block is no reading, not a zero");
        }
    }

    // A bill the tally cannot add up is a malformed note, refused where every
    // other malformed field is - not silently read as zero agents.
    #[test]
    fn a_malformed_fleet_block_is_a_malformed_note() {
        let note = |fleet: &str| {
            serde_yaml::from_str::<CrNote>(&format!(
                "schema: cr/v5\nbase: a\ndiff_sha256: c\nverdict: PASS\n{fleet}"
            ))
        };
        assert!(note("fleet: 3\n").is_err(), "a scalar is not a bill");
        assert!(note("fleet:\n  finders: two\n").is_err(), "a word is not a count");
        assert!(note("fleet:\n  by_finder: [correctness]\n").is_err(), "a list names no counts");
        assert!(note("fleet:\n  finders: 5\n").is_ok(), "a partial bill still adds up");
    }

    // W8: the bill grew a clock and a pool. Each is a reading in its own right,
    // so the ones a round did not take stay absent rather than arriving as zeros.
    #[test]
    fn a_bill_carries_the_clock_the_pool_and_what_judged_the_round() {
        let yaml = "\
schema: cr/v5
base: aaaa
diff_sha256: cccc
fleet:
  finders: 5
  verifiers: 4
  verifier_model: haiku-4.5
  shards: 2
  candidates: { blocker: 1, important: 3, nit: 6 }
  stage_seconds: { gate0: 58, finders: 145, verifiers: 110, total: 255 }
verdict: PASS
";
        let fleet = serde_yaml::from_str::<CrNote>(yaml).expect("parses").fleet.expect("the bill");
        assert_eq!(fleet.verifier_model.as_deref(), Some("haiku-4.5"));
        assert_eq!(fleet.shards, Some(2));
        let pool = fleet.candidates.expect("the pool");
        assert_eq!((pool.total(), pool.verdict_bearing()), (10, 4));
        assert_eq!(pool.verdict_bearing(), fleet.verifiers, "a nit buys no verifier");
        let clock = fleet.stage_seconds.expect("the clock");
        assert_eq!((clock.gate0, clock.finders, clock.total), (Some(58), Some(145), Some(255)));
    }

    #[test]
    fn a_stage_the_round_did_not_time_is_absent_rather_than_zero() {
        let bill = |bill: &str| {
            serde_yaml::from_str::<CrNote>(&format!(
                "schema: cr/v5\nbase: a\ndiff_sha256: c\nverdict: PASS\n{bill}"
            ))
            .expect("parses")
            .fleet
            .expect("the bill")
        };
        let clock = bill("fleet:\n  finders: 5\n  stage_seconds: { total: 255 }\n")
            .stage_seconds
            .expect("the clock");
        assert_eq!(clock.total, Some(255));
        assert_eq!(clock.finders, None, "a stage nobody timed is not a stage that took no time");

        let uncounted = bill("fleet:\n  finders: 5\n");
        assert!(uncounted.stage_seconds.is_none(), "no clock is no reading");
        assert!(uncounted.candidates.is_none(), "no pool is no reading");
        assert!(uncounted.verifier_model.is_none(), "an unnamed model is not a named one");
    }

    // The severity rows are the rubric's closed set: a row it does not name would
    // otherwise read as a silent zero on the row it was meant to be.
    #[test]
    fn a_severity_the_rubric_does_not_name_is_a_malformed_note() {
        let note = |bill: &str| {
            serde_yaml::from_str::<CrNote>(&format!(
                "schema: cr/v5\nbase: a\ndiff_sha256: c\nverdict: PASS\n{bill}"
            ))
        };
        assert!(note("fleet:\n  candidates: { nits: 6 }\n").is_err(), "the row is nit");
        assert!(note("fleet:\n  candidates: { nit: many }\n").is_err(), "a word is not a count");
        assert!(
            note("fleet:\n  stage_seconds: { setup: 12 }\n").is_err(),
            "not a stage a round runs"
        );
        assert!(note("fleet:\n  candidates: { nit: 6 }\n").is_ok(), "a partial pool still adds up");
    }

    // The two whole-diff contracts are the closed set; everything after them is
    // code-bound, so the gate never has to learn a new schema string to bind it.
    #[test]
    fn every_contract_after_the_whole_diff_pair_binds_the_code_paths() {
        assert!(!binds_code("cr/v2"));
        assert!(!binds_code("cr/v3"));
        assert!(binds_code("cr/v4"));
        assert!(binds_code("cr/v5"));
        assert_eq!(CR_SCHEMAS.iter().filter(|s| !binds_code(s)).count(), 2);
    }

    // The value test above cannot bite: a hand-written literal that happens to
    // equal today's schema passes it, and only stops passing at the next bump -
    // which is exactly when nobody is looking. So the rule is enforced as a rule:
    // the modules that tell a reader which note to write may not spell one.
    #[test]
    fn nothing_that_instructs_a_reader_spells_a_review_schema_of_its_own() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for module in ["mr.rs", "lib.rs"] {
            let text = std::fs::read_to_string(src.join(module)).unwrap();
            let shipped = text.split("#[cfg(test)]").next().unwrap();
            for (n, line) in shipped.lines().enumerate() {
                assert!(
                    !line.contains("cr/v"),
                    "{module}:{} spells a review schema instead of asking cr_schema(): {}",
                    n + 1,
                    line.trim()
                );
            }
        }
    }

    #[test]
    fn the_current_review_contract_is_the_newest_one_the_list_names() {
        assert_eq!(super::cr_schema(), *CR_SCHEMAS.last().unwrap());
        assert!(binds_code(super::cr_schema()));
    }

    // The bump is for enforcement: a v2 note has no design section and cannot be
    // given one after the fact, so the schema string is itself the record of
    // which contract the review ran under.
    #[test]
    fn cr_v3_carries_the_design_dimensions_verdict_and_v2_carries_none() {
        let yaml = "\
schema: cr/v3
base: aaaa
diff_sha256: cccc
design:
  artifacts: [docs/work/2026-08-13-dflow-run-4.html]
  rulebook: 796c547
  findings: { found: 3, verified: 1, waived: 0 }
  verdict: PASS
verdict: PASS
";
        let note: CrNote = serde_yaml::from_str(yaml).expect("parses");
        let design = note.design.expect("the design section");
        let design = DesignCarrier::Section(&design);
        assert_eq!(design.verdict(), "PASS");
        assert_eq!(design.rulebook(), "796c547");
        assert!(design.judged("docs/work/2026-08-13-dflow-run-4.html"));
        assert!(!design.judged("docs/work/components/button.html"), "unlisted is unjudged");

        let v2: CrNote =
            serde_yaml::from_str("schema: cr/v2\nbase: a\ndiff_sha256: c\nverdict: PASS\n")
                .expect("parses");
        assert!(v2.design.is_none());
    }

    // Absent and empty are different silences, and the note must keep them apart:
    // one says nobody wrote a citation, the other says somebody wrote that there
    // is none. Defaulting the field to an empty list would erase the difference.
    #[test]
    fn an_absent_implements_stays_distinct_from_an_empty_one() {
        let note = |design: &str| -> CrNote {
            serde_yaml::from_str(&format!(
                "schema: cr/v3\nbase: a\ndiff_sha256: c\nverdict: PASS\n{design}"
            ))
            .expect("parses")
        };

        let cited = note(
            "design:\n  verdict: PASS\n  implements:\n    - path: docs/work/components/status-pill.html\n      blob: 796c547\n",
        );
        let listed = cited.design.expect("design").implements.expect("implements");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "docs/work/components/status-pill.html");
        assert_eq!(listed[0].blob, "796c547");

        let stated_none = note("design:\n  verdict: PASS\n  implements: []\n");
        assert_eq!(
            stated_none.design.expect("design").implements.map(|c| c.len()),
            Some(0),
            "an empty list is present and empty"
        );
        assert!(
            note("design:\n  verdict: PASS\n").design.expect("design").implements.is_none(),
            "an absent field is absent, never an empty list"
        );
    }

    // A drop is the epic's answer to "this design was never built" - so it says
    // which one and why. The reasonless entry is the silence the record exists
    // to refuse, exactly as a blank walk waiver is.
    #[test]
    fn a_drop_records_a_path_and_a_reason_or_it_is_no_drop() {
        let yaml = "\
schema: cr/v3
base: aaaa
diff_sha256: cccc
verdict: PASS
design:
  artifacts: []
  rulebook: 796c547
  verdict: PASS
  dropped:
    - path: docs/work/components/body-renderer.html
      why: \"never built; the panel renders bodies inline\"
    - path: docs/work/components/header-table.html
      why: \"   \"
";
        let note: CrNote = serde_yaml::from_str(yaml).expect("parses");
        let design = note.design.expect("the design section");
        let design = DesignCarrier::Section(&design);
        assert_eq!(
            design.dropped_why("docs/work/components/body-renderer.html"),
            Some("never built; the panel renders bodies inline")
        );
        assert_eq!(design.dropped_why("docs/work/components/header-table.html"), None);
        assert_eq!(design.dropped_why("docs/work/components/unlisted.html"), None);

        let v2: CrNote =
            serde_yaml::from_str("schema: cr/v2\nbase: a\ndiff_sha256: c\nverdict: PASS\n")
                .expect("parses");
        assert!(v2.design.is_none(), "a v2 note can drop nothing");
    }

    // W4's contract, at the unit level: the verdict a design-bearing tip ships now
    // travels on its own note, and the checks read it through one shape.
    #[test]
    fn a_design_note_carries_the_verdict_and_the_handoff_it_answers_for() {
        let yaml = "\
schema: design/v1
epic: E9
wu: E9.W1
base: aaaa
design_sha256: cccc
verdict: PASS
artifacts: [docs/work/2026-08-13-dflow-run-4.html]
rulebook: 796c547
implements:
  - path: docs/work/components/status-pill.html
    blob: 28f5437
    built: [ui/src/components/StatusPill.tsx]
undesigned:
  - path: ui/src/FlowDetail.tsx
    why: \"a copy fix; the layout is untouched\"
";
        let note: DesignNote = serde_yaml::from_str(yaml).expect("parses");
        let carrier = DesignCarrier::Note(&note);
        assert_eq!(note.design_sha256, "cccc");
        assert_eq!(carrier.verdict(), "PASS");
        assert_eq!(carrier.rulebook(), "796c547");
        assert!(carrier.judged("docs/work/2026-08-13-dflow-run-4.html"));
        assert_eq!(carrier.built().collect::<Vec<_>>(), vec!["ui/src/components/StatusPill.tsx"]);
        assert!(carrier.claims("ui/src/FlowDetail.tsx"));
        assert_eq!(
            carrier.undesigned_why("ui/src/FlowDetail.tsx"),
            Some("a copy fix; the layout is untouched")
        );
    }

    #[test]
    fn walk_waiver_matches_only_a_named_seam_with_a_reason() {
        let yaml = "\
schema: cr/v2
base: aaaa
diff_sha256: cccc
verdict: PASS
walk_waivers:
  - seam: deadbeefcafe1234
    why: \"unlanded sibling epic integrated live; gated by its own epic gate\"
  - seam: 0000badnoreason
    why: \"  \"
";
        let note: CrNote = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(
            note.walk_waiver_for("deadbeefcafe1234deadbeef"),
            Some("unlanded sibling epic integrated live; gated by its own epic gate"),
            "a prefix match with a real reason waives"
        );
        assert_eq!(note.walk_waiver_for("0000badnoreason0000"), None, "blank why never waives");
        assert_eq!(note.walk_waiver_for("feedface99887766"), None, "unmatched seam never waives");
    }

    #[test]
    fn parses_doc_note_with_both_distilled_to_shapes() {
        let yaml = "\
schema: doc/v1
epic: E1
changes:
  - path: docs/use-cases/control-api.html
    tier: T2
    action: updated
    distills: \"UC-API-7 pagination contract\"
  - path: docs/work/plan.html
    tier: T3
    action: archived
    distilled_to: [docs/use-cases/control-api.html]
  - path: docs/work/dead.html
    tier: T3
    action: archived
    distilled_to: \"none: dead end\"
t2_waived: null
";
        let note: DocNote = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(note.changes.len(), 3);
        assert!(note.changes[1].distilled_to.as_ref().expect("paths").recorded());
        assert!(note.changes[2].distilled_to.as_ref().expect("dead end").recorded());
    }

    #[test]
    fn malformed_yaml_is_an_error() {
        assert!(serde_yaml::from_str::<TestNote>("schema: [").is_err());
        assert!(serde_yaml::from_str::<TestNote>("schema: test/v2\n").is_err());
    }

    #[test]
    fn path_classification() {
        assert_eq!(tier_of("docs/workflow.html"), Some("T1"));
        assert_eq!(tier_of("docs/use-cases/capture.html"), Some("T2"));
        assert_eq!(tier_of("docs/decisions/event-bus.html"), Some("T2"));
        assert_eq!(tier_of("docs/backlog/roadmap.html"), Some("T2"));
        assert_eq!(tier_of("docs/design-rules/design-rulebook.html"), Some("T2"));
        assert_eq!(tier_of("docs/work/plan.html"), Some("T3"));
        assert_eq!(tier_of("src/lib.rs"), None);

        assert!(is_code("src/lib.rs"));
        assert!(is_code("xtask/src/gate.rs"));
        assert!(is_code("tests/api.rs"));
        assert!(is_code("ui/src/FlowDetail.tsx"), "frontend source is gated code");
        assert!(is_code("ui/e2e/specs/flows.spec.ts"), "frontend e2e is gated code");
        assert!(!is_code("ui/vite.config.ts"), "ui-root config stays infra");
        assert!(!is_code("docs/workflow.html"));
        assert!(!is_code("Cargo.toml"));

        assert!(is_doc("docs/workflow.html"));
        assert!(is_doc("docs/use-cases/capture.html"));
        assert!(!is_doc("src/lib.rs"));
        assert!(!is_doc("ui/app.tsx"));

        assert!(is_rule_bearing("CLAUDE.md"), "T0 is rule-bearing");
        assert!(is_rule_bearing("docs/workflow.html"), "flat T1 is rule-bearing");
        assert!(!is_rule_bearing("docs/use-cases/capture.html"), "T2 is not");
        assert!(!is_rule_bearing("docs/decisions/event-bus.html"), "T2 is not");
        assert!(!is_rule_bearing("docs/backlog/roadmap.html"), "T2 is not");
        assert!(!is_rule_bearing("docs/design-rules/design-rulebook.html"), "T2 is not");
        assert!(!is_rule_bearing("docs/work/plan.html"), "T3 is not");
        assert!(!is_rule_bearing("src/lib.rs"));
        assert!(!is_rule_bearing(".mr-allowed"), "local trust config, never committed");

        assert!(is_expected_tracked_doc("docs/workflow.html"));
        assert!(is_expected_tracked_doc("docs/use-cases/capture.html"));
        assert!(is_expected_tracked_doc("docs/decisions/decisions-index.html"));
        assert!(is_expected_tracked_doc("docs/backlog/roadmap.html"));
        assert!(is_expected_tracked_doc("docs/design-rules/design-rules-index.html"));
        assert!(!is_expected_tracked_doc("docs/work/plan.html"));
        assert!(!is_expected_tracked_doc("docs/work/archive/plan.html"));
        assert!(!is_expected_tracked_doc("docs/.archive/plan.html"));
        assert!(!is_expected_tracked_doc("docs/scratch/notes.html"));
        assert!(!is_expected_tracked_doc("src/lib.rs"));
    }
}
