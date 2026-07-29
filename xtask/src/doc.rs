use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::gate::Failure;

const EPIC_PLAN: &[&str] = &[
    "intent",
    "use-cases",
    "expected",
    "non-goals",
    "wu-summaries",
    "e2e-plan",
    "distillation-plan",
    "open-questions",
];
const WU_PLAN: &[&str] = &["requirement", "maps-to", "expected", "test-plan"];
const WU_RESULT: &[&str] = &["outcome", "deviations", "evidence", "distillation", "follow-ups"];
const EPIC_RESULT: &[&str] =
    &["outcome", "wu-rollup", "deviations", "evidence", "distillation", "follow-ups"];
const WU_SUB: &[&str] = &["wu-summary", "wu-maps-to", "wu-expected", "wu-deps"];
const DESIGN_RUN: &[&str] = &[
    "frame",
    "layout",
    "elements",
    "interaction",
    "mockup",
    "tune",
    "receipt",
    "capabilities",
    "proposals",
    "blocked-on",
];
const VERDICTS: &[&str] = &[
    "satisfied",
    "inherited",
    "satisfied-via-proposal",
    "blocked",
    "n/a",
    "violated",
    "violated-inherited",
];
// Hard rules whose Tier-1 predicate exists today. The rest are W7's, and the
// tally line reports the gap on every run so it cannot be quietly forgotten.
// Hand-maintained: nothing derives "this slug has a predicate" from a function
// name. Forgetting an entry undercounts the numerator, which reads as more work
// remaining - the safe direction. A slug listed here without a predicate would
// overclaim, so entries land with their check, never ahead of it.
const HARD_PREDICATES: &[&str] = &["colour-tokens-only"];
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";

fn checklist(artifact: &str) -> Option<&'static [&'static str]> {
    match artifact {
        "epic-plan" => Some(EPIC_PLAN),
        "wu-plan" => Some(WU_PLAN),
        "wu-result" => Some(WU_RESULT),
        "epic-result" => Some(EPIC_RESULT),
        "design-run" => Some(DESIGN_RUN),
        _ => None,
    }
}

pub struct Rule {
    pub slug: String,
    pub hard: bool,
}

#[derive(Deserialize)]
struct Receipt {
    rulebook: String,
    soft: BTreeMap<String, String>,
}

pub fn rules_from_rulebook(html: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    for (table, hard) in [("hard-rules-table", true), ("soft-rules-table", false)] {
        let Some((pos, _)) = attr_positions(html, "data-c").into_iter().find(|(_, v)| v == table)
        else {
            continue;
        };
        let ((start, end), _) = element_content(html, pos);
        for row in html[start..end].split("<tr").skip(1) {
            if let Some(slug) = row_slug(row) {
                out.push(Rule { slug, hard });
            }
        }
    }
    out
}

fn row_slug(row: &str) -> Option<String> {
    let open = row.find("<code>")? + "<code>".len();
    let close = row[open..].find("</code>")? + open;
    let slug = &row[open..close];
    let shaped = !slug.is_empty()
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    shaped.then(|| slug.to_string())
}

pub enum Verdict {
    Ungoverned,
    Exempt(String),
    Checked { artifacts: Vec<String>, failures: Vec<Failure>, summary: Option<String> },
}

impl Verdict {
    pub fn red(&self) -> bool {
        matches!(self, Self::Checked { failures, .. } if !failures.is_empty())
    }
}

#[derive(Clone)]
struct Occurrence {
    value: String,
    span: (usize, usize),
    filled: bool,
}

type Tagged = (usize, Occurrence);

pub fn verify_html(html: &str) -> Verdict {
    verify_html_against(html, None)
}

pub fn verify_html_against(html: &str, rules: Option<&[Rule]>) -> Verdict {
    let Some((artifact_pos, artifact_decl)) =
        attr_positions(html, "data-artifact").into_iter().next()
    else {
        return Verdict::Ungoverned;
    };
    if let Some(status) = exempt_declared(html, artifact_pos) {
        return Verdict::Exempt(status);
    }

    let mut failures = Vec::new();
    let mut artifacts: Vec<String> = Vec::new();
    let mut required: Vec<&str> = Vec::new();
    if artifact_decl.split_whitespace().next().is_none() {
        failures.push(Failure {
            check: "artifact".to_string(),
            what: "data-artifact is declared but empty".to_string(),
            fix: "declare one of: epic-plan, wu-plan, wu-result, epic-result (space-separated when combined)".to_string(),
        });
    }
    for token in artifact_decl.split_whitespace() {
        if let Some(items) = checklist(token) {
            artifacts.push(token.to_string());
            for item in items {
                if !required.contains(item) {
                    required.push(item);
                }
            }
        } else {
            failures.push(Failure {
                check: "artifact".to_string(),
                what: format!("unknown data-artifact \"{token}\""),
                fix: "declare one of: epic-plan, wu-plan, wu-result, epic-result (space-separated when combined)".to_string(),
            });
        }
    }
    if artifacts.is_empty() {
        return Verdict::Checked { artifacts, failures, summary: None };
    }

    let wu_blocks: Vec<Occurrence> = attr_positions(html, "data-wu")
        .into_iter()
        .map(|(pos, value)| {
            let (span, filled) = element_content(html, pos);
            Occurrence { value, span, filled }
        })
        .collect();
    let in_wu_block = |pos: usize| wu_blocks.iter().any(|b| pos >= b.span.0 && pos < b.span.1);
    let (sub_items, top_items): (Vec<Tagged>, Vec<Tagged>) = attr_positions(html, "data-item")
        .into_iter()
        .map(|(pos, value)| {
            let (span, filled) = element_content(html, pos);
            (pos, Occurrence { value, span, filled })
        })
        .partition(|(pos, _)| in_wu_block(*pos));

    check_items(&mut failures, &required, &top_items, "item");
    check_unknown(&mut failures, &required, &top_items, "item");

    if artifacts.iter().any(|a| a == "epic-plan") {
        if wu_blocks.is_empty() {
            failures.push(Failure {
                check: "structure·wu-summaries".to_string(),
                what: "an epic plan carries >= 1 data-wu block; found none".to_string(),
                fix: "add one data-wu=\"<WU id>\" block per WU inside the wu-summaries section, each with wu-summary, wu-maps-to, wu-expected, wu-deps".to_string(),
            });
        }
        for block in &wu_blocks {
            let inside: Vec<Tagged> = sub_items
                .iter()
                .filter(|(pos, _)| *pos >= block.span.0 && *pos < block.span.1)
                .map(|(pos, occ)| (*pos, occ.clone()))
                .collect();
            let label = format!("wu·{}", block.value);
            check_items(&mut failures, WU_SUB, &inside, &label);
            check_unknown(&mut failures, WU_SUB, &inside, &label);
        }
    } else if let Some(block) = wu_blocks.first() {
        failures.push(Failure {
            check: format!("wu·{}", block.value),
            what: "data-wu blocks belong to epic plans; this doc declares none".to_string(),
            fix: "add epic-plan to data-artifact or drop the data-wu block".to_string(),
        });
    }

    let mut summary = None;
    if artifacts.iter().any(|a| a == "design-run") {
        let receipt = top_items.iter().find(|(_, o)| o.value == "receipt").map(|(_, o)| o);
        summary = check_receipt(&mut failures, receipt, html, rules);
        check_composition(&mut failures, html, &top_items);
    }

    Verdict::Checked { artifacts, failures, summary }
}

