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
    } else if path.starts_with("docs/use-cases/") || path.starts_with("docs/decisions/") {
        Some("T2")
    } else {
        Some("T1")
    }
}

pub fn is_code(path: &str) -> bool {
    path.starts_with("src/") || path.starts_with("xtask/") || path.starts_with("tests/")
}

pub fn is_unarchived_work_doc(path: &str) -> bool {
    path.strip_prefix("docs/work/")
        .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".html"))
}

#[cfg(test)]
mod tests {
    use super::{is_code, is_unarchived_work_doc, tier_of, CrNote, DocNote, TestNote};

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
        assert_eq!(tier_of("docs/work/plan.html"), Some("T3"));
        assert_eq!(tier_of("docs/work/archive/plan.html"), Some("T3"));
        assert_eq!(tier_of("src/lib.rs"), None);

        assert!(is_code("src/lib.rs"));
        assert!(is_code("xtask/src/gate.rs"));
        assert!(is_code("tests/api.rs"));
        assert!(!is_code("docs/workflow.html"));
        assert!(!is_code("Cargo.toml"));

        assert!(is_unarchived_work_doc("docs/work/plan.html"));
        assert!(!is_unarchived_work_doc("docs/work/archive/plan.html"));
        assert!(!is_unarchived_work_doc("docs/workflow.html"));
    }
}
