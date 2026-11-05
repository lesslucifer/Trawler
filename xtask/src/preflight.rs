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
// Gate 0 is the only part of the preflight that costs a minute; everything else
// is a read. Deferring it lets the finder stage start beside it instead of behind
// it, which is the whole saving - the round still refuses on a red tree, it just
// refuses concurrently, at the cost of the finder tokens a doomed round spent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tree {
    Measure,
    Defer,
}

pub fn run(
    repo: &Path,
    level: Level,
    tip_ref: &str,
    target_ref: &str,
    tree: Tree,
) -> Result<(i32, String), String> {
    let mut report = Gate { repo }.preflight(level, tip_ref, target_ref)?;
    if report.exempt {
        return Ok((0, exempt(&report)));
    }
    if let Some(failure) = tree_failure(repo, &report.tip, tree)? {
        report.failures.insert(0, failure);
    }
    if report.green() {
        Ok((0, green(&report, tree)))
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
fn tree_failure(repo: &Path, tip: &str, tree: Tree) -> Result<Option<Failure>, String> {
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
    if tree == Tree::Defer {
        return Ok(None);
    }
    gate0::gate0(repo).map(gate0_blocker)
}

fn gate0_blocker((code, said): (i32, String)) -> Option<Failure> {
    let fix = match code {
        0 => return None,
        gate0::MACHINE_REFUSED => "fix the machine as the refusal says, then run the preflight again - the tree was not measured",
        _ => "get fmt, clippy and the suite green before the review runs - a fix that lands after the verdict orphans it and buys the fleet again",
    };
    Some(tree_blocker(said, fix.to_string()))
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

fn green(report: &Report, tree: Tree) -> String {
    let (proved, next) = match tree {
        Tree::Measure => (
            "the tree is green and the notes the review reads are on the tip",
            "spawn the finders".to_string(),
        ),
        Tree::Defer => (
            "the checkout is the reviewed tip and the notes the review reads are on it - the tree is NOT measured here",
            format!(
                "spawn the finders and `cargo xtask gate0` together; a red Gate 0 discards the round, so nothing this fleet returns may be written to a note until it comes back green (tip {})",
                short(&report.tip)
            ),
        ),
    };
    format!(
        "preflight {} green on {} - {}; {}",
        report.level.as_note_level(),
        short(&report.tip),
        proved,
        next
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
    use super::{exempt, gate0_blocker, green, refused, Tree};
    use crate::gate::{Failure, Level, Report};
    use crate::gate0::{MACHINE_REFUSED, NEXTEST_MISSING};

    #[test]
    fn a_machine_refusal_names_the_machine_fix_not_the_tree_fix() {
        let blocker = gate0_blocker((MACHINE_REFUSED, NEXTEST_MISSING.to_string())).unwrap();
        assert_eq!(blocker.what, NEXTEST_MISSING);
        assert!(!blocker.fix.contains("fmt, clippy"), "{}", blocker.fix);
        assert!(blocker.fix.contains("machine"), "{}", blocker.fix);
    }

    #[test]
    fn a_red_tree_names_the_tree_fix_and_green_is_no_blocker() {
        let blocker = gate0_blocker((1, "gate0 red".to_string())).unwrap();
        assert!(blocker.fix.contains("fmt, clippy and the suite green"), "{}", blocker.fix);
        assert!(gate0_blocker((0, "gate0 green".to_string())).is_none());
    }

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
        let text = green(&report(Level::Wu, "abcdef0123456789", Vec::new()), Tree::Measure);
        assert!(text.contains("preflight working-unit green on abcdef0123"), "got: {text}");
        assert!(text.contains("spawn the finders"), "the next step is named: {text}");
        assert!(text.contains("the tree is green"), "a measured tree says so: {text}");
    }

    // The saving is only safe if the green says plainly that it did not measure
    // the tree: a caller who reads this as the old green would write a note for a
    // round whose suite never ran.
    #[test]
    fn a_deferred_green_refuses_to_vouch_for_a_tree_it_did_not_measure() {
        let text = green(&report(Level::Wu, "abcdef0123456789", Vec::new()), Tree::Defer);
        assert!(text.contains("the tree is NOT measured here"), "the gap is stated: {text}");
        assert!(!text.contains("the tree is green"), "it never claims the suite passed: {text}");
        assert!(text.contains("cargo xtask gate0"), "the caller is told to run it: {text}");
        assert!(text.contains("discards the round"), "a red Gate 0 costs the round: {text}");
        assert!(
            text.contains("may be written to a note"),
            "the note is what a red Gate 0 must stop: {text}"
        );
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
