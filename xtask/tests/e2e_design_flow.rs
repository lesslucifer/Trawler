//! E-DFLOW epic e2e — the design flow driven end to end over a hermetic temp
//! repo, through the real `xtask` binary, real `node` oracles, real git and the
//! real merge hook (no shims).
//!
//! A fake designing agent emits one item of the mockup doc at a time and a fake
//! human answers with a taste instruction; the checker and the merge gate then
//! judge the result. **Tier-1** (`verify-doc`, the write-time hook) must name
//! exactly what is still owed after every item — a flow that goes quiet, or that
//! reds for something other than being half-written, has stalled — and must
//! refuse a run still carrying one of the stations DEC-53 cut. The **design
//! step** rides the merge: `design-scope` says what is in scope, the `design/v1`
//! note accounts for it beside a `cr/v5` code note, and `9·design` refuses the
//! merge when it does not.
//!
//! The waiver W1 through W6 each wrote in identical words is paid here.
#![cfg(all(unix, feature = "design-e2e"))]
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask::git::{diff_names, diff_sha256_paths, sha256_hex, show_bytes};

const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const RUN: &str = "docs/work/2026-08-13-dflow-run-4.html";
const WU_RESULT: &str = "docs/work/2026-08-13-dflow-w7-result.html";
const COMPONENTS: &str = "docs/work/components";

