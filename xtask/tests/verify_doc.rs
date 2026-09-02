#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

fn doc(artifact: &str, status: &str, body: &str) -> String {
    let status_attr =
        if status.is_empty() { String::new() } else { format!(" data-status=\"{status}\"") };
    format!(
        "<!DOCTYPE html><html lang=\"en\" data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
         <main data-artifact=\"{artifact}\"{status_attr}>{body}</main>\
         <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
         </body></html>"
    )
}

fn epic_plan_items() -> &'static str {
    "<p data-item=\"intent\">why</p><p data-item=\"use-cases\">UC-1</p>\
     <ul data-item=\"expected\"><li>observable</li></ul><ul data-item=\"non-goals\"><li>out</li></ul>\
     <p data-item=\"e2e-plan\">flow</p><p data-item=\"distillation-plan\">T2 targets</p>\
     <p data-item=\"open-questions\">none</p>"
}

fn wu_block(id: &str) -> String {
    format!(
        "<div data-wu=\"{id}\"><p data-item=\"wu-summary\">builds</p>\
         <p data-item=\"wu-maps-to\">UC-1</p><p data-item=\"wu-expected\">outcome</p>\
         <p data-item=\"wu-deps\">none</p></div>"
    )
}

fn epic_plan_body() -> String {
    format!("{}<div data-item=\"wu-summaries\">{}</div>", epic_plan_items(), wu_block("E-X.W1"))
}

fn wu_plan_body() -> &'static str {
    "<p data-item=\"requirement\">one sentence</p><p data-item=\"maps-to\">E-X / UC-1</p>\
     <p data-item=\"expected\">observable</p><table data-item=\"test-plan\"><tr><td>asserts</td></tr></table>"
}

fn wu_result_body() -> &'static str {
    "<p data-item=\"outcome\">delivered</p><p data-item=\"deviations\">none</p>\
     <p data-item=\"evidence\">tests, tips</p><p data-item=\"distillation\">DEC-9</p>\
     <p data-item=\"follow-ups\">none</p>"
}

fn epic_result_body() -> &'static str {
    "<p data-item=\"outcome\">landed</p><p data-item=\"wu-rollup\">W1 done</p>\
     <p data-item=\"deviations\">none</p><p data-item=\"evidence\">e2e green</p>\
     <p data-item=\"distillation\">T2 current</p><p data-item=\"follow-ups\">none</p>"
}

fn run_verify(dir: &TempDir, files: &[(&str, &str)]) -> Output {
    let mut paths: Vec<PathBuf> = Vec::new();
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        paths.push(path);
    }
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("verify-doc")
        .args(&paths)
        .current_dir(dir.path())
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn conforming_docs_of_all_four_types_exit_zero() {
    let dir = TempDir::new().unwrap();
    let out = run_verify(
        &dir,
        &[
            ("epic-plan.html", &doc("epic-plan", "ready", &epic_plan_body())),
            ("wu-plan.html", &doc("wu-plan", "", wu_plan_body())),
            ("wu-result.html", &doc("wu-result", "", wu_result_body())),
            ("epic-result.html", &doc("epic-result", "", epic_result_body())),
        ],
    );
    assert!(out.status.success(), "expected green: {}", stdout(&out));
}

#[test]
fn epic_plan_missing_two_items_names_exactly_those() {
    let dir = TempDir::new().unwrap();
    let body = epic_plan_body()
        .replace("<p data-item=\"use-cases\">UC-1</p>", "")
        .replace("<p data-item=\"e2e-plan\">flow</p>", "");
    let out = run_verify(&dir, &[("plan.html", &doc("epic-plan", "", &body))]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("item·use-cases") && text.contains("item·e2e-plan"), "{text}");
    assert_eq!(text.matches("FAIL").count(), 2, "exactly the two missing items: {text}");
}