// The receipt is a YAML document, parsed as one: serde_yaml owns indentation,
// quoting, duplicate keys and error positions, so this only splits the scalar.
fn check_receipt(
    failures: &mut Vec<Failure>,
    receipt: Option<&Occurrence>,
    html: &str,
    rules: Option<&[Rule]>,
) -> Option<String> {
    let receipt = receipt?;
    let parsed: Receipt = match serde_yaml::from_str(&inner_text(html, receipt.span)) {
        Ok(parsed) => parsed,
        Err(e) => {
            failures.push(Failure {
                check: "receipt·yaml".to_string(),
                what: format!("the receipt block is not valid YAML: {e}"),
                fix: "the receipt is `rulebook: <blob>` plus a `soft:` map of `<slug>: <verdict> — <why>`".to_string(),
            });
            return None;
        }
    };
    if parsed.rulebook.trim().is_empty() {
        failures.push(Failure {
            check: "receipt·rulebook".to_string(),
            what: "the receipt names no rulebook version".to_string(),
            fix: "pin it: rulebook: <blob sha of docs/design-rules/design-rules-index.html>"
                .to_string(),
        });
    }
    let Some(rules) = rules else {
        failures.push(Failure {
            check: "receipt·rulebook-unreadable".to_string(),
            what: format!("cannot read {RULEBOOK} - receipt coverage is unverifiable"),
            fix: "run verify-doc from the repo root so the rulebook's slug list can be read"
                .to_string(),
        });
        return None;
    };

    let soft: Vec<&str> = rules.iter().filter(|r| !r.hard).map(|r| r.slug.as_str()).collect();
    let hard: Vec<&str> = rules.iter().filter(|r| r.hard).map(|r| r.slug.as_str()).collect();
    let missing: Vec<&str> =
        soft.iter().copied().filter(|s| !parsed.soft.contains_key(*s)).collect();
    for slug in &missing {
        failures.push(Failure {
            check: format!("receipt·{slug}"),
            what: format!("no row for soft rule \"{slug}\""),
            fix: format!("add `{slug}: <verdict> — <why>` under soft:"),
        });
    }

    let mut counts: Vec<usize> = vec![0; VERDICTS.len()];
    for (slug, value) in &parsed.soft {
        if hard.contains(&slug.as_str()) {
            failures.push(Failure {
                check: "receipt·unknown-slug".to_string(),
                what: format!("\"{slug}\" is a hard rule"),
                fix: "hard rules are machine-checked, not receipted - drop the row".to_string(),
            });
            continue;
        }
        if !soft.contains(&slug.as_str()) {
            let nearest =
                missing.iter().map(|m| (edit_distance(slug, m), *m)).min().filter(|(d, _)| *d <= 3);
            failures.push(Failure {
                check: "receipt·unknown-slug".to_string(),
                what: nearest.map_or_else(
                    || format!("\"{slug}\" is not a rulebook slug"),
                    |(_, m)| format!("unknown slug \"{slug}\" / missing soft rule \"{m}\""),
                ),
                fix: nearest.map_or_else(
                    || format!("drop the row, or add \"{slug}\" to the rulebook first"),
                    |(_, m)| format!("rename the row to `{m}`"),
                ),
            });
            continue;
        }
        let (verdict, why) = value
            .split_once('—')
            .map_or((value.trim(), ""), |(verdict, why)| (verdict.trim(), why.trim()));
        match VERDICTS.iter().position(|v| *v == verdict) {
            Some(i) => counts[i] += 1,
            None => failures.push(Failure {
                check: format!("receipt·{slug}"),
                what: format!("verdict \"{verdict}\" is not one of the seven"),
                fix: format!("use one of: {}", VERDICTS.join(" · ")),
            }),
        }
        if why.is_empty() {
            failures.push(Failure {
                check: format!("receipt·{slug}"),
                what: format!("\"{slug}\" carries no citation"),
                fix: "every row cites its evidence: `<verdict> — <why>` (em dash)".to_string(),
            });
        }
    }

    Some(tally_line(&soft, &missing, &counts, &hard))
}

fn tally_line(soft: &[&str], missing: &[&str], counts: &[usize], hard: &[&str]) -> String {
    let verdicts: Vec<String> = VERDICTS
        .iter()
        .zip(counts)
        .filter(|(_, n)| **n > 0)
        .map(|(v, n)| format!("{n} {v}"))
        .collect();
    let checked: Vec<&str> = hard.iter().copied().filter(|s| HARD_PREDICATES.contains(s)).collect();
    format!(
        "receipt {}/{} soft · {}\nhard rules: {}/{} machine-checked ({}) — {} unchecked, W7 Tier-1",
        soft.len() - missing.len(),
        soft.len(),
        verdicts.join(" · "),
        checked.len(),
        hard.len(),
        checked.join(" "),
        hard.len() - checked.len(),
    )
}

fn check_composition(failures: &mut Vec<Failure>, html: &str, items: &[Tagged]) {
    let item = |name: &str| {
        items.iter().find(|(_, o)| o.value == name).map(|(_, o)| &html[o.span.0..o.span.1])
    };
    if let Some(elements) = item("elements") {
        check_elements_map(failures, elements);
        check_avoid_cited(failures, elements);
        check_gist_and_collapse(
            failures,
            elements,
            "elements·gist",
            "catalog rows",
            "the Fit column",
        );
    }
    if let Some(interaction) = item("interaction") {
        check_gist_and_collapse(
            failures,
            interaction,
            "interaction·gist",
            "use-case rows",
            "the responds column",
        );
    }
    if let Some(tune) = item("tune") {
        check_tune_drawn(failures, tune);
    }
    if let Some(capabilities) = item("capabilities") {
        check_capabilities(failures, capabilities);
    }
    if let Some(mockup) = item("mockup") {
        if !mockup.contains("data-state-toggle") {
            failures.push(Failure {
                check: "mockup·state".to_string(),
                what: "the mockup is static - no state switch".to_string(),
                fix: "add data-state-toggle for the states the Interaction table named".to_string(),
            });
        }
    }
    // the attribute, not the path: a prose mention of an asset composes nothing
    for (attr, asset) in [
        ("src", "https://cdn.tailwindcss.com"),
        ("src", "../../doc-assets/components/tw-config.js"),
        ("src", "../../doc-assets/components/specimen.js"),
    ] {
        if !html.contains(&format!("{attr}=\"{asset}\"")) {
            failures.push(Failure {
                check: "assets·specimen".to_string(),
                what: format!("the run does not load {asset}"),
                fix: "compose the component design files' own rendering instead of re-typing it"
                    .to_string(),
            });
        }
    }
    if let Some(literal) = colour_literal(html) {
        failures.push(Failure {
            check: "mockup·colour-literals".to_string(),
            what: format!("colour literal {literal} in the doc's own CSS"),
            fix: "every colour is a token from ui/tailwind.config.js - write its utility class"
                .to_string(),
        });
    }
    if let Some(frame) = item("frame") {
        check_frame_density(failures, frame);
    }
}

// A reuse is a rejection test that passed, so the trace must show the rejection
// half. Run #3 cited CopyButton's `@use` "any one-click copy affordance"
// correctly and shipped a wrong reuse for a whole stage — both that component's
// and CopyText's `@avoid` lines excluded it, and neither was consulted. The
// reading ORDER is unobservable in the artifact; that the `@avoid` was consulted
// at all is not.
fn check_avoid_cited(failures: &mut Vec<Failure>, elements: &str) {
    let claimed: Vec<&str> = elements_of(elements, "tr")
        .into_iter()
        .filter(|r| ["reuse", "extend"].iter().any(|v| r.contains(&format!(">{v}<"))))
        .collect();
    let silent = claimed.iter().filter(|r| !r.contains("@avoid")).count();
    if silent > 0 {
        failures.push(Failure {
            check: "elements·avoid-cited".to_string(),
            what: format!(
                "{silent} of {} reuse/extend rows cite no @avoid",
                claimed.len()
            ),
            fix: "a reuse names the @avoid it clears, not only the @use it matches - the @avoid is the half that discriminates".to_string(),
        });
    }
}

// A highlight that lights one place traces nothing: the map's whole value is
// clicking a component in the tree and seeing its catalog row light up too. The
// old check passed on data-map plus any single data-key, so a keyed tree over an
// unkeyed table — half the map dead — read as green.
fn check_elements_map(failures: &mut Vec<Failure>, elements: &str) {
    let mut fail = |what: String, fix: &str| {
        failures.push(Failure { check: "elements·map".to_string(), what, fix: fix.to_string() });
    };
    if !elements.contains("data-map") {
        fail(
            "the catalog is not a keyed map".to_string(),
            "wrap tree + table in data-map and key every component mention data-key=\"<slug>\" (behaviour: doc-assets/components/specimen.js)",
        );
        return;
    }
    // The tree half is what <pre> encloses; the catalog half is every key that isn't
    // in it. A row's key rides its <tr> opening tag, which inner-text extraction
    // drops, so the catalog is identified by exclusion rather than by its own tag.
    let tree_spans: Vec<(usize, usize)> = elements_of(elements, "pre")
        .into_iter()
        .map(|body| {
            let at = body.as_ptr() as usize - elements.as_ptr() as usize;
            (at, at + body.len())
        })
        .collect();
    let in_tree = |at: usize| tree_spans.iter().any(|&(s, e)| at >= s && at < e);
    let (tree, catalog): (Vec<_>, Vec<_>) =
        attr_positions(elements, "data-key").into_iter().partition(|&(at, _)| in_tree(at));
    let keys = |half: Vec<(usize, String)>| -> Vec<String> {
        half.into_iter().map(|(_, value)| value).collect()
    };
    let (tree_keys, row_keys) = (keys(tree), keys(catalog));
    for (half, keys, fix) in [
        (
            "tree",
            &tree_keys,
            "key every component mention in the placement tree: data-key=\"<slug>\"",
        ),
        ("catalog", &row_keys, "key every catalog row: <tr data-key=\"<slug>\">"),
    ] {
        if keys.is_empty() {
            fail(format!("the {half} half of the map carries no data-key"), fix);
        }
    }
    let mut unpaired: Vec<&str> = tree_keys
        .iter()
        .filter(|k| !row_keys.contains(k) && !k.is_empty())
        .map(String::as_str)
        .collect();
    unpaired.sort_unstable();
    unpaired.dedup();
    if !unpaired.is_empty() && !row_keys.is_empty() {
        let names = unpaired;
        fail(
            format!("placed but not in the catalog: {}", names.join(" ")),
            "every component in the tree resolves to a catalog row (reuse · extend · propose)",
        );
    }
}

