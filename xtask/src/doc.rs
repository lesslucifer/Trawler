use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
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
// Hard rules whose Tier-1 predicate exists today. Hand-maintained: nothing
// derives "this slug has a predicate" from a function name. Forgetting an entry
// undercounts the numerator, which reads as more work remaining - the safe
// direction. A slug listed here without a predicate would overclaim, so entries
// land with their check, never ahead of it.
const HARD_PREDICATES: &[&str] = &[
    "colour-tokens-only",
    "spacing-scale",
    "type-tokens-only",
    "radius-tokens",
    "zero-accent",
    "overflow-declared",
    "state-declarations",
    "wrap-not-truncate",
    "mono-for-wire-data",
    "wire-order-preserved",
    "uppercase-mute-labels",
    "keyboard-parity",
    "colour-plus-text",
];
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const VOCABULARY: &str = "scripts/design-vocabulary.mjs";

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
        let Some(declared) = attrs(html, "data-c").into_iter().find(|a| a.value == table) else {
            continue;
        };
        let ((start, end), _) = element_content(html, declared.tag);
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

// The closed vocabulary, emitted by scripts/design-vocabulary.mjs from
// ui/tailwind.config.js. Rust cannot read an ES module, so the config is read by
// the same oracle the lint rule uses and handed over as JSON - one source of
// truth, two surfaces (DEC-17). JSON is YAML, so this needs no new parser.
#[derive(Deserialize)]
pub struct Vocabulary {
    legal: BTreeSet<String>,
    rules: BTreeMap<String, Vec<String>>,
    values: BTreeMap<String, Vec<String>>,
    modifiers: BTreeMap<String, Vec<String>>,
    chromatic: BTreeSet<String>,
    chroma_floor: u32,
}

impl Vocabulary {
    fn emits(&self, base: &str) -> bool {
        if base.is_empty() || self.legal.contains(base) {
            return true;
        }
        // A `/step` draws from the scale its own base draws from; the wrong
        // pairing emits nothing at all, not even the base.
        match base.rsplit_once('/') {
            Some((owner, step)) => {
                self.modifiers.get(owner).is_some_and(|steps| steps.iter().any(|s| s == step))
            }
            None => false,
        }
    }

    fn governing(&self, base: &str) -> Vec<&str> {
        self.rules
            .iter()
            .filter(|(_, prefixes)| prefixes.iter().any(|p| base.starts_with(p.as_str())))
            .map(|(slug, _)| slug.as_str())
            .collect()
    }

    fn token<'a>(&self, base: &'a str) -> Option<&'a str> {
        let prefixes = self.rules.get("colour-tokens-only")?;
        let prefix =
            prefixes.iter().filter(|p| base.starts_with(p.as_str())).max_by_key(|p| p.len())?;
        Some(base[prefix.len()..].split('/').next().unwrap_or_default())
    }

    fn declares(&self, slug: &str, value: &str) -> bool {
        self.values.get(slug).is_some_and(|known| known.iter().any(|v| v == value))
    }
}

