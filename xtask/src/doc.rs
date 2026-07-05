use std::fmt::Write as _;
use std::path::Path;

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

fn checklist(artifact: &str) -> Option<&'static [&'static str]> {
    match artifact {
        "epic-plan" => Some(EPIC_PLAN),
        "wu-plan" => Some(WU_PLAN),
        "wu-result" => Some(WU_RESULT),
        "epic-result" => Some(EPIC_RESULT),
        _ => None,
    }
}

pub enum Verdict {
    Ungoverned,
    Draft,
    Checked { artifacts: Vec<String>, failures: Vec<Failure> },
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
    let Some((artifact_pos, artifact_decl)) =
        attr_positions(html, "data-artifact").into_iter().next()
    else {
        return Verdict::Ungoverned;
    };
    if draft_declared(html, artifact_pos) {
        return Verdict::Draft;
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
        return Verdict::Checked { artifacts, failures };
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

    Verdict::Checked { artifacts, failures }
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
// declaration anywhere else must not silence the checks
fn draft_declared(html: &str, artifact_pos: usize) -> bool {
    let start = html[..artifact_pos].rfind('<').unwrap_or(0);
    let end = html[start..].find('>').map_or(html.len(), |i| start + i);
    attr_positions(&html[start..end], "data-status").first().is_some_and(|(_, v)| v == "draft")
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
    let mut out = String::new();
    let mut any_red = false;
    for path in paths {
        let resolved =
            if Path::new(path).exists() { Path::new(path).to_path_buf() } else { repo.join(path) };
        let html =
            std::fs::read_to_string(&resolved).map_err(|e| format!("cannot read {path}: {e}"))?;
        let verdict = verify_html(&html);
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
        Verdict::Draft => format!("verify-doc draft on {path} - checks silent until ready"),
        Verdict::Checked { artifacts, failures } if failures.is_empty() => {
            format!("verify-doc green on {path} ({})", artifacts.join(" "))
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
    use super::{attr_positions, edit_distance, verify_html, Verdict};

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
        assert!(matches!(verify_html(&doc("wu-result", "draft", "")), Verdict::Draft));
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