// "Every cell is a clause, not a sentence, and never a paragraph" — the Frame's
// own form rule, mechanised. The mean is what bites: a per-cell cap alone is
// gameable by splitting one paragraph across two rows, which keeps the prose and
// passes the check. Calibrated on the accepted run #2 (mean 9.0, max 35).
const FRAME_CELL_MEAN: usize = 12;
const FRAME_CELL_MAX: usize = 40;
// The `on this screen` cell is a ✓/✗ pair — two clauses by design, so it is
// measured on its own budget instead of dragging the reasoning columns' mean up
// to meet it. Calibrated on the rulebook's 14 DO/DON'T cells (mean 13.7, max 19),
// widened for a screen-specific example naming real copy or a real component.
const FRAME_EXAMPLE_MEAN: usize = 24;
const FRAME_EXAMPLE_MAX: usize = 36;
// purpose · given · not-given — the Frame's three named lines, nothing else
const FRAME_PROSE_BLOCKS: usize = 3;
const FRAME_PROSE_WORDS: usize = 60;
// A gist is the verdict alone — the trace lives in the collapsed half. Calibrated
// on run #3's 30 gists (mean 9.6, max 13): a ceiling that holds today and reds
// the failure it exists for, an agent writing the 900-char trace into the
// visible column.
const GIST_MAX_WORDS: usize = 14;

// A run doc has two readers: an agent that needs the whole trace and a human who
// needs the verdict. Both tables state the conclusion in a .gist and collapse the
// proof behind <details>. What this catches is the regression, not the shape — an
// agent writing the trace straight into the visible cell.
fn check_gist_and_collapse(
    failures: &mut Vec<Failure>,
    fragment: &str,
    check: &str,
    unit: &str,
    column: &str,
) {
    let rows = data_rows(fragment);
    if rows.is_empty() {
        return;
    }
    let mut fail = |what: String, fix: String| {
        failures.push(Failure { check: check.to_string(), what, fix });
    };
    let bare = rows.iter().filter(|r| !r.contains("class=\"gist\"")).count();
    if bare > 0 {
        fail(
            format!("{bare} of {} {unit} carry no gist", rows.len()),
            format!("{column} states the verdict in <div class=\"gist\">, one line"),
        );
    }
    let uncollapsed = rows.iter().filter(|r| !r.contains("<details")).count();
    if uncollapsed > 0 {
        fail(
            format!("{uncollapsed} of {} {unit} collapse nothing", rows.len()),
            "the trace goes behind <details class=\"why\"> - written for the agent, not read by the reviewer".to_string(),
        );
    }
    let longest = gists(fragment).into_iter().map(words).max();
    if let Some(over) = longest.filter(|w| *w > GIST_MAX_WORDS) {
        fail(
            format!("a gist runs {over} words (max {GIST_MAX_WORDS})"),
            "the gist is the verdict; the reasoning is what <details> is for".to_string(),
        );
    }
}

// Tune judges how the screen LOOKS, so its before/after is drawn, not described.
// Run #3's prose version needed three sentences to say what a ragged column and
// an aligned one show at a glance.
fn check_tune_drawn(failures: &mut Vec<Failure>, tune: &str) {
    let rows: Vec<&str> = data_rows(tune);
    let mut fail = |what: String, fix: &str| {
        failures.push(Failure { check: "tune·drawn".to_string(), what, fix: fix.to_string() });
    };
    if rows.is_empty() {
        fail(
            "Tune has no before/after table".to_string(),
            "one row per call: what changed | before (drawn) | after (drawn) | the soft-rule slug",
        );
        return;
    }
    let undrawn = rows.iter().filter(|r| r.matches("class=\"tb\"").count() < 2).count();
    if undrawn > 0 {
        fail(
            format!("{undrawn} of {} tune calls are not drawn", rows.len()),
            "before and after are each a <pre class=\"tb\"> - a stage that judges appearance shows it",
        );
    }
    let unslugged = rows
        .iter()
        .filter(|r| elements_of(r, "td").last().is_none_or(|last| !last.contains("<code>")))
        .count();
    if unslugged > 0 {
        fail(
            format!("{unslugged} of {} tune calls cite no soft-rule slug", rows.len()),
            "a call with no slug to cite is a rulebook hole - it goes to the findings, not into the design",
        );
    }
}

const CAPABILITY_STATES: [&str; 3] = ["used", "deliberately unused", "blocked"];

fn check_capabilities(failures: &mut Vec<Failure>, capabilities: &str) {
    let rows = data_rows(capabilities);
    let mut fail = |what: String, fix: &str| {
        failures.push(Failure {
            check: "capabilities·answered".to_string(),
            what,
            fix: fix.to_string(),
        });
    };
    if rows.is_empty() {
        fail(
            "no capability rows".to_string(),
            "one row per Frame `Given`: what it is | used · deliberately unused · blocked | where / why",
        );
        return;
    }
    let unanswered = rows
        .iter()
        .filter(|r| {
            elements_of(r, "td")
                .get(1)
                .is_none_or(|state| !CAPABILITY_STATES.iter().any(|known| state.contains(known)))
        })
        .count();
    if unanswered > 0 {
        fail(
            format!("{unanswered} of {} capabilities carry no state", rows.len()),
            "each Given ends the run used · deliberately unused · blocked - an unmentioned one reads as correctly constrained rather than under-designed",
        );
    }
}

fn is_example(cell: &str) -> bool {
    cell.contains('✓') || cell.contains('✗')
}

// The gist bodies, found by their opening tag rather than by filtering every
// <div>: a run doc's cells nest divs, and only the one carrying the marker class
// is the gist.
fn gists(fragment: &str) -> Vec<&str> {
    const OPEN: &str = "<div class=\"gist\">";
    fragment
        .match_indices(OPEN)
        .filter_map(|(at, _)| {
            let body = &fragment[at + OPEN.len()..];
            body.find("</div>").map(|end| &body[..end])
        })
        .collect()
}

fn words(fragment: &str) -> usize {
    let mut count = 0;
    let mut in_tag = false;
    let mut in_word = false;
    for c in fragment.chars() {
        match c {
            '<' => {
                in_tag = true;
                in_word = false;
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            c if c.is_whitespace() => in_word = false,
            _ => {
                if !in_word {
                    count += 1;
                    in_word = true;
                }
            }
        }
    }
    count
}

// A stage table's data rows: three cells or more. The header (<th>) and any
// two-cell legend row are not the stage's content.
fn data_rows(fragment: &str) -> Vec<&str> {
    elements_of(fragment, "tr").into_iter().filter(|r| elements_of(r, "td").len() >= 3).collect()
}

// Top-level elements of `tag`, nesting-aware: a <td> holding a <table> ends at
// its own </td>, not the inner one. Naive first-close matching silently
// truncated such a cell, so every word count over it was measured on a
// fragment.
fn elements_of<'a>(fragment: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = fragment;
    while let Some(at) = rest.find(&open) {
        rest = &rest[at + open.len()..];
        let Some(body_start) = rest.find('>').map(|rel| rel + 1) else { break };
        let body = &rest[body_start..];
        let mut depth = 1usize;
        let mut scan = 0usize;
        let end = loop {
            let next_open = body[scan..].find(&open).map(|rel| scan + rel);
            let Some(next_close) = body[scan..].find(&close).map(|rel| scan + rel) else {
                break None;
            };
            match next_open {
                Some(o) if o < next_close => {
                    depth += 1;
                    scan = o + open.len();
                }
                _ => {
                    depth -= 1;
                    if depth == 0 {
                        break Some(next_close);
                    }
                    scan = next_close + close.len();
                }
            }
        };
        let Some(end) = end else { break };
        out.push(&body[..end]);
        rest = &body[end + close.len()..];
    }
    out
}

