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

/// The review contracts the gate reads. `cr/v3` adds the design dimension's
/// section; a `cr/v2` note is, by its own schema declaration, evidence written
/// before that dimension existed.
pub const CR_SCHEMAS: &[&str] = &["cr/v2", "cr/v3"];

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
    #[serde(default)]
    pub walk_waivers: Vec<WalkWaiver>,
    #[serde(default)]
    pub design: Option<Design>,
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

impl Design {
    pub fn judged(&self, path: &str) -> bool {
        self.artifacts.iter().any(|a| a == path)
    }

    // An empty reason is not a drop, for the same reason an empty waiver is not
    // a waiver: it records that something went, not why.
    pub fn dropped_why(&self, path: &str) -> Option<&str> {
        why_for(&self.dropped, path)
    }

    pub fn undesigned_why(&self, path: &str) -> Option<&str> {
        why_for(&self.undesigned, path)
    }

    pub fn claims(&self, path: &str) -> bool {
        self.undesigned.iter().any(|r| r.path == path)
    }

    pub fn built(&self) -> impl Iterator<Item = &str> {
        self.implements.iter().flatten().flat_map(|c| c.built.iter()).map(String::as_str)
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

#[derive(Deserialize)]
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
        is_code, is_doc, is_expected_tracked_doc, is_rule_bearing, tier_of, CrNote, DocNote,
        TestNote,
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
        assert_eq!(design.verdict, "PASS");
        assert_eq!(design.rulebook, "796c547");
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
