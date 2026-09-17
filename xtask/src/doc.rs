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
const DESIGN_RUN: &[&str] =
    &["mockup", "capabilities", "keyboard", "proposals", "blocked-on", "review"];
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
    "wrap-not-truncate",
    "mono-for-wire-data",
    "wire-order-preserved",
    "uppercase-mute-labels",
    "keyboard-parity",
    "colour-plus-text",
];
const RULEBOOK: &str = "docs/design-rules/design-rules-index.html";
const VOCABULARY: &str = "scripts/design-vocabulary.mjs";
const REGISTRY: &str = "scripts/design-registry.mjs";
const HTMLDOC_BOOTSTRAP: &str = "https://htmldoc.wfomobile.com/htmldoc.js";
pub const COMPONENTS: &str = "docs/work/components/";
const ARCHIVE_DIR: &str = ".archive";
pub const COLLECTION: &str = "ui/src/components/";
pub const SCREENS: &str = "ui/src/";

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

// The hard rules a run is judged against. Every rule in the book is hard and
// carries a predicate (DEC-53), so the list is the numerator's denominator and
// nothing else - there is no receipt to cover and no blob to pin.
pub struct Rulebook {
    rules: Vec<String>,
}

// A pin shorter than this names no blob: git's own abbreviation floor, and the
// length every receipt in the corpus already writes.
pub const PIN_MIN: usize = 7;

pub fn pin_matches(pinned: &str, blob: &str) -> bool {
    let pin = pinned.to_ascii_lowercase();
    pin.len() >= PIN_MIN && blob.starts_with(&pin)
}

// What a receipt or a note should have pinned, for every message that tells an
// author to go and pin it. A blob already shorter than the floor is shown whole:
// nothing longer exists to offer.
pub fn pin_of(blob: &str) -> &str {
    blob.get(..PIN_MIN).unwrap_or(blob)
}

// What pulls the design step onto a tip. It answers for a FINISHED design, so a
// run doc counts once it is closed at the tip - the merge that
// closes it is the first one answerable for it, and a run still `draft` is
// judged at the merge that finishes it, which touches the file again. A
// component design file is a finished contract the moment it is in the tree.
pub fn design_artifact(path: &str, html: &str) -> Option<&'static str> {
    if component_designed(path).is_some() {
        return Some("component-design");
    }
    (design_run_status(html)? == "archived").then_some("design-run")
}

fn shaped_file(file: &str, exts: &[&str]) -> bool {
    !file.contains('/')
        && !file.contains(".test.")
        && file.rsplit_once('.').is_some_and(|(_, ext)| exts.contains(&ext))
}

fn collection_file(path: &str) -> bool {
    path.strip_prefix(COLLECTION).is_some_and(|file| shaped_file(file, &["ts", "tsx"]))
}

// Everything a user meets that a design could have shaped: a non-test `.tsx`
// anywhere under ui/src/. Depth is not membership - a surface moved into a
// subfolder is still rendered - so this reads the basename, unlike the
// collection's own deliberately flat test.
pub fn ui_surface(path: &str) -> bool {
    path.strip_prefix(SCREENS)
        .and_then(|rest| rest.rsplit('/').next())
        .is_some_and(|file| shaped_file(file, &["tsx"]))
}

// Anything rendered that the collection does not govern. The collection's own
// test is deliberately flat because its oracle reads one directory, so a `.tsx`
// nested below `ui/src/components/` is in nobody's registry - which makes it a
// screen, not an exemption. Judging it by prefix instead would leave one folder
// as the way to add a surface no design is owed for.
fn screen_file(path: &str) -> bool {
    ui_surface(path) && !collection_file(path)
}

// Membership is `@tier`, the same tag scripts/design-registry.mjs reads - so a
// gate and the oracle cannot disagree about who is in the roster. Read from git
// rather than through the oracle because a gate judges the roster its own tip
// ships, and because an epic tip is often not the checkout.
pub fn registry_entry(path: &str, source: &str) -> bool {
    collection_file(path) && source.contains("@tier")
}

// Whether a path is a surface at one revision - the one answer both the roster
// and the added-surface trigger want, so the two can never drift.
//
// The path test comes first and settles every screen, which is load-bearing
// twice. It is what keeps a blob from being fetched for a path that could never
// need one (fetching first cost ~12% of an epic gate's wall time). And it is what
// stops a screen's membership from depending on a successful UTF-8 decode: bytes
// that fail to decode read as "not a surface", so one stray byte in a new screen
// would drop it out of the added set and let a stated reason stand in for the
// design it owes.
pub fn surface_at(repo: &Path, rev: &str, path: &str) -> bool {
    screen_file(path)
        || (collection_file(path)
            && crate::git::read_utf8_at(repo, rev, path)
                .is_some_and(|source| registry_entry(path, &source)))
}

pub fn surface_name(path: &str) -> Option<String> {
    let file = path.rsplit('/').next()?;
    Some(file.rsplit_once('.')?.0.to_string())
}

// Who counts as built, at one revision. Screens are in it as well as `@tier`
// components: a screen design resolves where its screen was built, or designing
// one would cost a drop record while designing nothing cost nothing - the
// incentive inverted.
pub fn roster_at(repo: &Path, rev: &str) -> BTreeSet<String> {
    crate::git::git(repo, &["ls-tree", "-r", "--name-only", rev, "--", SCREENS])
        .unwrap_or_default()
        .lines()
        .filter(|path| surface_at(repo, rev, path))
        .filter_map(surface_name)
        .collect()
}

// Every slug the rulebook declares - all of them hard since DEC-53. The gate
// reads it to check a citation is followable rather than merely well-shaped - at
// the revision it is judging, never the working tree, so a later rename cannot
// unfollow a citation that resolved when its seam was reviewed.
pub fn rulebook_slugs(repo: &Path, rev: &str) -> Vec<String> {
    crate::git::read_utf8_at(repo, rev, RULEBOOK)
        .map(|html| rules_from_rulebook(&html))
        .unwrap_or_default()
}

pub fn rulebook(repo: &Path) -> Option<Rulebook> {
    let rules = std::fs::read_to_string(repo.join(RULEBOOK))
        .ok()
        .map(|html| rules_from_rulebook(&html))
        .filter(|rules| !rules.is_empty())?;
    Some(Rulebook { rules })
}

