//! E-DFLOW epic e2e — the design flow driven end to end over a hermetic temp
//! repo, through the real `xtask` binary, real `node` oracles, real git and the
//! real merge hook (no shims).
//!
//! A fake designing agent emits one stage artifact at a time and a fake human
//! answers each with a taste instruction; all three tiers then judge the result.
//! **Tier-1** (`verify-doc`, the write-time hook) must name exactly what is
//! still owed at every stage — a flow that goes quiet, or that reds for
//! something other than being half-written, has stalled. **Tier-2** (the run's
//! own receipt) is re-opened when the rulebook moves under the run, which is the
//! close's first step. **Tier-3** (the design dimension of the code review)
//! rides the merge: `design-scope` says what must be judged, the `cr/v3` note
//! says it was, and `9·design` refuses the merge when it does not.
//!
//! The waiver W1 through W6 each wrote in identical words is paid here.
#![cfg(all(unix, feature = "design-e2e"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask::git::{diff_sha256, sha256_hex, show_bytes};

const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const RUN: &str = "docs/work/2026-08-13-dflow-run-4.html";
const WU_RESULT: &str = "docs/work/2026-08-13-dflow-w7-result.html";
const COMPONENTS: &str = "docs/work/components";

// The rulebook the run is judged against. Two hard slugs (both predicated) and
// two soft ones, which is all the receipt's coverage rule needs to bite.
const SOFT_RULES: &[&str] = &["data-ink-first", "calm-microcopy"];

fn rulebook_html(soft: &[&str]) -> String {
    let rows = |slugs: &[&str]| {
        let mut out = String::new();
        for slug in slugs {
            let _ = write!(out, "<tr><td><code>{slug}</code></td></tr>");
        }
        out
    };
    format!(
        "<html><body><table data-c=\"hard-rules-table\"><tbody>{}</tbody></table>\
         <table data-c=\"soft-rules-table\"><tbody>{}</tbody></table></body></html>\n",
        rows(&["colour-tokens-only", "spacing-scale"]),
        rows(soft)
    )
}

struct Flow {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Flow {
    /// A repo carrying the whole substrate a design run reads: the rulebook, the
    /// token vocabulary, the component collection behind the registry oracle, and
    /// the `epic/x` → `wu/x/one` branches the merge gate walks.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let flow = Self { _dir: dir, path };
        flow.git(&["init", "-q", "-b", "main"]);
        flow.git(&["config", "user.email", "flow@test"]);
        flow.git(&["config", "user.name", "flow"]);
        flow.git(&["config", "commit.gpgsign", "false"]);
        flow.write("src/lib.rs", "pub fn base() {}\n");
        flow.write(".mr-allowed", "email: a@b.co\n");
        flow.write(RULEBOOK, &rulebook_html(SOFT_RULES));
        flow.write_vocabulary();
        flow.write_collection();
        flow.commit("the substrate a design run loads");
        flow.git(&["branch", "epic/x"]);
        flow.git(&["checkout", "-q", "-b", "wu/x/one", "epic/x"]);
        flow.install_merge_hook();
        flow
    }

    // Five hard rules resolve every class through ui/tailwind.config.js. A temp
    // repo has no ui/, so the emitter is stubbed to its shape: what this e2e
    // drives is the flow, not the palette.
    fn write_vocabulary(&self) {
        self.write(
            "scripts/design-vocabulary.mjs",
            "process.stdout.write(JSON.stringify({legal:[\"text-ink\"],\
             rules:{\"colour-tokens-only\":[\"text-\"]},values:{},modifiers:{},\
             chromatic:[\"ok\"],chroma_floor:40}));",
        );
    }