fn mean_check(
    failures: &mut Vec<Failure>,
    cells: &[usize],
    cap: usize,
    check: &str,
    label: &str,
    fix: &str,
) {
    let total: usize = cells.iter().sum();
    let count = cells.len();
    if count > 0 && total > cap * count {
        let over = cells.iter().filter(|w| **w > cap).count();
        let (whole, tenth) = (total / count, total * 10 / count % 10);
        failures.push(Failure {
            check: check.to_string(),
            what: format!(
                "{label} average {whole}.{tenth} words (max {cap}); {over} of {count} are over"
            ),
            fix: fix.to_string(),
        });
    }
}

fn check_frame_density(failures: &mut Vec<Failure>, frame: &str) {
    let driver_rows = data_rows(frame);
    let unexampled = driver_rows.iter().filter(|r| !is_example(r)).count();
    if unexampled > 0 {
        failures.push(Failure {
            check: "frame·example".to_string(),
            what: format!("{unexampled} of {} driver rows carry no ✓/✗ pair", driver_rows.len()),
            fix: "every driver lands on a concrete example: ✓ what honours it, ✗ the plausible thing that breaks it".to_string(),
        });
    }

    let (examples, reasoning): (Vec<usize>, Vec<usize>) = elements_of(frame, "td")
        .into_iter()
        .filter(|c| words(c) > 0)
        .map(|c| (words(c), is_example(c)))
        .fold((Vec::new(), Vec::new()), |(mut ex, mut re), (w, is_ex)| {
            if is_ex {
                ex.push(w);
            } else {
                re.push(w);
            }
            (ex, re)
        });

    mean_check(
        failures,
        &reasoning,
        FRAME_CELL_MEAN,
        "frame·cell-density",
        "driver cells",
        "every cell is a clause, not a sentence, and never a paragraph - the row is the reasoning, not the cell",
    );
    if let Some(longest) = reasoning.iter().copied().max().filter(|w| *w > FRAME_CELL_MAX) {
        failures.push(Failure {
            check: "frame·cell-length".to_string(),
            what: format!("a driver cell runs {longest} words (max {FRAME_CELL_MAX})"),
            fix: "split the reasoning across the row's columns, or cut it to a clause".to_string(),
        });
    }

    mean_check(
        failures,
        &examples,
        FRAME_EXAMPLE_MEAN,
        "frame·example-density",
        "✓/✗ pairs",
        "an example is two clauses - name the concrete thing, not the reasoning behind it",
    );
    if let Some(longest) = examples.iter().copied().max().filter(|w| *w > FRAME_EXAMPLE_MAX) {
        failures.push(Failure {
            check: "frame·example-length".to_string(),
            what: format!("a ✓/✗ pair runs {longest} words (max {FRAME_EXAMPLE_MAX})"),
            fix: "one concrete ✓ and one concrete ✗ - not a paragraph each".to_string(),
        });
    }

    let prose: Vec<usize> = elements_of(frame, "p").into_iter().map(words).collect();
    if prose.len() > FRAME_PROSE_BLOCKS {
        failures.push(Failure {
            check: "frame·prose-blocks".to_string(),
            what: format!("{} prose blocks in the Frame (max {FRAME_PROSE_BLOCKS})", prose.len()),
            fix: "the Frame is purpose + capabilities + the driver table - commentary about the run belongs in the findings table".to_string(),
        });
    }
    if let Some(longest) = prose.iter().copied().max().filter(|w| *w > FRAME_PROSE_WORDS) {
        failures.push(Failure {
            check: "frame·prose-length".to_string(),
            what: format!("a Frame prose block runs {longest} words (max {FRAME_PROSE_WORDS})"),
            fix: "purpose is one sentence; capabilities are one ·-separated line".to_string(),
        });
    }
}

// A complete hex token only: `#mock` is an id selector, `#2b2622` is a colour.
fn colour_literal(html: &str) -> Option<String> {
    for block in html.split("<style").skip(1) {
        let css = block.split("</style>").next().unwrap_or(block);
        if let Some(func) = ["rgb(", "rgba(", "hsl(", "hsla("].iter().find(|f| css.contains(**f)) {
            return Some((*func).to_string());
        }
        let mut rest = css;
        while let Some(at) = rest.find('#') {
            rest = &rest[at + 1..];
            let hex: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
            let after = rest[hex.len()..].chars().next();
            if matches!(hex.len(), 3 | 4 | 6 | 8)
                && after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            {
                return Some(format!("#{hex}"));
            }
        }
    }
    None
}

fn inner_text(html: &str, span: (usize, usize)) -> String {
    let open_end = html[span.0..].find('>').map_or(span.0, |rel| span.0 + rel + 1);
    let mut out = String::new();
    let mut in_tag = false;
    for c in html[open_end..span.1].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn check_items(
    failures: &mut Vec<Failure>,
    required: &[&str],
    found: &[(usize, Occurrence)],
    scope: &str,
) {
    for item in required {
        let matching: Vec<&Occurrence> =
            found.iter().filter(|(_, o)| o.value == *item).map(|(_, o)| o).collect();
        if matching.is_empty() {
            failures.push(Failure {
                check: format!("{scope}·{item}"),
                what: format!("no data-item=\"{item}\" section found"),
                fix: format!("tag the element holding this item: data-item=\"{item}\""),
            });
        } else if !matching.iter().any(|o| o.filled) {
            failures.push(Failure {
                check: format!("{scope}·{item}"),
                what: format!("data-item=\"{item}\" section is empty"),
                fix: "write the item's content inside the tagged element".to_string(),
            });
        }
    }
}

fn check_unknown(
    failures: &mut Vec<Failure>,
    allowed: &[&str],
    found: &[(usize, Occurrence)],
    scope: &str,
) {
    let missing: Vec<&str> = allowed
        .iter()
        .filter(|item| !found.iter().any(|(_, o)| o.value == **item))
        .copied()
        .collect();
    for (_, occ) in found {
        if allowed.contains(&occ.value.as_str()) {
            continue;
        }
        let nearest = missing
            .iter()
            .map(|m| (edit_distance(&occ.value, m), *m))
            .min()
            .filter(|(d, _)| *d <= 3);
        let what = nearest.map_or_else(
            || format!("unknown tag \"{}\"", occ.value),
            |(_, m)| format!("unknown tag \"{}\" / missing item \"{m}\"", occ.value),
        );
        let fix = nearest.map_or_else(
            || {
                format!(
                    "not in this artifact's checklist - retag or remove data-item=\"{}\"",
                    occ.value
                )
            },
            |(_, m)| format!("retag it data-item=\"{m}\""),
        );
        failures.push(Failure { check: format!("{scope}·unknown-tag"), what, fix });
    }
}

// the markup contract co-locates data-status with data-artifact - a draft
// declaration anywhere else must not silence the checks.
//
// `archived` exempts a doc for the opposite reason to `draft`: not unfinished but
// finished under an older contract. Every predicate reads the LIVE rule set, so
// tightening a stage's form re-opens runs that closed before the rule existed -
// run #3's checks red runs #1 and #2, which are records of what the flow was, not
// artifacts anyone will fix. A run still open is never archived: that would hide
// exactly the reds the flow exists to surface.
fn exempt_declared(html: &str, artifact_pos: usize) -> Option<String> {
    let start = html[..artifact_pos].rfind('<').unwrap_or(0);
    let close = html[start..].find('>')?;
    attr_positions(&html[start..start + close], "data-status")
        .into_iter()
        .next()
        .map(|(_, v)| v)
        .filter(|v| v == "draft" || v == "archived")
}

fn attr_positions(html: &str, name: &str) -> Vec<(usize, String)> {
    let needle = format!("{name}=\"");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = html[from..].find(&needle) {
        let pos = from + rel;
        from = pos + needle.len();
        let before = &html[..pos];
        let preceded = before.chars().next_back().is_some_and(char::is_whitespace);
        let in_tag = match (before.rfind('<'), before.rfind('>')) {
            (Some(lt), Some(gt)) => lt > gt,
            (Some(_), None) => true,
            _ => false,
        };
        if !preceded || !in_tag {
            continue;
        }
        let vstart = pos + needle.len();
        if let Some(vlen) = html[vstart..].find('"') {
            out.push((pos, html[vstart..vstart + vlen].to_string()));
        }
    }
    out
}

fn element_content(html: &str, attr_pos: usize) -> ((usize, usize), bool) {
    let empty = ((attr_pos, attr_pos), false);
    let Some(start) = html[..attr_pos].rfind('<') else { return empty };
    let name: String = html[start + 1..].chars().take_while(char::is_ascii_alphanumeric).collect();
    let Some(open_rel) = html[start..].find('>') else { return empty };
    let open_end = start + open_rel;
    if name.is_empty() || html[..open_end].ends_with('/') {
        return empty;
    }
    let open_tag = format!("<{name}");
    let close_tag = format!("</{name}");
    let mut depth = 1usize;
    let mut cursor = open_end + 1;
    loop {
        let rest = &html[cursor..];
        let Some(close_rel) = find_tag(rest, &close_tag) else { return empty };
        if let Some(open_inner) = find_tag(&rest[..close_rel], &open_tag) {
            depth += 1;
            cursor += open_inner + open_tag.len();
        } else {
            depth -= 1;
            let close_abs = cursor + close_rel;
            if depth == 0 {
                let inner = &html[open_end + 1..close_abs];
                return ((start, close_abs), has_text(inner));
            }
            cursor = close_abs + close_tag.len();
        }
    }
}

fn find_tag(hay: &str, tag: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = hay[from..].find(tag) {
        let pos = from + rel;
        let after = hay[pos + tag.len()..].chars().next();
        if after.is_none_or(|c| c == '>' || c == '/' || c.is_whitespace()) {
            return Some(pos);
        }
        from = pos + tag.len();
    }
    None
}

fn has_text(inner: &str) -> bool {
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag && !c.is_whitespace() => return true,
            _ => {}
        }
    }
    false
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            let next = (prev + cost).min(row[j] + 1).min(row[j + 1] + 1);
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b_chars.len()]
}