pub fn vocabulary(repo: &Path) -> Result<Vocabulary, String> {
    let script = repo.join(VOCABULARY);
    let out = Command::new("node")
        .arg(&script)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("cannot launch node for {VOCABULARY}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{VOCABULARY} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_yaml::from_slice(&out.stdout)
        .map_err(|e| format!("{VOCABULARY} emitted no vocabulary: {e}"))
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
    verify_html_against(html, None, None)
}

pub fn verify_html_against(
    html: &str,
    rules: Option<&[Rule]>,
    vocabulary: Option<&Vocabulary>,
) -> Verdict {
    let Some(artifact) = attrs(html, "data-artifact").into_iter().next() else {
        return Verdict::Ungoverned;
    };
    if let Some(status) = exempt_declared(html, &artifact) {
        return Verdict::Exempt(status);
    }

    let mut failures = Vec::new();
    check_escaped_markup(&mut failures, html);
    let mut artifacts: Vec<String> = Vec::new();
    let mut required: Vec<&str> = Vec::new();
    if artifact.value.split_whitespace().next().is_none() {
        failures.push(Failure {
            check: "artifact".to_string(),
            what: "data-artifact is declared but empty".to_string(),
            fix: "declare one of: epic-plan, wu-plan, wu-result, epic-result (space-separated when combined)".to_string(),
        });
    }
    for token in artifact.value.split_whitespace() {
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

    let wu_blocks: Vec<Occurrence> = attrs(html, "data-wu")
        .into_iter()
        .map(|block| {
            let (span, filled) = element_content(html, block.tag);
            Occurrence { value: block.value, span, filled }
        })
        .collect();
    let in_wu_block = |pos: usize| wu_blocks.iter().any(|b| pos >= b.span.0 && pos < b.span.1);
    let (sub_items, top_items): (Vec<Tagged>, Vec<Tagged>) = attrs(html, "data-item")
        .into_iter()
        .map(|item| {
            let (span, filled) = element_content(html, item.tag);
            (item.at, Occurrence { value: item.value, span, filled })
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
        let allowed = read_pragmas(&mut failures, html);
        check_hard_rules(&mut failures, html, &top_items, vocabulary, &allowed);
        summary = summary.map(|s| format!("{s}\n{}", allowed.tally()));
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
    let unchecked: Vec<&str> =
        hard.iter().copied().filter(|s| !HARD_PREDICATES.contains(s)).collect();
    // "(design)" is the whole point of the line: the predicates judge the design
    // run, and ui/src/ is judged by design/vocabulary. Without the label a full
    // count reads as a claim about the built UI, which no predicate here makes.
    let gap = if unchecked.is_empty() {
        String::new()
    } else {
        format!(" — {} unchecked: {}", unchecked.len(), unchecked.join(" "))
    };
    format!(
        "receipt {}/{} soft · {}\nhard rules: {}/{} machine-checked (design){gap}",
        soft.len() - missing.len(),
        soft.len(),
        verdicts.join(" · "),
        hard.len() - unchecked.len(),
        hard.len(),
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

// ─── the thirteen hard rules ────────────────────────────────────────────────
//
// Each predicate is named for its slug and judges the DESIGN RUN, never
// ui/src/ - that surface is design/vocabulary's (the rulebook's binds axis).
// Five read the closed vocabulary; the rest read what the doc declares.
fn check_hard_rules(
    failures: &mut Vec<Failure>,
    html: &str,
    items: &[Tagged],
    vocabulary: Option<&Vocabulary>,
    allowed: &Allowances,
) {
    let item = |name: &str| {
        items.iter().find(|(_, o)| o.value == name).map(|(_, o)| &html[o.span.0..o.span.1])
    };
    let Some(vocabulary) = vocabulary else {
        failures.push(Failure {
            check: "vocabulary·unreadable".to_string(),
            what: format!("cannot read the closed vocabulary via {VOCABULARY}"),
            fix: "run verify-doc from the repo root with node available - five hard rules resolve every class against ui/tailwind.config.js and cannot pass unread".to_string(),
        });
        return;
    };
    check_vocabulary(failures, html, vocabulary, allowed);
    check_uppercase_mute_labels(failures, html, vocabulary);
    check_colour_plus_text(failures, html, vocabulary);
    check_wire_data(failures, html);
    if let Some(layout) = item("layout") {
        check_overflow_declared(failures, layout);
    }
    if let Some(elements) = item("elements") {
        check_state_declarations(failures, elements);
        check_wire_order_preserved(failures, elements);
    }
    if let Some(interaction) = item("interaction") {
        check_keyboard_parity(failures, interaction);
    }
}

// colour-tokens-only · spacing-scale · type-tokens-only · radius-tokens ·
// zero-accent. One scan, five verdicts: a class is judged by whether the config
// emits it, and the rules it answers to are the ones whose scales own its
// prefix. `text-` belongs to two scales, so an off-list `text-*` answers to both
// - the alternative is a precedence rule that would silently exempt one of them.
fn check_vocabulary(
    failures: &mut Vec<Failure>,
    html: &str,
    vocabulary: &Vocabulary,
    allowed: &Allowances,
) {
    let mut reported: BTreeSet<(String, String)> = BTreeSet::new();
    for (pos, cls) in authored_classes(html) {
        let base = base_class(&cls);
        if vocabulary.emits(base) {
            continue;
        }
        if allowed.covers(line_of(html, pos), base) {
            continue;
        }
        let arbitrary = arbitrary_value(base);
        for slug in vocabulary.governing(base) {
            if !reported.insert((slug.to_string(), base.to_string())) {
                continue;
            }
            failures.push(Failure {
                check: format!("{slug}·off-vocabulary"),
                what: match arbitrary {
                    Some(value) => format!("`{base}` names {value} inline, not a token"),
                    None => format!("`{base}` resolves to nothing - Tailwind emits no CSS for it"),
                },
                fix: "every value is a token in ui/tailwind.config.js - name it there, or use the token that already means this".to_string(),
            });
        }
        // zero-accent is not the membership test its siblings are: with the
        // config closed, no legal class can name an off-set colour. What it
        // still owns is the value written past the config - an inline chromatic
        // colour, which the vocabulary check reds as untokenised and this one
        // reds as chromatic.
        if let Some(value) = arbitrary.filter(|v| is_chromatic(v, vocabulary.chroma_floor)) {
            if reported.insert(("zero-accent".to_string(), base.to_string())) {
                failures.push(Failure {
                    check: "zero-accent·off-set".to_string(),
                    what: format!("`{base}` paints {value}, a chromatic colour outside the set"),
                    fix: format!(
                        "chromatic colour is the config's set and nothing else: {}",
                        vocabulary.chromatic.iter().cloned().collect::<Vec<_>>().join(" ")
                    ),
                });
            }
        }
    }

    // The mockup may own CSS a class cannot reach - the `.a-` assembly rules.
    // Those declarations carry raw magnitudes, and every value that leaked past
    // the vocabulary in a shipped run leaked here.
    for (selector, property, value) in mockup_declarations(html) {
        let Some(slug) = governed_property(&property) else { continue };
        for part in value.split_whitespace() {
            let part = normalise(part);
            if RELATIONSHIP_KEYWORDS.contains(&part.as_str())
                || part.starts_with("var(")
                || vocabulary.declares(slug, &part)
            {
                continue;
            }
            failures.push(Failure {
                check: format!("{slug}·raw-value"),
                what: format!("{selector} writes {property}: {part}, which is no token"),
                fix: "the mockup's own CSS is assembly only - every magnitude in it is a ui/tailwind.config.js value or it is not in the vocabulary".to_string(),
            });
        }
    }
}

// A CSS property this rulebook governs. Shorthands that mix keywords with
// magnitudes (`border`, `font`, `background`) are left alone: their non-length
// halves would have to be special-cased, and each has a longhand that carries
// the value the rule is about.
fn governed_property(property: &str) -> Option<&'static str> {
    const SPACING: &[&str] = &[
        "padding",
        "margin",
        "gap",
        "row-gap",
        "column-gap",
        "width",
        "height",
        "min-width",
        "min-height",
        "max-width",
        "max-height",
        "inset",
        "top",
        "right",
        "bottom",
        "left",
        "flex-basis",
    ];
    const TYPE: &[&str] = &["font-size", "font-family", "line-height", "letter-spacing"];
    let base = property.split('-').next().unwrap_or(property);
    if SPACING.contains(&property)
        || (matches!(base, "padding" | "margin") && property.starts_with(base))
    {
        return Some("spacing-scale");
    }
    if TYPE.contains(&property) {
        return Some("type-tokens-only");
    }
    (property == "border-radius").then_some("radius-tokens")
}

// "Relationship keywords are always legal" - the W6 plan's one standing ruling.
// A value that says how a box relates to its content or its parent carries no
// magnitude for a scale to govern.
const RELATIONSHIP_KEYWORDS: &[&str] = &[
    "auto",
    "none",
    "inherit",
    "initial",
    "unset",
    "revert",
    "normal",
    "min-content",
    "max-content",
    "fit-content",
    "0px",
    "100%",
];

fn normalise(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches(',').to_ascii_lowercase();
    if trimmed == "0" {
        return "0px".to_string();
    }
    match trimmed.strip_prefix('.') {
        Some(rest) => format!("0.{rest}"),
        None => trimmed,
    }
}

// uppercase-mute-labels. A label is what is uppercased and is not a payload:
// `font-mono` is the wire-data channel, so MethodLabel's verb is a value that
// happens to be uppercase, never a zone label. Both surfaces are read, because
// both archived runs wrote their labels' tracking as a raw value - one as an
// arbitrary class, one as a CSS declaration.
fn check_uppercase_mute_labels(failures: &mut Vec<Failure>, html: &str, vocabulary: &Vocabulary) {
    let mut fail = |what: String| {
        failures.push(Failure {
            check: "uppercase-mute-labels·pattern".to_string(),
            what,
            fix: "a zone or column label is the uppercase-mute set: uppercase + tracking-label + a mute ink - never a tracking written by hand".to_string(),
        });
    };
    for declared in attrs(html, "class") {
        let classes: Vec<&str> = declared.value.split_whitespace().map(base_class).collect();
        if !classes.contains(&"uppercase") || classes.contains(&"font-mono") {
            continue;
        }
        if !classes.contains(&"tracking-label") {
            let tracking =
                classes.iter().find(|c| c.starts_with("tracking-")).copied().unwrap_or("nothing");
            fail(format!("an uppercase label sets its tracking with {tracking}"));
        }
        if !classes.iter().any(|c| c.starts_with("text-ink-")) {
            fail("an uppercase label carries no mute ink".to_string());
        }
    }
    for (selector, property, value) in mockup_declarations(html) {
        if property != "text-transform" || value.trim() != "uppercase" {
            continue;
        }
        let tracking = mockup_declarations(html)
            .into_iter()
            .find(|(s, p, _)| *s == selector && p == "letter-spacing")
            .map(|(_, _, v)| v);
        match tracking {
            Some(v) if vocabulary.declares("type-tokens-only", &normalise(&v)) => {}
            Some(v) => fail(format!("{selector} tracks its uppercase label at {v}")),
            None => fail(format!("{selector} uppercases without the tracking the pattern names")),
        }
    }
}

// colour-plus-text. Colour is never the only channel, so an element painted with
// a chromatic token says in words what the colour says in hue. A tinted glyph
// counts - it is a character; an empty span painted `ok` does not.
fn check_colour_plus_text(failures: &mut Vec<Failure>, html: &str, vocabulary: &Vocabulary) {
    for declared in attrs(html, "class") {
        let chromatic = declared.value.split_whitespace().map(base_class).find(|cls| {
            vocabulary.emits(cls)
                && vocabulary.token(cls).is_some_and(|t| vocabulary.chromatic.contains(t))
        });
        let Some(cls) = chromatic else { continue };
        let (_, filled) = element_content(html, declared.tag);
        if !filled {
            failures.push(Failure {
                check: "colour-plus-text·channel".to_string(),
                what: format!("`{cls}` paints an element that says nothing"),
                fix: "the meaning rides a word or a glyph as well as the hue - a colour-only element is invisible to anyone who cannot see it".to_string(),
            });
        }
    }
}

// mono-for-wire-data + wrap-not-truncate. Which elements are payload is a fact
// about content, not about markup, so the mockup declares it: data-wire on every
// element rendering a verbatim payload, or data-wire="none" once when the screen
// renders none. Silence is not the third answer - an unmarked mockup reads as
// correctly typed when nobody looked.
const WIRE_HOSTILE: &[&str] = &["truncate", "text-ellipsis", "whitespace-nowrap"];

fn check_wire_data(failures: &mut Vec<Failure>, html: &str) {
    let marked = attrs(html, "data-wire");
    let declared = attrs(html, "class");
    let bases = |declared: &Attr| -> Vec<String> {
        declared.value.split_whitespace().map(|c| base_class(c).to_string()).collect()
    };
    if marked.is_empty() {
        failures.push(Failure {
            check: "mono-for-wire-data·undeclared".to_string(),
            what: "the mockup marks no payload element and does not say it has none".to_string(),
            fix: "mark every verbatim payload data-wire=\"<what it is>\", or declare data-wire=\"none\" once - a payload is anything the user reads or copies exactly".to_string(),
        });
        return;
    }
    for wire in &marked {
        let kind = &wire.value;
        if kind == "none" || kind == "collection" {
            continue;
        }
        let mono = declared
            .iter()
            .filter(|c| c.tag == wire.tag)
            .any(|c| bases(c).iter().any(|b| b == "font-mono"));
        if !mono {
            failures.push(Failure {
                check: "mono-for-wire-data·family".to_string(),
                what: format!("the {kind} payload does not render mono"),
                fix: "a verbatim payload carries font-mono; UI chrome carries font-ui. The subject is machine-readability, not provenance".to_string(),
            });
        }
    }
    // A payload is anything mono, whether or not it is marked: the two ways of
    // saying "this is wire data" both bind the wrapping rule.
    for class in &declared {
        let classes = bases(class);
        let payload = classes.iter().any(|c| c == "font-mono")
            || marked.iter().any(|w| w.value != "none" && w.at > class.at && w.tag == class.tag);
        let Some(hostile) = classes.iter().find(|c| WIRE_HOSTILE.contains(&c.as_str())) else {
            continue;
        };
        if payload {
            failures.push(Failure {
                check: "wrap-not-truncate·payload".to_string(),
                what: format!("a payload element carries `{hostile}`"),
                fix: "long values wrap - never truncate, never widen the column. A clipped payload is a wrong payload".to_string(),
            });
        }
    }
}

// wire-order-preserved. A captured collection renders in wire order unless a
// sort is declared with its why; a screen may render no captured collection at
// all, which is the rule's n/a path and must be said, not assumed.
// The answer is declared, never inferred from prose: no keyword can tell "sorted
// newest first, because the ring is a stack" from "the rows look orderly", and a
// keyword that scans markup instead reads `border` as an answer. So the row says
// so - the same shape `data-wire` itself uses, one attribute beside it.
fn check_wire_order_preserved(failures: &mut Vec<Failure>, elements: &str) {
    let collections: Vec<Attr> =
        attrs(elements, "data-wire").into_iter().filter(|w| w.value == "collection").collect();
    if collections.is_empty() {
        return;
    }
    let declared = attrs(elements, "data-order");
    let silent = collections
        .iter()
        .filter(|row| {
            !declared.iter().any(|how| !how.value.trim().is_empty() && how.tag == row.tag)
        })
        .count();
    if silent > 0 {
        failures.push(Failure {
            check: "wire-order-preserved·ordering".to_string(),
            what: format!(
                "{silent} of {} captured collections declare no ordering",
                collections.len()
            ),
            fix:
                "the row carries data-order=\"wire\", or names the sort and why it beats the wire's"
                    .to_string(),
        });
    }
}

// overflow-declared. One overflow line under the Layout sketch, naming every
// zone the sketch draws. A range names the zones it spans - "B-D" answers for C.
fn check_overflow_declared(failures: &mut Vec<Failure>, layout: &str) {
    let Some(sketch) = elements_of(layout, "pre").into_iter().next() else {
        return;
    };
    let text = inner_text(&format!("<pre>{sketch}</pre>"), (0, sketch.len() + 11));
    let Some(at) = text.to_lowercase().find("overflow") else {
        failures.push(Failure {
            check: "overflow-declared·line".to_string(),
            what: "the Layout sketch declares no overflow".to_string(),
            fix: "one overflow line under the sketch: which zones scroll, which are capped, and which cap a component already owns".to_string(),
        });
        return;
    };
    let declaration = &text[at..];
    let missing: Vec<String> = sketch_zones(&text[..at])
        .into_iter()
        .filter(|zone| !names_zone(declaration, *zone))
        .map(|zone| zone.to_string())
        .collect();
    if !missing.is_empty() {
        failures.push(Failure {
            check: "overflow-declared·zones".to_string(),
            what: format!("the overflow line is silent about zone {}", missing.join(" ")),
            fix: "every zone the sketch draws is named: it scrolls, it is capped, or it cannot overflow".to_string(),
        });
    }
}

// The sketch's zone ids: a single capital opening a boxed row. Anything the
// sketch says in its margin notes is commentary, not a zone.
fn sketch_zones(sketch: &str) -> Vec<char> {
    let mut zones: Vec<char> = Vec::new();
    for line in sketch.lines() {
        let mut chars = line.trim_start().chars();
        if chars.next() != Some('│') {
            continue;
        }
        let rest: String = chars.collect();
        let mut body = rest.trim_start().chars();
        if let (Some(id), Some(' ')) = (body.next(), body.next()) {
            if id.is_ascii_uppercase() && !zones.contains(&id) {
                zones.push(id);
            }
        }
    }
    zones
}

fn names_zone(declaration: &str, zone: char) -> bool {
    let chars: Vec<char> = declaration.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == zone {
            return true;
        }
        let (Some(from), Some(to)) = (chars.get(i.wrapping_sub(1)), chars.get(i + 1)) else {
            continue;
        };
        if matches!(c, '-' | '–' | '—')
            && from.is_ascii_uppercase()
            && to.is_ascii_uppercase()
            && (*from..=*to).contains(&zone)
        {
            return true;
        }
    }
    false
}

// state-declarations. Every catalog row answers empty · loading · error ·
// overflow. A reuse may answer by citation - its file already decided; a propose
// has no file to cite, so an em dash there is a component nobody has designed.
fn check_state_declarations(failures: &mut Vec<Failure>, elements: &str) {
    let rows = data_rows(elements);
    let mut blank = 0usize;
    let mut uncited = 0usize;
    for row in &rows {
        let cell =
            elements_of(row, "td").last().map(|c| inner_text(c, (0, c.len()))).unwrap_or_default();
        let states = cell.trim();
        if states.is_empty() {
            blank += 1;
        } else if states.chars().all(|c| c == '—' || c.is_whitespace())
            && ["propose", "extend"].iter().any(|v| row.contains(&format!(">{v}<")))
        {
            uncited += 1;
        }
    }
    if blank > 0 {
        failures.push(Failure {
            check: "state-declarations·blank".to_string(),
            what: format!("{blank} of {} catalog rows declare no states", rows.len()),
            fix: "every component answers empty · loading · error · overflow - \"—\" counts, blank does not".to_string(),
        });
    }
    if uncited > 0 {
        failures.push(Failure {
            check: "state-declarations·proposed".to_string(),
            what: format!("{uncited} proposed components answer their states with \"—\""),
            fix: "a citation completes the cell for a reuse, never for a propose - a component that does not exist yet has no file to cite".to_string(),
        });
    }
}

// keyboard-parity. Every pointer interaction has a keyboard path; a use case
// that is passive says so, which is an answer and not a blank.
fn check_keyboard_parity(failures: &mut Vec<Failure>, interaction: &str) {
    let rows = data_rows(interaction);
    let silent = rows
        .iter()
        .filter(|row| {
            elements_of(row, "td")
                .last()
                .is_none_or(|cell| inner_text(cell, (0, cell.len())).trim().is_empty())
        })
        .count();
    if silent > 0 {
        failures.push(Failure {
            check: "keyboard-parity·path".to_string(),
            what: format!("{silent} of {} use cases name no keyboard path", rows.len()),
            fix: "name the binding from the rulebook's conventions, propose a new one, or say the use case is passive".to_string(),
        });
    }
}

// ─── the surfaces the predicates read ───────────────────────────────────────

fn authored_classes(html: &str) -> Vec<(usize, String)> {
    attrs(html, "class")
        .into_iter()
        .flat_map(|declared| {
            declared
                .value
                .split_whitespace()
                .map(|cls| (declared.at, cls.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

// A variant chain decorates a class, it never changes which token it names:
// `hover:focus-visible:text-ink` is `text-ink`. The colon inside an arbitrary
// value is not a variant separator, which is why this counts brackets.
fn base_class(cls: &str) -> &str {
    let (mut depth, mut base) = (0usize, 0usize);
    for (i, c) in cls.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => base = i + c.len_utf8(),
            _ => {}
        }
    }
    cls[base..].strip_prefix('!').unwrap_or(&cls[base..])
}

fn arbitrary_value(base: &str) -> Option<&str> {
    let open = base.find('[')?;
    let close = base[open..].find(']')? + open;
    Some(&base[open + 1..close])
}

fn is_bare_size(value: &str) -> bool {
    let digits = value.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        && digits.chars().any(|c| c.is_ascii_digit())
}

// Chromatic means the channels disagree by more than the floor the vocabulary
// script measured its own chromatic set with - carried across the bridge rather
// than restated, because a threshold written twice can be raised once.
fn is_chromatic(value: &str, floor: u32) -> bool {
    // The `#` is the whole difference between a colour and a magnitude: every
    // digit of `font-[900]` is a hex digit too.
    let Some(hex) = value.trim().strip_prefix('#') else { return false };
    let channels: Vec<u8> = match hex.len() {
        3 => hex
            .chars()
            .map(|c| u8::from_str_radix(&c.to_string().repeat(2), 16).ok())
            .collect::<Option<_>>(),
        6 => (0..3)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
            .collect::<Option<_>>(),
        _ => None,
    }
    .unwrap_or_default();
    match (channels.iter().max(), channels.iter().min()) {
        (Some(high), Some(low)) => u32::from(*high) - u32::from(*low) > floor,
        _ => false,
    }
}

// The mockup's own CSS: rules under the `.a-` prefix, which a design run uses
// for assembly the component collection does not own. Doc scaffolding lives in
// the same <style> and is not the design, so it is left alone.
fn mockup_declarations(html: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for block in html.split("<style").skip(1) {
        let css = block.split("</style>").next().unwrap_or(block);
        let css = strip_css_comments(css);
        for rule in css.split('}') {
            let Some((selector, body)) = rule.split_once('{') else { continue };
            let selector = selector.trim().replace('\n', " ");
            if !selector.contains(".a-") {
                continue;
            }
            for declaration in body.split(';') {
                if let Some((property, value)) = declaration.split_once(':') {
                    let value = value.trim();
                    if !value.is_empty() {
                        out.push((
                            selector.clone(),
                            property.trim().to_ascii_lowercase(),
                            value.to_string(),
                        ));
                    }
                }
            }
        }
    }
    out
}

fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        match rest[at..].find("*/") {
            Some(end) => rest = &rest[at + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

// ─── the design-allow pragma ────────────────────────────────────────────────
//
// `<!-- design-allow: <class> — <reason> -->`, applying to the next line only.
// Same five rules as the lint rule's, because it is the same escape: the class
// must be on the line below (an edited line stops matching and reds) · the
// reason is non-empty, mirroring the gate's `waived:` · it covers a template
// shape, never a size · and the live total is printed, so a suppression is
// visible debt rather than a silent hole.
#[derive(Default)]
pub struct Allowances {
    granted: BTreeSet<(usize, String)>,
}

impl Allowances {
    fn covers(&self, line: usize, base: &str) -> bool {
        self.granted.contains(&(line, base.to_string()))
    }

    fn tally(&self) -> String {
        format!("design-allow: {} live suppression(s)", self.granted.len())
    }
}

fn read_pragmas(failures: &mut Vec<Failure>, html: &str) -> Allowances {
    let mut allowances = Allowances::default();
    let mut fail = |what: String, fix: &str| {
        failures.push(Failure { check: "design-allow".to_string(), what, fix: fix.to_string() });
    };
    for (at, _) in html.match_indices("<!--") {
        let Some(end) = html[at..].find("-->").map(|rel| at + rel) else { continue };
        let body = html[at + 4..end].trim();
        let Some(rest) = body.strip_prefix("design-allow:") else { continue };
        // The class is the longest prefix of the first token that a separator
        // follows - rightmost, so `w-[--tw-x:1]` keeps the dashes it owns, and no
        // whitespace required, so a separator glued to the class reads here the way
        // it reads in the lint rule's regex. `--` as well as the em dash, since the
        // lint rule takes both and one rule cannot have two dialects.
        let decl = rest.trim_start();
        let token = decl.split_whitespace().next().unwrap_or("");
        let split = (1..=token.len()).rev().filter(|n| token.is_char_boundary(*n)).find_map(|n| {
            let after = decl[n..].trim_start();
            let why = after.strip_prefix('—').or_else(|| after.strip_prefix("--"))?;
            (!why.trim().is_empty()).then_some((&decl[..n], why))
        });
        let Some(cls) = split.map(|(cls, _)| cls.trim()) else {
            fail(
                format!("`{}` gives no reason", rest.trim()),
                "a pragma reads `design-allow: <class> — <reason>` (em dash) - an escape nobody justified is a hole",
            );
            continue;
        };
        match arbitrary_value(cls) {
            Some(value) if !is_bare_size(value) => {}
            _ => {
                // Which reading an author meant is undecidable from the string, and the
                // lint rule's regex resolves it the same way, so the message names the
                // reading rather than the checker guessing a cause it cannot know.
                let read_inside = if cls == token {
                    String::new()
                } else {
                    format!(" - `{token}` read as this class plus a reason, its own rightmost separator being the split")
                };
                fail(
                    format!("`{cls}` carries a value, and the pragma never excuses one{read_inside}"),
                    "the pragma covers a template shape only - name the magnitude as a token in ui/tailwind.config.js, and spell a reason after a space so the dashes a class carries stay inside it",
                );
                continue;
            }
        }
        let line = line_of(html, at) + 1;
        if !line_text(html, line).contains(cls) {
            fail(
                format!("`{cls}` is not on the line below the pragma"),
                "a pragma applies to the next line only, and names the class it excuses - an edited line stops matching on purpose",
            );
            continue;
        }
        allowances.granted.insert((line, cls.to_string()));
    }
    allowances
}

fn line_of(html: &str, pos: usize) -> usize {
    html[..pos.min(html.len())].matches('\n').count() + 1
}

fn line_text(html: &str, line: usize) -> &str {
    html.lines().nth(line.saturating_sub(1)).unwrap_or_default()
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
        attrs(elements, "data-key").into_iter().partition(|key| in_tree(key.at));
    let keys = |half: Vec<Attr>| -> Vec<String> { half.into_iter().map(|key| key.value).collect() };
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
    let open_end = html[span.0..].find('>').map_or(span.0, |rel| span.0 + rel + 1).min(span.1);
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
fn exempt_declared(html: &str, artifact: &Attr) -> Option<String> {
    if !artifact.closed {
        return None;
    }
    attrs(html, "data-status")
        .into_iter()
        .find(|status| status.tag == artifact.tag)
        .map(|status| status.value)
        .filter(|v| v == "draft" || v == "archived")
}

// ─── the markup, tokenised once ─────────────────────────────────────────────
//
// Every predicate reads HTML through this. What it replaced were four scanners
// that read markup by hand - find a `<`, count the quotes, guess where the tag
// ends - and six code-review rounds found six holes in them, each fix revealing
// the next: an attribute quoted inside another's value read as a declaration · a
// tag end taken from a bracket inside a value · quotes re-pairing across a fault
// so one element was lent the next one's classes · a snippet cut mid-character.
// Every one is a case a tokeniser already owns, so one owns them: quick-xml,
// already in this repo's tree.
#[derive(PartialEq)]
enum Shape {
    Open,
    Close,
    Void,
    Other,
}

struct Node {
    name: String,
    shape: Shape,
    // `<` .. one past the `>` that closes it
    span: (usize, usize),
    // false only for a tag that never reaches its `>`. What it declared is still
    // read, so a truncated doc is checked rather than silently ungoverned.
    closed: bool,
    attrs: Vec<(usize, String, String)>,
}

// One attribute declaration: where its owning tag opens - two attributes of one
// element share it, which is how "on the same element" is asked - where the name
// itself sits, and the raw value.
struct Attr {
    tag: usize,
    at: usize,
    value: String,
    closed: bool,
}

// A `<` inside a script or a stylesheet is data, not markup - HTML's raw-text
// rule. Tokenising those bodies invents tags out of `a < b`.
const RAW_TEXT: &[&str] = &["script", "style"];

fn nodes(html: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let mut base = 0usize;
    while base < html.len() {
        let mut reader = Reader::from_str(&html[base..]);
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.check_comments = false;
        let mut resume = None;
        loop {
            let from = base + offset(reader.buffer_position());
            let event = reader.read_event();
            let to = base + offset(reader.buffer_position());
            match event {
                Ok(Event::Eof) => break,
                Ok(Event::Text(_)) => {}
                Ok(event) => {
                    let node = node_of(html, &event, (from, to));
                    if node.shape == Shape::Open && RAW_TEXT.contains(&node.name.as_str()) {
                        let end = format!("</{}", node.name);
                        resume = Some(html[to..].find(&end).map_or(html.len(), |rel| to + rel));
                    }
                    out.push(node);
                    if resume.is_some() {
                        break;
                    }
                }
                // A tag that never closes has no honest reading, so the reader
                // takes what it declared and picks up at the next tag: a fault
                // must not silence the markup after it. `markup·escaped` names it.
                Err(_) => {
                    out.extend(truncated(html, from));
                    resume = html[from + 1..].find('<').map(|rel| from + 1 + rel);
                    break;
                }
            }
        }
        match resume {
            Some(next) if next > base => base = next,
            _ => break,
        }
    }
    out
}

fn node_of(html: &str, event: &Event<'_>, span: (usize, usize)) -> Node {
    let (shape, open) = match event {
        Event::Start(tag) => (Shape::Open, Some(tag)),
        Event::Empty(tag) => (Shape::Void, Some(tag)),
        Event::End(_) => (Shape::Close, None),
        _ => (Shape::Other, None),
    };
    let name = match event {
        Event::Start(tag) | Event::Empty(tag) => text(tag.name().as_ref()),
        Event::End(tag) => text(tag.name().as_ref()),
        _ => String::new(),
    };
    let attrs = open.map(|tag| attrs_of(html, tag)).unwrap_or_default();
    Node { name, shape, span, closed: true, attrs }
}

fn attrs_of(html: &str, tag: &BytesStart<'_>) -> Vec<(usize, String, String)> {
    let mut attributes = tag.attributes();
    attributes.with_checks(false);
    // A valueless attribute (`data-map`, `defer`) is an error to an XML reader
    // and a declaration no predicate here reads, so it is skipped, not reported.
    attributes
        .flatten()
        .map(|a| (offset_in(html, a.key.as_ref()), text(a.key.as_ref()), text(&a.value)))
        .collect()
}

fn truncated(html: &str, from: usize) -> Option<Node> {
    let content = html[from..].strip_prefix('<')?;
    let len = content.find(char::is_whitespace).unwrap_or(content.len());
    let name = content[..len].to_string();
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let attrs = attrs_of(html, &BytesStart::from_content(content, len));
    Some(Node { name, shape: Shape::Open, span: (from, html.len()), closed: false, attrs })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// quick-xml hands back subslices of the input, so this is arithmetic, not a
// search - the reading `check_elements_map` already takes of its tree spans.
fn offset_in(html: &str, part: &[u8]) -> usize {
    part.as_ptr() as usize - html.as_ptr() as usize
}

fn offset(position: u64) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}

fn attrs(html: &str, name: &str) -> Vec<Attr> {
    let mut out = Vec::new();
    for node in nodes(html) {
        let (tag, closed) = (node.span.0, node.closed);
        for (at, key, value) in node.attrs {
            if key == name {
                out.push(Attr { tag, at, value, closed });
            }
        }
    }
    out
}

// A raw angle bracket inside a tag is invalid HTML - it belongs written `&lt;` or
// `&gt;` - and it is the one shape no reader can resolve: `title="a > b"` and an
// unterminated `title="oops>` look identical left to right, and a raw `<` in
// prose opens a tag that swallows the sentence after it, classes and all. So
// rather than guess, the case that cannot be parsed is named. Zero sites in the
// corpus when it landed; it costs nothing to keep shut.
fn check_escaped_markup(failures: &mut Vec<Failure>, html: &str) {
    for node in nodes(html).iter().filter(|n| n.shape != Shape::Other) {
        // between the `<` and the `>` that closed it - and for a tag that never
        // closed, everything it swallowed, which is the case being named
        let inside = html[node.span.0 + 1..node.span.1].trim_end_matches('>');
        let Some(raw) = inside.chars().find(|c| *c == '<' || *c == '>') else { continue };
        let seen: String = inside.chars().take(80).collect();
        failures.push(Failure {
            check: "markup·escaped".to_string(),
            what: format!("a tag holds a raw `{raw}`: {seen}"),
            fix: "write it `&lt;` or `&gt;` - a raw angle bracket inside a tag is invalid HTML, and no reader can tell it from an unterminated attribute".to_string(),
        });
    }
}

fn element_content(html: &str, tag: usize) -> ((usize, usize), bool) {
    let empty = ((tag, tag), false);
    let nodes = nodes(html);
    let Some(open) = nodes.iter().position(|n| n.span.0 == tag) else { return empty };
    if nodes[open].shape != Shape::Open || !nodes[open].closed {
        return empty;
    }
    let mut depth = 1usize;
    for (i, node) in nodes.iter().enumerate().skip(open + 1) {
        if node.name != nodes[open].name {
            continue;
        }
        match node.shape {
            Shape::Open => depth += 1,
            Shape::Close => {
                depth -= 1;
                if depth == 0 {
                    let filled =
                        has_text(html, &nodes[open + 1..i], nodes[open].span.1, node.span.0);
                    return ((tag, node.span.0), filled);
                }
            }
            _ => {}
        }
    }
    empty
}

// Text is what the tokeniser did not claim: the gaps between nodes. A comment or
// an empty nested element is markup, so an element holding only those is empty -
// the reading `check_items` needs to tell a written section from a tagged one.
fn has_text(html: &str, inner: &[Node], from: usize, to: usize) -> bool {
    let mut at = from;
    for node in inner {
        if node.span.0 < at || node.span.0 >= to {
            continue;
        }
        if !html[at..node.span.0].trim().is_empty() {
            return true;
        }
        at = node.span.1.clamp(at, to);
    }
    at < to && !html[at..to].trim().is_empty()
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
    // Read once for the whole invocation, and only when a design run is in the
    // batch: the vocabulary costs a node process, and no other artifact has a
    // rule that reads it.
    let mut vocab: Option<Result<Vocabulary, String>> = None;
    let mut out = String::new();
    let mut any_red = false;
    for path in paths {
        let resolved =
            if Path::new(path).exists() { Path::new(path).to_path_buf() } else { repo.join(path) };
        let html =
            std::fs::read_to_string(&resolved).map_err(|e| format!("cannot read {path}: {e}"))?;
        if vocab.is_none() && html.contains("data-artifact=\"design-run\"") {
            vocab = Some(vocabulary(repo));
        }
        let verdict = verify_html_against(
            &html,
            rules.as_deref(),
            vocab.as_ref().and_then(|read| read.as_ref().ok()),
        );
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
        attrs, check_frame_density, edit_distance, elements_of, format_verdict,
        rules_from_rulebook, verify_html, verify_html_against, Verdict, Vocabulary,
        CAPABILITY_STATES, FRAME_CELL_MAX, FRAME_CELL_MEAN, FRAME_EXAMPLE_MAX, FRAME_PROSE_WORDS,
        GIST_MAX_WORDS,
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

    // A hand-written stand-in for what scripts/design-vocabulary.mjs emits, so
    // the unit tests neither spawn node nor depend on the real palette. Parsed
    // through the same path the real one takes - JSON is YAML.
    const VOCABULARY: &str = r##"
legal: [text-ink, text-ink-mute, text-ok, text-body, text-label, bg-canvas, font-mono, font-ui,
        p-2, p-4, gap-2, max-w-pane, rounded-panel, uppercase, tracking-label, truncate,
        whitespace-nowrap, border, flex]
rules:
  colour-tokens-only: [bg-, text-, border-]
  spacing-scale: [p-, gap-, max-w-, w-]
  type-tokens-only: [font-, text-, tracking-, leading-]
  radius-tokens: [rounded-]
values:
  colour-tokens-only: ["#2b2622", "#4cc38a"]
  spacing-scale: [8px, 16px, 24px]
  type-tokens-only: [10px, 12px, 0.025em, "1.5"]
  radius-tokens: [8px]
modifiers:
  bg-canvas: [wash, scrim]
chromatic: [ok, warn]
chroma_floor: 40
"##;

    fn vocab() -> Vocabulary {
        serde_yaml::from_str(VOCABULARY).expect("the test vocabulary parses like the real one")
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
             <div data-item=\"mockup\" data-wire=\"none\"><button data-state-toggle=\"mock\">flip</button></div>\
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
        assert!(!verify_html_against(&frame_run(&body), Some(&rules()), Some(&vocab())).red());
    }

    #[test]
    fn a_frame_of_paragraphs_is_red_by_its_mean() {
        let body = format!(
            "<div data-item=\"frame\"><p>purpose</p><table>{}</table></div>",
            cells(20, FRAME_CELL_MEAN + 6)
        );
        let v = verify_html_against(&frame_run(&body), Some(&rules()), Some(&vocab()));
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
        let v = verify_html_against(&frame_run(&split), Some(&rules()), Some(&vocab()));
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
        let v = verify_html_against(&frame_run(&body), Some(&rules()), Some(&vocab()));
        assert!(v.red());
        assert!(what(&v, "frame·cell-length").contains("41 words"), "{:?}", failures(&v));
    }

    #[test]
    fn commentary_bolted_onto_the_frame_is_red() {
        let body = "<div data-item=\"frame\"><p>purpose</p><p>given</p><p>not given</p>\
                    <p>why this screen matters</p><table><td>clause</td></table></div>";
        let v = verify_html_against(&frame_run(body), Some(&rules()), Some(&vocab()));
        assert!(v.red());
        assert!(what(&v, "frame·prose-blocks").contains("4 prose blocks"), "{:?}", failures(&v));
    }

    #[test]
    fn a_purpose_paragraph_is_red() {
        let body = format!(
            "<div data-item=\"frame\"><p>{}</p><table><td>clause</td></table></div>",
            "word ".repeat(FRAME_PROSE_WORDS + 1)
        );
        let v = verify_html_against(&frame_run(&body), Some(&rules()), Some(&vocab()));
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
        let v = verify_html_against(&frame_run(body), Some(&rules()), Some(&vocab()));
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
        let v = verify_html_against(&green_run(), Some(&rules()), Some(&vocab()));
        assert!(!v.red(), "{:?}", failures(&v));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(summary.contains("receipt 2/2 soft · 1 satisfied · 1 inherited"), "{summary}");
        assert!(summary.contains("hard rules: 2/2 machine-checked (design)"), "{summary}");
        assert!(summary.contains("design-allow: 0 live suppression"), "{summary}");
    }

    // The surface label is the load-bearing word: 13/13 without it reads as a
    // claim about the built UI, which no predicate in this file makes.
    #[test]
    fn the_gap_line_names_the_surface_and_the_slugs_still_unchecked() {
        let widened = RULEBOOK.replace(
            "<tr><td><code>spacing-scale</code></td>",
            "<tr><td><code>not-a-predicate</code></td><td>x</td></tr>\
             <tr><td><code>spacing-scale</code></td>",
        );
        let rules = rules_from_rulebook(&widened);
        let v = verify_html_against(&green_run(), Some(&rules), Some(&vocab()));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(
            summary.contains("2/3 machine-checked (design) — 1 unchecked: not-a-predicate"),
            "{summary}"
        );
    }

    #[test]
    fn a_missing_soft_slug_fails_by_name() {
        let doc = design_run(&receipt("  data-ink-first: satisfied — no card frames\n"));
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert_eq!(failures(&v), vec!["receipt·calm-microcopy"]);
    }

    #[test]
    fn a_hard_slug_in_the_receipt_is_red() {
        let doc = design_run(&receipt(
            "  data-ink-first: satisfied — x\n  calm-microcopy: satisfied — y\n  spacing-scale: satisfied — z\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert!(what(&v, "receipt·unknown-slug").contains("machine-checked, not receipted"));
    }

    #[test]
    fn a_misspelled_slug_pairs_with_the_missing_one() {
        let doc = design_run(&receipt(
            "  data-ink-first: satisfied — x\n  calm-microcopi: satisfied — y\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        let msg = what(&v, "receipt·unknown-slug");
        assert!(msg.contains("calm-microcopi") && msg.contains("calm-microcopy"), "{msg}");
    }

    #[test]
    fn a_verdict_outside_the_enum_and_a_missing_why_each_fail() {
        let doc = design_run(&receipt(
            "  data-ink-first: probably-fine — x\n  calm-microcopy: satisfied\n",
        ));
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert!(what(&v, "receipt·data-ink-first").contains("not one of the seven"));
        assert!(what(&v, "receipt·calm-microcopy").contains("no citation"));
    }

    #[test]
    fn malformed_receipt_yaml_reports_the_parser_position() {
        let doc = design_run(
            "<pre data-item=\"receipt\">rulebook: 28f5437\nsoft:\n  data-ink-first: ok\n   calm-microcopy: ok\n</pre>",
        );
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
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
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert!(failures(&v).contains(&"receipt·rulebook".to_string()));
    }

    #[test]
    fn an_unkeyed_elements_catalog_is_red() {
        let doc = green_run().replace(" data-map", "").replace("data-key=\"section\"", "");
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
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

    // These two judge run #3's form, not its status, and archiving it exempts
    // the whole doc - so they would keep passing while asserting nothing.
    fn reopened(run: &str) -> String {
        let open = run.replace(" data-status=\"archived\"", "");
        assert_ne!(open, run, "run is not archived - reopening it is now a silent no-op");
        open
    }

    #[test]
    fn the_run_3_elements_map_is_a_live_two_half_map() {
        let doc = reopened(include_str!("../../docs/work/2026-07-26-dflow-w4-run-3.html"));
        assert!(!failures(&doc_verdict(&doc)).contains(&"elements·map".to_string()));
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
        let doc = reopened(include_str!("../../docs/work/2026-07-26-dflow-w4-run-3.html"));
        let reds = failures(&doc_verdict(&doc));
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
        verify_html_against(html, Some(&rules()), Some(&vocab()))
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
            include_str!("../../docs/work/2026-07-26-dflow-w4-run-3.html"),
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
        let values: Vec<String> = attrs(html, "data-item").into_iter().map(|a| a.value).collect();
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

    // ─── the thirteen hard rules ───────────────────────────────────────────

    fn mockup(markup: &str) -> String {
        green_run().replace(
            "<button data-state-toggle=\"mock\">flip</button>",
            &format!("<button data-state-toggle=\"mock\">flip</button>{markup}"),
        )
    }

    fn reds(html: &str) -> Vec<String> {
        failures(&verify_html_against(html, Some(&rules()), Some(&vocab())))
    }

    #[test]
    fn a_class_the_config_cannot_emit_reds_every_rule_whose_scale_owns_its_prefix() {
        let v = verify_html_against(
            &mockup("<span class=\"text-pink-500\">x</span>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "colour-tokens-only·off-vocabulary").contains("resolves to nothing"));
        assert!(
            what(&v, "type-tokens-only·off-vocabulary").contains("text-pink-500"),
            "`text-` is two scales at once; naming one of them would exempt the other"
        );
    }

    #[test]
    fn an_arbitrary_size_is_red_because_closing_the_config_cannot_reach_it() {
        let v = verify_html_against(
            &mockup("<span class=\"p-[13px]\">x</span>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "spacing-scale·off-vocabulary").contains("names 13px inline"));
    }

    // The one job zero-accent still has once the config is closed: a legal class
    // cannot name an off-set colour, but a value written past the config can.
    #[test]
    fn an_inline_chromatic_colour_reds_zero_accent_as_well_as_the_vocabulary() {
        let v = verify_html_against(
            &mockup("<span class=\"text-[#ff00aa]\">x</span>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "zero-accent·off-set").contains("chromatic colour outside the set"));

        let neutral = mockup("<span class=\"text-[#f7f5f0]\">x</span>");
        assert!(
            !reds(&neutral).contains(&"zero-accent·off-set".to_string()),
            "an untokenised grey breaks the vocabulary, not the zero-accent rule"
        );
    }

    // Every digit of 900 is a hex digit, so a colourless magnitude parses as one
    // unless the `#` is required - and then the rule reports a colour nobody wrote.
    #[test]
    fn an_arbitrary_magnitude_that_reads_as_hex_is_not_a_colour() {
        for base in ["font-[900]", "z-[999]", "leading-[100]"] {
            let doc = mockup(&format!("<span class=\"{base}\">x</span>"));
            assert!(
                !reds(&doc).contains(&"zero-accent·off-set".to_string()),
                "{base} names a magnitude past the config, not a chromatic colour"
            );
        }
    }

    #[test]
    fn a_radius_outside_the_named_set_is_red() {
        let v = verify_html_against(
            &mockup("<div class=\"rounded-md\">x</div>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "radius-tokens·off-vocabulary").contains("rounded-md"));
    }

    // Variants decorate a class; they never change which token it names. Reading
    // the whole string would red every hover: and focus-visible: in a mockup.
    #[test]
    fn variants_bangs_and_slash_steps_resolve_to_the_token_underneath() {
        let legal =
            mockup("<span class=\"hover:focus-visible:text-ink !p-2 bg-canvas/wash\">x</span>");
        assert!(
            !verify_html_against(&legal, Some(&rules()), Some(&vocab())).red(),
            "{:?}",
            reds(&legal)
        );

        let wrong_step = mockup("<span class=\"bg-canvas/loose\">x</span>");
        assert!(
            what(
                &verify_html_against(&wrong_step, Some(&rules()), Some(&vocab())),
                "colour-tokens-only·off-vocabulary"
            )
            .contains("bg-canvas/loose"),
            "a step the scale does not carry emits nothing at all, not even the base"
        );
    }

    // Every magnitude that leaked past the vocabulary in a shipped run leaked
    // through the mockup's own CSS, which no class check can reach.
    #[test]
    fn a_raw_magnitude_in_the_mockups_own_css_is_red() {
        let raw = green_run().replace(
            "<style>.mock{color:var(--a-ink);}</style>",
            "<style>.a-measure{max-width:460px;padding:16px;font-size:12px;}</style>",
        );
        let v = verify_html_against(&raw, Some(&rules()), Some(&vocab()));
        assert!(what(&v, "spacing-scale·raw-value").contains("max-width: 460px"));
        assert!(
            !failures(&v).contains(&"type-tokens-only·raw-value".to_string()),
            "12px is a named size and 16px a named step - only the stranger reds"
        );
    }

    #[test]
    fn relationship_keywords_and_tokens_pass_the_css_scan() {
        let ok = green_run().replace(
            "<style>.mock{color:var(--a-ink);}</style>",
            "<style>.a-win{margin:0 auto;max-width:100%;padding:8px 24px;color:var(--a-ink);}</style>",
        );
        assert!(!verify_html_against(&ok, Some(&rules()), Some(&vocab())).red(), "{:?}", reds(&ok));
    }

    // Both archived runs wrote their labels' tracking by hand - one as an
    // arbitrary class, one as a declaration. The rule catches both spellings.
    #[test]
    fn an_uppercase_label_that_tracks_by_hand_is_red_on_either_surface() {
        let by_class = mockup("<div class=\"uppercase tracking-[.06em] text-ink-mute\">zone</div>");
        assert!(what(
            &verify_html_against(&by_class, Some(&rules()), Some(&vocab())),
            "uppercase-mute-labels·pattern"
        )
        .contains("tracking-[.06em]"));

        let by_css = green_run().replace(
            "<style>.mock{color:var(--a-ink);}</style>",
            "<style>.a-zone-lbl{text-transform:uppercase;letter-spacing:0.05em;}</style>",
        );
        assert!(what(
            &verify_html_against(&by_css, Some(&rules()), Some(&vocab())),
            "uppercase-mute-labels·pattern"
        )
        .contains(".a-zone-lbl"));
    }

    // MethodLabel's verb is uppercase and is not a label: font-mono is the
    // wire-data channel, so what it marks is a value, not screen chrome.
    #[test]
    fn an_uppercase_payload_is_a_value_not_a_label() {
        let verb = mockup("<span class=\"font-mono uppercase text-ink\">POST</span>");
        assert!(
            !reds(&verb).contains(&"uppercase-mute-labels·pattern".to_string()),
            "{:?}",
            reds(&verb)
        );
    }

    #[test]
    fn a_chromatic_element_that_says_nothing_is_red() {
        let v = verify_html_against(
            &mockup("<span class=\"text-ok\"></span>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "colour-plus-text·channel").contains("says nothing"));

        let with_text = mockup("<span class=\"text-ok\">201</span>");
        assert!(!reds(&with_text).contains(&"colour-plus-text·channel".to_string()));
    }

    // An unmarked mockup reads as correctly typed when nobody looked, which is
    // the same silence the capabilities table exists to refuse.
    #[test]
    fn a_mockup_that_declares_no_payload_at_all_is_red() {
        let silent = green_run().replace(" data-wire=\"none\"", "");
        assert!(what(&doc_verdict(&silent), "mono-for-wire-data·undeclared").contains("has none"));
    }

    #[test]
    fn a_marked_payload_that_is_not_mono_is_red() {
        let v = verify_html_against(
            &mockup("<span data-wire=\"uri\" class=\"text-ink\">https://x</span>"),
            Some(&rules()),
            Some(&vocab()),
        );
        assert!(what(&v, "mono-for-wire-data·family").contains("does not render mono"));

        let mono = mockup("<span data-wire=\"uri\" class=\"font-mono text-ink\">https://x</span>");
        assert!(!reds(&mono).contains(&"mono-for-wire-data·family".to_string()));
    }

    #[test]
    fn a_payload_that_truncates_is_red_by_either_way_of_being_one() {
        let by_class = mockup("<span class=\"font-mono truncate\">x</span>");
        assert!(what(&doc_verdict(&by_class), "wrap-not-truncate·payload").contains("truncate"));

        let by_mark =
            mockup("<span data-wire=\"uri\" class=\"font-mono whitespace-nowrap\">x</span>");
        assert!(
            what(&doc_verdict(&by_mark), "wrap-not-truncate·payload").contains("whitespace-nowrap")
        );

        let chrome = mockup("<span class=\"text-ink whitespace-nowrap\">142 ms</span>");
        assert!(
            !reds(&chrome).contains(&"wrap-not-truncate·payload".to_string()),
            "a duration is chrome; the rule is about payloads"
        );
    }

    #[test]
    fn a_captured_collection_that_declares_no_ordering_is_red_and_a_screen_without_one_is_not() {
        let collection = green_run().replace(
            "<tr data-key=\"section\">",
            "<tr data-key=\"section\" data-wire=\"collection\">",
        );
        assert!(what(&doc_verdict(&collection), "wire-order-preserved·ordering")
            .contains("no ordering"));
        assert!(
            !reds(&green_run()).contains(&"wire-order-preserved·ordering".to_string()),
            "a screen may render no captured collection at all"
        );
    }

    // No keyword can judge prose: one that reds "sorted newest first, because the
    // ring is a stack" and greens "the rows look orderly" is wrong both ways. The
    // row declares its answer, the same way it declares which elements are wire.
    #[test]
    fn the_ordering_answer_is_a_declaration_and_prose_is_not_one() {
        let collection = |row: &str| green_run().replace("<tr data-key=\"section\">", row);

        for silent in [
            "<tr data-key=\"section\" data-wire=\"collection\" class=\"border-b border-hairline\">",
            "<tr data-key=\"section\" data-wire=\"collection\"><td>sorted newest first</td>",
            "<tr data-key=\"section\" data-wire=\"collection\" data-order=\"  \">",
        ] {
            assert!(
                what(&doc_verdict(&collection(silent)), "wire-order-preserved·ordering")
                    .contains("no ordering"),
                "neither a styling class, nor prose, nor a blank answer declares an ordering: {silent}"
            );
        }

        for answered in [
            "<tr data-key=\"section\" data-wire=\"collection\" data-order=\"wire\">",
            "<tr data-key=\"section\" data-wire=\"collection\" data-order=\"newest first — the ring is a stack\">",
        ] {
            assert!(
                !reds(&collection(answered))
                    .contains(&"wire-order-preserved·ordering".to_string()),
                "a declared ordering answers the rule: {answered}"
            );
        }
    }

    // Every declaration check reads attributes through one helper, so a value
    // that merely CONTAINS an attribute is one hole in all of them at once - and
    // a doc quoting the contract it is judged by is the likeliest place for it.
    #[test]
    fn an_attribute_quoted_inside_another_value_declares_nothing() {
        let decoy_order = green_run().replace(
            "<tr data-key=\"section\">",
            "<tr data-key=\"section\" data-wire=\"collection\" title='say data-order=\"wire\" here'>",
        );
        assert!(
            what(&doc_verdict(&decoy_order), "wire-order-preserved·ordering")
                .contains("no ordering"),
            "a quoted lookalike is text, not a declaration"
        );

        let decoy_wire =
            green_run().replace(" data-wire=\"none\"", " title='mark it data-wire=\"none\" once'");
        assert!(
            !what(&doc_verdict(&decoy_wire), "mono-for-wire-data·undeclared").is_empty(),
            "the same hole in the same helper answers for the payload question too"
        );
    }

    // The other half of the same confusion: a tag ends at its first UNQUOTED `>`.
    // Stopping at a quoted one reads part of the tag and reports the rest missing,
    // which is a false RED - the direction a checker cannot afford.
    #[test]
    fn a_bracket_inside_a_value_does_not_end_the_tag() {
        let doc = green_run().replace(
            "<div data-item=\"mockup\" data-wire=\"none\">",
            "<div data-item=\"mockup\"><span title=\"a > b\" class=\"font-mono\" data-wire=\"body\">x</span>",
        );
        for check in ["mono-for-wire-data·family", "mono-for-wire-data·undeclared"] {
            assert!(
                !reds(&doc).contains(&check.to_string()),
                "the element is declared and mono; {check} must not fire"
            );
        }
    }

    // A bracket may sit legally inside a closed value on a tag that LATER goes
    // malformed. Truncating at that bracket drops the attributes between it and
    // the fault - a false red, which is the direction a checker cannot afford.
    #[test]
    fn a_legal_bracket_survives_a_later_fault_in_the_same_tag() {
        let doc = green_run().replace(
            "<div data-item=\"mockup\" data-wire=\"none\">",
            "<div data-item=\"mockup\" data-wire=\"none\"><span title=\"a>b\" class=\"font-mono\" data-wire=\"body\" hint=\"oops>x</span>",
        );
        assert!(
            !reds(&doc).contains(&"mono-for-wire-data·family".to_string()),
            "the class sits before the fault and must still be read"
        );
    }

    // An unterminated quote has no honest reading, so the scan must not run past
    // the tag looking for one: re-syncing on a LATER tag's quotes would lend this
    // element the next element's classes, and lending font-mono is a false green.
    #[test]
    fn an_unclosed_quote_does_not_lend_one_element_anothers_classes() {
        let doc = green_run().replace(
            "<div data-item=\"mockup\" data-wire=\"none\">",
            "<div data-item=\"mockup\" data-wire=\"none\"><span data-wire=\"body\" title=\"oops>x</span><span class=\"font-mono\">y</span>",
        );
        assert!(
            !what(&doc_verdict(&doc), "mono-for-wire-data·family").is_empty(),
            "the payload carries no mono of its own and must not borrow the next element's"
        );
    }

    // A script writes markup; it does not carry it. Every component design file
    // in the corpus builds its specimen from template literals, so tokenising a
    // script body reads `class="${NT}"` as a declaration and `a < b` as a tag.
    #[test]
    fn a_class_a_script_writes_is_not_a_declaration() {
        let doc = mockup(
            "<script>const NT = 'p-[13px]'; if (a < b) render(`<span class=\"${NT}\">x</span>`);</script>",
        );
        assert!(
            !verify_html_against(&doc, Some(&rules()), Some(&vocab())).red(),
            "{:?}",
            reds(&doc)
        );
    }

    // A fault is local: the tag that carries it has no honest reading, and every
    // declaration after it still does. Silencing the rest of the doc would turn
    // one malformed tag into a green run with nine sections nobody checked.
    #[test]
    fn a_fault_does_not_silence_the_declarations_after_it() {
        let doc = green_run().replace(
            "<p data-item=\"frame\">",
            "<span title=\"oops>x</span><p data-item=\"frame\">",
        );
        let reported = reds(&doc);
        assert!(reported.contains(&"markup·escaped".to_string()), "{reported:?}");
        for silenced in ["item·receipt", "item·capabilities", "item·proposals"] {
            assert!(!reported.contains(&silenced.to_string()), "{silenced} lost: {reported:?}");
        }
    }

    // An item is filled by TEXT. Markup the tokeniser claimed is not text, so a
    // section holding only a comment is as empty as an untouched one - the
    // reading that keeps a tagged-but-unwritten section from passing.
    #[test]
    fn a_section_holding_only_a_comment_is_empty() {
        let commented = wu_result_body().replace(
            "<p data-item=\"evidence\">tests</p>",
            "<p data-item=\"evidence\"><!-- soon --></p>",
        );
        let v = verify_html(&doc("wu-result", "", &commented));
        assert_eq!(failures(&v), vec!["item·evidence"]);
    }

    fn layout(sketch: &str) -> String {
        green_run().replace(
            "<p data-item=\"layout\">zones</p>",
            &format!("<div data-item=\"layout\"><pre>{sketch}</pre></div>"),
        )
    }

    #[test]
    fn a_layout_that_never_says_what_scrolls_is_red() {
        let silent = layout("│ A  identity\n│ B  target\n");
        assert!(
            what(&doc_verdict(&silent), "overflow-declared·line").contains("declares no overflow")
        );
    }

    #[test]
    fn a_zone_the_overflow_line_skips_is_named() {
        let partial = layout("│ A  identity\n│ B  target\n\noverflow: A is pinned\n");
        assert!(what(&doc_verdict(&partial), "overflow-declared·zones").contains("zone B"));
    }

    // "B–D" answers for C: a range names every zone it spans, or the rule would
    // force a run to spell out what it already said.
    #[test]
    fn a_range_answers_for_the_zones_it_spans() {
        let ranged = layout(
            "│ A  identity\n│ B  target\n│ C  request\n│ D  response\n\noverflow: A pinned · B–D scroll as one\n",
        );
        assert!(
            !reds(&ranged).contains(&"overflow-declared·zones".to_string()),
            "{:?}",
            reds(&ranged)
        );
    }

    #[test]
    fn a_catalog_row_that_declares_no_states_is_red() {
        let blank = green_run().replace(
            "<details class=\"why\"><div>@use every labelled zone · @avoid nesting</div></details></td>",
            "<details class=\"why\"><div>@use every labelled zone · @avoid nesting</div></details></td><td>  </td>",
        );
        assert!(
            what(&doc_verdict(&blank), "state-declarations·blank").contains("declare no states")
        );
    }

    // A reuse may answer by citation - its file already decided. A propose has
    // no file to cite, so an em dash there is a component nobody has designed.
    #[test]
    fn a_proposed_component_cannot_answer_its_states_with_an_em_dash() {
        let cited = green_run().replace(
            "<details class=\"why\"><div>@use every labelled zone · @avoid nesting</div></details></td>",
            "<details class=\"why\"><div>@use every labelled zone · @avoid nesting</div></details></td><td>—</td>",
        );
        assert!(!reds(&cited).contains(&"state-declarations·proposed".to_string()));

        let proposed = cited.replace("<td>reuse</td>", "<td>propose</td>");
        assert!(what(&doc_verdict(&proposed), "state-declarations·proposed")
            .contains("no file to cite"));
    }

    #[test]
    fn a_use_case_with_no_keyboard_path_is_red() {
        let silent = green_run().replace(
            "<details class=\"why\"><div>B → status</div></details></td></tr>",
            "<details class=\"why\"><div>B → status</div></details></td><td> </td></tr>",
        );
        assert!(what(&doc_verdict(&silent), "keyboard-parity·path").contains("no keyboard path"));
    }

    // The step's own gate: the live rulebook's hard table and this file's
    // predicate list are one list, or the printed tally is a claim nobody
    // checked. A rule added to the table without a predicate reds here first.
    #[test]
    fn every_hard_rule_in_the_live_rulebook_carries_a_predicate() {
        let live = include_str!("../../docs/design-rules/design-rules-index.html");
        let hard: Vec<String> =
            rules_from_rulebook(live).into_iter().filter(|r| r.hard).map(|r| r.slug).collect();
        let unchecked: Vec<&String> =
            hard.iter().filter(|s| !super::HARD_PREDICATES.contains(&s.as_str())).collect();
        assert!(unchecked.is_empty(), "hard rules with no Tier-1 predicate: {unchecked:?}");
        let orphans: Vec<&&str> =
            super::HARD_PREDICATES.iter().filter(|p| !hard.iter().any(|s| s == *p)).collect();
        assert!(orphans.is_empty(), "predicates for rules the book no longer states: {orphans:?}");
        assert_eq!(hard.len(), 13, "the hard list is frozen at thirteen for this WU");
    }

    // ─── the design-allow pragma ───────────────────────────────────────────

    fn pragma(line: &str, markup: &str) -> String {
        mockup(&format!("\n<!-- design-allow: {line} -->\n{markup}\n"))
    }

    #[test]
    fn a_valid_pragma_suppresses_its_class_and_is_counted() {
        let doc = pragma(
            "grid-cols-[auto_1fr] — the two-column payload grid has no token shape",
            "<div class=\"grid-cols-[auto_1fr]\">x</div>",
        );
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert!(!v.red(), "{:?}", failures(&v));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(summary.contains("design-allow: 1 live suppression"), "{summary}");
    }

    // The lint rule has always taken `--` for authors without an em dash to hand.
    // One rule, two surfaces: a pragma the other surface honours cannot read as
    // reasonless here.
    // The reader takes the safe bound when a value holds a raw angle bracket, so
    // the convention it rests on is checked rather than trusted: an unparseable
    // value must be impossible to write silently, not merely rare.
    #[test]
    fn a_raw_angle_bracket_in_an_attribute_value_is_red() {
        for tag in ["<span title=\"a < b\">x</span>", "<span title=\"a > b\">x</span>"] {
            let doc = mockup(tag);
            assert!(
                what(&doc_verdict(&doc), "markup·escaped").contains("raw"),
                "unescaped markup must be named, not silently mis-parsed: {tag}"
            );
        }
        let escaped = mockup("<span title=\"a &lt; b\" class=\"font-mono\">x</span>");
        assert!(
            !reds(&escaped).contains(&"markup·escaped".to_string()),
            "the entity form is the correct spelling and stays green"
        );
    }

    // A raw `<` in prose that a letter follows is a tag open to every HTML parser
    // alive, so the sentence after it is swallowed as attributes - the two token
    // reds below are that swallowing. Naming it is the fail-safe reading; going
    // quiet on it would leave the classes silently lost.
    #[test]
    fn a_raw_bracket_in_prose_is_named_not_tolerated() {
        let doc = mockup("<p class=\"text-sm\">x<y here; it's fine, and 3 > 2 too.</p>");
        assert!(
            reds(&doc).contains(&"markup·escaped".to_string()),
            "{:?}",
            failures(&doc_verdict(&doc))
        );
    }

    // The named snippet is cut to a length, and a length in bytes lands mid
    // character on the em dashes this project writes everywhere.
    #[test]
    fn naming_a_raw_bracket_survives_a_multibyte_value() {
        let long = "—".repeat(40);
        let doc = mockup(&format!("<span title=\"{long} a < b\">x</span>"));
        assert!(what(&doc_verdict(&doc), "markup·escaped").contains("raw"));
    }

    #[test]
    fn an_ascii_dash_separates_a_pragma_the_way_the_lint_rule_allows() {
        let doc = pragma(
            "grid-cols-[auto_1fr] -- the two-column payload grid has no token shape",
            "<div class=\"grid-cols-[auto_1fr]\">x</div>",
        );
        let v = verify_html_against(&doc, Some(&rules()), Some(&vocab()));
        assert!(!v.red(), "{:?}", failures(&v));
    }

    // A class may carry the separator inside itself - a CSS custom property is
    // spelled with two dashes - so the class is a token, not everything left of
    // the first `--`. The lint rule's regex has always read it that way.
    #[test]
    fn a_class_holding_two_dashes_is_not_split_inside_itself() {
        let doc = pragma(
            "grid-cols-[--tw-cols] -- the template shape comes from a custom property",
            "<div class=\"grid-cols-[--tw-cols]\">x</div>",
        );
        let reds = failures(&verify_html_against(&doc, Some(&rules()), Some(&vocab())));
        assert!(!reds.contains(&"design-allow".to_string()), "{reds:?}");
    }

    // The lint rule's regex needs no whitespace before the separator, so neither
    // may this - one pragma form, or an escape legal in a component is red in the
    // mockup that specimen came from.
    #[test]
    fn a_separator_glued_to_the_class_reads_the_way_the_lint_rule_reads_it() {
        let doc = pragma(
            "grid-cols-[auto_1fr]--the two-column payload grid has no token shape",
            "<div class=\"grid-cols-[auto_1fr]\">x</div>",
        );
        let reds = failures(&verify_html_against(&doc, Some(&rules()), Some(&vocab())));
        assert!(!reds.contains(&"design-allow".to_string()), "{reds:?}");
    }

    #[test]
    fn a_pragma_with_no_reason_is_red() {
        let doc = pragma("grid-cols-[auto_1fr]", "<div class=\"grid-cols-[auto_1fr]\">x</div>");
        assert!(what(&doc_verdict(&doc), "design-allow").contains("no reason"));
    }

    // The line moves on, the pragma does not: an edited line stops matching, so
    // a suppression cannot outlive the code it was written for.
    #[test]
    fn a_pragma_whose_class_has_left_the_line_below_is_red() {
        let doc = pragma("grid-cols-[auto_1fr] — the payload grid", "<div class=\"flex\">x</div>");
        assert!(what(&doc_verdict(&doc), "design-allow").contains("not on the line below"));
    }

    // The escape covers a template shape, never a magnitude - a size always has
    // somewhere to be named, which is the whole point of closing the config.
    #[test]
    fn a_pragma_over_a_size_is_rejected_rather_than_honoured() {
        for cls in ["max-w-[460px]", "p-5"] {
            let doc = pragma(
                &format!("{cls} — the mockup needs this exact measure"),
                &format!("<div class=\"{cls}\">x</div>"),
            );
            let v = doc_verdict(&doc);
            assert!(what(&v, "design-allow").contains("never excuses one"), "{cls}");
            assert!(
                failures(&v).iter().any(|f| f.ends_with("off-vocabulary")),
                "a rejected pragma suppresses nothing: {cls}"
            );
        }
    }

    // ─── the vocabulary itself ─────────────────────────────────────────────

    #[test]
    fn a_vocabulary_that_cannot_be_read_is_red_never_silently_green() {
        let v = verify_html_against(&green_run(), Some(&rules()), None);
        assert!(v.red(), "five rules that cannot be checked must not pass");
        assert!(what(&v, "vocabulary·unreadable").contains("cannot pass unread"));
    }

    // A renamed or moved config takes the emitter down with it, and a token rule
    // that reports nothing looks exactly like a token rule with nothing to say.
    #[test]
    fn the_emitter_fails_loudly_when_it_cannot_read_the_config() {
        let empty = tempfile::tempdir().expect("tempdir");
        let err = super::vocabulary(empty.path()).err().expect("no config, no vocabulary");
        assert!(err.contains("design-vocabulary.mjs"), "{err}");
    }

    #[test]
    fn distance_is_small_for_typos_only() {
        assert_eq!(edit_distance("expcted", "expected"), 1);
        assert!(edit_distance("banana", "expected") > 3);
    }
    // A class carrying its own `--` and no reason cannot be told from a class with
    // a glued one-word reason, here or in the lint rule's regex. The verdict is red
    // either way; what must not happen is a message blaming a cause nobody can know.
    #[test]
    fn a_reasonless_class_carrying_its_own_dashes_is_red_and_says_which_reading_it_took() {
        let doc = pragma("grid-cols-[--tw-cols]", "<div class=\"grid-cols-[--tw-cols]\">x</div>");
        let said = what(&verify_html_against(&doc, Some(&rules()), Some(&vocab())), "design-allow");
        assert!(said.contains("grid-cols-[--tw-cols]` read as this class plus a reason"), "{said}");
    }
}