// The rulebook the run is judged against: two hard slugs, both predicated, which
// is all the tally and the gate's slug-citation check need. There is no soft
// half any more (DEC-53).
fn rulebook_html() -> String {
    let mut rows = String::new();
    for slug in ["colour-tokens-only", "spacing-scale"] {
        let _ = write!(rows, "<tr><td><code>{slug}</code></td></tr>");
    }
    format!(
        "<html><body><table data-c=\"hard-rules-table\"><tbody>{rows}</tbody></table>\
         </body></html>\n"
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
        // Tracked, the tally the gate appends mid-merge becomes an unstaged
        // change that `git merge --abort` then refuses to undo.
        flow.write(".gitignore", "/target\n");
        flow.write(RULEBOOK, &rulebook_html());
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

    fn remove(&self, rel: &str) {
        fs::remove_file(self.path.join(rel)).unwrap();
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
// What the reviewer said · what it moved (a run item, or `nothing` when the
// design argued and held) · the contract it hit. The third row is the one the
// `review` item exists for: an instruction heard and refused is evidence too.
const REVIEWER: &[[&str; 3]] = &[
    ["lead the row with the verdict", "mockup", "<code>data-ink-first</code>"],
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

const MOCKUP_COMPLIANT: &str = "";
// The silent fork: a colour the closed vocabulary does not emit, written into
// the mockup as if it were a token. Run #2 shipped this shape once.
const MOCKUP_VIOLATING: &str = "<div class=\"text-crimson\">oops</div>";

/// The six items of the mockup doc, in the order the flow writes them. `mockup`
/// is the one the violating run breaks, so its body is the only variable.
fn items(mockup_body: &str) -> Vec<(&'static str, String)> {
    vec![
        ("mockup", mockup(mockup_body)),
        ("capabilities", capabilities()),
        ("keyboard", keyboard()),
        ("proposals", "<p data-item=\"proposals\">none — the roster covered every zone</p>".into()),
        ("blocked-on", "<p data-item=\"blocked-on\">none</p>".into()),
        ("review", review_artifact()),
    ]
}

/// The mockup: the mounted screen, the zone hosts three predicates read, the
/// overflow line under it and the state toggle.
fn mockup(extra: &str) -> String {
    format!(
        "<div data-item=\"mockup\" data-wire=\"none\">\
         <div id=\"m-header\" data-zone=\"header\"></div>\
         <div id=\"m-zone\" data-zone=\"verdicts\"></div>\
         <script type=\"text/babel\">mount('m-zone', <Section title=\"headers\">rows</Section>)</script>\
         <p>overflow: header is pinned · verdicts scrolls to its own cap</p>{extra}\
         <button class=\"statebtn\" data-state-toggle=\"mock\" data-state-class=\"inflight\" \
         data-label-off=\"▶ show inflight\" data-label-on=\"◀ show base\">▶ show inflight</button></div>"
    )
}

fn capabilities() -> String {
    "<table data-item=\"capabilities\"><tr><th>Given</th><th>State</th><th>Where / why</th></tr>\
     <tr><td>CA install</td><td>used</td><td>the header action</td></tr>\
     <tr><td>proxy unset</td><td>deliberately unused</td><td>a non-goal for this panel</td></tr></table>"
        .to_string()
}

fn keyboard() -> String {
    "<table data-item=\"keyboard\"><tr><th>Pointer does</th><th>Where</th><th>Key path</th></tr>\
     <tr><td>open a verdict</td><td>the verdicts zone</td>\
     <td><code>↑</code> / <code>↓</code> then <code>Enter</code>; <code>Esc</code> closes</td></tr></table>"
        .to_string()
}

/// The shape DEC-53 cut, still being written: one item per cut station, so the
/// checker has to name each rather than tolerate it.
fn cut_stations() -> String {
    "<p data-item=\"frame\">drivers</p><p data-item=\"layout\">zones</p>\
     <p data-item=\"elements\">a catalog</p><p data-item=\"interaction\">use cases</p>\
     <p data-item=\"tune\">before/after</p><pre data-item=\"receipt\">rulebook: 28f5437</pre>"
        .to_string()
}

fn run_doc(status: &str, body: &str) -> String {
    let attr = if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
    format!(
        "<!DOCTYPE html><html lang=\"en\" data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\">\
         <head><meta charset=\"utf-8\">\
         <title>Trawler — design run #4</title>\
         <script src=\"https://cdn.tailwindcss.com\"></script>\
         <script src=\"../../doc-assets/components/specimens.bundle.js\"></script>\
         <script src=\"https://cdn.jsdelivr.net/npm/@babel/standalone@7/babel.min.js\"></script>\
         <style>.tb{{color:var(--ink);}}</style></head>\
         <body><main data-artifact=\"design-run\"{attr}>{body}</main>\
         <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
         </body></html>\n"
    )
}

/// Drive the agent through `items`, writing one at a time and asking Tier-1
/// what is still owed after each. The whole assertion of the flow's plumbing
/// lives here: after item *n* the run must be red **for exactly the items not
/// yet written** — never quietly green (which would mean nothing is asked for),
/// and never red for anything else (which would mean the item just written was
/// refused without being wrong).
fn drive(flow: &Flow, items: &[(&'static str, String)]) {
    let mut body = String::new();
    for (at, (item, artifact)) in items.iter().enumerate() {
        body.push_str(artifact);
        flow.write(RUN, &run_doc("", &body));
        let out = flow.verify_run();
        let owed: Vec<String> =
            items[at + 1..].iter().map(|(next, _)| format!("item·{next}")).collect();
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
const EPIC_IDENTITY: &str = "epic: E-DFLOW\n";

fn doc_changes(entries: &[(&str, &str)]) -> String {
    let mut out = String::from("changes:\n");
    for (path, action) in entries {
        let tier = if path.starts_with("docs/work/") { "T3" } else { "T2" };
        let _ = write!(
            out,
            "  - path: {path}\n    tier: {tier}\n    action: {action}\n    distills: \"driven by the flow e2e\"\n"
        );
    }
    out
}

/// The doc-note entry DEC-16 demands of a design file this Working Unit retires:
/// archived, and its forks carried somewhere named.
fn retired(path: &str, carried_by: &str) -> String {
    format!(
        "  - path: {path}\n    tier: T3\n    action: archived\n    distilled_to: \"carried verbatim by {carried_by}\"\n"
    )
}

fn bless(flow: &Flow, tip: &str, design: Option<&str>, changes: &str) {
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
        &format!("schema: doc/v1\n{IDENTITY}{changes}t2_waived: \"no T2 fact moved\"\n"),
    );
    flow.note("cr", tip, &cr_note(flow, tip));
    match design {
        Some(note) => flow.note("design", tip, note),
        None => {
            flow.git_raw(&["notes", "--ref=design", "remove", tip]);
        }
    }
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

/// The code review's note, on the schema a review written today declares. It
/// binds the bytes of the code paths the diff changes - not the whole diff - and
/// it carries no design section: since DEC-46 the design verdict travels on its
/// own note, and since DEC-53 there is no finder whose verdict it could carry.
fn cr_note(flow: &Flow, tip: &str) -> String {
    let base = flow.git(&["rev-parse", "epic/x"]);
    let sha = code_sha256(flow, &base, tip);
    format!(
        "schema: {}\n{IDENTITY}base: {base}\nhead: {tip}\ndiff_sha256: {sha}\nverdict: PASS\ndate: 2026-08-13\n",
        xtask::notes::cr_schema()
    )
}

/// What a code-bound review binds, computed the way the gate recomputes it: the
/// code paths the range changes, and nothing else. An empty answer says the diff
/// changed no code, which is a note that binds nothing rather than a hash of an
/// empty diff.
fn code_sha256(flow: &Flow, base: &str, tip: &str) -> String {
    let paths: Vec<String> = diff_names(&flow.path, base, tip)
        .unwrap()
        .into_iter()
        .filter(|p| xtask::notes::is_code(p))
        .collect();
    if paths.is_empty() {
        return String::new();
    }
    diff_sha256_paths(&flow.path, base, tip, &paths).unwrap()
}

/// Merge a branch into its target through the real hook, and report what the
/// gate said. The hook's own output rides the merge's stdout.
fn merge_into(flow: &Flow, target: &str, branch: &str) -> (bool, String) {
    flow.git(&["checkout", "-q", target]);
    let out = flow.git_raw(&["merge", "--no-ff", "-m", &format!("merge {branch}"), branch]);
    let said = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        flow.git_raw(&["merge", "--abort"]);
    }
    (out.status.success(), said)
}

// ─── the design step, read from the one function it and the gate share ──────

struct Scope {
    artifacts: Vec<String>,
    surfaces: Vec<String>,
    touched: Vec<String>,
}

/// What `cargo xtask design-scope` prints, parsed the way the design step reads
/// it — the note below is written from this and nothing else, which is the whole
/// point of the step and the merge gate calling one function. Three blocks: the
/// designs in scope, the surfaces the diff must cite, and the files it touched.
fn design_scope(flow: &Flow, branch: &str) -> Scope {
    let out = flow.xtask(&["design-scope", "epic/x", branch]);
    assert!(out.status.success(), "design-scope: {}", stdout(&out));
    let text = stdout(&out);
    assert!(!text.contains("rulebook"), "nothing pins the rulebook any more: {text}");
    let mut scope = Scope { artifacts: Vec::new(), surfaces: Vec::new(), touched: Vec::new() };
    let mut block = 0;
    for line in text.lines() {
        if line.contains("design artifact(s) to judge") {
            block = 0;
        } else if line.contains("left the tree in this range") {
            block = 3;
        } else if line.contains("new surface(s)") {
            block = 1;
        } else if line.contains("rendered file(s) touched") {
            block = 2;
        } else {
            match block {
                0 => scope.artifacts.push(line.to_string()),
                1 => scope.surfaces.push(line.to_string()),
                2 => scope.touched.push(line.to_string()),
                _ => {}
            }
        }
    }
    scope
}

/// The `design/v1` note the design step writes, bound to exactly the bytes the
/// gate recomputes over: the artifacts in scope plus every rendered file the
/// diff touched (DEC-46). `body` is the accounting — the citations and the
/// declarations — which is the whole of what the verdict follows (DEC-53).
fn design_note(flow: &Flow, tip: &str, scope: &Scope, verdict: &str, body: &str) -> String {
    let base = flow.git(&["rev-parse", "epic/x"]);
    let mut paths: Vec<String> = scope.artifacts.clone();
    paths.extend(scope.touched.iter().cloned());
    paths.extend(scope.surfaces.iter().cloned());
    paths.sort();
    paths.dedup();
    let bound = if paths.is_empty() {
        String::new()
    } else {
        diff_sha256_paths(&flow.path, &base, tip, &paths).unwrap()
    };
    format!(
        "schema: design/v1\n{IDENTITY}base: {base}\ndesign_sha256: {bound}\nverdict: {verdict}\n\
         artifacts: [{}]\n{body}date: 2026-08-13\n",
        scope.artifacts.join(", ")
    )
}

/// The citation: which design a new surface was built from, and which of its
/// bytes the build read.
fn cites(pairs: &[(&str, &str, &str)]) -> String {
    let mut out = String::from("implements:\n");
    for (path, blob, built) in pairs {
        let _ = write!(out, "  - path: {path}\n    blob: {blob}\n    built: [{built}]\n");
    }
    out
}

/// The other answer a build can give: this file needed no design, and here is
/// why. An edit is accounted for, never judged.
fn declares(records: &[(&str, &str)]) -> String {
    let mut out = String::from("undesigned:\n");
    for (path, why) in records {
        let _ = write!(out, "  - path: {path}\n    why: \"{why}\"\n");
    }
    out
}

// ─── the flow, end to end ───────────────────────────────────────────────────

#[test]
fn the_whole_design_flow_runs_green_and_the_merge_gate_lands_it() {
    let flow = Flow::new();

    // ① Tier-1, item by item: the agent writes one and the harness answers with
    // exactly what is still owed.
    drive(&flow, &items(MOCKUP_COMPLIANT));

    // The human's instructions are all recorded, including the one the design
    // argued and held - a run that rewrites its own history silently is the
    // defect the `review` item exists to refuse.
    let written = fs::read_to_string(flow.path.join(RUN)).unwrap();
    for [said, _, _] in REVIEWER {
        assert!(written.contains(said), "the reviewer instruction \"{said}\" is not recorded");
    }

    // ② The tally the green carries: every hard rule the book states has a
    // predicate, and nothing claims a receipt was read (DEC-53).
    let green = flow.verify_run();
    let text = stdout(&green);
    assert!(text.contains("hard rules: 2/2 machine-checked (design)"), "{text}");
    assert!(!text.contains("receipt"), "{text}");

    // ③ The shape the record cut, still being written: each cut station is
    // refused BY NAME. A checker that merely ignored them would let the old flow
    // survive by tolerance, with the spec and the code disagreeing unseen.
    let mut body = String::new();
    for (_, artifact) in items(MOCKUP_COMPLIANT) {
        body.push_str(&artifact);
    }
    flow.write(RUN, &run_doc("", &format!("{body}{}", cut_stations())));
    let old_shape = flow.verify_run();
    assert!(!old_shape.status.success(), "a run carrying the cut stations is not green");
    let said = stdout(&old_shape);
    for cut in ["frame", "layout", "elements", "interaction", "tune", "receipt"] {
        assert!(said.contains(cut), "the red does not name the cut {cut}: {said}");
    }

    // ④ The close: green the run, then archive it in the commit that finishes it.
    flow.write(RUN, &run_doc("", &body));
    assert!(flow.verify_run().status.success(), "{}", stdout(&flow.verify_run()));
    flow.write(RUN, &run_doc("archived", &body));
    assert!(stdout(&flow.verify_run()).contains("archived"), "a closed run is not re-opened");
    flow.write(WU_RESULT, "<html><body>the WU result report</body></html>\n");
    let tip = flow.commit("close the fourth design run");

    // ⑤ The design step activates: one function tells it what is in scope and
    // the merge what to demand.
    let scope = design_scope(&flow, "wu/x/one");
    assert_eq!(scope.artifacts, vec![RUN.to_string()], "the closed run is what it answers for");

    // A code note alone is silence about the design. The merge refuses it, and
    // the red says which note is missing.
    let changes = doc_changes(&[(RUN, "added"), (WU_RESULT, "added")]);
    bless(&flow, &tip, None, &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/one");
    assert!(!merged, "a design-bearing tip must not merge with no design note: {said}");
    assert!(said.contains("9·design"), "{said}");
    assert!(said.contains("design/v1"), "the red names the note it wants: {said}");

    // A note bound to other bytes is the same silence, one step later: it says a
    // step ran, over something this tip does not ship.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    let elsewhere =
        design_note(&flow, &tip, &scope, "PASS", "").replace("design_sha256: ", "design_sha256: 0");
    bless(&flow, &tip, Some(&elsewhere), &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/one");
    assert!(!merged, "a note pinned to other bytes must not merge: {said}");
    assert!(said.contains("different bytes"), "{said}");

    // A failing verdict is not a merge either: the gate holds that the step ran
    // AND passed.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    bless(&flow, &tip, Some(&design_note(&flow, &tip, &scope, "FAIL", "")), &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/one");
    assert!(!merged, "a failing design verdict must not merge: {said}");

    // ⑥ The note written from what design-scope printed, beside the code note -
    // the two carriers a live tip ships - and the merge lands.
    flow.git(&["checkout", "-q", "wu/x/one"]);
    bless(&flow, &tip, Some(&design_note(&flow, &tip, &scope, "PASS", "")), &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/one");
    assert!(merged, "a complete design/v1 lands the design-bearing merge: {said}");
    assert!(
        flow.git(&["notes", "--ref=cr", "show", &tip]).contains(xtask::notes::cr_schema()),
        "the code note the tip ships is the current schema, not the retired carrier"
    );

    let receipt = flow.git(&["notes", "--ref=dod", "show", &tip]);
    assert!(receipt.contains("dod/v1"), "green writes the receipt: {receipt}");
    assert!(receipt.contains("design"), "the receipt names the design check it passed: {receipt}");
    flow.git(&["rev-parse", "-q", "--verify", "HEAD^2"]);
}

/// The other half of the epic's e2e requirement: a rule-violating input reds.
/// It reds **at the write that produced it** — not at the end, and not at the
/// merge — and the chain from there is what keeps it out of the tree: a run
/// that cannot be greened cannot be closed, and a run left open cannot merge.
#[test]
fn a_rule_violating_mockup_reds_at_the_write_that_produced_it_and_never_reaches_the_epic() {
    let flow = Flow::new();
    let items = items(MOCKUP_VIOLATING);

    let mut body = String::new();
    for (at, (item, artifact)) in items.iter().enumerate() {
        body.push_str(artifact);
        flow.write(RUN, &run_doc("", &body));
        let checks = failed_checks(&flow.verify_run());
        assert!(
            checks.iter().any(|c| c == "colour-tokens-only·off-vocabulary"),
            "the mockup is the first item written, so the breach is reported from there onward, at {item}: {checks:?}"
        );
        if at == 0 {
            assert!(
                stdout(&flow.verify_run()).contains("text-crimson"),
                "the red names the class that broke the rule"
            );
        }
    }

    // Every item is written and the run is still red: the close's step ② - green
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
    bless(&flow, &tip, None, &doc_changes(&[(RUN, "added"), (WU_RESULT, "added")]));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/one");
    assert!(!merged, "a run left open must not merge: {said}");
    assert!(said.contains("6·run-closed"), "{said}");
    assert!(said.contains("archived"), "the red names the close it is missing: {said}");
}

// ─── the build leg: what a judged design hands to the thing built from it ───

const PILL_DESIGN: &str = "docs/work/components/status-pill.html";
const PILL: &str = "ui/src/components/StatusPill.tsx";
const SCREEN: &str = "ui/src/FlowDetail.tsx";
const DECISION: &str = "docs/decisions/dec-9-the-pill-contract.html";

const PILL_SOURCE: &str = "/**\n * StatusPill states one verdict.\n * @tier T1 — primitive\n\
     \x20* @use one verdict inside a row\n * @avoid a sentence\n * @states rest · hover\n\
     \x20* @madeof nothing\n */\nexport const StatusPill = () => null;\n";
const SCREEN_SOURCE: &str = "export const FlowDetail = () => null;\n";

fn pill_contract(states: &str) -> String {
    format!(
        "<html><body><h1>StatusPill</h1><p>one verdict, read at a glance</p>\
         <p>states: {states}</p></body></html>\n"
    )
}

/// The design Working Unit: a component contract, accounted for and landed on
/// the epic. Returns the blob a build that reads it must cite.
fn land_the_contract(flow: &Flow) -> String {
    flow.write(PILL_DESIGN, &pill_contract("rest · hover"));
    flow.write(DECISION, "<html><body>why the pill states a verdict</body></html>\n");
    flow.write(WU_RESULT, "<html><body>the design WU result</body></html>\n");
    let tip = flow.commit("design the pill");

    let scope = design_scope(flow, "wu/x/one");
    assert_eq!(
        scope.artifacts,
        vec![PILL_DESIGN.to_string()],
        "a component contract is a design artifact the moment it is in the tree"
    );
    assert!(scope.surfaces.is_empty(), "a design Working Unit builds nothing");

    bless(
        flow,
        &tip,
        Some(&design_note(flow, &tip, &scope, "PASS", "")),
        &doc_changes(&[(PILL_DESIGN, "added"), (DECISION, "added"), (WU_RESULT, "added")]),
    );
    let (merged, said) = merge_into(flow, "epic/x", "wu/x/one");
    assert!(merged, "a design in scope, accounted for, lands: {said}");
    flow.git(&["rev-parse", &format!("epic/x:{PILL_DESIGN}")])
}

/// The build Working Unit: the component the contract describes - and, per
/// DEC-16, the contract retired in the same Working Unit, so the tip no longer
/// ships the design the build read. This is the history every build WU produces
/// (DEC-28), and the one E-DFLOW's fixtures never drove. Returns the tip, the
/// doc-note changes a legal note carries, and what the design step was handed.
fn build_the_pill_and_retire_its_design(flow: &Flow) -> (String, String, Scope) {
    flow.git(&["checkout", "-q", "-b", "wu/x/two", "epic/x"]);
    flow.write(PILL, PILL_SOURCE);
    flow.remove(PILL_DESIGN);
    flow.write(WU_RESULT, "<html><body>the build WU result</body></html>\n");
    let tip = flow.commit("build the pill, retire its design");
    assert!(
        show_bytes(&flow.path, &tip, PILL_DESIGN).is_err(),
        "the build tip does not ship the design it cites"
    );

    let scope = design_scope(flow, "wu/x/two");
    assert!(scope.artifacts.is_empty(), "the build answers for no design - it consumes one");
    assert_eq!(
        scope.surfaces,
        vec![PILL.to_string()],
        "the surface it added is what owes a citation"
    );
    let changes =
        format!("{}{}", doc_changes(&[(WU_RESULT, "updated")]), retired(PILL_DESIGN, "@states"));
    (tip, changes, scope)
}

/// The hand-over, driven: a Working Unit that adds a surface says which design
/// it was built from and which bytes of it it read. The last leg is D1a case 2 —
/// a build that amends the design it was handed re-enters the design step on its
/// own merge, with nobody having to remember the rule.
#[test]
fn a_build_that_adds_a_surface_cites_the_design_it_read_or_the_merge_refuses_it() {
    let flow = Flow::new();
    let handed = land_the_contract(&flow);

    // ① The build, on wu/x/two: the component, and the design retired with it.
    let (tip, changes, scope) = build_the_pill_and_retire_its_design(&flow);

    // ② The silent build: the trigger fired and the note names no design.
    bless(&flow, &tip, None, &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/two");
    assert!(!merged, "a new surface with no citation must not merge: {said}");
    assert!(said.contains("accounts for none of them"), "{said}");
    assert!(said.contains("StatusPill.tsx"), "the red names the surface that owes one: {said}");

    // ②b A reason is not an answer for something nobody has ever designed.
    let excused = declares(&[(PILL, "small, and the tier is obvious")]);
    flow.note("design", &tip, &design_note(&flow, &tip, &scope, "PASS", &excused));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/two");
    assert!(!merged, "a new surface may not be declared undesigned: {said}");
    assert!(said.contains("new surface this diff adds"), "{said}");

    // ③ Path and blob: the design, and which of its bytes the build read. The
    // path is gone from the tip; it resolves at its last state in the tip's
    // history, and the pin must be that state's.
    let mistyped = cites(&[(PILL_DESIGN, "0000000", PILL)]);
    flow.note("design", &tip, &design_note(&flow, &tip, &scope, "PASS", &mistyped));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/two");
    assert!(!merged, "a pin the retired design never carried must not merge: {said}");
    assert!(said.contains("before it left the tree"), "the red names the last state: {said}");

    let cited = cites(&[(PILL_DESIGN, &handed[..7], PILL)]);
    flow.note("design", &tip, &design_note(&flow, &tip, &scope, "PASS", &cited));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/two");
    assert!(merged, "a citation resolving in the tip's history lands the build: {said}");

    // ④ The amendment: building the screen that hosts the pill exposes a gap in
    // the pill's contract, so this Working Unit re-opens the retired design and
    // edits it - the design is back in the tree at this tip.
    flow.git(&["checkout", "-q", "-b", "wu/x/three", "epic/x"]);
    flow.write(SCREEN, SCREEN_SOURCE);
    flow.write(PILL_DESIGN, &pill_contract("rest · hover · empty"));
    flow.write(WU_RESULT, "<html><body>the screen WU result</body></html>\n");
    let tip = flow.commit("build the screen, and amend the contract it read");
    let changes = doc_changes(&[(PILL_DESIGN, "updated"), (WU_RESULT, "updated")]);
    let amended = flow.git(&["rev-parse", &format!("{tip}:{PILL_DESIGN}")]);
    assert_ne!(amended, handed, "the amendment moved the bytes");

    let scope = design_scope(&flow, "wu/x/three");
    assert_eq!(
        scope.artifacts,
        vec![PILL_DESIGN.to_string()],
        "an amended design is judged again, on the merge that lands the amendment"
    );
    assert_eq!(scope.surfaces, vec![SCREEN.to_string()]);

    // The stale citation - the blob this build was handed is not the blob its own
    // amendment left behind. A path-only pointer passes this; a blob does not.
    let stale = cites(&[(PILL_DESIGN, &handed[..7], SCREEN)]);
    bless(&flow, &tip, Some(&design_note(&flow, &tip, &scope, "PASS", &stale)), &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/three");
    assert!(!merged, "a citation of bytes the tip no longer ships must not merge: {said}");
    assert!(
        said.contains(&format!("cites {PILL_DESIGN} at blob")),
        "the red names the pin that went stale: {said}"
    );

    // Re-pinned, and the amendment itself unjudged: the design section is the
    // same one Tier-3 writes, so the re-judgment is demanded automatically.
    let unaccounted =
        design_note(&flow, &tip, &scope, "PASS", &cites(&[(PILL_DESIGN, &amended[..7], SCREEN)]))
            .replace(&format!("artifacts: [{PILL_DESIGN}]"), "artifacts: []");
    flow.note("design", &tip, &unaccounted);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/three");
    assert!(!merged, "an amended design nobody re-judged must not merge: {said}");
    assert!(
        said.contains(&format!(
            "{PILL_DESIGN} is a design artifact the design section does not name"
        )),
        "Tier-3 re-fires on the merge that lands the amendment: {said}"
    );

    // Re-judged and re-pinned.
    let complete = cites(&[(PILL_DESIGN, &amended[..7], SCREEN)]);
    flow.note("design", &tip, &design_note(&flow, &tip, &scope, "PASS", &complete));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/three");
    assert!(merged, "a re-judged amendment citing the bytes it left lands: {said}");

    // ⑤ The edit leg: no surface added, an existing one re-shaped. Nothing here
    // owes a design - and everything here owes an answer.
    flow.git(&["checkout", "-q", "-b", "wu/x/four", "epic/x"]);
    flow.write(SCREEN, "export const FlowDetail = () => <section>rows</section>;\n");
    flow.write(WU_RESULT, "<html><body>the re-shape WU result</body></html>\n");
    let tip = flow.commit("re-shape the panel nobody designed");
    let changes = doc_changes(&[(WU_RESULT, "updated")]);

    let scope = design_scope(&flow, "wu/x/four");
    assert!(scope.surfaces.is_empty(), "an edit adds nothing: {:?}", scope.surfaces);
    assert_eq!(
        scope.touched,
        vec![SCREEN.to_string()],
        "and the reviewer is told what it did touch"
    );

    bless(&flow, &tip, None, &changes);
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/four");
    assert!(!merged, "an unaccounted edit must not merge: {said}");
    assert!(said.contains(SCREEN), "the red names the file it re-shaped: {said}");

    let reason = "a copy fix inside the existing layout";
    let excused = declares(&[(SCREEN, reason)]);
    flow.note("design", &tip, &design_note(&flow, &tip, &scope, "PASS", &excused));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/four");
    assert!(merged, "a stated reason lands the edit: {said}");
    assert!(said.contains(reason), "and the reason is billed where the epic reads it: {said}");
}

fn bless_epic(flow: &Flow, tip: &str, design: Option<&str>) {
    let base = flow.git(&["merge-base", "main", tip]);
    flow.note(
        "test",
        tip,
        &format!(
            "schema: test/v2\nlevel: epic\n{EPIC_IDENTITY}tests:\n  e2e: {{ count: 1, covers: \"the design flow\" }}\n"
        ),
    );
    flow.note(
        "doc",
        tip,
        &format!(
            "schema: doc/v1\n{EPIC_IDENTITY}{}t2_waived: \"the decision is the T2 record\"\n",
            doc_changes(&[(DECISION, "added")])
        ),
    );
    flow.note(
        "cr",
        tip,
        &format!(
            "schema: {}\n{EPIC_IDENTITY}base: {base}\nhead: {tip}\ndiff_sha256: {}\nverdict: PASS\ndate: 2026-08-13\n",
            xtask::notes::cr_schema(),
            code_sha256(flow, &base, tip)
        ),
    );
    match design {
        Some(body) => flow.note(
            "design",
            tip,
            &format!(
                "schema: design/v1\n{EPIC_IDENTITY}base: {base}\ndesign_sha256: \n{body}date: 2026-08-13\n"
            ),
        ),
        None => {
            flow.git_raw(&["notes", "--ref=design", "remove", tip]);
        }
    }
    let report = "<html><body>the epic result report</body></html>\n";
    flow.note(
        "mr",
        tip,
        &format!(
            "schema: mr/v1\nlevel: epic\ntip: {tip}\ntier: email\nsigner: a@b.co\n\
             report: docs/work/2026-08-13-dflow-epic-result.html\nreport_sha256: {}\ndate: 2026-08-13\n",
            sha256_hex(report.as_bytes()).unwrap()
        ),
    );
}

fn dropped(path: &str, why: &str) -> String {
    format!("dropped:\n  - path: {path}\n    why: \"{why}\"\n")
}

/// The downstream half of the rule (D3·E): a design's life ends at its epic.
/// Check 6 forces the design FILE out of the tree by the epic tip and says
/// nothing about whether anything was built — so a `git rm` of a contract nobody
/// implemented merged green until this predicate. The epic either built it or
/// says why it went.
#[test]
fn a_design_nobody_built_refuses_the_epic_merge_until_the_drop_is_recorded() {
    let flow = Flow::new();
    land_the_contract(&flow);

    // The epic ships code of its own, so what refuses it below is the unbuilt
    // design and not the docs-only exemption.
    flow.git(&["checkout", "-q", "-b", "wu/x/two", "epic/x"]);
    flow.write("src/lib.rs", "pub fn base() {}\npub fn shipped() {}\n");
    flow.write(WU_RESULT, "<html><body>the code WU result</body></html>\n");
    let tip = flow.commit("ship code the design never touched");
    bless(&flow, &tip, None, &doc_changes(&[(WU_RESULT, "updated")]));
    let (merged, said) = merge_into(&flow, "epic/x", "wu/x/two");
    assert!(merged, "a build that adds no surface owes no citation: {said}");

    // Check 6 satisfied, and the contract now traceable only through the range.
    flow.git(&["rm", "-q", PILL_DESIGN, WU_RESULT]);
    let epic_tip = flow.commit("archive the work docs out of git");
    bless_epic(&flow, &epic_tip, None);

    let (merged, said) = merge_into(&flow, "main", "epic/x");
    assert!(!merged, "an epic that designed a component and built nothing must not land: {said}");
    assert!(said.contains("components·unbuilt"), "{said}");
    assert!(said.contains("StatusPill"), "the red names the component nobody built: {said}");

    bless_epic(&flow, &epic_tip, Some(&dropped(PILL_DESIGN, "   ")));
    let (merged, said) = merge_into(&flow, "main", "epic/x");
    assert!(!merged, "a drop with no reason records that it went, not why: {said}");
    assert!(said.contains("components·unbuilt"), "{said}");

    bless_epic(
        &flow,
        &epic_tip,
        Some(&dropped(PILL_DESIGN, "never built - the row states its verdict")),
    );
    let (merged, said) = merge_into(&flow, "main", "epic/x");
    assert!(merged, "a drop with a reason lands the epic: {said}");
    assert!(
        said.contains("WAIVE"),
        "the drop is billed as a waiver, not hidden beside one: {said}"
    );
}