pub fn run_paths(repo: &Path, paths: &[&str]) -> Result<(i32, String), String> {
    let single = paths.len() == 1;
    let rules = std::fs::read_to_string(repo.join(RULEBOOK))
        .ok()
        .map(|html| rules_from_rulebook(&html))
        .filter(|rules| !rules.is_empty());
    let mut out = String::new();
    let mut any_red = false;
    for path in paths {
        let resolved =
            if Path::new(path).exists() { Path::new(path).to_path_buf() } else { repo.join(path) };
        let html =
            std::fs::read_to_string(&resolved).map_err(|e| format!("cannot read {path}: {e}"))?;
        let verdict = verify_html_against(&html, rules.as_deref());
        if verdict.red() {
            any_red = true;
        }
        if single || verdict.red() {
            let _ = writeln!(out, "{}", format_verdict(path, &verdict));
        }
    }
    if !single && !any_red {
        out = format!("verify-doc: {} doc(s) checked, none red", paths.len());
    }
    Ok((i32::from(any_red), out.trim_end().to_string()))
}

pub fn format_verdict(path: &str, verdict: &Verdict) -> String {
    match verdict {
        Verdict::Ungoverned => {
            format!("verify-doc ungoverned on {path} - no data-artifact, not checked")
        }
        Verdict::Exempt(status) if status == "archived" => {
            format!("verify-doc archived on {path} - closed under an older contract, not reopened")
        }
        Verdict::Exempt(_) => format!("verify-doc draft on {path} - checks silent until ready"),
        Verdict::Checked { artifacts, failures, summary } if failures.is_empty() => {
            let head = format!("verify-doc green on {path} ({})", artifacts.join(" "));
            match summary {
                Some(s) => format!("{head}\n{s}"),
                None => head,
            }
        }
        Verdict::Checked { failures, .. } => {
            let mut out = String::new();
            let _ = writeln!(out, "verify-doc red on {path}");
            for f in failures {
                let _ = writeln!(out, "FAIL  {:<24} {}", f.check, f.what);
                let _ = writeln!(out, "      fix -> {}", f.fix);
            }
            let _ = write!(out, "{} failed -> fix the doc before building on it", failures.len());
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attr_positions, check_frame_density, edit_distance, elements_of, format_verdict,
        rules_from_rulebook, verify_html, verify_html_against, Verdict, CAPABILITY_STATES,
        FRAME_CELL_MAX, FRAME_CELL_MEAN, FRAME_EXAMPLE_MAX, FRAME_PROSE_WORDS, GIST_MAX_WORDS,
    };

    fn doc(artifact: &str, status: &str, body: &str) -> String {
        let status_attr =
            if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
        format!("<html><body><main data-artifact=\"{artifact}\"{status_attr}>{body}</main></body></html>")
    }

    fn wu_result_body() -> &'static str {
        "<p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
         <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-9</p>\
         <ul data-item=\"follow-ups\"><li>none</li></ul>"
    }

    fn failures(v: &Verdict) -> Vec<String> {
        match v {
            Verdict::Checked { failures, .. } => failures.iter().map(|f| f.check.clone()).collect(),
            _ => panic!("expected a checked verdict"),
        }
    }

    fn what(v: &Verdict, check: &str) -> String {
        match v {
            Verdict::Checked { failures, .. } => {
                let names: Vec<&str> = failures.iter().map(|f| f.check.as_str()).collect();
                let Some(hit) = failures.iter().find(|f| f.check == check) else {
                    panic!("no {check} failure in {names:?}")
                };
                format!("{} | {}", hit.what, hit.fix)
            }
            _ => panic!("expected a checked verdict"),
        }
    }

    const RULEBOOK: &str = "<table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>colour-tokens-only</code></td><td>no literals</td></tr>\
         <tr><td><code>spacing-scale</code></td><td>from the scale</td></tr>\
         </tbody></table>\
         <table data-c=\"soft-rules-table\"><tbody>\
         <tr><td><code>data-ink-first</code></td><td>ink</td><td><code>text-xs</code></td></tr>\
         <tr><td><code>calm-microcopy</code></td><td>calm</td></tr>\
         </tbody></table>";

    fn rules() -> Vec<super::Rule> {
        rules_from_rulebook(RULEBOOK)
    }

    fn receipt(rows: &str) -> String {
        format!("<pre data-item=\"receipt\">rulebook: 28f5437\nsoft:\n{rows}</pre>")
    }

    fn design_run(receipt_block: &str) -> String {
        let body = format!(
            "<script src=\"https://cdn.tailwindcss.com\"></script>\
             <script src=\"../../doc-assets/components/tw-config.js\"></script>\
             <script src=\"../../doc-assets/components/specimen.js\" defer></script>\
             <style>.mock{{color:var(--a-ink);}}</style>\
             <main data-artifact=\"design-run\">\
             <p data-item=\"frame\">drivers</p><p data-item=\"layout\">zones</p>\
             <div data-item=\"elements\" data-map>\
             <pre><span data-key=\"section\">Section</span></pre>\
             <table><tr data-key=\"section\"><td>Section</td><td>reuse</td>\
             <td><div class=\"gist\">fits the labelled-zone use</div>\
             <details class=\"why\"><div>@use every labelled zone · @avoid nesting</div></details></td></tr>\
             </table></div>\
             <table data-item=\"interaction\"><tr><td>land cold</td><td>orient</td>\
             <td><div class=\"gist\">B renders the real verdicts</div>\
             <details class=\"why\"><div>B → status</div></details></td></tr></table>\
             <div data-item=\"mockup\"><button data-state-toggle=\"mock\">flip</button></div>\
             <table data-item=\"tune\"><tr><td>weight</td><td><pre class=\"tb\">before</pre></td>\
             <td><pre class=\"tb\">after</pre></td><td><code>data-ink-first</code></td></tr></table>\
             {receipt_block}\
             <table data-item=\"capabilities\"><tr><td>CA install</td><td>used</td>\
             <td>B1's button</td></tr></table>\
             <p data-item=\"proposals\">band</p><p data-item=\"blocked-on\">verb tokens</p></main>"
        );
        format!("<html><body>{body}</body></html>")
    }

    fn green_run() -> String {
        design_run(&receipt(
            "  data-ink-first: satisfied — no card frames\n  calm-microcopy: inherited — lowercase\n",
        ))
    }

    fn frame_run(frame_body: &str) -> String {
        green_run().replace("<p data-item=\"frame\">drivers</p>", frame_body)
    }

    fn cells(count: usize, words_each: usize) -> String {
        format!("<td>{}</td>", "word ".repeat(words_each)).repeat(count)
    }

    #[test]
    fn a_frame_of_clauses_passes_the_density_check() {
        let body = format!(
            "<div data-item=\"frame\"><p>purpose</p><table>{}</table></div>",
            cells(20, FRAME_CELL_MEAN)
        );
        assert!(!verify_html_against(&frame_run(&body), Some(&rules())).red());
    }

    #[test]
    fn a_frame_of_paragraphs_is_red_by_its_mean() {
        let body = format!(
            "<div data-item=\"frame\"><p>purpose</p><table>{}</table></div>",
            cells(20, FRAME_CELL_MEAN + 6)
        );
        let v = verify_html_against(&frame_run(&body), Some(&rules()));
        assert!(v.red());
        assert!(
            what(&v, "frame·cell-density").contains("average 18.0 words"),
            "{:?}",
            failures(&v)
        );
    }

    // The mean is the load-bearing half: splitting a bloated cell across two rows
    // keeps every cell under the ceiling while the prose survives intact.
    #[test]
    fn splitting_a_long_cell_across_rows_does_not_dodge_the_check() {
        let split = format!(
            "<div data-item=\"frame\"><p>purpose</p><table>{}</table></div>",
            cells(20, FRAME_CELL_MAX / 2)
        );
        let v = verify_html_against(&frame_run(&split), Some(&rules()));
        assert!(v.red(), "half-length cells still average over the mean");
        let checks = failures(&v);
        assert!(checks.contains(&"frame·cell-density".to_string()), "{checks:?}");
        assert!(!checks.contains(&"frame·cell-length".to_string()), "no cell breaks the ceiling");
    }

    #[test]
    fn one_runaway_cell_is_red_even_when_the_mean_holds() {
        let body = format!(
            "<div data-item=\"frame\"><p>purpose</p><table>{}<td>{}</td></table></div>",
            cells(40, 1),
            "word ".repeat(FRAME_CELL_MAX + 1)
        );
        let v = verify_html_against(&frame_run(&body), Some(&rules()));
        assert!(v.red());
        assert!(what(&v, "frame·cell-length").contains("41 words"), "{:?}", failures(&v));
    }

    #[test]
    fn commentary_bolted_onto_the_frame_is_red() {
        let body = "<div data-item=\"frame\"><p>purpose</p><p>given</p><p>not given</p>\
                    <p>why this screen matters</p><table><td>clause</td></table></div>";
        let v = verify_html_against(&frame_run(body), Some(&rules()));
        assert!(v.red());
        assert!(what(&v, "frame·prose-blocks").contains("4 prose blocks"), "{:?}", failures(&v));
    }

    #[test]
    fn a_purpose_paragraph_is_red() {
        let body = format!(
            "<div data-item=\"frame\"><p>{}</p><table><td>clause</td></table></div>",
            "word ".repeat(FRAME_PROSE_WORDS + 1)
        );
        let v = verify_html_against(&frame_run(&body), Some(&rules()));
        assert!(v.red());
        assert!(what(&v, "frame·prose-length").contains("61 words"), "{:?}", failures(&v));
    }

    fn frame_of(run: &str) -> &str {
        run.split_once("data-item=\"frame\"")
            .and_then(|(_, rest)| rest.split_once("data-item=\"layout\""))
            .expect("a run carries a frame and a layout")
            .0
    }

    fn density_of(frame: &str) -> Vec<String> {
        let mut found = Vec::new();
        check_frame_density(&mut found, frame);
        found.into_iter().map(|f| f.check).collect()
    }

    // Calibration: the reasoning columns of the accepted run #2 Frame must pass
    // untouched, or the thresholds measure the checker's taste, not the flow's
    // rule. Run #2 predates the ✓/✗ column (added at run #3), so it is expected
    // to fail frame·example and nothing else — the density limits still hold on a
    // Frame written before them.
    #[test]
    fn the_accepted_run_2_frame_passes_every_density_limit() {
        let reported =
            density_of(frame_of(include_str!("../../docs/work/2026-07-25-dflow-w4-run-2.html")));
        assert_eq!(reported, vec!["frame·example".to_string()], "{reported:?}");
    }

    // The example column exists to be filled: run #1 left 7 of 11 criteria cells
    // blank and the column silently rotted. A ✓/✗ pair per driver is checked.
    #[test]
    fn a_driver_row_without_an_example_is_red() {
        let body = "<div data-item=\"frame\"><table>\
                    <tr><td>driver</td><td>why</td><td>therefore</td>\
                    <td><span>✓ concrete</span><span>✗ concrete</span></td></tr>\
                    <tr><td>driver</td><td>why</td><td>therefore</td><td>a criterion</td></tr>\
                    </table></div>";
        let v = verify_html_against(&frame_run(body), Some(&rules()));
        assert!(v.red());
        assert!(what(&v, "frame·example").contains("1 of 2"), "{:?}", failures(&v));
    }

    // A ✓/✗ pair is two clauses by design; measuring it against the one-clause
    // cap would force the reasoning columns to pad or the examples to go vague.
    #[test]
    fn an_example_pair_is_measured_on_its_own_budget() {
        let clause = FRAME_CELL_MEAN - 1; // each half a legal one-clause cell
        let pair = format!(
            "<td><span>✓ {}</span><span>✗ {}</span></td>",
            "word ".repeat(clause),
            "word ".repeat(clause)
        );
        let body = format!(
            "<div data-item=\"frame\"><table><tr><td>driver</td><td>why</td><td>therefore</td>{pair}</tr></table></div>"
        );
        let reported = density_of(frame_of(&frame_run(&body)));
        assert!(reported.is_empty(), "two legal clauses in one pair must pass: {reported:?}");
    }

    #[test]
    fn a_bloated_example_pair_is_still_red() {
        let pair = format!(
            "<td><span>✓ {}</span><span>✗ {}</span></td>",
            "word ".repeat(FRAME_EXAMPLE_MAX),
            "word ".repeat(FRAME_EXAMPLE_MAX)
        );
        let body = format!(
            "<div data-item=\"frame\"><table><tr><td>driver</td><td>why</td><td>therefore</td>{pair}</tr></table></div>"
        );
        let reported = density_of(frame_of(&frame_run(&body)));
        assert!(reported.contains(&"frame·example-length".to_string()), "{reported:?}");
    }

    #[test]
    fn rulebook_slugs_are_read_from_the_rulebook_tables() {
        let extracted: Vec<(String, bool)> =
            rules().into_iter().map(|r| (r.slug, r.hard)).collect();
        assert_eq!(
            extracted,
            vec![
                ("colour-tokens-only".to_string(), true),
                ("spacing-scale".to_string(), true),
                ("data-ink-first".to_string(), false),
                ("calm-microcopy".to_string(), false),
            ],
            "header rows and non-slug code spans must not become rules"
        );
    }

    #[test]
    fn conforming_design_run_is_green_with_a_computed_tally() {
        let v = verify_html_against(&green_run(), Some(&rules()));
        assert!(!v.red(), "{:?}", failures(&v));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(summary.contains("receipt 2/2 soft · 1 satisfied · 1 inherited"), "{summary}");
        assert!(
            summary.contains("hard rules: 1/2 machine-checked (colour-tokens-only) — 1 unchecked"),
            "{summary}"
        );
    }

    #[test]
    fn a_missing_soft_slug_fails_by_name() {
        let doc = design_run(&receipt("  data-ink-first: satisfied — no card frames\n"));
        let v = verify_html_against(&doc, Some(&rules()));
        assert_eq!(failures(&v), vec!["receipt·calm-microcopy"]);
    }

    #[test]
    fn a_hard_slug_in_the_receipt_is_red() {
        let doc = design_run(&receipt(
            "  data-ink-first: satisfied — x\n  calm-microcopy: satisfied — y\n  spacing-scale: satisfied — z\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()));
        assert!(what(&v, "receipt·unknown-slug").contains("machine-checked, not receipted"));
    }

    #[test]
    fn a_misspelled_slug_pairs_with_the_missing_one() {
        let doc = design_run(&receipt(
            "  data-ink-first: satisfied — x\n  calm-microcopi: satisfied — y\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()));
        let msg = what(&v, "receipt·unknown-slug");
        assert!(msg.contains("calm-microcopi") && msg.contains("calm-microcopy"), "{msg}");
    }

    #[test]
    fn a_verdict_outside_the_enum_and_a_missing_why_each_fail() {
        let doc = design_run(&receipt(
            "  data-ink-first: probably-fine — x\n  calm-microcopy: satisfied\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()));
        assert!(what(&v, "receipt·data-ink-first").contains("not one of the seven"));
        assert!(what(&v, "receipt·calm-microcopy").contains("no citation"));
    }

    #[test]
    fn malformed_receipt_yaml_reports_the_parser_position() {
        let doc = design_run(
            "<pre data-item=\"receipt\">rulebook: 28f5437\nsoft:\n  data-ink-first: ok\n   calm-microcopy: ok\n</pre>",
        );
        let v = verify_html_against(&doc, Some(&rules()));
        let msg = what(&v, "receipt·yaml");
        assert!(msg.contains("line"), "serde_yaml's own position is the error text: {msg}");
    }

    #[test]
    fn an_unreadable_rulebook_is_red_never_silently_green() {
        let v = verify_html(&green_run());
        assert!(v.red(), "coverage that cannot be checked must not pass");
        assert!(what(&v, "receipt·rulebook-unreadable").contains("unverifiable"));
    }

    #[test]
    fn an_unpinned_rulebook_version_is_red() {
        let doc = green_run().replace("rulebook: 28f5437", "rulebook: \"\"");
        let v = verify_html_against(&doc, Some(&rules()));
        assert!(failures(&v).contains(&"receipt·rulebook".to_string()));
    }

    #[test]
    fn an_unkeyed_elements_catalog_is_red() {
        let doc = green_run().replace(" data-map", "").replace("data-key=\"section\"", "");
        let v = verify_html_against(&doc, Some(&rules()));
        assert!(what(&v, "elements·map").contains("specimen.js"));
    }

    // The hole run #3 fell through: data-map plus one data-key passed, so a keyed
    // tree over an unkeyed table read as green while half the map did nothing.
    #[test]
    fn a_keyed_tree_over_an_unkeyed_catalog_is_red() {
        let doc = green_run().replace("<tr data-key=\"section\">", "<tr>");
        assert!(what(&doc_verdict(&doc), "elements·map").contains("catalog half"));
    }

    #[test]
    fn a_keyed_catalog_under_an_unkeyed_tree_is_red() {
        let doc = green_run().replace("<span data-key=\"section\">", "<span>");
        assert!(what(&doc_verdict(&doc), "elements·map").contains("tree half"));
    }

    // A component drawn in the tree with no catalog row never faced the
    // reuse-or-propose pass at all — the stage's one load-bearing question.
    #[test]
    fn a_placed_component_with_no_catalog_row_is_red() {
        let doc = green_run().replace(
            "<pre><span data-key=\"section\">Section</span></pre>",
            "<pre><span data-key=\"section\">Section</span>\
             <span data-key=\"notice\">Notice</span></pre>",
        );
        assert!(what(&doc_verdict(&doc), "elements·map").contains("notice"));
    }

    #[test]
    fn the_run_3_elements_map_is_a_live_two_half_map() {
        let doc = include_str!("../../docs/work/2026-07-26-dflow-w4-run-3.html");
        assert!(!failures(&doc_verdict(doc)).contains(&"elements·map".to_string()));
    }

    #[test]
    fn a_static_mockup_is_red() {
        let doc = green_run().replace("data-state-toggle=\"mock\"", "");
        assert!(what(&doc_verdict(&doc), "mockup·state").contains("Interaction table"));
    }

    #[test]
    fn a_catalog_row_that_states_the_trace_instead_of_a_gist_is_red() {
        let doc = green_run().replace("<div class=\"gist\">fits the labelled-zone use</div>", "");
        assert!(what(&doc_verdict(&doc), "elements·gist").contains("carry no gist"));
    }

    #[test]
    fn a_use_case_row_that_collapses_nothing_is_red() {
        let doc = green_run()
            .replace("<details class=\"why\"><div>B → status</div></details>", "B → status");
        assert!(what(&doc_verdict(&doc), "interaction·gist").contains("collapse nothing"));
    }

    // The regression the pattern exists for: the trace written straight into the
    // visible column instead of behind the toggle.
    #[test]
    fn a_gist_that_runs_long_is_red() {
        let doc = green_run()
            .replace("fits the labelled-zone use", &"clause ".repeat(GIST_MAX_WORDS + 1));
        assert!(what(&doc_verdict(&doc), "elements·gist").contains("max 14"));
    }

    #[test]
    fn a_reuse_that_cites_only_its_use_line_is_red() {
        let doc = green_run().replace("@avoid nesting", "");
        assert!(what(&doc_verdict(&doc), "elements·avoid-cited").contains("cite no @avoid"));
    }

    // A propose row has no @avoid to clear - the component does not exist yet.
    #[test]
    fn a_propose_row_owes_no_avoid_citation() {
        let doc =
            green_run().replace("<td>reuse</td>", "<td>propose</td>").replace("@avoid nesting", "");
        assert!(!failures(&doc_verdict(&doc)).contains(&"elements·avoid-cited".to_string()));
    }

    #[test]
    fn a_tune_call_described_instead_of_drawn_is_red() {
        let doc = green_run().replace("<pre class=\"tb\">before</pre>", "it was cramped");
        assert!(what(&doc_verdict(&doc), "tune·drawn").contains("not drawn"));
    }

    #[test]
    fn a_tune_call_with_no_slug_is_red() {
        let doc = green_run().replace("<code>data-ink-first</code>", "it reads better");
        assert!(what(&doc_verdict(&doc), "tune·drawn").contains("no soft-rule slug"));
    }

    #[test]
    fn a_prose_tune_stage_is_red() {
        let doc = green_run().replace(
            "<table data-item=\"tune\"><tr><td>weight</td><td><pre class=\"tb\">before</pre></td>\
             <td><pre class=\"tb\">after</pre></td><td><code>data-ink-first</code></td></tr></table>",
            "<p data-item=\"tune\">Before: cramped. After: roomier.</p>",
        );
        assert!(what(&doc_verdict(&doc), "tune·drawn").contains("no before/after table"));
    }

    // Run #3 designed a received capability away and only the cross-check caught
    // it: the receipt forces coverage of rules, this forces coverage of capabilities.
    #[test]
    fn a_capability_with_no_state_is_red() {
        let doc = green_run().replace("<td>used</td>", "<td>we'll see</td>");
        assert!(what(&doc_verdict(&doc), "capabilities·answered").contains("carry no state"));
    }

    // The vocabulary is closed on purpose. Run #3 ended a capability "dropped,
    // then recovered" and the state list was widened to accept it, so the gate
    // rubber-stamped the very defect it exists to catch. A run whose outcome does
    // not fit the three states is reporting an unfinished design, not a missing
    // state.
    #[test]
    fn a_fourth_capability_state_is_not_admitted_by_widening_the_list() {
        assert_eq!(CAPABILITY_STATES.len(), 3);
        let doc = green_run().replace("<td>used</td>", "<td>dropped, then recovered</td>");
        assert!(what(&doc_verdict(&doc), "capabilities·answered").contains("carry no state"));
    }

    #[test]
    fn a_run_that_never_answers_its_capabilities_is_red() {
        let doc = green_run().replace(" data-item=\"capabilities\"", "");
        assert!(failures(&doc_verdict(&doc)).contains(&"item·capabilities".to_string()));
    }

    // A cell holding a nested table ends at its OWN close tag: the naive scan
    // stopped at the inner one and measured every word count on a fragment.
    #[test]
    fn elements_of_closes_on_the_matching_tag_not_the_first() {
        let nested = "<td>outer <table><tr><td>inner</td></tr></table> tail</td>";
        assert_eq!(
            elements_of(nested, "td"),
            vec!["outer <table><tr><td>inner</td></tr></table> tail"]
        );
    }

    #[test]
    fn the_run_3_form_rules_hold_on_the_real_doc() {
        let doc = include_str!("../../docs/work/2026-07-26-dflow-w4-run-3.html");
        let reds = failures(&doc_verdict(doc));
        for check in [
            "elements·gist",
            "interaction·gist",
            "tune·drawn",
            "capabilities·answered",
            "elements·avoid-cited",
        ] {
            assert!(!reds.contains(&check.to_string()), "{check} red on run #3: {reds:?}");
        }
    }

    fn doc_verdict(html: &str) -> Verdict {
        verify_html_against(html, Some(&rules()))
    }

    #[test]
    fn a_run_that_does_not_link_the_shared_specimens_is_red() {
        let doc = green_run()
            .replace("src=\"../../doc-assets/components/tw-config.js\"", "src=\"own-theme.js\"");
        assert!(what(&doc_verdict(&doc), "assets·specimen").contains("re-typing"));

        let mentioned =
            doc.replace("<style>", "<a href=\"x\">doc-assets/components/tw-config.js</a><style>");
        assert!(
            doc_verdict(&mentioned).red(),
            "naming the theme in prose composes nothing - the attribute is the proof"
        );
    }

    #[test]
    fn a_colour_literal_is_red_but_an_id_selector_is_not() {
        let hex = green_run().replace("color:var(--a-ink)", "color:#f7f5f0");
        assert!(what(&doc_verdict(&hex), "mockup·colour-literals").contains("#f7f5f0"));

        let func = green_run().replace("color:var(--a-ink)", "color:rgba(0,0,0,.2)");
        assert!(what(&doc_verdict(&func), "mockup·colour-literals").contains("rgba("));

        let id_selector = green_run().replace(".mock{", "#mock{");
        assert!(!doc_verdict(&id_selector).red(), "#mock is a selector, not a colour");

        let tokens = green_run().replace(
            "color:var(--a-ink)",
            "background:color-mix(in srgb, var(--accent) 24%, transparent)",
        );
        assert!(!doc_verdict(&tokens).red(), "color-mix over tokens is not a literal");
    }

    #[test]
    fn no_artifact_means_ungoverned() {
        assert!(matches!(
            verify_html("<html><body><p>plain doc</p></body></html>"),
            Verdict::Ungoverned
        ));
    }

    #[test]
    fn artifact_mentioned_as_text_does_not_govern() {
        let html = "<html><body><p>use <code>data-artifact=\"epic-plan\"</code> to opt in</p></body></html>";
        assert!(matches!(verify_html(html), Verdict::Ungoverned));
    }

    #[test]
    fn draft_is_silent() {
        let v = verify_html(&doc("wu-result", "draft", ""));
        assert!(matches!(&v, Verdict::Exempt(s) if s == "draft"));
        assert!(format_verdict("d.html", &v).contains("draft"));
    }

    // Tightening a stage's form re-opens every run that closed before the rule
    // existed. An archived run is a record of what the flow was, not work anyone
    // will redo.
    #[test]
    fn an_archived_run_is_silent_but_a_ready_one_is_not() {
        let stale = design_run(&receipt("  data-ink-first: satisfied — x\n"))
            .replace("<div class=\"gist\">fits the labelled-zone use</div>", "");
        assert!(!failures(&doc_verdict(&stale)).is_empty(), "a ready run is checked");
        let archived = stale.replace(
            "data-artifact=\"design-run\"",
            "data-artifact=\"design-run\" data-status=\"archived\"",
        );
        let v = verify_html(&archived);
        assert!(matches!(&v, Verdict::Exempt(s) if s == "archived"), "archived exempts");
        let line = format_verdict("run.html", &v);
        assert!(line.contains("archived") && !line.contains("until ready"), "{line}");
    }

    #[test]
    fn the_closed_runs_are_archived_so_a_new_rule_does_not_reopen_them() {
        for run in [
            include_str!("../../docs/work/2026-07-16-dflow-run-1.html"),
            include_str!("../../docs/work/2026-07-25-dflow-w4-run-2.html"),
        ] {
            assert!(
                matches!(verify_html(run), Verdict::Exempt(s) if s == "archived"),
                "a closed run must be archived"
            );
        }
    }

    #[test]
    fn absent_status_means_ready_and_checked() {
        let v = verify_html(&doc("wu-result", "", ""));
        assert!(v.red());
    }

    #[test]
    fn draft_status_on_an_unrelated_element_does_not_silence_the_checks() {
        let html = "<html><body><div data-status=\"draft\">aside</div>\
             <main data-artifact=\"wu-result\"><p data-item=\"outcome\">x</p></main></body></html>";
        assert!(verify_html(html).red(), "status must sit next to data-artifact to count");
    }

    #[test]
    fn unclosed_artifact_tag_fails_toward_checking_not_draft_silence() {
        let html = "<main data-artifact=\"wu-result\" data-status=\"draft\" truncated";
        assert!(verify_html(html).red(), "a doc whose tag never closes must never fall silent");
    }

    #[test]
    fn unknown_artifact_is_red() {
        let v = verify_html(&doc("epic-planz", "", ""));
        assert_eq!(failures(&v), vec!["artifact"]);
    }

    #[test]
    fn empty_artifact_declaration_is_red_not_green() {
        let v = verify_html(&doc("", "", "<p data-item=\"outcome\">x</p>"));
        assert!(v.red(), "data-artifact=\"\" is an explicit but empty declaration");
        assert_eq!(failures(&v), vec!["artifact"]);
    }

    #[test]
    fn conforming_wu_result_is_green() {
        let v = verify_html(&doc("wu-result", "ready", wu_result_body()));
        assert!(!v.red());
    }

    #[test]
    fn missing_and_empty_items_each_fail_by_name() {
        let body = "<p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">   </p>\
                    <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-9</p>";
        let v = verify_html(&doc("wu-result", "", body));
        let checks = failures(&v);
        assert!(checks.contains(&"item·deviations".to_string()), "whitespace-only is empty");
        assert!(checks.contains(&"item·follow-ups".to_string()), "absent tag is missing");
        assert_eq!(checks.len(), 2);
    }

    #[test]
    fn misspelled_tag_pairs_unknown_with_missing() {
        let body = wu_result_body().replace("data-item=\"deviations\"", "data-item=\"deviatons\"");
        let v = verify_html(&doc("wu-result", "", body.as_str()));
        let Verdict::Checked { failures, .. } = &v else { panic!() };
        assert!(failures.iter().any(|f| f.check == "item·deviations"));
        let unknown = failures.iter().find(|f| f.check == "item·unknown-tag").expect("typo guard");
        assert!(unknown.what.contains("\"deviatons\"") && unknown.what.contains("\"deviations\""));
        assert!(unknown.fix.contains("data-item=\"deviations\""));
    }

    #[test]
    fn epic_plan_requires_wu_blocks_with_all_sub_tags() {
        let items = "<p data-item=\"intent\">i</p><p data-item=\"use-cases\">u</p>\
             <p data-item=\"expected\">e</p><p data-item=\"non-goals\">n</p>\
             <p data-item=\"e2e-plan\">e</p><p data-item=\"distillation-plan\">d</p>\
             <p data-item=\"open-questions\">none</p>";
        let no_blocks = format!("{items}<div data-item=\"wu-summaries\">text</div>");
        let v = verify_html(&doc("epic-plan", "", &no_blocks));
        assert!(failures(&v).contains(&"structure·wu-summaries".to_string()));

        let block_missing_dep = format!(
            "{items}<div data-item=\"wu-summaries\"><div data-wu=\"E-X.W1\">\
             <p data-item=\"wu-summary\">s</p><p data-item=\"wu-maps-to\">m</p>\
             <p data-item=\"wu-expected\">e</p></div></div>"
        );
        let v = verify_html(&doc("epic-plan", "", &block_missing_dep));
        assert!(failures(&v).contains(&"wu·E-X.W1·wu-deps".to_string()));

        let full_block = format!(
            "{items}<div data-item=\"wu-summaries\"><div data-wu=\"E-X.W1\">\
             <p data-item=\"wu-summary\">s</p><p data-item=\"wu-maps-to\">m</p>\
             <p data-item=\"wu-expected\">e</p><p data-item=\"wu-deps\">none</p></div></div>"
        );
        assert!(!verify_html(&doc("epic-plan", "", &full_block)).red());
    }

    #[test]
    fn combined_artifact_needs_the_union_once() {
        let combined = "<p data-item=\"outcome\">o</p><p data-item=\"deviations\">n</p>\
             <p data-item=\"evidence\">e</p><p data-item=\"distillation\">d</p>\
             <ul data-item=\"follow-ups\"><li>none</li></ul><p data-item=\"wu-rollup\">r</p>";
        assert!(!verify_html(&doc("epic-result wu-result", "", combined)).red());
        let missing_rollup = combined.replace("<p data-item=\"wu-rollup\">r</p>", "");
        let v = verify_html(&doc("epic-result wu-result", "", &missing_rollup));
        assert_eq!(failures(&v), vec!["item·wu-rollup"]);
    }

    #[test]
    fn wu_block_outside_an_epic_plan_is_red() {
        let body = format!(
            "{}<div data-wu=\"E-X.W1\"><p data-item=\"wu-summary\">s</p></div>",
            wu_result_body()
        );
        let v = verify_html(&doc("wu-result", "", &body));
        assert!(failures(&v).contains(&"wu·E-X.W1".to_string()));
    }

    #[test]
    fn attr_scan_requires_a_real_tag_position() {
        let html =
            "<p data-item=\"a\">x</p><p>text data-item=\"b\" more</p><code>data-item=\"c\"</code>";
        let values: Vec<String> =
            attr_positions(html, "data-item").into_iter().map(|(_, v)| v).collect();
        assert_eq!(values, vec!["a"]);
    }

    #[test]
    fn nested_same_tag_elements_resolve_their_own_close() {
        let html = "<html><body><main data-artifact=\"wu-result\">\
             <div data-item=\"outcome\"><div>inner</div></div>\
             <div data-item=\"deviations\"><div></div></div>\
             <p data-item=\"evidence\">e</p><p data-item=\"distillation\">d</p>\
             <p data-item=\"follow-ups\">none</p></main></body></html>";
        let v = verify_html(html);
        assert_eq!(failures(&v), vec!["item·deviations"], "nested-but-empty div is empty");
    }

    #[test]
    fn distance_is_small_for_typos_only() {
        assert_eq!(edit_distance("expcted", "expected"), 1);
        assert!(edit_distance("banana", "expected") > 3);
    }
}