    // The registry oracle is the real script over a real collection: the bridge
    // under test is node reading a TSDoc block, and a canned answer would prove
    // only that serde works.
    fn write_collection(&self) {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/design-registry.mjs"),
            self.path.join("scripts/design-registry.mjs"),
        )
        .unwrap();
        self.write(
            "ui/src/components/Section.tsx",
            "/**\n * Section frames one labelled zone.\n * @tier T2 — widget\n\
             \x20* @use a labelled zone of one screen\n * @avoid nesting inside another Section\n\
             \x20* @states rest · hover · empty\n * @madeof nothing\n */\n\
             export const Section = () => null;\n",
        );
        fs::create_dir_all(self.path.join(COMPONENTS)).unwrap();
    }

    // The real dispatch rule, with the built binary in place of cargo: merging
    // into epic/* runs the WU gate, into main the epic gate, anything else is
    // ungated - which is what lets the run pick its epic up mid-flight.
    fn install_merge_hook(&self) {
        let hooks = self.path.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-merge-commit");
        fs::write(
            &hook,
            format!(
                "#!/bin/sh\nset -e\nbranch=$(git symbolic-ref --short -q HEAD || true)\n\
                 case \"$branch\" in\nmain) gate=epic ;;\nepic/*) gate=wu ;;\n*) exit 0 ;;\nesac\n\
                 exec \"{}\" verify-dod hook \"$gate\"\n",
                env!("CARGO_BIN_EXE_xtask")
            ),
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn git_raw(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.args(args)
            .current_dir(&self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "flow")
            .env("GIT_AUTHOR_EMAIL", "flow@test.invalid")
            .env("GIT_COMMITTER_NAME", "flow")
            .env("GIT_COMMITTER_EMAIL", "flow@test.invalid")
            .output()
            .expect("spawn git")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.git_raw(args);
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }

    fn commit(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn note(&self, note_ref: &str, tip: &str, body: &str) {
        self.git(&["notes", &format!("--ref={note_ref}"), "add", "-f", "-m", body, tip]);
    }

    fn xtask(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(args)
            .current_dir(&self.path)
            .output()
            .expect("spawn xtask")
    }

    /// Tier-1 over the run doc — what the `PostToolUse` hook runs on every write.
    fn verify_run(&self) -> Output {
        self.xtask(&["verify-doc", RUN])
    }

    /// The pin a receipt must carry: `git hash-object` over the rulebook on disk.
    fn pin(&self) -> String {
        self.git(&["hash-object", RULEBOOK])[..7].to_string()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn failed_checks(out: &Output) -> Vec<String> {
    stdout(out)
        .lines()
        .filter_map(|line| line.strip_prefix("FAIL "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

// ─── the fake human ─────────────────────────────────────────────────────────
//
// What the reviewer said · what it moved (a stage, or `nothing` when the design
// argued and held) · the contract it hit. The third row is the one the `review`
// item exists for: an instruction heard and refused is evidence too.
const REVIEWER: &[[&str; 3]] = &[
    ["lead the catalog with the verdict", "elements", "<code>data-ink-first</code>"],
    ["one copy affordance, not two", "mockup", "<code>Section</code> @avoid"],
    ["keep the header pinned", "nothing", "<code>taste</code>"],
];

fn review_artifact() -> String {
    let mut rows = String::new();
    for [said, moved, contract] in REVIEWER {
        let _ = write!(rows, "<tr><td>{said}</td><td>{moved}</td><td>{contract}</td></tr>");
    }
    format!(
        "<table data-item=\"review\"><tr><th>Said</th><th>Moved</th><th>Contract</th></tr>{rows}</table>"
    )
}

// ─── the fake designing agent ───────────────────────────────────────────────

const MOCKUP_COMPLIANT: &str = "<div class=\"text-ink\">flow detail</div>";
// The silent fork: a colour the closed vocabulary does not emit, written into
// the mockup as if it were a token. Run #2 shipped this shape once.
const MOCKUP_VIOLATING: &str = "<div class=\"text-crimson\">flow detail</div>";

/// The eleven artifacts, in the order the flow emits them. `mockup` is the
/// stage the violating run breaks, so its body is the only variable.
fn stages(pin: &str, mockup_body: &str) -> Vec<(&'static str, String)> {
    vec![
        ("frame", frame()),
        ("layout", layout()),
        ("elements", elements()),
        ("interaction", interaction()),
        ("mockup", mockup(mockup_body)),
        ("tune", tune()),
        ("receipt", receipt(pin, SOFT_RULES)),
        ("capabilities", capabilities()),
        ("proposals", "<p data-item=\"proposals\">none — the roster covered every zone</p>".into()),
        ("blocked-on", "<p data-item=\"blocked-on\">none</p>".into()),
        ("review", review_artifact()),
    ]
}

fn frame() -> String {
    "<div data-item=\"frame\">\
     <p>Purpose: read one flow and decide whether to act on it.</p>\
     <p>Given: CA install · proxy unset</p>\
     <p>Not given: no remote probe</p>\
     <table><tr><th>Driver</th><th>why</th><th>therefore</th><th>on this screen</th></tr>\
     <tr><td>read the verdict fast</td><td>the reviewer is the bottleneck</td>\
     <td>the verdict leads the row</td>\
     <td>✓ status column first<br>✗ status buried in prose</td></tr>\
     <tr><td>Section must not nest</td><td>its contract says so</td><td>zones stay flat</td>\
     <td>✓ one Section per zone<br>✗ a Section inside a Section</td></tr></table></div>"
        .to_string()
}

fn layout() -> String {
    "<div data-item=\"layout\"><pre>\n\
     ┌───────────────┐\n\
     │ A  header     │  ← pinned\n\
     ├───────────────┤\n\
     │ B  verdicts   │  ← the reading surface\n\
     └───────────────┘\n\n\
     overflow: A is pinned · B scrolls to its own cap\n\
     </pre></div>"
        .to_string()
}

fn elements() -> String {
    "<div data-item=\"elements\" data-map>\
     <pre>panel\n\
     ├─ A header ── <span data-key=\"section\">Section</span>\n\
     └─ B verdicts ── <span data-key=\"section\">Section</span></pre>\
     <table><tr><th>Component</th><th>Placement · verdict</th><th>Fit</th><th>States</th></tr>\
     <tr data-key=\"section\"><td>Section</td><td>T2 <b>reuse</b> · A · B</td>\
     <td><div class=\"gist\">clears the nesting @avoid, lands in the labelled-zone @use</div>\
     <details class=\"why\"><div>@avoid nesting: the zoning keeps every Section flat</div></details></td>\
     <td>—</td></tr></table></div>"
        .to_string()
}

fn interaction() -> String {
    "<table data-item=\"interaction\">\
     <tr><th>Use case</th><th>Goal</th><th>User does</th><th>Screen responds</th><th>Keyboard</th></tr>\
     <tr><td>land cold</td><td>read the verdict fast</td><td>opens the flow</td>\
     <td><div class=\"gist\">B renders the real verdicts, A stays pinned</div>\
     <details class=\"why\"><div>A → UNCHANGED · B → verdict rows</div></details></td>\
     <td><code>j</code> / <code>k</code></td></tr></table>"
        .to_string()
}

fn mockup(body: &str) -> String {
    format!(
        "<div data-item=\"mockup\" data-wire=\"none\">{body}\
         <button class=\"statebtn\" data-state-toggle=\"mock\" data-state-class=\"inflight\" \
         data-label-off=\"▶ show inflight\" data-label-on=\"◀ show base\">▶ show inflight</button></div>"
    )
}

fn tune() -> String {
    "<table data-item=\"tune\"><tr><th>Call</th><th>Before</th><th>After</th><th>Slug</th></tr>\
     <tr><td>weight</td><td><pre class=\"tb\">status  ok</pre></td>\
     <td><pre class=\"tb\">ok  status</pre></td><td><code>data-ink-first</code></td></tr></table>"
        .to_string()
}

const WHY: &[(&str, &str)] = &[
    ("data-ink-first", "satisfied — the verdict leads its row"),
    ("calm-microcopy", "inherited — Section owns its labels"),
    ("hero-one", "n/a — this panel has no hero"),
];

fn receipt(pin: &str, soft: &[&str]) -> String {
    let mut rows = String::new();
    for slug in soft {
        let why = WHY.iter().find(|(s, _)| s == slug).map(|(_, w)| *w).unwrap();
        let _ = writeln!(rows, "  {slug}: {why}");
    }
    format!("<pre data-item=\"receipt\">rulebook: {pin}\nsoft:\n{rows}</pre>")
}

fn capabilities() -> String {
    "<table data-item=\"capabilities\"><tr><th>Given</th><th>State</th><th>Where / why</th></tr>\
     <tr><td>CA install</td><td>used</td><td>zone A action</td></tr>\
     <tr><td>proxy unset</td><td>deliberately unused</td><td>a non-goal for this panel</td></tr></table>"
        .to_string()
}

fn run_doc(status: &str, body: &str) -> String {
    let attr = if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Trawler — design run #4</title>\
         <script src=\"https://cdn.tailwindcss.com\"></script>\
         <script type=\"module\" src=\"../../doc-assets/components/tw-config.js\"></script>\
         <script src=\"../../doc-assets/components/specimen.js\" defer></script>\
         <style>.tb{{color:var(--ink);}}</style></head>\
         <body><main data-artifact=\"design-run\"{attr}>{body}</main></body></html>\n"
    )
}

/// Drive the agent through `stages`, writing one artifact at a time and asking
/// Tier-1 what is still owed after each. The whole assertion of the flow's
/// plumbing lives here: after stage *n* the run must be red **for exactly the
/// stages not yet written** — never quietly green (which would mean nothing is
/// asked for), and never red for anything else (which would mean the stage just
/// written was refused without being wrong).
fn drive(flow: &Flow, stages: &[(&'static str, String)]) {
    let mut body = String::new();
    for (at, (item, artifact)) in stages.iter().enumerate() {
        body.push_str(artifact);
        flow.write(RUN, &run_doc("", &body));
        let out = flow.verify_run();
        let owed: Vec<String> =
            stages[at + 1..].iter().map(|(next, _)| format!("item·{next}")).collect();
        let mut got = failed_checks(&out);
        got.sort();
        let mut want = owed.clone();
        want.sort();
        assert_eq!(got, want, "after writing {item}: {}", stdout(&out));
        assert_eq!(
            out.status.success(),
            owed.is_empty(),
            "a half-written run is red and a finished one is green, at {item}: {}",
            stdout(&out)
        );
    }
}

// ─── the WU's evidence, so the merge can be about the design ────────────────

const IDENTITY: &str = "epic: E-DFLOW\nwu: E-DFLOW.W7\nbranch: wu/design-flow/harness\n";

const DOC_CHANGES: &str = "changes:\n\
     \x20 - path: docs/work/2026-08-13-dflow-run-4.html\n    tier: T3\n    action: added\n    distills: \"the fourth design run\"\n\
     \x20 - path: docs/work/2026-08-13-dflow-w7-result.html\n    tier: T3\n    action: added\n    distills: \"the WU result report\"\n";

fn bless(flow: &Flow, tip: &str, cr: &str) {
    flow.note(
        "test",
        tip,
        &format!(
            "schema: test/v2\nlevel: working-unit\n{IDENTITY}tests:\n  integration: {{ count: 1, covers: \"the design gate\" }}\n"
        ),
    );
    flow.note(
        "doc",
        tip,
        &format!("schema: doc/v1\n{IDENTITY}{DOC_CHANGES}t2_waived: \"no T2 fact moved\"\n"),
    );
    flow.note("cr", tip, cr);
    let bytes = show_bytes(&flow.path, tip, WU_RESULT).unwrap();
    flow.note(
        "mr",
        tip,
        &format!(
            "schema: mr/v1\nlevel: working-unit\ntip: {tip}\ntier: email\nsigner: a@b.co\n\
             report: {WU_RESULT}\nreport_sha256: {}\ndate: 2026-08-13\n",
            sha256_hex(&bytes).unwrap()
        ),
    );
}

fn cr_note(flow: &Flow, tip: &str, schema: &str, design: &str) -> String {
    let base = flow.git(&["rev-parse", "epic/x"]);
    let sha = diff_sha256(&flow.path, &base, tip).unwrap();
    format!(
        "schema: {schema}\n{IDENTITY}base: {base}\nhead: {tip}\ndiff_sha256: {sha}\nverdict: PASS\n{design}date: 2026-08-13\n"
    )
}

/// Merge the WU into its epic through the real hook, and report what the gate
/// said. The hook's own output rides the merge's stdout.
fn merge_into_epic(flow: &Flow) -> (bool, String) {
    flow.git(&["checkout", "-q", "epic/x"]);
    let out = flow.git_raw(&["merge", "--no-ff", "-m", "merge wu/x/one", "wu/x/one"]);
    let said = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        flow.git_raw(&["merge", "--abort"]);
    }
    (out.status.success(), said)
}

// ─── tier 3, read from the one function the reviewer and the gate share ─────

struct Scope {
    artifacts: Vec<String>,
    pin: String,
}

/// What `cargo xtask design-scope` prints, parsed the way a reviewer reads it —
/// the note below is written from this and nothing else, which is the whole
/// point of the reviewer and the merge gate calling one function.
fn design_scope(flow: &Flow) -> Scope {
    let out = flow.xtask(&["design-scope", "epic/x", "wu/x/one"]);
    assert!(out.status.success(), "design-scope: {}", stdout(&out));
    let text = stdout(&out);
    let mut lines = text.lines();
    let head = lines.next().expect("design-scope says what it found");
    let pin = head.trim_end_matches(':').split_whitespace().last().unwrap().to_string();
    Scope { artifacts: lines.map(str::to_string).collect(), pin }
}

fn design_section(scope: &Scope, verdict: &str) -> String {
    format!(
        "design:\n  artifacts: [{}]\n  rulebook: {}\n  findings: {{ found: 0, verified: 0, waived: 0 }}\n  verdict: {verdict}\n",
        scope.artifacts.join(", "),
        scope.pin
    )
}

// ─── the flow, end to end ───────────────────────────────────────────────────

#[test]
fn the_whole_design_flow_runs_green_and_the_three_tiers_land_it() {
    let flow = Flow::new();

    // ① Tier-1, stage by stage: the agent writes one artifact and the harness
    // answers with exactly what is still owed.
    drive(&flow, &stages(&flow.pin(), MOCKUP_COMPLIANT));

    // The human's instructions are all recorded, including the one the design
    // argued and held - a run that rewrites its own history silently is the
    // defect the `review` item exists to refuse.
    let written = fs::read_to_string(flow.path.join(RUN)).unwrap();
    for [said, _, _] in REVIEWER {
        assert!(written.contains(said), "the reviewer instruction \"{said}\" is not recorded");
    }

    // ② Tier-2: the receipt is checked against the rulebook ON DISK, so the
    // green above is a claim about one version of it and nothing more.
    let green = flow.verify_run();
    assert!(stdout(&green).contains("receipt 2/2 soft"), "{}", stdout(&green));

    // ③ The rulebook moves under the open run - another Working Unit lands a
    // soft rule on the epic, and this one picks its epic up. Both halves of the
    // close's first step must bite: the pin is stale, and the new rule is
    // unreceipted.
    flow.git(&["checkout", "-q", "epic/x"]);
    let mut soft = SOFT_RULES.to_vec();
    soft.push("hero-one");
    flow.write(RULEBOOK, &rulebook_html(&soft));
    flow.commit("a later Working Unit adds a soft rule");
    flow.git(&["checkout", "-q", "wu/x/one"]);
    flow.git(&["merge", "--no-ff", "-q", "-m", "pick up the epic", "epic/x"]);

    let reopened = flow.verify_run();
    let checks = failed_checks(&reopened);
    assert!(checks.contains(&"receipt·rulebook-stale".to_string()), "{}", stdout(&reopened));
    assert!(checks.contains(&"receipt·hero-one".to_string()), "{}", stdout(&reopened));

    // ④ The close, step ①: re-judge every soft row against the rulebook that is
    // actually there, then re-pin.
    let mut body = String::new();
    for (_, artifact) in stages(&flow.pin(), MOCKUP_COMPLIANT) {
        body.push_str(&artifact);
    }
    let body = body.replace(&receipt(&flow.pin(), SOFT_RULES), &receipt(&flow.pin(), &soft));
    flow.write(RUN, &run_doc("", &body));
    let repinned = flow.verify_run();
    assert!(repinned.status.success(), "re-judged and re-pinned: {}", stdout(&repinned));
    assert!(stdout(&repinned).contains("receipt 3/3 soft"), "{}", stdout(&repinned));

    // ⑤ The close, step ④: archive it, in the commit that finishes the run.
    flow.write(RUN, &run_doc("archived", &body));
    assert!(stdout(&flow.verify_run()).contains("archived"), "a closed run is not re-opened");
    flow.write(WU_RESULT, "<html><body>the WU result report</body></html>\n");
    let tip = flow.commit("close the fourth design run");

    // ⑥ Tier-3 activates: one function tells the reviewer what to judge and the
    // merge what to demand.
    let scope = design_scope(&flow);
    assert_eq!(scope.artifacts, vec![RUN.to_string()], "the closed run is what Tier-3 judges");
    assert_eq!(scope.pin, flow.pin(), "and the rulebook its verdict is a claim about");

    // A cr/v2 PASS is a review from before the design dimension existed. On a
    // design-bearing tip it is silence, and the merge refuses it.
    bless(&flow, &tip, &cr_note(&flow, &tip, "cr/v2", ""));
    let (merged, said) = merge_into_epic(&flow);
    assert!(!merged, "a design-bearing tip must not merge on a cr/v2 note: {said}");
    assert!(said.contains("9·design-schema"), "{said}");
    assert!(said.contains("design finder"), "the red says how to get the missing evidence: {said}");

    // A cr/v3 whose design section skips the artifact is the same silence, one
    // schema later - and this one its own schema says it should have covered.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    flow.note("cr", &tip, &cr_note(&flow, &tip, "cr/v3", ""));
    let (merged, said) = merge_into_epic(&flow);
    assert!(!merged, "a cr/v3 with no design section must not merge: {said}");
    assert!(said.contains("9·design"), "{said}");

    // A failing design verdict is not a merge either: the gate holds that the
    // dimension ran AND passed.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    flow.note("cr", &tip, &cr_note(&flow, &tip, "cr/v3", &design_section(&scope, "FAIL")));
    let (merged, said) = merge_into_epic(&flow);
    assert!(!merged, "a failing design verdict must not merge: {said}");

    // ⑦ The note written from what design-scope printed - every artifact judged,
    // pinned to the rulebook the tip ships - and the merge lands.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    flow.note("cr", &tip, &cr_note(&flow, &tip, "cr/v3", &design_section(&scope, "PASS")));
    let (merged, said) = merge_into_epic(&flow);
    assert!(merged, "a complete cr/v3 lands the design-bearing merge: {said}");

    let receipt = flow.git(&["notes", "--ref=dod", "show", &tip]);
    assert!(receipt.contains("dod/v1"), "green writes the receipt: {receipt}");
    assert!(receipt.contains("design"), "the receipt names the design check it passed: {receipt}");
    flow.git(&["rev-parse", "-q", "--verify", "HEAD^2"]);
}

/// The other half of the epic's e2e requirement: a rule-violating input reds.
/// It reds **at the stage that wrote it** — not at the end, and not at the
/// merge — and the chain from there is what keeps it out of the tree: a run
/// that cannot be greened cannot be closed, and a run left open cannot merge.
#[test]
fn a_rule_violating_mockup_reds_at_its_own_stage_and_never_reaches_the_epic() {
    let flow = Flow::new();
    let pin = flow.pin();
    let stages = stages(&pin, MOCKUP_VIOLATING);

    let mut body = String::new();
    for (at, (item, artifact)) in stages.iter().enumerate() {
        body.push_str(artifact);
        flow.write(RUN, &run_doc("", &body));
        let checks = failed_checks(&flow.verify_run());
        let breached = checks.iter().any(|c| c == "colour-tokens-only·off-vocabulary");
        assert_eq!(
            breached,
            at >= 4,
            "the breach is reported from the mockup stage onward, at {item}: {checks:?}"
        );
        if at == 4 {
            assert!(
                stdout(&flow.verify_run()).contains("text-crimson"),
                "the red names the class that broke the rule"
            );
        }
    }

    // Every stage is written and the run is still red: the close's step ③ - green
    // the run - cannot be reached, so the run cannot honestly be archived.
    let finished = flow.verify_run();
    assert!(!finished.status.success(), "a complete but violating run stays red");
    assert_eq!(
        failed_checks(&finished),
        vec!["colour-tokens-only·off-vocabulary".to_string()],
        "the breach is the only thing wrong with it: {}",
        stdout(&finished)
    );

    // Committed open - the only honest state left - the merge refuses it.
    flow.write(WU_RESULT, "<html><body>the WU result report</body></html>\n");
    let tip = flow.commit("the run that could not be closed");
    bless(&flow, &tip, &cr_note(&flow, &tip, "cr/v3", ""));
    let (merged, said) = merge_into_epic(&flow);
    assert!(!merged, "a run left open must not merge: {said}");
    assert!(said.contains("6·run-closed"), "{said}");
    assert!(said.contains("archived"), "the red names the close it is missing: {said}");
}
