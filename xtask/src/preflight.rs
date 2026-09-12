use std::path::Path;

use crate::gate::{format_failures, short, Failure, Gate, Level, Report};
use crate::gate0;
use crate::git::git;

// The review's first step, and the only refusal in it that is still cheap: run
// before a finder spawns, it settles what the review is about to read - the tree
// its verdict will bind, and the notes its finders read as claims to judge. A red
// suite or a malformed note is repaired by a commit, and a commit after the
// verdict orphans it: the fleet is bought a second time for a finding it never
// saw. Nothing here judges the work and nothing here is evidence - it writes no
// note and no receipt, and the merge gate re-derives every verdict from the tree
// exactly as it did before (DEC-2).
pub fn run(
    repo: &Path,
    level: Level,
    tip_ref: &str,
    target_ref: &str,
) -> Result<(i32, String), String> {
    let mut report = Gate { repo }.preflight(level, tip_ref, target_ref)?;
    if report.exempt {
        return Ok((0, exempt(&report)));
    }
    if let Some(tree) = tree_failure(repo, &report.tip)? {
        report.failures.insert(0, tree);
    }
    if report.green() {
        Ok((0, green(&report)))
    } else {
        Ok((1, refused(&report)))
    }
}

const TREE: &str = "0·tree";

// The tree has to be the reviewed tip's tree, and not merely some tree in the
// same checkout: Gate 0 proves the working tree, while the review binds
// `base..tip`, so a green read off another commit - or off uncommitted work -
// would vouch for bytes the verdict is not about. Both print under the one check,
// because both mean the same thing: the green proves nothing about this diff.
fn tree_failure(repo: &Path, tip: &str) -> Result<Option<Failure>, String> {
    let head = git(repo, &["rev-parse", "HEAD"])?;
    if head != tip {
        return Ok(Some(tree_blocker(
            format!(
                "the checkout is on {}, not the reviewed tip {} - Gate 0 measures the working tree, not a named commit",
                short(&head),
                short(tip)
            ),
            format!("check out the reviewed tip (git checkout {tip}), then run the preflight again"),
        )));
    }
    if !git(repo, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Ok(Some(tree_blocker(
            "the working tree has uncommitted changes - the review binds a diff, and the commit that lands them is a different one".to_string(),
            "commit or stash them, then run the preflight again".to_string(),
        )));
    }
    let (code, said) = gate0::gate0(repo)?;
    Ok((code != 0).then(|| {
        tree_blocker(
            said,
            "get fmt, clippy and the suite green before the review runs - a fix that lands after the verdict orphans it and buys the fleet again".to_string(),
        )
    }))
}

fn tree_blocker(what: String, fix: String) -> Failure {
    Failure { check: TREE.to_string(), what, fix }
}

fn exempt(report: &Report) -> String {
    format!(
        "preflight {}: no review is asked for on {} - the diff asks for none, so no notes are demanded and no finder may spawn",
        report.level.as_note_level(),
        short(&report.tip)
    )
}

fn green(report: &Report) -> String {
    format!(
        "preflight {} green on {} - the tree is green and the notes the review reads are on the tip; spawn the finders",
        report.level.as_note_level(),
        short(&report.tip)
    )
}

fn refused(report: &Report) -> String {
    format!(
        "preflight {} refused on {} - the tree or the paperwork is not settled, so no finder may spawn\n{}{} blocker(s) -> fix them, then run the preflight again",
        report.level.as_note_level(),
        short(&report.tip),
        format_failures(&report.failures),
        report.failures.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{exempt, green, refused};
    use crate::gate::{Failure, Level, Report};

    fn report(level: Level, tip: &str, failures: Vec<Failure>) -> Report {
        Report {
            level,
            tip_ref: tip.to_string(),
            tip: tip.to_string(),
            base: String::new(),
            diff_sha256: String::new(),
            epic: None,
            wu: None,
            exempt: false,
            failures,
            waived: Vec::new(),
        }
    }

    fn blocker(check: &str, what: &str) -> Failure {
        Failure {
            check: check.to_string(),
            what: what.to_string(),
            fix: "do the thing".to_string(),
        }
    }

    // The refusal is the whole product of a red preflight: a session reads this
    // block and nothing else, so every blocker has to be in it, named.
    #[test]
    fn a_refusal_carries_one_block_per_blocker() {
        let text = refused(&report(
            Level::Wu,
            "abcdef0123456789",
            vec![
                blocker("0·tree", "gate0 red - fmt/clippy/tests failed; not cached"),
                blocker("2·manifest", "level working-unit needs integration >= 1 or a waiver"),
            ],
        ));

        assert!(text.contains("no finder may spawn"), "the refusal states what it stopped: {text}");
        assert!(text.contains("preflight working-unit refused on abcdef0123"), "got: {text}");
        assert_eq!(text.matches("FAIL  ").count(), 2, "one block per blocker: {text}");
        for check in ["0·tree", "2·manifest"] {
            assert!(text.contains(check), "every blocker is named, including {check}: {text}");
        }
        assert!(text.contains("2 blocker(s)"), "the count is stated: {text}");
    }

    #[test]
    fn a_green_preflight_says_what_it_proved_and_what_to_do_next() {
        let text = green(&report(Level::Wu, "abcdef0123456789", Vec::new()));
        assert!(text.contains("preflight working-unit green on abcdef0123"), "got: {text}");
        assert!(text.contains("spawn the finders"), "the next step is named: {text}");
    }

    // A diff that asks for no review is not a refusal, and demanding a note of it
    // would be the preflight inventing work the gate does not owe.
    #[test]
    fn a_diff_that_asks_for_no_review_is_reported_as_nothing_to_review() {
        let text = exempt(&report(Level::Epic, "abcdef0123456789", Vec::new()));
        assert!(text.contains("no review is asked for"), "got: {text}");
        assert!(text.contains("no notes are demanded"), "no note is owed: {text}");
        assert!(!text.contains("FAIL"), "a diff with nothing to review is never a refusal: {text}");
    }
}