#[test]
fn misspelled_tag_pairs_unknown_with_missing_item() {
    let dir = TempDir::new().unwrap();
    let body = wu_result_body().replace("data-item=\"evidence\"", "data-item=\"evidnce\"");
    let out = run_verify(&dir, &[("result.html", &doc("wu-result", "", &body))]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(
        text.contains("unknown tag \"evidnce\"") && text.contains("missing item \"evidence\""),
        "{text}"
    );
}

#[test]
fn wu_block_lacking_a_sub_tag_is_red() {
    let dir = TempDir::new().unwrap();
    let body = epic_plan_body().replace("<p data-item=\"wu-deps\">none</p>", "");
    let out = run_verify(&dir, &[("plan.html", &doc("epic-plan", "", &body))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("wu·E-X.W1·wu-deps"), "{}", stdout(&out));
}

#[test]
fn combined_docs_for_a_collapsed_epic_check_both_checklists() {
    let dir = TempDir::new().unwrap();
    let combined_plan = format!("{}{}", epic_plan_body(), wu_plan_body());
    let combined_result =
        format!("{}<p data-item=\"wu-rollup\">the one WU delivered</p>", wu_result_body());
    let out = run_verify(
        &dir,
        &[
            ("plan.html", &doc("epic-plan wu-plan", "", &combined_plan)),
            ("result.html", &doc("epic-result wu-result", "", &combined_result)),
        ],
    );
    assert!(out.status.success(), "{}", stdout(&out));

    let missing = combined_plan.replace("<p data-item=\"requirement\">one sentence</p>", "");
    let out = run_verify(&dir, &[("plan2.html", &doc("epic-plan wu-plan", "", &missing))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("item·requirement"), "{}", stdout(&out));
}

#[test]
fn draft_and_ungoverned_docs_exit_zero() {
    let dir = TempDir::new().unwrap();
    let out = run_verify(
        &dir,
        &[
            ("draft.html", &doc("epic-plan", "draft", "<p>half-written</p>")),
            ("plain.html", "<html><body><p>pre-standard doc</p></body></html>"),
        ],
    );
    assert!(out.status.success(), "{}", stdout(&out));
}

// The design-run contract: a run doc is checked against the rulebook that
// actually sits on disk, so a rulebook change re-opens every receipt.
#[test]
fn a_design_run_is_receipted_against_the_rulebook_on_disk() {
    let dir = TempDir::new().unwrap();
    let rulebook = dir.path().join("docs/design-rules");
    std::fs::create_dir_all(&rulebook).unwrap();
    stub_vocabulary(&dir);
    stub_registry(&dir, &[("Section", "rest · hover")]);
    std::fs::write(
        rulebook.join("design-rules-index.html"),
        "<table data-c=\"hard-rules-table\"><tbody>\
         <tr><td><code>colour-tokens-only</code></td></tr>\
         <tr><td><code>spacing-scale</code></td></tr></tbody></table>\
         <table data-c=\"soft-rules-table\"><tbody>\
         <tr><td><code>data-ink-first</code></td></tr>\
         <tr><td><code>calm-microcopy</code></td></tr></tbody></table>",
    )
    .unwrap();

    let pin = blob_of(&rulebook.join("design-rules-index.html"));
    let receipt = |rows: &str| {
        format!("<pre data-item=\"receipt\">rulebook: {}\nsoft:\n{rows}</pre>", &pin[..7])
    };
    let run = |rows: &str| {
        format!(
            "<html data-htmldoc-id=\"665f2c1a9b04e2a91d72c6b0\"><body>\
             <script src=\"https://cdn.tailwindcss.com\"></script>\
             <script src=\"../../doc-assets/components/specimens.bundle.js\"></script>\
             <style>.m{{color:var(--ink);}}</style>\
             {}\
             <script type=\"module\" src=\"https://htmldoc.wfomobile.com/htmldoc.js\"></script>\
             </body></html>",
            doc_main(&format!(
                "<p data-item=\"frame\">d</p><p data-item=\"layout\">z</p>\
                 <div data-item=\"elements\" data-map>\
                 <pre><span data-key=\"section\">Section</span></pre>\
                 <table><tr data-key=\"section\"><td>Section</td><td>reuse</td>\
                 <td><div class=\"gist\">fits the labelled-zone use</div>\
                 <details class=\"why\"><div>@use labelled zone · @avoid nesting</div></details></td>\
                 </tr></table></div>\
                 <table data-item=\"interaction\"><tr><td>land cold</td><td>orient</td>\
                 <td><div class=\"gist\">B renders the real verdicts</div>\
                 <details class=\"why\"><div>B -&gt; status</div></details></td></tr></table>\
                 <div data-item=\"mockup\" data-wire=\"none\"><div id=\"m-zone\"></div>\
                 <script type=\"text/babel\">mount('m-zone', <Section title=\"headers\">rows</Section>)</script>\
                 <button data-state-toggle=\"m\">flip</button></div>\
                 <table data-item=\"tune\"><tr><td>weight</td><td><pre class=\"tb\">before</pre></td>\
                 <td><pre class=\"tb\">after</pre></td><td><code>data-ink-first</code></td></tr></table>\
                 {}<table data-item=\"capabilities\"><tr><td>CA install</td><td>used</td>\
                 <td>B1 button</td></tr></table>\
                 <p data-item=\"proposals\">band</p>\
                 <p data-item=\"blocked-on\">tokens</p>\
                 <table data-item=\"review\"><tr><td>copy icon only</td><td>elements</td>\
                 <td><code>CopyButton</code> @avoid</td></tr></table>",
                receipt(rows)
            ))
        )
    };

    let out = run_verify(
        &dir,
        &[(
            "run.html",
            &run("  data-ink-first: satisfied — no card frames\n  calm-microcopy: inherited — lowercase\n"),
        )],
    );
    let text = stdout(&out);
    assert!(out.status.success(), "expected green: {text}");
    assert!(text.contains("receipt 2/2 soft · 1 satisfied · 1 inherited"), "{text}");
    assert!(text.contains("hard rules: 2/2 machine-checked (design)"), "{text}");
    assert!(text.contains("design-allow: 0 live suppression"), "{text}");

    let out = run_verify(
        &dir,
        &[(
            "bad.html",
            &run("  data-ink-first: definitely-fine — x\n  spacing-scale: satisfied — y\n"),
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("receipt·calm-microcopy"), "uncovered soft rule: {text}");
    assert!(text.contains("not one of the seven"), "bad verdict: {text}");
    assert!(text.contains("machine-checked, not receipted"), "hard rule row: {text}");

    // The rulebook moves under a run that already receipted it - same slugs, so
    // coverage still passes and only the pin can tell that every verdict above it
    // was judged against a rulebook that no longer exists.
    let rules_file = rulebook.join("design-rules-index.html");
    let moved =
        format!("<!-- W6 revised the wording -->{}", std::fs::read_to_string(&rules_file).unwrap());
    std::fs::write(&rules_file, &moved).unwrap();
    let out = run_verify(
        &dir,
        &[(
            "stale.html",
            &run("  data-ink-first: satisfied — no card frames\n  calm-microcopy: inherited — lowercase\n"),
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("receipt·rulebook-stale"), "{text}");
    assert!(text.contains(&pin[..7]) && text.contains(&blob_of(&rules_file)[..7]), "{text}");
}

// The pin the receipt must carry, computed the way the flow spec tells a run to
// compute it: `git hash-object <the rulebook>`.
fn blob_of(path: &std::path::Path) -> String {
    let out = Command::new("git").arg("hash-object").arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// The five vocabulary rules read ui/tailwind.config.js through a node emitter.
// A temp repo has no ui/, so it gets the emitter's shape rather than its
// content - what these tests assert is the doc contract, not the palette.
fn stub_vocabulary(dir: &TempDir) {
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(
        scripts.join("design-vocabulary.mjs"),
        "process.stdout.write(JSON.stringify({legal:[\"text-ink\"],         rules:{\"colour-tokens-only\":[\"text-\"]},values:{},modifiers:{},chromatic:[\"ok\"],chroma_floor:40}));",
    )
    .unwrap();
}

// The real oracle over a collection written into the temp tree, not a stub that
// prints an answer: the bridge under test is node reading a TSDoc block, so a
// canned JSON would assert only that serde works.
fn stub_registry(dir: &TempDir, components: &[(&str, &str)]) {
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/design-registry.mjs"),
        scripts.join("design-registry.mjs"),
    )
    .unwrap();
    let collection = dir.path().join("ui/src/components");
    std::fs::create_dir_all(&collection).unwrap();
    std::fs::create_dir_all(dir.path().join("docs/work/components")).unwrap();
    for (name, states) in components {
        std::fs::write(
            collection.join(format!("{name}.tsx")),
            format!(
                "/**\n * {name} does one thing.\n * @tier T1 — test entry\n * @use anywhere\n\
                 \x20* @avoid nowhere\n * @states {states}\n * @madeof nothing\n */\n\
                 export const {name} = () => null;\n"
            ),
        )
        .unwrap();
    }
}

// A vocabulary that cannot be read is the same shape of hole as an unreadable
// rulebook: five rules that report nothing look exactly like five rules with
// nothing to say.
#[test]
fn a_design_run_without_the_vocabulary_emitter_is_red() {
    let dir = TempDir::new().unwrap();
    let out = run_verify(&dir, &[("run.html", &doc_main("<p data-item=\"frame\">d</p>"))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("vocabulary·unreadable"), "{}", stdout(&out));
}

fn doc_main(body: &str) -> String {
    format!("<main data-artifact=\"design-run\">{body}</main>")
}

#[test]
fn multi_path_output_reports_only_red_docs() {
    let dir = TempDir::new().unwrap();
    let out = run_verify(
        &dir,
        &[
            ("green.html", &doc("wu-result", "", wu_result_body())),
            ("red.html", &doc("wu-result", "", "<p data-item=\"outcome\">only this</p>")),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("red.html") && !text.contains("green.html"), "{text}");
}