pub fn rules_from_rulebook(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(declared) = attrs(html, "data-c").into_iter().find(|a| a.value == "hard-rules-table")
    else {
        return out;
    };
    let ((start, end), _) = element_content(html, declared.tag);
    for row in html[start..end].split("<tr").skip(1) {
        if let Some(slug) = row_slug(row) {
            out.push(slug);
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

// Both oracles are ES modules the design surfaces already read, so Rust reads
// their answer rather than the config behind it - one source of truth, several
// surfaces (DEC-17). JSON is YAML, so this needs no new parser.
fn oracle<T: serde::de::DeserializeOwned>(repo: &Path, script: &str) -> Result<T, String> {
    let out = Command::new("node")
        .arg(repo.join(script))
        .current_dir(repo)
        .output()
        .map_err(|e| format!("cannot launch node for {script}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{script} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_yaml::from_slice(&out.stdout).map_err(|e| format!("{script} emitted nothing usable: {e}"))
}

pub fn vocabulary(repo: &Path) -> Result<Vocabulary, String> {
    oracle(repo, VOCABULARY)
}

// The component vocabulary: the collection's own TSDoc, emitted by
// scripts/design-registry.mjs (W7 ①) and never stored. Only the names are read
// now - what consumes the roster is check_retirement, and a design file whose
// component has code is a second copy of a contract the TSDoc already owns
// (DEC-16). The contract's own fields are the TSDoc's, never re-read here.
pub struct Roster {
    registry: BTreeSet<String>,
}

pub fn roster(repo: &Path) -> Result<Roster, String> {
    let emitted: serde_yaml::Mapping = oracle(repo, REGISTRY)?;
    let registry =
        emitted.into_keys().filter_map(|name| name.as_str().map(str::to_string)).collect();
    Ok(Roster { registry })
}

// A catalog row keys itself by the slug of the component it names, which is the
// name the roster carries: `copy-text` is CopyText.
fn component_name(slug: &str) -> String {
    slug.split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
        })
        .collect()
}

pub enum Verdict {
    // A doc that declares no artifact type, and one that declares itself draft or
    // archived, are both silent on the checklists - but neither is silent on its
    // links, so each carries the failures the link predicate found.
    Ungoverned { failures: Vec<Failure> },
    Exempt { status: String, failures: Vec<Failure> },
    Checked { artifacts: Vec<String>, failures: Vec<Failure>, summary: Option<String> },
}

impl Verdict {
    pub fn red(&self) -> bool {
        !self.failures().is_empty()
    }

    fn failures(&self) -> &[Failure] {
        match self {
            Self::Ungoverned { failures }
            | Self::Exempt { failures, .. }
            | Self::Checked { failures, .. } => failures,
        }
    }

    fn failures_mut(&mut self) -> &mut Vec<Failure> {
        match self {
            Self::Ungoverned { failures }
            | Self::Exempt { failures, .. }
            | Self::Checked { failures, .. } => failures,
        }
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
    rulebook: Option<&Rulebook>,
    vocabulary: Option<&Vocabulary>,
) -> Verdict {
    let Some(artifact) = attrs(html, "data-artifact").into_iter().next() else {
        return Verdict::Ungoverned { failures: Vec::new() };
    };
    // Before the exempt gate: a draft and an archived run both still have to
    // render, and this is the one property whose absence no later check can see.
    // Scope: governed docs only - an ungoverned page stays silent (the hooks'
    // standing promise), so the 62 T1/T2 pages keep the layer by convention.
    let mut layer = Vec::new();
    check_htmldoc_layer(&mut layer, html);
    if let Some(status) = exempt_declared(html, &artifact) {
        if layer.is_empty() {
            return Verdict::Exempt { status, failures: Vec::new() };
        }
        return Verdict::Checked {
            artifacts: vec![artifact.value],
            failures: layer,
            summary: None,
        };
    }

    let mut failures = layer;
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
        check_composition(&mut failures, html, &top_items);
        let allowed = read_pragmas(&mut failures, html);
        check_hard_rules(&mut failures, html, &top_items, vocabulary, &allowed);
        summary = Some(format!("{}\n{}", tally_line(rulebook), allowed.tally()));
    }

    Verdict::Checked { artifacts, failures, summary }
}

// The machine-checked tally, printed on every run: how many of the rulebook's
// hard rules carry a predicate, and - while any does not - which. With the
// receipt gone this is the only thing a run says about the rulebook, and it is a
// count of predicates, never a claim that the design honours them.
fn tally_line(rulebook: Option<&Rulebook>) -> String {
    let Some(rulebook) = rulebook else {
        return format!("hard rules: {RULEBOOK} unreadable - no machine-checked tally");
    };
    let hard = &rulebook.rules;
    let unchecked: Vec<&str> =
        hard.iter().map(String::as_str).filter(|s| !HARD_PREDICATES.contains(s)).collect();
    // "(design)" is the whole point of the line: the predicates judge the design
    // run, and ui/src/ is judged by design/vocabulary. Without the label a full
    // count reads as a claim about the built UI, which no predicate here makes.
    let gap = if unchecked.is_empty() {
        String::new()
    } else {
        format!(" — {} unchecked: {}", unchecked.len(), unchecked.join(" "))
    };
    format!(
        "hard rules: {}/{} machine-checked (design){gap}",
        hard.len() - unchecked.len(),
        hard.len(),
    )
}

fn check_composition(failures: &mut Vec<Failure>, html: &str, items: &[Tagged]) {
    let item = |name: &str| {
        items.iter().find(|(_, o)| o.value == name).map(|(_, o)| &html[o.span.0..o.span.1])
    };
    if let Some(capabilities) = item("capabilities") {
        check_capabilities(failures, capabilities);
    }
    if let Some(review) = item("review") {
        check_review_record(failures, review);
    }
    if let Some(mockup) = item("mockup") {
        if !mockup.contains("data-state-toggle") {
            failures.push(Failure {
                check: "mockup·state".to_string(),
                what: "the mockup is static - no state switch".to_string(),
                fix: "add data-state-toggle for every state the screen can reach - the mockup is the only place the human sees them".to_string(),
            });
        }
    }
    check_assets(failures, html);
    if let Some(literal) = colour_literal(html) {
        failures.push(Failure {
            check: "mockup·colour-literals".to_string(),
            what: format!("colour literal {literal} in the doc's own CSS"),
            fix: "every colour is a token from ui/tailwind.config.js - write its utility class"
                .to_string(),
        });
    }
}

// W5 made a specimen the component itself - build-specimens.mjs compiles
// ui/src/components/ plus React into one classic script a file:// page can load
// as window.Trawler. With the Elements catalog gone nothing enumerates which
// components a run reuses, so the bundle is what keeps a reuse from being drawn:
// a component's rendering is reachable only through window.Trawler, and a run
// that does not load it cannot mount anything (DEC-53).
const CDN: &str = "https://cdn.tailwindcss.com";
const THEME: &str = "../../doc-assets/components/tw-config.js";
const SPECIMEN: &str = "../../doc-assets/components/specimen.js";
/// The `src` a design run must carry, matched literally by `check_assets` - so
/// the run spec's own template is only correct while it spells this string.
pub const BUNDLE: &str = "../../doc-assets/components/specimens.bundle.js";

// One tag, three things, in the order they must arrive: the app theme, the
// shared specimen behaviours, then the collection. Loading the bundle's parts
// beside it double-binds every state toggle, because the bundle imports
// specimen.js for its side effects, and two toggles per click is no toggle.
fn check_assets(failures: &mut Vec<Failure>, html: &str) {
    let loads = |asset: &str| html.contains(&format!("src=\"{asset}\""));
    let mut fail = |what: String, fix: &str| {
        failures
            .push(Failure { check: "assets·specimen".to_string(), what, fix: fix.to_string() });
    };
    if !loads(CDN) {
        fail(
            format!("the run does not load {CDN}"),
            "compose the app's own utilities instead of re-typing them",
        );
    }
    if !loads(BUNDLE) {
        fail(
            format!("the run does not load {BUNDLE}"),
            "the mockup mounts real components from window.Trawler - build the collection with `cargo xtask ui specimens` and load the bundle instead of re-typing a rendering",
        );
        return;
    }
    for asset in [THEME, SPECIMEN] {
        if loads(asset) {
            fail(
                format!("{asset} is loaded beside the bundle, which already folds it in"),
                "load the bundle alone - a second copy of specimen.js binds every state toggle twice",
            );
        }
    }
}

// ─── the design files ───────────────────────────────────────────────────────
//
// A component design file carries no data-artifact: the folder is the artifact
// type, so these are keyed by path rather than by declaration.
pub fn verify_component(
    html: &str,
    name: &str,
    roster: Option<&Roster>,
    vocabulary: Option<&Vocabulary>,
) -> Verdict {
    let mut failures = Vec::new();
    check_htmldoc_layer(&mut failures, html);
    check_escaped_markup(&mut failures, html);
    match roster {
        Some(roster) => check_retirement(&mut failures, name, roster),
        None => failures.push(Failure {
            check: "components·registry-unreadable".to_string(),
            what: format!("cannot read the component roster via {REGISTRY}"),
            fix: "run verify-doc from the repo root with node available - whether this file is a retired contract's leftover copy cannot be answered unread".to_string(),
        }),
    }
    if let Some(vocabulary) = vocabulary {
        check_redrawn(&mut failures, html, vocabulary);
    }
    Verdict::Checked { artifacts: vec!["component-design".to_string()], failures, summary: None }
}

pub fn component_designed(path: &str) -> Option<String> {
    let file = path.split(COMPONENTS).nth(1)?.strip_suffix(".html")?;
    (!file.contains('/')).then(|| component_name(file))
}

// The registry and the design files are DISJOINT, and retirement is what keeps
// them so: once a component's contract is its TSDoc, the file holding the older
// copy is a second contract with nothing checking it against the first - the
// drift this epic exists to kill. So a file whose component has code is red
// until DEC-16's order has run over it, and the four that have no roster entry
// are not violations but the case DEC-16 names: a design file with no code is
// not retired, it is unbuilt.
fn check_retirement(failures: &mut Vec<Failure>, name: &str, roster: &Roster) {
    if roster.registry.contains(name) {
        failures.push(Failure {
            check: "components·unretired".to_string(),
            what: format!("{name}'s contract is its TSDoc, and this file still holds a copy"),
            fix: format!("retire it in DEC-16's order: distil the forks to docs/decisions/, verify {name}'s TSDoc carries the whole contract, then remove the file"),
        });
    }
}

// The design docs and the app speak one Tailwind vocabulary because they read
// one config (W5 dissolved the specimen sheet that was the second copy). A
// stylesheet rule for a class the config already emits re-opens the gap from
// the other side: the doc renders its own version of a shared utility, and
// nothing says which one the build inherits.
fn check_redrawn(failures: &mut Vec<Failure>, html: &str, vocabulary: &Vocabulary) {
    let mut redrawn: Vec<(String, String)> = Vec::new();
    for (selector, class) in stylesheet_classes(html) {
        if vocabulary.emits(&class) && !redrawn.iter().any(|(_, c)| *c == class) {
            redrawn.push((selector, class));
        }
    }
    for (selector, class) in redrawn {
        failures.push(Failure {
            check: "components·redrawn".to_string(),
            what: format!("`{selector}` re-draws `{class}`, which ui/tailwind.config.js emits"),
            fix: "the shared vocabulary owns this class - drop the rule and let the config render it, or name the doc's own class something the config does not emit".to_string(),
        });
    }
}

fn stylesheet_classes(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for block in html.split("<style").skip(1) {
        let css = strip_css_comments(block.split("</style>").next().unwrap_or(block));
        for rule in css.split('}') {
            let Some((selector, _)) = rule.split_once('{') else { continue };
            let selector = selector.trim().replace('\n', " ");
            for (at, _) in selector.match_indices('.') {
                let class: String = selector[at + 1..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !class.is_empty() {
                    out.push((selector.clone(), class));
                }
            }
        }
    }
    out
}

// ─── the twelve hard rules ──────────────────────────────────────────────────
//
// Each predicate is named for its slug and judges the DESIGN RUN, never
// ui/src/ - that surface is design/vocabulary's (the rulebook's binds axis).
// Five read the closed vocabulary; the rest read what the mockup doc declares -
// nine on the mockup itself, three beside it (DEC-53).
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
    check_wire_data(failures, html, item("mockup").is_some());
    if let Some(mockup) = item("mockup") {
        check_overflow_declared(failures, mockup);
        check_wire_order_preserved(failures, mockup);
    }
    if let Some(keyboard) = item("keyboard") {
        check_keyboard_parity(failures, keyboard);
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

// The marking rule reads the whole doc - a payload can be marked outside the
// mockup, and the wrapping half scans every class - but the DEMAND for a marking
// waits for the mockup, the way overflow and wire order wait for it too. A run
// whose mockup is not written yet, told it marks no payload, is a prompt for
// work it has not reached (E-DFLOW.W7 ⑤).
fn check_wire_data(failures: &mut Vec<Failure>, html: &str, mockup_written: bool) {
    let marked = attrs(html, "data-wire");
    let declared = attrs(html, "class");
    let bases = |declared: &Attr| -> Vec<String> {
        declared.value.split_whitespace().map(|c| base_class(c).to_string()).collect()
    };
    if marked.is_empty() {
        if mockup_written {
            failures.push(Failure {
                check: "mono-for-wire-data·undeclared".to_string(),
                what: "the mockup marks no payload element and does not say it has none".to_string(),
                fix: "mark every verbatim payload data-wire=\"<what it is>\", or declare data-wire=\"none\" once - a payload is anything the user reads or copies exactly".to_string(),
            });
        }
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
// keyword that scans markup instead reads `border` as an answer. So the zone
// host says so - the same shape `data-wire` itself uses, one attribute beside
// it, on the plain HTML the mockup mounts into (DEC-53).
fn check_wire_order_preserved(failures: &mut Vec<Failure>, mockup: &str) {
    let collections: Vec<Attr> =
        attrs(mockup, "data-wire").into_iter().filter(|w| w.value == "collection").collect();
    if collections.is_empty() {
        return;
    }
    let declared = attrs(mockup, "data-order");
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
                "the zone host carries data-order=\"wire\", or names the sort and why it beats the wire's"
                    .to_string(),
        });
    }
}

// overflow-declared. One overflow line under the mockup, naming every zone the
// mockup hosts. Both halves read the tokeniser, which is the only reading that
// holds: the line is the mockup's own paragraph opening with `overflow:`, and a
// zone is `data-zone="<name>"` on the host the mockup mounts into, because the
// ASCII sketch that used to name them is what DEC-53 cut. The prose form is the
// rulebook's, not this predicate's - the line says which zone is capped and who
// owns the cap, which no attribute carries.
const OVERFLOW_OPENS: &str = "overflow:";

fn check_overflow_declared(failures: &mut Vec<Failure>, mockup: &str) {
    let declaration = paragraphs(mockup)
        .into_iter()
        .map(|text| text.trim().to_lowercase())
        .find(|text| text.starts_with(OVERFLOW_OPENS));
    let Some(declaration) = declaration else {
        failures.push(Failure {
            check: "overflow-declared·line".to_string(),
            what: "the mockup declares no overflow".to_string(),
            fix: "one overflow line under the mockup - a paragraph opening with \"overflow:\": which zones scroll, which are capped, and which cap a component already owns".to_string(),
        });
        return;
    };
    let mut missing: Vec<String> = attrs(mockup, "data-zone")
        .into_iter()
        .map(|zone| zone.value.trim().to_lowercase())
        .filter(|zone| !zone.is_empty() && !names_zone(&declaration, zone))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        failures.push(Failure {
            check: "overflow-declared·zones".to_string(),
            what: format!("the overflow line is silent about zone {}", missing.join(" ")),
            fix: "every data-zone the mockup hosts is named: it scrolls, it is capped, or it cannot overflow".to_string(),
        });
    }
}

// A zone is named where the line says its name and not merely where some longer
// word contains it: `tab` is not declared by "table", nor `log` by "catalog".
// The sketch this rule used to read had single-letter zone ids and compared them
// exactly; free-text names re-opened the question, and the wrong answer here is
// the dangerous one - the gate goes green over a zone nobody declared. `-` and
// `_` count as part of a word, so `list` is not answered for by `flow-list`.
fn names_zone(declaration: &str, zone: &str) -> bool {
    let inside_word = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    declaration.match_indices(zone).any(|(at, _)| {
        let before = declaration[..at].chars().next_back();
        let after = declaration[at + zone.len()..].chars().next();
        !before.is_some_and(inside_word) && !after.is_some_and(inside_word)
    })
}

// keyboard-parity. Every pointer interaction has a keyboard path. Re-homed onto
// the Keyboard item, which exists for this one column: a key path is the one
// thing a static mockup cannot show, and the Interaction table it used to ride
// was too large to keep for a column's sake (DEC-53).
fn check_keyboard_parity(failures: &mut Vec<Failure>, keyboard: &str) {
    let rows = data_rows(keyboard);
    if rows.is_empty() {
        failures.push(Failure {
            check: "keyboard-parity·path".to_string(),
            what: "the Keyboard item names no interaction".to_string(),
            fix: "one row per pointer interaction: what it does | its key path from the rulebook's conventions - a screen the pointer cannot drive says so in a row of its own".to_string(),
        });
        return;
    }
    let silent = rows
        .iter()
        .filter(|row| {
            elements_of(row, "td").last().is_none_or(|cell| fragment_text(cell).trim().is_empty())
        })
        .count();
    if silent > 0 {
        failures.push(Failure {
            check: "keyboard-parity·path".to_string(),
            what: format!("{silent} of {} pointer interactions name no keyboard path", rows.len()),
            fix: "name the binding from the rulebook's conventions, propose a new one, or say the interaction is passive".to_string(),
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
            "one row per input the screen was handed: what it is | used · deliberately unused · blocked | where / why",
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

// W4 run #2, finding 29: two reviewer instructions moved half the run and it
// read as if the design had produced all of it. The strongest signal
// a run carries - a human overrode the design, and here is the contract it hit -
// was the one the artifact dropped. So the moved column is answered from the
// run's own item names: "the reviewer changed things" is not an answer, and
// `nothing` (the instruction was argued and the design held) is.
fn check_review_record(failures: &mut Vec<Failure>, review: &str) {
    let rows = data_rows(review);
    let mut fail = |what: String, fix: &str| {
        failures
            .push(Failure { check: "review·recorded".to_string(), what, fix: fix.to_string() });
    };
    if rows.is_empty() {
        fail(
            "the review item records no instruction".to_string(),
            "one row per reviewer instruction: what was said | what it moved (a run item, or `nothing`) | the contract it hit",
        );
        return;
    }
    let cell = |row: &str, at: usize| {
        elements_of(row, "td").get(at).map_or_else(String::new, |c| c.to_lowercase())
    };
    let unmoved = rows
        .iter()
        .filter(|r| {
            let moved = cell(r, 1);
            !moved.contains("nothing") && !DESIGN_RUN.iter().any(|item| moved.contains(item))
        })
        .count();
    if unmoved > 0 {
        fail(
            format!("{unmoved} of {} review rows name no item they moved", rows.len()),
            "name the run items the instruction moved, or `nothing` if the design held - a run that rewrites its own history silently is the defect this item exists to refuse",
        );
    }
    let uncited = rows
        .iter()
        .filter(|r| elements_of(r, "td").get(2).is_none_or(|hit| !hit.contains("<code>")))
        .count();
    if uncited > 0 {
        fail(
            format!("{uncited} of {} review rows cite no contract", rows.len()),
            "cite what the change hit: a rule slug, a component's @use/@avoid, or a decision record - <code>taste</code> when the design genuinely had latitude there",
        );
    }
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

// The mockup's own paragraphs, located by the tokeniser rather than by scanning
// for the string `<p`. That scan read `<pre>` and `<progress>` as paragraphs -
// and, finding no close for them, gave up on the real one - and it read a mount
// script's body as markup, so a line commented out inside the script counted as
// a declaration. Both directions were wrong, and `nodes` is wrong in neither: it
// matches the tag name exactly and treats a script or style body as opaque.
fn paragraphs(html: &str) -> Vec<String> {
    nodes(html)
        .into_iter()
        .filter(|node| node.name == "p" && node.shape == Shape::Open && node.closed)
        .map(|node| element_text(html, element_content(html, node.span.0).0))
        .collect()
}

// The text inside an element, given the span `element_content` returns: that
// span opens at the element's own tag, so the tag is skipped before the body is
// read as a fragment.
fn element_text(html: &str, span: (usize, usize)) -> String {
    let open_end = html[span.0..].find('>').map_or(span.0, |rel| span.0 + rel + 1).min(span.1);
    fragment_text(&html[open_end..span.1])
}

// The text of an element BODY - what `elements_of` hands back, already unwrapped.
// It reads from the first character, because a body has no opening tag of its
// own to skip: a reader that skipped one would eat everything before the first
// `>` the body itself contains, so a zone named `<code>verdicts</code>` in the
// overflow line would read as no overflow line at all - a hard rule refusing the
// run that obeyed it.
fn fragment_text(fragment: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in fragment.chars() {
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
// What a doc declares itself to be, read the way the checks read it: a mention of
// `data-artifact="design-run"` inside a <code> block - which the flow spec and two
// plans carry - declares nothing. `Some("")` is a run still open.
pub fn design_run_status(html: &str) -> Option<String> {
    let artifact = attrs(html, "data-artifact").into_iter().next()?;
    artifact
        .value
        .split_whitespace()
        .any(|token| token == "design-run")
        .then(|| exempt_declared(html, &artifact).unwrap_or_default())
}

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
// search: the offset is where the part already sits in the whole.
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
// The shared stylesheet, the comment layer and the diff layer all arrive through
// one bootstrap keyed by an id on <html>. A doc missing either renders unstyled -
// every var(--ink) it writes resolves to nothing - and no other predicate sees it,
// because they all read structure the doc still has.
fn check_htmldoc_layer(failures: &mut Vec<Failure>, html: &str) {
    // The runtime reads `document.documentElement.getAttribute(...)`, so the id
    // counts only on <html>. Anywhere else it is a string the bootstrap will
    // never find, and the layers it scopes stay dead on a doc that looks stamped.
    let on_root = nodes(html)
        .into_iter()
        .find(|node| node.name.eq_ignore_ascii_case("html"))
        .is_some_and(|node| node.attrs.iter().any(|(_, key, _)| key == "data-htmldoc-id"));
    if !on_root {
        failures.push(Failure {
            check: "htmldoc\u{b7}id".to_string(),
            what: "no data-htmldoc-id on <html>".to_string(),
            fix: "mint one - `curl -sS -X POST $SERVER/api/docs -H 'Content-Type: application/json' -d '{\"title\":\"...\"}'` - and stamp it on the <html> tag itself; the runtime reads document.documentElement, so the id on any other element is one the comment and diff layers never find".to_string(),
        });
    }
    let loads_bootstrap = attrs(html, "src").into_iter().any(|a| a.value == HTMLDOC_BOOTSTRAP);
    if !loads_bootstrap {
        failures.push(Failure {
            check: "htmldoc\u{b7}bootstrap".to_string(),
            what: format!("the doc does not load {HTMLDOC_BOOTSTRAP}"),
            fix: format!("add `<script type=\"module\" src=\"{HTMLDOC_BOOTSTRAP}\"></script>` before </body> - it injects doc.css from its own origin, so a doc without it, or pointing at a copy elsewhere, renders with every var(--...) resolving to nothing"),
        });
    }
}

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

// Any URI scheme, not a list of the ones seen so far: `https:`, `mailto:`,
// `tel:`, `data:` are all targets the tree cannot answer for. A colon alone does
// not make one - a scheme is letters, digits, `+-.` before it, and never a `/`,
// which keeps `notes/2026:1.html` a path.
fn has_scheme(target: &str) -> bool {
    target.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

// Asked of every doc, beside the artifact read rather than behind it: a doc with
// no `data-artifact` never reaches a checklist predicate, and its links still
// have to resolve.
fn link_targets(html: &str) -> Vec<String> {
    attrs(html, "href")
        .into_iter()
        .filter_map(|link| {
            let target = link.value.split('#').next().unwrap_or_default().trim();
            let unresolvable = target.is_empty()
                || has_scheme(target)
                // Root-relative: it means nothing to a reader opening the doc
                // over `file://`, which is the only way these docs are read.
                || target.starts_with('/')
                // DEC-3 keeps archives out of git, so a link into one cannot
                // resolve in a clean checkout BY DESIGN - the deliberate pointer
                // at an archived doc, not a link that rotted into one.
                || target.split('/').any(|part| part == ARCHIVE_DIR);
            (!unresolvable).then(|| target.to_string())
        })
        .collect()
}

fn check_links(failures: &mut Vec<Failure>, html: &str, doc: &Path) {
    let Some(dir) = doc.parent() else { return };
    // One dead target is one thing to fix, however many times the doc links it.
    let targets: BTreeSet<String> = link_targets(html).into_iter().collect();
    for target in targets {
        if dir.join(&target).exists() {
            continue;
        }
        failures.push(Failure {
            check: "link".to_string(),
            what: format!("href \"{target}\" resolves to no file in the tree"),
            fix: "point at the file that exists, or - when the target was archived out of git - drop the href and mark the citation archived".to_string(),
        });
    }
}

// The link failures fold into whatever verdict the doc's own checks produced, so
// an ungoverned doc still reports itself ungoverned while naming a dead link, and
// a governed one reports its checklist problems and its links together.
fn with_links(verdict: Verdict, html: &str, doc: &Path) -> Verdict {
    // A draft is the one silent surface the work-planning standard promises an
    // author, and a link to a doc not written yet is the ordinary midpoint of
    // writing a set of docs. Docs under the archive are out of git entirely.
    let draft = matches!(&verdict, Verdict::Exempt { status, .. } if status == "draft");
    if draft || doc.components().any(|part| part.as_os_str() == ARCHIVE_DIR) {
        return verdict;
    }
    let mut found = Vec::new();
    check_links(&mut found, html, doc);
    let mut verdict = verdict;
    verdict.failures_mut().extend(found);
    verdict
}

pub fn run_paths(repo: &Path, paths: &[&str]) -> Result<(i32, String), String> {
    let single = paths.len() == 1;
    let rules = rulebook(repo);
    // Read once for the whole invocation, and only when a doc in the batch has a
    // rule that reads them: each oracle costs a node process.
    let mut vocab: Option<Result<Vocabulary, String>> = None;
    let mut names: Option<Result<Roster, String>> = None;
    let mut out = String::new();
    let mut any_red = false;
    for path in paths {
        let resolved =
            if Path::new(path).exists() { Path::new(path).to_path_buf() } else { repo.join(path) };
        let html =
            std::fs::read_to_string(&resolved).map_err(|e| format!("cannot read {path}: {e}"))?;
        let component = component_designed(path);
        let designs = declares_a_design(component.is_some(), &html);
        if vocab.is_none() && designs {
            vocab = Some(vocabulary(repo));
        }
        if names.is_none() && component.is_some() {
            names = Some(roster(repo));
        }
        let vocab_read = vocab.as_ref().and_then(|read| read.as_ref().ok());
        let roster_read = names.as_ref().and_then(|read| read.as_ref().ok());
        let verdict = match component {
            Some(name) => verify_component(&html, &name, roster_read, vocab_read),
            None => verify_html_against(&html, rules.as_ref(), vocab_read),
        };
        let verdict = with_links(verdict, &html, &resolved);
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

// Read the declaration, never the string: a substring scan says yes to every doc
// that merely quotes the contract in prose, and each yes costs two node processes
// nothing then reads. `design_run_status` is the same tokenised answer the gate's
// run-finder already takes.
fn declares_a_design(is_component: bool, html: &str) -> bool {
    is_component || design_run_status(html).is_some()
}

pub fn format_verdict(path: &str, verdict: &Verdict) -> String {
    match verdict {
        Verdict::Ungoverned { failures } if failures.is_empty() => {
            format!("verify-doc ungoverned on {path} - no data-artifact, not checked")
        }
        Verdict::Exempt { status, failures } if failures.is_empty() && status == "archived" => {
            format!("verify-doc archived on {path} - closed under an older contract, not reopened")
        }
        Verdict::Exempt { failures, .. } if failures.is_empty() => {
            format!("verify-doc draft on {path} - checks silent until ready")
        }
        Verdict::Checked { artifacts, failures, summary } if failures.is_empty() => {
            let head = format!("verify-doc green on {path} ({})", artifacts.join(" "));
            match summary {
                Some(s) => format!("{head}\n{s}"),
                None => head,
            }
        }
        red => {
            let failures = red.failures();
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
    use std::fmt::Write as _;

    use super::{
        attrs, check_links, component_designed, declares_a_design, edit_distance, elements_of,
        format_verdict, link_targets, registry_entry, rules_from_rulebook, screen_file,
        surface_name, ui_surface, verify_component, verify_html, verify_html_against, with_links,
        Roster, Verdict, Vocabulary, CAPABILITY_STATES,
    };

    fn doc(artifact: &str, status: &str, body: &str) -> String {
        let status_attr =
            if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
        format!(
            "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
             <main data-artifact=\"{artifact}\"{status_attr}>{body}</main>\
             <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
             </body></html>"
        )
    }

    // Every real doc carries the htmldoc layer; a fixture that omits it is
    // testing the layer check, not the predicate it means to exercise.
    fn with_layer(body: &str) -> String {
        format!(
            "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>{body}\
             <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
             </body></html>"
        )
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
         </tbody></table>";

    fn rules() -> super::Rulebook {
        rulebook_of(RULEBOOK)
    }

    fn rulebook_of(html: &str) -> super::Rulebook {
        super::Rulebook { rules: rules_from_rulebook(html) }
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

    // Two components with code: what check_retirement reads a design file against.
    fn roster() -> Roster {
        Roster { registry: ["Section", "CopyText"].into_iter().map(str::to_string).collect() }
    }

    // The mockup the fixture ships: several tests swap a piece of it, and a
    // literal repeated that often drifts on the first edit. The zone host, the
    // overflow line and the state toggle are all load-bearing - three re-homed
    // predicates read them (DEC-53).
    const RUN_MOCKUP: &str = "<div data-item=\"mockup\" data-wire=\"none\">\
             <div id=\"m-zone\" data-zone=\"verdicts\"></div>\
             <script type=\"text/babel\">mount('m-zone', <Section title=\"headers\">rows</Section>)</script>\
             <p>overflow: verdicts scrolls to its own cap</p>\
             <button data-state-toggle=\"mock\">flip</button></div>";

    // The three declarations the re-homed rules read, named once: several tests
    // swap a zone host for a collection host or blank the key path, and a literal
    // repeated that often drifts on the first edit.
    const Z: &str = "data-zone=\"verdicts\"";
    const COLL: &str = "data-zone=\"verdicts\" data-wire=\"collection\"";
    const KEY: &str =
        "<td><code>\u{2191}</code> / <code>\u{2193}</code> then <code>Enter</code></td>";

    fn green_run() -> String {
        let body = format!(
            "<script src=\"https://cdn.tailwindcss.com\"></script>\
             <script src=\"../../doc-assets/components/specimens.bundle.js\"></script>\
             <style>.mock{{color:var(--a-ink);}}</style>\
             <main data-artifact=\"design-run\">\
             {RUN_MOCKUP}\
             <table data-item=\"capabilities\"><tr><td>CA install</td><td>used</td>\
             <td>B1's button</td></tr></table>\
             <table data-item=\"keyboard\"><tr><td>open a verdict</td><td>the row</td>\
             <td><code>↑</code> / <code>↓</code> then <code>Enter</code></td></tr></table>\
             <p data-item=\"proposals\">band</p><p data-item=\"blocked-on\">verb tokens</p>\
             <table data-item=\"review\"><tr><td>copy icon only</td><td>mockup</td>\
             <td><code>CopyButton</code> @avoid</td></tr></table></main>"
        );
        with_layer(&body)
    }

    #[test]
    fn rulebook_slugs_are_read_from_the_rulebook_tables() {
        assert_eq!(
            rules().rules,
            vec!["colour-tokens-only".to_string(), "spacing-scale".to_string()],
            "header rows and non-slug code spans must not become rules"
        );
    }

    // Every doc that explains the design contract quotes the declaration, and a
    // substring scan reads each quotation as a design - buying two node processes
    // per doc that nothing goes on to read. The run-finder already tokenises; this
    // is the same answer at the site that pays for it.
    #[test]
    fn a_doc_that_only_quotes_the_design_declaration_is_not_designing() {
        let quoted = doc(
            "wu-result",
            "",
            "<p>gate on <code>data-artifact=\"design-run\"</code> and never on the string</p>",
        );
        assert!(
            !declares_a_design(false, &quoted),
            "a quoted declaration is prose, not an artifact"
        );
        assert!(
            declares_a_design(false, &doc("design-run", "draft", "<p>x</p>")),
            "the real declaration still asks for the oracles, draft or not"
        );
        assert!(
            declares_a_design(true, &quoted),
            "a component design file is named by its path, whatever its body quotes"
        );
    }

    // DEC-53's whole shape, asserted as a list rather than inferred from which
    // fixtures happen to pass: six items, and a run still writing one of the cut
    // stations is told which one by name rather than tolerated.
    #[test]
    fn the_run_owes_the_six_items_and_a_cut_station_is_named() {
        assert_eq!(
            super::DESIGN_RUN,
            ["mockup", "capabilities", "keyboard", "proposals", "blocked-on", "review"],
        );
        let half =
            green_run().replace("<table data-item=\"keyboard\">", "<table data-item=\"kyboard\">");
        let owed = reds(&half);
        assert!(owed.contains(&"item·keyboard".to_string()), "{owed:?}");
        assert!(
            what(&doc_verdict(&half), "item·unknown-tag").contains("missing item \"keyboard\""),
            "a near miss pairs with the item it meant"
        );

        for cut in ["frame", "layout", "elements", "interaction", "tune", "receipt"] {
            let carried = green_run().replace(
                "<p data-item=\"proposals\">band</p>",
                &format!(
                    "<p data-item=\"{cut}\">the old shape</p><p data-item=\"proposals\">band</p>"
                ),
            );
            assert!(
                what(&doc_verdict(&carried), "item·unknown-tag").contains(cut),
                "a run carrying the cut {cut} must be red by name, never tolerated"
            );
        }
    }

    // The declaration is located by the tokeniser, so the three ways a scan for
    // the string `<p` got it wrong are all closed at once: a native tag that
    // merely starts with p is not a paragraph, a mount script's body is not
    // markup, and a word that merely starts with "overflow" is not the line.
    #[test]
    fn the_overflow_line_is_the_mockups_own_paragraph_and_nothing_that_looks_like_one() {
        let before_the_line = |markup: &str| {
            green_run().replace(
                "<p>overflow: verdicts scrolls to its own cap</p>",
                &format!("{markup}<p>overflow: verdicts scrolls to its own cap</p>"),
            )
        };
        for native in [
            "<pre>GET /flows HTTP/1.1</pre>",
            "<progress value=\"3\" max=\"10\"></progress>",
            "<svg><path d=\"M0 0 L1 1\"></path></svg>",
            "<picture><img src=\"x.png\"></picture>",
        ] {
            assert!(
                !reds(&before_the_line(native)).contains(&"overflow-declared·line".to_string()),
                "a native tag starting with p is not a paragraph: {native}"
            );
        }

        // The failure this check's own comment promises to refuse, reached by a
        // different route: the line exists only where nothing renders it.
        let only_in_the_mount =
            green_run().replace("<p>overflow: verdicts scrolls to its own cap</p>", "").replace(
                "mount('m-zone'",
                "// <p>overflow: verdicts scrolls to its own cap</p>\nmount('m-zone'",
            );
        assert!(
            what(&doc_verdict(&only_in_the_mount), "overflow-declared·line")
                .contains("no overflow"),
            "a line commented out inside the mount script declares nothing"
        );

        let merely_starts_with_the_word = green_run().replace(
            "<p>overflow: verdicts scrolls to its own cap</p>",
            "<p>Overflowing verdicts are capped elsewhere</p>",
        );
        assert!(
            what(&doc_verdict(&merely_starts_with_the_word), "overflow-declared·line")
                .contains("no overflow"),
            "\"Overflowing\" is a sentence about overflow, not the declaration"
        );
    }

    // A zone is named or it is not, and a longer word that merely contains its
    // name is not naming it. The sketch this rule used to read had single-letter
    // zone ids and compared them exactly; free-text names re-opened the question,
    // and a substring test answers it wrong in the direction that matters - the
    // gate goes green over a zone whose overflow nobody declared.
    #[test]
    fn a_zone_is_not_named_by_a_longer_word_that_merely_contains_it() {
        let two_zones = |line: &str| {
            green_run()
                .replace(Z, "data-zone=\"verdicts\"></div><div data-zone=\"tab\"")
                .replace("<p>overflow: verdicts scrolls to its own cap</p>", line)
        };
        let collides =
            two_zones("<p>overflow: verdicts scrolls to its own cap, rendered as a data table</p>");
        assert!(
            what(&doc_verdict(&collides), "overflow-declared·zones").contains("zone tab"),
            "\"table\" does not declare what the tab zone does"
        );

        let named = two_zones("<p>overflow: verdicts scrolls to its own cap; tab is pinned</p>");
        assert!(
            !reds(&named).contains(&"overflow-declared·zones".to_string()),
            "the zone is named as itself, so the line answers for it"
        );
    }

    // The overflow line is read as a fragment, and a fragment carries no wrapping
    // tag - so reading it the way an element is read eats every word before the
    // first `>` the body itself contains. A zone named in <code>, which is how
    // this codebase writes an identifier everywhere else, then read as a mockup
    // declaring no overflow at all: a hard rule refusing a run that obeyed it.
    #[test]
    fn an_overflow_line_that_names_its_zone_in_markup_is_still_read() {
        for line in [
            "<p>overflow: <code>verdicts</code> scrolls to its own cap</p>",
            "<p>overflow: verdicts scrolls to its own <code>cap</code></p>",
        ] {
            let doc = green_run().replace("<p>overflow: verdicts scrolls to its own cap</p>", line);
            assert!(
                !reds(&doc).contains(&"overflow-declared·line".to_string()),
                "inline markup is not a missing declaration: {line}"
            );
        }
    }

    // Re-homed from the Layout sketch onto the mockup (DEC-53). The line is a
    // paragraph opening with the word, so a mount's own `overflow-y-auto` cannot
    // answer for it, and the zones it must name are declared beside the mounts.
    #[test]
    fn the_mockup_declares_its_overflow_and_names_every_zone_it_hosts() {
        let silent = green_run().replace("<p>overflow: verdicts scrolls to its own cap</p>", "");
        assert!(what(&doc_verdict(&silent), "overflow-declared·line").contains("no overflow"));

        let mounted_class = green_run().replace(
            "<p>overflow: verdicts scrolls to its own cap</p>",
            "<div class=\"overflow-auto\"></div>",
        );
        assert!(
            what(&doc_verdict(&mounted_class), "overflow-declared·line").contains("no overflow"),
            "a utility class is a rendering, not a declaration"
        );

        let second_zone =
            green_run().replace(Z, "data-zone=\"verdicts\"></div><div data-zone=\"detail\"");
        assert!(
            what(&doc_verdict(&second_zone), "overflow-declared·zones").contains("zone detail"),
            "a zone the line skipped is named"
        );
        assert!(
            !reds(&green_run()).contains(&"overflow-declared·line".to_string()),
            "the fixture's own line answers for the zone it hosts"
        );
    }

    #[test]
    fn conforming_design_run_is_green_with_a_computed_tally() {
        let v = verify_html_against(&green_run(), Some(&rules()), Some(&vocab()));
        assert!(!v.red(), "{:?}", failures(&v));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(summary.contains("hard rules: 2/2 machine-checked (design)"), "{summary}");
        assert!(summary.contains("design-allow: 0 live suppression"), "{summary}");
    }

    // A tally is a count of predicates, so a rulebook nobody can read has no
    // count to print - and saying so beats printing a number the checker did not
    // measure. The run itself is unaffected: the twelve predicates read the
    // mockup doc, never the book.
    #[test]
    fn an_unreadable_rulebook_says_so_instead_of_claiming_a_tally() {
        let v = verify_html_against(&green_run(), None, Some(&vocab()));
        assert!(!v.red(), "{:?}", failures(&v));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(summary.contains("unreadable - no machine-checked tally"), "{summary}");
        assert!(summary.contains("design-allow: 0 live suppression"), "{summary}");
    }

    // The surface label is the load-bearing word: 12/12 without it reads as a
    // claim about the built UI, which no predicate in this file makes.
    #[test]
    fn the_gap_line_names_the_surface_and_the_slugs_still_unchecked() {
        let widened = RULEBOOK.replace(
            "<tr><td><code>spacing-scale</code></td>",
            "<tr><td><code>not-a-predicate</code></td><td>x</td></tr>\
             <tr><td><code>spacing-scale</code></td>",
        );
        let rules = rulebook_of(&widened);
        let v = verify_html_against(&green_run(), Some(&rules), Some(&vocab()));
        let Verdict::Checked { summary: Some(summary), .. } = &v else { panic!("no tally") };
        assert!(
            summary.contains("2/3 machine-checked (design) — 1 unchecked: not-a-predicate"),
            "{summary}"
        );
    }

    #[test]
    fn a_static_mockup_is_red() {
        let doc = green_run().replace("data-state-toggle=\"mock\"", "");
        assert!(
            what(&doc_verdict(&doc), "mockup·state").contains("every state the screen can reach")
        );
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

    // W4 finding 29: the run doc read as if the stages had produced what two
    // reviewer instructions moved. The item is required, so a run that had a
    // reviewer and says nothing about them cannot be told from one that did not.
    #[test]
    fn a_run_that_records_no_review_is_red() {
        let doc = green_run().replace(" data-item=\"review\"", "");
        assert!(failures(&doc_verdict(&doc)).contains(&"item·review".to_string()));
    }

    #[test]
    fn a_review_item_with_no_instruction_row_is_red() {
        let doc = green_run().replace(
            "<tr><td>copy icon only</td><td>mockup</td>\
             <td><code>CopyButton</code> @avoid</td></tr>",
            "<tr><td>the reviewer had notes</td></tr>",
        );
        assert!(what(&doc_verdict(&doc), "review·recorded").contains("no instruction"));
    }

    #[test]
    fn a_review_row_that_names_no_item_it_moved_is_red() {
        let doc = green_run().replace("<td>mockup</td>", "<td>several things</td>");
        assert!(what(&doc_verdict(&doc), "review·recorded").contains("no item"));
    }

    // An instruction the design argued and held is a first-class outcome - the
    // row exists to record that it was heard, not that it won.
    #[test]
    fn a_review_row_that_moved_nothing_says_so_and_passes() {
        let doc = green_run().replace("<td>mockup</td>", "<td>nothing</td>");
        assert!(!doc_verdict(&doc).red(), "{:?}", failures(&doc_verdict(&doc)));
    }

    #[test]
    fn a_review_row_citing_no_contract_is_red() {
        let doc =
            green_run().replace("<td><code>CopyButton</code> @avoid</td>", "<td>it fits</td>");
        assert!(what(&doc_verdict(&doc), "review·recorded").contains("cite no contract"));
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

    fn doc_verdict(html: &str) -> Verdict {
        verify_html_against(html, Some(&rules()), Some(&vocab()))
    }

    #[test]
    fn a_run_that_does_not_link_the_shared_specimens_is_red() {
        let doc = green_run()
            .replace("src=\"../../doc-assets/components/specimens.bundle.js\"", "src=\"mine.js\"");
        assert!(what(&doc_verdict(&doc), "assets·specimen").contains("re-typing"));

        let mentioned = doc.replace(
            "<style>",
            "<a href=\"x\">doc-assets/components/specimens.bundle.js</a><style>",
        );
        assert!(
            doc_verdict(&mentioned).red(),
            "naming the bundle in prose composes nothing - the attribute is the proof"
        );
    }

    // Both at once is the migration's own trap: the bundle imports specimen.js
    // for its side effects, so a second copy binds every toggle twice and the
    // state switch stops switching while every predicate stays green.
    #[test]
    fn loading_the_bundle_beside_what_it_folds_in_is_red() {
        let doc = green_run().replace(
            "src=\"../../doc-assets/components/specimens.bundle.js\"></script>",
            "src=\"../../doc-assets/components/specimens.bundle.js\"></script>\
             <script src=\"../../doc-assets/components/specimen.js\" defer></script>",
        );
        assert!(what(&doc_verdict(&doc), "assets·specimen").contains("twice"));
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
        assert!(matches!(verify_html(&with_layer("<p>plain doc</p>")), Verdict::Ungoverned { .. }));
    }

    #[test]
    fn artifact_mentioned_as_text_does_not_govern() {
        let html = with_layer("<p>use <code>data-artifact=\"epic-plan\"</code> to opt in</p>");
        assert!(matches!(verify_html(&html), Verdict::Ungoverned { .. }));
    }

    #[test]
    fn draft_is_silent() {
        let v = verify_html(&doc("wu-result", "draft", ""));
        assert!(matches!(&v, Verdict::Exempt { status: s, .. } if s == "draft"));
        assert!(format_verdict("d.html", &v).contains("draft"));
    }

    // The defect this predicate exists for shipped twice before it was written:
    // a run doc whose every var(--...) resolved to nothing, and W10's own proof
    // run, styled but with both interactive layers dead. No other check sees it -
    // they read structure a layerless doc still has.
    #[test]
    fn a_doc_without_the_htmldoc_layer_is_red_even_when_archived() {
        let body = "<p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
                    <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-9</p>\
                    <p data-item=\"follow-ups\">none</p>";
        assert!(
            failures(&verify_html(&doc("wu-result", "", body))).is_empty(),
            "the layer is present"
        );

        let no_id =
            doc("wu-result", "", body).replace(" data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"", "");
        assert_eq!(
            failures(&verify_html(&no_id)),
            vec!["htmldoc·id"],
            "the id is the comment layer's scope"
        );

        let no_boot = doc("wu-result", "", body).replace(
            "<script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>",
            "",
        );
        assert_eq!(
            failures(&verify_html(&no_boot)),
            vec!["htmldoc·bootstrap"],
            "the bootstrap injects doc.css"
        );

        // The trap: `archived` returns before every other predicate, so a closed
        // run could carry the defect forever. A record still has to render.
        let archived = no_id.replace(
            "data-artifact=\"wu-result\"",
            "data-artifact=\"wu-result\" data-status=\"archived\"",
        );
        assert_eq!(
            failures(&verify_html(&archived)),
            vec!["htmldoc·id"],
            "archived does not excuse it"
        );
        let closed = doc("wu-result", "archived", body);
        assert!(
            matches!(verify_html(&closed), Verdict::Exempt { status: s, .. } if s == "archived"),
            "a layered archive stays exempt"
        );
    }

    // Both predicates shipped naming a placement they never checked: the id was
    // read from anywhere in the document and the bootstrap from any src ending in
    // the filename, so a stray id on a <div> and a local copy of the script both
    // passed a doc whose layers were dead.
    #[test]
    fn the_layer_counts_only_on_the_root_and_from_its_own_origin() {
        let body = "<p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
                    <p data-item=\"evidence\">tests</p><p data-item=\"distillation\">DEC-9</p>\
                    <p data-item=\"follow-ups\">none</p>";

        let misplaced = doc("wu-result", "", body)
            .replace(" data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"", "")
            .replace(
                "<body>",
                "<body><div data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\">stray</div>",
            );
        assert_eq!(
            failures(&verify_html(&misplaced)),
            vec!["htmldoc\u{b7}id"],
            "the runtime reads document.documentElement, so only <html> carries it"
        );

        let elsewhere = doc("wu-result", "", body)
            .replace("https://htmldoc.wfomobile.com/htmldoc.js", "./vendor/htmldoc.js");
        assert_eq!(
            failures(&verify_html(&elsewhere)),
            vec!["htmldoc\u{b7}bootstrap"],
            "a copy elsewhere serves a different doc.css, or none"
        );
    }

    // Tightening a stage's form re-opens every run that closed before the rule
    // existed. An archived run is a record of what the flow was, not work anyone
    // will redo.
    #[test]
    fn an_archived_run_is_silent_but_a_ready_one_is_not() {
        let stale = green_run().replace("<p>overflow: verdicts scrolls to its own cap</p>", "");
        assert!(!failures(&doc_verdict(&stale)).is_empty(), "a ready run is checked");
        let archived = stale.replace(
            "data-artifact=\"design-run\"",
            "data-artifact=\"design-run\" data-status=\"archived\"",
        );
        let v = verify_html(&archived);
        assert!(
            matches!(&v, Verdict::Exempt { status: s, .. } if s == "archived"),
            "archived exempts"
        );
        let line = format_verdict("run.html", &v);
        assert!(line.contains("archived") && !line.contains("until ready"), "{line}");
    }

    // What activates the review's design dimension. The asymmetry is the point:
    // Tier-1 goes SILENT on an archived run (closed under an older contract) and
    // the design step wakes up on exactly that file, because a finished design is the only
    // kind there is anything independent to say about.
    #[test]
    fn only_a_closed_run_or_a_component_file_is_a_design_artifact() {
        let run = green_run();
        let archived = run.replace(
            "data-artifact=\"design-run\"",
            "data-artifact=\"design-run\" data-status=\"archived\"",
        );
        let draft = run.replace(
            "data-artifact=\"design-run\"",
            "data-artifact=\"design-run\" data-status=\"draft\"",
        );
        let path = "docs/work/2026-08-13-dflow-run-4.html";
        assert_eq!(super::design_artifact(path, &archived), Some("design-run"));
        assert_eq!(super::design_artifact(path, &draft), None, "a draft run is unfinished");
        assert_eq!(super::design_artifact(path, &run), None, "an open run is unfinished");
        assert_eq!(
            super::design_artifact("docs/work/components/button.html", "<html></html>"),
            Some("component-design"),
            "a component design file is a finished contract whatever it contains"
        );
        assert_eq!(super::design_artifact("docs/work/components/specimen.js", ""), None);
        assert_eq!(
            super::design_artifact(
                "docs/workflow.html",
                &archived.replace("design-run", "wu-plan")
            ),
            None
        );
        assert_eq!(
            super::design_artifact(
                "docs/work/flow-spec.html",
                "<html><body><main data-artifact=\"wu-plan\"><code>data-artifact=\"design-run\" data-status=\"archived\"</code></main></body></html>"
            ),
            None,
            "a doc that quotes the declaration is not a run"
        );
    }

    #[test]
    fn a_pin_names_a_blob_only_at_git_s_own_abbreviation_floor() {
        let blob = "796c547cb1de0f21e5a1b1a1c0ffee0123456789";
        assert!(super::pin_matches("796c547", blob));
        assert!(super::pin_matches("796C547", blob), "case is not the claim");
        assert!(super::pin_matches(blob, blob));
        assert!(!super::pin_matches("796c54", blob), "under the floor names ~1 blob in 16");
        assert!(!super::pin_matches("28f5437", blob), "another blob is another judgment");
    }

    // What this asserts is that the archived flag is load-bearing on each run it
    // finds - lifted, the run is governed again - so an archived run can never
    // pass a check vacuously.
    //
    // It scans docs/work/ AND xtask/fixtures/, and requires runs from the second
    // rather than the first. docs/work/ is empty at an epic tip by rule
    // (check_tracked_docs), so a test that demanded runs there asserted the
    // opposite of what the epic gate enforces and could only pass mid-epic - it
    // did, until E-DFLOW.W12 archived the epic's scratch and left it with nothing
    // to find. The fixtures are the frozen specimens; docs/work/ is still scanned
    // so a run being designed right now is held to the same property.
    //
    // It does NOT assert every run is archived: a run mid-design is open, and red,
    // for as long as it is being designed. Closing is enforced where the tree is
    // answerable for it, at the merge (6·run-closed).
    #[test]
    fn every_archived_run_is_exempt_by_a_flag_that_is_doing_work() {
        let runs: Vec<(String, String)> = design_runs_in_the_tree();
        let archived = runs
            .iter()
            .filter(|(_, h)| super::design_run_status(h).as_deref() == Some("archived"))
            .count();
        assert!(archived >= 2, "the discovery found no archived runs: {archived}");
        for (path, html) in runs {
            let Some(status) = super::design_run_status(&html) else {
                panic!("{path} is not a run")
            };
            if status != "archived" {
                continue;
            }
            assert!(
                matches!(verify_html(&html), Verdict::Exempt { status: s, .. } if s == "archived"),
                "{path} declares archived but is not exempt"
            );
            let reopened = html.replace(" data-status=\"archived\"", "");
            assert_ne!(reopened, html, "{path}: lifting the flag changed nothing");
            assert!(
                !matches!(verify_html(&reopened), Verdict::Exempt { .. }),
                "{path} is silent for some other reason - the archived flag proves nothing here"
            );
        }
    }

    fn design_runs_in_the_tree() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found: Vec<(String, String)> = ["fixtures", "../docs/work"]
            .iter()
            .filter_map(|dir| std::fs::read_dir(root.join(dir)).ok())
            .flatten()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let html = std::fs::read_to_string(&path).ok()?;
                super::design_run_status(&html)?;
                Some((path.file_name()?.to_string_lossy().to_string(), html))
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    // The flow spec and two plans quote `data-artifact="design-run"` inside a
    // <code> block; a scan for the string would call all three runs and gate them.
    #[test]
    fn a_quoted_artifact_declaration_is_not_a_run() {
        assert_eq!(
            super::design_run_status(
                "<p>a run declares <code>data-artifact=\"design-run\"</code></p>"
            ),
            None
        );
        assert_eq!(super::design_run_status(&green_run()).as_deref(), Some(""));
        assert_eq!(
            super::design_run_status(&green_run().replace(
                "data-artifact=\"design-run\"",
                "data-artifact=\"design-run\" data-status=\"archived\"",
            ))
            .as_deref(),
            Some("archived")
        );
        // `ready` is a declaration that the run is open and checked, not a close.
        assert_eq!(
            super::design_run_status(&green_run().replace(
                "data-artifact=\"design-run\"",
                "data-artifact=\"design-run\" data-status=\"ready\"",
            ))
            .as_deref(),
            Some("")
        );
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
        let html = doc(
            "wu-result",
            "",
            "<div data-item=\"outcome\"><div>inner</div></div>\
             <div data-item=\"deviations\"><div></div></div>\
             <p data-item=\"evidence\">e</p><p data-item=\"distillation\">d</p>\
             <p data-item=\"follow-ups\">none</p>",
        );
        let v = verify_html(&html);
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

    fn raised(html: &str) -> Vec<crate::gate::Failure> {
        match verify_html_against(html, Some(&rules()), Some(&vocab())) {
            Verdict::Checked { failures, .. } => failures,
            _ => panic!("expected a checked verdict"),
        }
    }

    // "Every red says what to do about it" is the harness's hand-over rule
    // (E-DFLOW.W7 ⑥), and a rule an author has to remember is one an author
    // forgets - which is this epic's own thesis. So it is asserted rather than
    // audited: over every failure a broken run can raise, the fix must exist, be
    // an instruction rather than a restatement of the observation, and not repeat
    // the `what` it follows. W6's round 9 is the precedent for the third clause -
    // a message that restated its own observation named a cause it could not know.
    #[test]
    fn every_red_a_run_can_raise_says_what_to_do_about_it() {
        // Every mangling below keeps the markup balanced. An unclosed tag empties
        // the section instead of breaking the rule inside it, which trades a whole
        // family of checks for one `item·` red and quietly narrows the guard.
        let bare = doc("design-run", "", "<p>nothing tagged at all</p>");
        let broken = green_run()
            .replace(" data-wire=\"none\"", "")
            .replace("data-state-toggle=\"mock\"", "static")
            .replace("<p>overflow: verdicts scrolls to its own cap</p>", "")
            .replace("specimens.bundle.js", "mine.js")
            .replace("<td>used</td>", "<td>maybe</td>")
            .replace("<td>mockup</td>", "<td>the reviewer changed things</td>")
            .replace(".mock{color:var(--a-ink);}", ".mock{color:#f7f5f0;}");
        let unanswered = green_run()
            .replace("data-zone=\"verdicts\"", "data-zone=\"verdicts\" data-wire=\"collection\"")
            .replace("<code>\u{2191}</code> / <code>\u{2193}</code> then <code>Enter</code>", "")
            .replace("<td><code>CopyButton</code> @avoid</td>", "<td>it felt wrong</td>");
        let off_vocabulary = mockup(
            "<span class=\"text-pink-500 font-mono truncate rounded-xl p-[13px]\">x</span>\
             <span class=\"text-[#ff00aa]\">tinted</span>\
             <span class=\"uppercase text-ink\">LABEL</span>\
             <!-- design-allow: text-pink-500 -->\
             <div class=\"text-pink-500\">excused by nothing</div>",
        );
        // The cut stations, still being written: the failure a run gets for
        // carrying what DEC-53 removed, and the one for a zone the line skipped.
        let cut_station = green_run().replace(
            "<p data-item=\"proposals\">band</p>",
            "<p data-item=\"frame\">drivers</p><p data-item=\"receipt\">rulebook: x</p>\
             <p data-item=\"proposals\">band</p>",
        );
        let silent_zone =
            green_run().replace(Z, "data-zone=\"verdicts\"></div><div data-zone=\"detail\"");

        let fixtures = [bare, broken, unanswered, off_vocabulary, cut_station, silent_zone];
        let raised: Vec<crate::gate::Failure> = fixtures.iter().flat_map(|d| raised(d)).collect();
        let distinct: super::BTreeSet<&str> = raised.iter().map(|f| f.check.as_str()).collect();
        assert!(
            distinct.len() >= 25,
            "the fixtures must exercise a real spread of checks, got {}: {distinct:?}",
            distinct.len()
        );
        for f in &raised {
            assert!(!f.fix.trim().is_empty(), "{} raises no fix", f.check);
            assert_ne!(f.fix.trim(), f.what.trim(), "{}'s fix restates its what", f.check);
            assert!(
                f.fix.split_whitespace().count() >= 4,
                "{}'s fix is too short to be an instruction: {:?}",
                f.check,
                f.fix
            );
        }
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

    // ...but a run that has not written its mockup yet is owed nothing about
    // payloads: the demand waits for the stage that answers it, the way overflow
    // waits for the layout. The mid-flow run is red for the mockup it still owes
    // and for nothing beyond that.
    #[test]
    fn a_run_that_has_not_reached_its_mockup_is_not_asked_to_mark_payloads() {
        let mid_flow = green_run().replace(RUN_MOCKUP, "");
        let checks = reds(&mid_flow);
        assert!(checks.contains(&"item·mockup".to_string()), "{checks:?}");
        assert!(!checks.contains(&"mono-for-wire-data·undeclared".to_string()), "{checks:?}");
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
        let collection = green_run().replace(Z, COLL);
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
        let collection = |host: &str| green_run().replace(Z, host);

        for silent in [
            "data-wire=\"collection\" class=\"border-b border-hairline\"",
            "data-wire=\"collection\" title=\"sorted newest first\"",
            "data-wire=\"collection\" data-order=\"  \"",
        ] {
            assert!(
                what(&doc_verdict(&collection(silent)), "wire-order-preserved·ordering")
                    .contains("no ordering"),
                "neither a styling class, nor prose, nor a blank answer declares an ordering: {silent}"
            );
        }

        for answered in [
            "data-wire=\"collection\" data-order=\"wire\"",
            "data-wire=\"collection\" data-order=\"newest first — the ring is a stack\"",
        ] {
            assert!(
                !reds(&collection(answered)).contains(&"wire-order-preserved·ordering".to_string()),
                "a declared ordering answers the rule: {answered}"
            );
        }
    }

    // Every declaration check reads attributes through one helper, so a value
    // that merely CONTAINS an attribute is one hole in all of them at once - and
    // a doc quoting the contract it is judged by is the likeliest place for it.
    #[test]
    fn an_attribute_quoted_inside_another_value_declares_nothing() {
        let decoy_order =
            green_run().replace(Z, "data-wire=\"collection\" title='say data-order=\"wire\" here'");
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
            "<table data-item=\"capabilities\">",
            "<span title=\"oops>x</span><table data-item=\"capabilities\">",
        );
        let reported = reds(&doc);
        assert!(reported.contains(&"markup·escaped".to_string()), "{reported:?}");
        for silenced in ["item·keyboard", "item·capabilities", "item·proposals"] {
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

    // ─── the design files ───────────────────────────────────────────────────

    // The registry and the design files are disjoint sets, and retirement is
    // what keeps them so: two contracts for one component, with nothing checking
    // them against each other, is the drift this epic exists to kill (DEC-16).
    #[test]
    fn a_design_file_whose_component_has_code_must_be_retired() {
        let v = verify_component(&with_layer("x"), "Section", Some(&roster()), None);
        assert!(what(&v, "components·unretired").contains("still holds a copy"));
        assert!(what(&v, "components·unretired").contains("distil the forks"));
    }

    // The other half of DEC-16: a design file with no code is not retired, it is
    // unbuilt, and the four T3/T4 files are exactly that case.
    #[test]
    fn a_design_file_for_an_unbuilt_component_is_green() {
        let v = verify_component(&with_layer("x"), "BodyViewer", Some(&roster()), None);
        assert!(failures(&v).is_empty(), "{:?}", failures(&v));
    }

    #[test]
    fn a_design_file_without_the_registry_emitter_is_red() {
        let v = verify_component(&with_layer("x"), "Section", None, None);
        assert!(what(&v, "components·registry-unreadable").contains("cannot be answered unread"));
    }

    // W5 dissolved the specimen sheet so the docs and the app read one config. A
    // stylesheet rule for a class that config already emits re-opens the gap
    // from the other side.
    #[test]
    fn a_design_file_may_not_re_draw_a_class_the_vocabulary_owns() {
        let redrawn = with_layer("<style>.row .text-ink{color:#111;}</style>x");
        let v = verify_component(&redrawn, "BodyViewer", Some(&roster()), Some(&vocab()));
        assert!(what(&v, "components·redrawn").contains("re-draws `text-ink`"));

        let own = with_layer("<style>.row-sim .cap{color:var(--muted);}</style>x");
        let v = verify_component(&own, "BodyViewer", Some(&roster()), Some(&vocab()));
        assert!(
            failures(&v).is_empty(),
            "a doc's own chrome is not the vocabulary's: {:?}",
            failures(&v)
        );
    }

    // The folder is the artifact type: a design file declares no data-artifact,
    // so nothing but its path says what it is.
    #[test]
    fn a_component_file_is_recognised_by_its_path_and_named_by_its_stem() {
        assert_eq!(
            component_designed("docs/work/components/copy-text.html").as_deref(),
            Some("CopyText")
        );
        assert_eq!(
            component_designed("/abs/repo/docs/work/components/status-pill.html").as_deref(),
            Some("StatusPill")
        );
        assert_eq!(component_designed("docs/work/components/nested/x.html"), None);
        assert_eq!(component_designed("docs/work/2026-07-16-dflow-run-1.html"), None);
        assert_eq!(component_designed("docs/work/components/README.md"), None);
    }

    // The other half of the same question: who is in the roster. @tier is the
    // membership tag, so a collection file without one is out - which is what
    // keeps the gate's answer identical to the oracle's.
    #[test]
    fn a_collection_file_joins_the_roster_only_when_it_declares_a_tier() {
        let tiered =
            "/**\n * Button.\n * @tier T1 — primitive\n */\nexport const Button = () => null;\n";
        assert!(registry_entry("ui/src/components/Button.tsx", tiered));
        assert!(!registry_entry(
            "ui/src/components/Button.tsx",
            "export const Button = () => null;\n"
        ));
        assert!(
            !registry_entry("ui/src/components/Button.test.tsx", tiered),
            "a test is not an entry"
        );
        assert!(!registry_entry("ui/src/components/nested/Button.tsx", tiered));
        assert!(
            !registry_entry("ui/src/BodyViewer.tsx", tiered),
            "a screen is not in the collection"
        );
        assert_eq!(surface_name("ui/src/components/CopyButton.tsx").as_deref(), Some("CopyButton"));
        assert_eq!(
            surface_name("ui/src/BodyViewer.tsx").as_deref(),
            Some("BodyViewer"),
            "a screen answers to its own name too - the roster carries both"
        );
    }

    // The widened trigger, in one place: depth is not membership, an extension
    // is, and the collection/screen split decides which half of the rule applies.
    #[test]
    fn every_rendered_file_is_a_surface_and_only_those_outside_the_collection_are_screens() {
        for path in [
            "ui/src/App.tsx",
            "ui/src/components/Button.tsx",
            "ui/src/panels/Inspector.tsx",
            "ui/src/features/flows/Detail.tsx",
        ] {
            assert!(ui_surface(path), "{path} renders");
        }
        for path in [
            "ui/src/App.test.tsx",
            "ui/src/api.ts",
            "ui/src/components/index.ts",
            "ui/src/index.css",
            "ui/tailwind.config.js",
            "src/lib.rs",
        ] {
            assert!(!ui_surface(path), "{path} renders nothing");
        }
        assert!(screen_file("ui/src/panels/Inspector.tsx"), "a nested surface is still a screen");
        assert!(!screen_file("ui/src/components/Button.tsx"), "the collection is not screens");
        assert!(
            screen_file("ui/src/components/status/StatusPill.tsx"),
            "the collection's oracle reads one flat directory, so a file below it is in no registry - \
             judging it by prefix would make one folder the way to add a surface owing no design"
        );
        assert!(!screen_file("ui/src/components/index.ts"), "a barrel renders nothing");
    }

    #[test]
    fn a_pointer_interaction_with_no_keyboard_path_is_red() {
        let silent = green_run().replace(KEY, "<td> </td>");
        assert!(what(&doc_verdict(&silent), "keyboard-parity·path").contains("no keyboard path"));
        let empty = green_run()
            .replace(&format!("<tr><td>open a verdict</td><td>the row</td>{KEY}</tr>"), "");
        assert!(
            what(&doc_verdict(&empty), "keyboard-parity·path").contains("names no interaction"),
            "an empty Keyboard item is a screen nobody asked about, not a screen with no pointer"
        );
    }

    // The step's own gate: the live rulebook's hard table and this file's
    // predicate list are one list, or the printed tally is a claim nobody
    // checked. A rule added to the table without a predicate reds here first.
    #[test]
    fn every_hard_rule_in_the_live_rulebook_carries_a_predicate() {
        let live = include_str!("../../docs/design-rules/design-rules-index.html");
        let hard = rules_from_rulebook(live);
        let unchecked: Vec<&String> =
            hard.iter().filter(|s| !super::HARD_PREDICATES.contains(&s.as_str())).collect();
        assert!(unchecked.is_empty(), "hard rules with no Tier-1 predicate: {unchecked:?}");
        let orphans: Vec<&&str> =
            super::HARD_PREDICATES.iter().filter(|p| !hard.iter().any(|s| s == *p)).collect();
        assert!(orphans.is_empty(), "predicates for rules the book no longer states: {orphans:?}");
        assert_eq!(hard.len(), 12, "state-declarations retired with the Elements catalog (DEC-53)");
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

    // ─── link integrity ─────────────────────────────────────────────────────

    fn linking(hrefs: &[&str]) -> String {
        let body = hrefs.iter().fold(String::new(), |mut body, h| {
            let _ = write!(body, "<a href=\"{h}\">t</a>");
            body
        });
        with_layer(&body)
    }

    #[test]
    fn only_relative_file_targets_are_link_checked() {
        let html = linking(&[
            "sibling.html",
            "sibling.html#anchor",
            "#local-only",
            "",
            "https://example.com/x.html",
            "http://example.com/x.html",
            "mailto:someone@example.com",
            "/rooted.html",
            "../.archive/retired.html",
            ".archive/retired.html",
        ]);
        assert_eq!(link_targets(&html), vec!["sibling.html", "sibling.html"]);
    }

    #[test]
    fn any_uri_scheme_is_skipped_but_a_colon_in_a_path_is_not() {
        let html = linking(&[
            "https://example.com/x",
            "mailto:a@example.com",
            "tel:+15550100",
            "data:text/html,x",
            "javascript:void(0)",
            "notes/2026:1.html",
        ]);
        assert_eq!(link_targets(&html), vec!["notes/2026:1.html"]);
    }

    #[test]
    fn an_href_inside_a_script_or_style_body_is_data_not_a_link() {
        let html = with_layer(
            "<script>const a = '<a href=\"ghost.html\">';</script>\
             <style>/* <a href=\"phantom.html\"> */</style>",
        );
        assert!(link_targets(&html).is_empty(), "{:?}", link_targets(&html));
    }

    #[test]
    fn a_resolving_href_is_green_and_a_missing_one_names_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("there.html"), "x").expect("write");
        let doc_path = dir.path().join("doc.html");

        let mut green = Vec::new();
        check_links(&mut green, &linking(&["there.html"]), &doc_path);
        assert!(green.is_empty(), "{green:?}");

        let mut red = Vec::new();
        check_links(&mut red, &linking(&["gone.html"]), &doc_path);
        assert_eq!(red.len(), 1);
        assert_eq!(red[0].check, "link");
        assert!(red[0].what.contains("href \"gone.html\""), "{}", red[0].what);
        assert!(red[0].fix.contains("point at the file that exists"), "{}", red[0].fix);
    }

    #[test]
    fn one_dead_target_linked_twice_is_one_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut found = Vec::new();
        check_links(
            &mut found,
            &linking(&["gone.html", "gone.html#a"]),
            &dir.path().join("d.html"),
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_link_resolves_against_the_doc_s_own_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("decisions")).expect("mkdir");
        std::fs::write(dir.path().join("workflow.html"), "x").expect("write");
        let doc_path = dir.path().join("decisions/dec.html");

        let mut up = Vec::new();
        check_links(&mut up, &linking(&["../workflow.html"]), &doc_path);
        assert!(up.is_empty(), "{up:?}");

        let mut flat = Vec::new();
        check_links(&mut flat, &linking(&["workflow.html"]), &doc_path);
        assert_eq!(flat.len(), 1, "a sibling of the repo root is not a sibling of the doc");
    }

    #[test]
    fn an_ungoverned_doc_with_a_dead_link_is_red_and_still_ungoverned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let html = linking(&["gone.html"]);
        let v = with_links(verify_html(&html), &html, &dir.path().join("dec.html"));
        assert!(v.red());
        assert!(matches!(&v, Verdict::Ungoverned { .. }), "identity survives the failure");
        let said = format_verdict("dec.html", &v);
        assert!(said.contains("verify-doc red on dec.html"), "{said}");
        assert!(said.contains("gone.html"), "{said}");
    }

    #[test]
    fn a_draft_is_silent_on_its_links_and_an_archived_doc_is_not() {
        let dir = tempfile::tempdir().expect("tempdir");
        let at = dir.path().join("d.html");
        for (status, red) in [("draft", false), ("archived", true)] {
            let html = doc("wu-plan", status, "<a href=\"gone.html\">t</a>");
            let v = with_links(verify_html(&html), &html, &at);
            assert_eq!(v.red(), red, "{status} should{} red", if red { "" } else { " not" });
        }
    }

    #[test]
    fn a_doc_living_under_the_archive_is_not_link_checked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let html = linking(&["gone.html"]);
        let at = dir.path().join(".archive/old.html");
        assert!(!with_links(verify_html(&html), &html, &at).red());
    }

    #[test]
    fn a_governed_doc_reports_its_checklist_and_its_links_together() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body = "<p data-item=\"requirement\">one sentence</p>\
                    <p data-item=\"maps-to\">E-X</p>\
                    <p data-item=\"test-plan\">asserts</p><a href=\"gone.html\">t</a>";
        let html = doc("wu-plan", "", body);
        let v = with_links(verify_html(&html), &html, &dir.path().join("plan.html"));
        let said = format_verdict("plan.html", &v);
        assert!(said.contains("item·expected"), "the checklist still speaks: {said}");
        assert!(said.contains("gone.html"), "and so does the link: {said}");
    }
}
