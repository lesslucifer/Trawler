pub mod debug;
pub mod doc;
pub mod gate;
pub mod gate0;
pub mod git;
pub mod mr;
pub mod notes;
pub mod notes_write;
pub mod preflight;
pub mod tally;
pub mod ui;
pub mod wizard;

use std::path::{Path, PathBuf};

use gate::{format_report, Gate, Level};
use git::{git, note};
use notes::DodReceipt;

pub const USAGE: &str = "\
usage:
  cargo xtask verify-dod wu <tip-ref> [--target <epic-branch>]
  cargo xtask verify-dod epic <tip-ref> [--target <ref>]      (target defaults to main)
  cargo xtask verify-dod commit <sha>
  cargo xtask verify-dod hook <wu|epic>                       (tip = MERGE_HEAD, target = HEAD)
  cargo xtask verify-dod commit-msg <msg-file>                (commit-msg hook: docs need an mr trailer)
  cargo xtask verify-dod range <old> <new>
  cargo xtask verify-doc <path> [<path>...]
  cargo xtask design-scope <base> <head>                     (what the design step must account for: the artifacts, the new surfaces, the files it renders)
  cargo xtask preflight <wu|epic> <tip> [--target <ref>] [--defer-gate0]  (the review's first step: gate0 + the notes it reads, before any finder spawns; --defer-gate0 skips the suite so it can run beside the finders)
  cargo xtask notes write <wu|epic|commit> <tip> [--target <ref>] [--judgment <path>|--template]  (write the tip's test/v2 + doc/v1 from one judgment)
  cargo xtask tally [--fleet]                                (local gate tally: refused check ids per level, review rounds per tip; --fleet: what each round's fleet cost, from the cr notes' fleet: blocks)
  cargo xtask gate0                                          (fmt+clippy+tests, skipped when the tree is unchanged since last green)
  cargo xtask mr setup [--as <id>] [--key <path>]            (wizard: pick + register an auth method; args skip prompts)
  cargo xtask mr commit [-m <msg>] [--as <id>]               (doc commit: Reviewed-By trailer; signs if <id> is a key)
  cargo xtask mr confirm [wu|epic] [<tip>] [--as <id>] [--target <ref>] [--report <path>] (mr/v1 note attesting the result report; level/tip/report inferred, signs if key)
  cargo xtask dod list [--epic <id>]
  cargo xtask ui build                                        (pnpm install + build → ui/dist/)
  cargo xtask ui dev                                          (pnpm install + launch the Tauri window)
  cargo xtask ui bench                                        (pnpm install + run the perf benchmarks)
  cargo xtask ui e2e                                          (build e2e app + drive the real window via WebdriverIO)
  cargo xtask ui specimens [--check]                          (compile the collection for design files; --check renders each over file://)
  cargo xtask debug serve [--port <n>]                        (workflow-notes view, default :48027)";

pub fn repo_root() -> Result<PathBuf, String> {
    git(Path::new("."), &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn run(args: &[String], repo: &Path) -> Result<(i32, String), String> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["verify-dod", rest @ ..] => verify_dod(rest, repo),
        ["verify-doc", paths @ ..] if !paths.is_empty() => doc::run_paths(repo, paths),
        ["design-scope", base, head] => design_scope(repo, base, head),
        ["preflight", rest @ ..] => preflight_cmd(rest, repo),
        ["notes", "write", rest @ ..] => notes_write::run(rest, repo),
        ["tally", rest @ ..] => tally::run(rest, repo),
        ["gate0"] => gate0::gate0(repo),
        ["mr", rest @ ..] => mr::run(rest, repo),
        ["dod", "list", rest @ ..] => Ok(dod_list(rest, repo)),
        ["ui", rest @ ..] => ui::run(rest, repo),
        ["debug", "serve", rest @ ..] => debug::serve(rest, repo),
        _ => Err(USAGE.to_string()),
    }
}

fn verify_dod(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["wu", tip, rest @ ..] => {
            gate_run(repo, Level::Wu, tip, &scope_target(Level::Wu, tip, rest)?)
        }
        ["epic", tip, rest @ ..] => {
            let target = scope_target(Level::Epic, tip, rest)?;
            gate_run(repo, Level::Epic, tip, &target)
        }
        ["commit", sha] => {
            let parent = scope_target(Level::Commit, sha, &[])?;
            let gate = Gate { repo };
            let mut report = gate.verify(Level::Commit, sha, &parent)?;
            let (tip, base) = (report.tip.clone(), report.base.clone());
            gate.check_commit_trailer(&mut report, &tip, &base)?;
            record(repo, &report);
            Ok((i32::from(!report.green()), format_report(&report)))
        }
        ["hook", level] => hook(repo, level),
        ["commit-msg", file] => mr::commit_msg_hook(repo, file),
        ["range", old, new] => range(repo, old, new),
        _ => Err(USAGE.to_string()),
    }
}

// The review's two scopes, resolved by the same rule and the same error line as
// `verify-dod wu|epic`: the preflight must pin the base the merge gate will pin,
// or it would judge a different range than the note it protects. A standalone
// commit carries no review, so it has no preflight either (SKILL.md §2).
fn preflight_cmd(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let (args, tree) = split_tree_flag(args);
    match args.as_slice() {
        ["wu", tip, rest @ ..] => {
            preflight::run(repo, Level::Wu, tip, &scope_target(Level::Wu, tip, rest)?, tree)
        }
        ["epic", tip, rest @ ..] => {
            let target = scope_target(Level::Epic, tip, rest)?;
            preflight::run(repo, Level::Epic, tip, &target, tree)
        }
        _ => Err(USAGE.to_string()),
    }
}

fn split_tree_flag<'a>(args: &[&'a str]) -> (Vec<&'a str>, preflight::Tree) {
    let deferred = args.contains(&"--defer-gate0");
    let kept = args.iter().copied().filter(|a| *a != "--defer-gate0").collect();
    (kept, if deferred { preflight::Tree::Defer } else { preflight::Tree::Measure })
}

// The ref each scope's range is measured against: a WU's epic (named, or inferred
// from the branch), an epic's main, a standalone commit's parent. One resolver for
// the commands that judge a range - the gate, the preflight (DEC-43) and the note
// writer (DEC-44) - so none of them can judge one range and write about another.
pub(crate) fn scope_target(level: Level, tip_ref: &str, rest: &[&str]) -> Result<String, String> {
    match level {
        Level::Commit => Ok(format!("{tip_ref}^")),
        Level::Wu => wu_scope(tip_ref, rest),
        Level::Epic => Ok(target_flag(rest).unwrap_or_else(|| "main".to_string())),
    }
}

// The epic a WU branch belongs to: named by the caller, or inferred from the
// branch. One helper because two commands now resolve it, and a disagreement
// here would gate one range and preflight another.
pub(crate) fn wu_scope(tip: &str, rest: &[&str]) -> Result<String, String> {
    target_flag(rest).or_else(|| infer_wu_target(tip)).ok_or_else(|| {
        "cannot infer the epic from the branch name; pass --target <epic-branch>".to_string()
    })
}

// The review and the merge gate must not each decide for themselves what a
// design artifact is - the same function answers both, and this is where a
// reviewer reads it. Merge-base, matching the scope the review pins.
fn design_scope(repo: &Path, base: &str, head: &str) -> Result<(i32, String), String> {
    use std::fmt::Write as _;
    let tip = git(repo, &["rev-parse", head])?;
    let merge_base = git(repo, &["merge-base", base, &tip])?;
    let names = git::diff_names(repo, &merge_base, &tip)?;
    let gate = Gate { repo };
    let retired = gate.retired_designs(&merge_base, &tip, &names);
    let artifacts = gate.design_artifacts_with(&tip, &names, &retired);
    let (added, owed) = gate.surfaces(&tip, &merge_base);
    if artifacts.is_empty() && owed.is_empty() {
        return Ok((
            0,
            format!(
                "no design artifact in {}..{} - the design dimension does not run, and the {} note carries no design section",
                gate::short(&merge_base),
                gate::short(&tip),
                gate::design_carrier_note(repo, &tip)
            ),
        ));
    }
    let mut out = String::new();
    if !artifacts.is_empty() {
        let _ = writeln!(
            out,
            "{} design artifact(s) to judge:\n{}",
            artifacts.len(),
            artifacts.join("\n")
        );
        if !retired.is_empty() {
            let _ = writeln!(
                out,
                "{} of them left the tree in this range at bytes the base never held - read the last state the design step answers for:\n{}",
                retired.len(),
                retired
                    .iter()
                    .map(|d| format!("git show {}:{}", gate::short(&d.commit), d.path))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
    // Printed as their own blocks so the artifact list a reviewer reads stays the
    // artifact list: these are not judged, they are files whose note owes an
    // answer. The two are separated because the answers differ - a new surface
    // takes a citation and nothing else.
    if !added.is_empty() {
        let _ = writeln!(
            out,
            "{} new surface(s) - the {} note must cite the design each was built from (design.implements: [{{path, blob, built}}]):\n{}",
            added.len(),
            gate::design_carrier_note(repo, &tip),
            added.join("\n")
        );
    }
    let touched: Vec<&String> = owed.iter().filter(|path| !added.contains(path)).collect();
    if !touched.is_empty() {
        let _ = writeln!(
            out,
            "{} rendered file(s) touched - each is cited the same way or declared (design.undesigned: [{{path, why}}]):\n{}",
            touched.len(),
            touched.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("\n")
        );
    }
    Ok((0, out.trim_end().to_string()))
}

fn gate_run(repo: &Path, level: Level, tip: &str, target: &str) -> Result<(i32, String), String> {
    let gate = Gate { repo };
    let report = gate.verify(level, tip, target)?;
    if report.exempt {
        record(repo, &report);
        return Ok((0, format_report(&report)));
    }
    if report.green() {
        gate.write_receipt(&report)?;
        record(repo, &report);
        Ok((0, format!("{} - dod/v1 receipt written", format_report(&report))))
    } else {
        record(repo, &report);
        Ok((1, format_report(&report)))
    }
}

// The tally is a measurement taken on the way past, never evidence: the verdict
// is already decided (and, on green, already receipted), and a checkout where
// the file cannot be written - a read-only tree - gates exactly as it did
// before. No check reads it back.
fn record(repo: &Path, report: &gate::Report) {
    let _ = tally::write(repo, report);
}

fn hook(repo: &Path, level: &str) -> Result<(i32, String), String> {
    let lvl = match level {
        "wu" => Level::Wu,
        "epic" => Level::Epic,
        _ => return Err(USAGE.to_string()),
    };
    let heads = merge_tips(repo)?;
    match heads.as_slice() {
        [] => Ok((0, "no merge in progress - gate not required".to_string())),
        // A back-merge of already-landed history (main, or a sibling already on
        // main) into an epic carries no net-new WU seam - its commits passed their
        // own gate before landing, and its tip is a merge commit with no WU notes.
        // Skip it, exactly as the epic walk's is_landed_history does (DEC-8); a
        // genuine forward WU tip carries work absent from main, so this never
        // swallows it. The final epic->main gate re-walks every real seam.
        [tip] if lvl == Level::Wu && is_ancestor(repo, tip, "main") => Ok((
            0,
            format!(
                "back-merge of landed history {} - not a WU seam, gate not required",
                gate::short(tip)
            ),
        )),
        [tip] => gate_run(repo, lvl, tip, "HEAD"),
        _ => Ok((
            1,
            "octopus merge refused - the gate verifies one branch per merge; merge WUs one at a time"
                .to_string(),
        )),
    }
}

fn is_ancestor(repo: &Path, commit: &str, of: &str) -> bool {
    git(repo, &["merge-base", "--is-ancestor", commit, of]).is_ok()
}

fn merge_tips(repo: &Path) -> Result<Vec<String>, String> {
    let merge_head_file = git(repo, &["rev-parse", "--git-path", "MERGE_HEAD"])?;
    if let Ok(content) = std::fs::read_to_string(repo.join(&merge_head_file)) {
        let heads: Vec<String> =
            content.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect();
        if !heads.is_empty() {
            return Ok(heads);
        }
    }
    // pre-merge-commit fires before git materializes MERGE_HEAD for automatic
    // (ort) merges - at that point the merged revs are only visible in the
    // reflog action. MERGE_HEAD exists for conflicted/--no-commit merges,
    // whose completion the pre-commit hook gates through the same path.
    let action = std::env::var("GIT_REFLOG_ACTION").unwrap_or_default();
    tips_from_reflog(&action)
        .into_iter()
        .map(|rev| git(repo, &["rev-parse", &format!("{rev}^{{commit}}")]))
        .collect()
}

fn tips_from_reflog(action: &str) -> Vec<&str> {
    action.strip_prefix("merge ").map_or_else(Vec::new, |rest| rest.split_whitespace().collect())
}

fn range(repo: &Path, old: &str, new: &str) -> Result<(i32, String), String> {
    use std::fmt::Write as _;
    let span = format!("{old}..{new}");
    let lines = git(repo, &["rev-list", "--first-parent", "--parents", &span])?;
    let gate = Gate { repo };
    let mut out = String::new();
    for line in lines.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let [commit, first] = parts.as_slice() {
            let mut report = gate.verify(Level::Commit, commit, first)?;
            gate.check_commit_trailer(&mut report, commit, first)?;
            if !report.green() {
                let _ = writeln!(out, "standalone commit {}:", gate::short(commit));
                let _ = writeln!(out, "{}", format_report(&report));
            }
        }
    }
    if out.is_empty() {
        Ok((0, format!("range {span} clean")))
    } else {
        Ok((1, out.trim_end().to_string()))
    }
}

fn dod_list(args: &[&str], repo: &Path) -> (i32, String) {
    use std::fmt::Write as _;
    let filter = flag_value(args, "--epic");
    let listing = git(repo, &["notes", "--ref=dod", "list"]).unwrap_or_default();
    let mut out = String::new();
    for line in listing.lines() {
        let Some(commit) = line.split_whitespace().nth(1) else { continue };
        let Some(raw) = note(repo, "dod", commit) else { continue };
        let Ok(receipt) = serde_yaml::from_str::<DodReceipt>(&raw) else { continue };
        if let Some(epic) = &filter {
            if receipt.epic.as_deref() != Some(epic.as_str()) {
                continue;
            }
        }
        let _ = writeln!(
            out,
            "{:<6} {:<8} {:<13} {}  {}  {}",
            receipt.epic.as_deref().unwrap_or("-"),
            receipt.wu.as_deref().unwrap_or("-"),
            receipt.level,
            gate::short(&receipt.tip),
            receipt.date.as_deref().unwrap_or("-"),
            receipt.branch.as_deref().unwrap_or("-"),
        );
    }
    if out.is_empty() {
        (0, "no dod/v1 receipts found".to_string())
    } else {
        (0, out.trim_end().to_string())
    }
}

fn target_flag(rest: &[&str]) -> Option<String> {
    flag_value(rest, "--target")
}

pub(crate) fn flag_value(rest: &[&str], flag: &str) -> Option<String> {
    rest.iter().position(|a| *a == flag).and_then(|i| rest.get(i + 1)).map(|s| (*s).to_string())
}

pub(crate) fn infer_wu_target(tip: &str) -> Option<String> {
    let epic = tip.strip_prefix("wu/")?.split('/').next()?;
    if epic.is_empty() {
        None
    } else {
        Some(format!("epic/{epic}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{flag_value, infer_wu_target};

    // `design-scope` tells a reader which note to write the verdict to. It said
    // `cr/v3` for a whole epic after DEC-46 moved the verdict to `design/v1`,
    // because the sentence restated the rule instead of asking for it.
    #[test]
    fn design_scope_sends_a_reader_to_the_note_the_gate_reads() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| crate::git::git(repo, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "base"]);
        g(&["branch", "epic/x"]);

        let empty = super::design_scope(repo, "epic/x", "HEAD").unwrap().1;
        let surfaced = {
            std::fs::create_dir_all(repo.join("ui/src")).unwrap();
            std::fs::write(repo.join("ui/src/Thing.tsx"), "export const Thing = () => null;\n")
                .unwrap();
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", "a new surface"]);
            super::design_scope(repo, "epic/x", "HEAD").unwrap().1
        };
        assert!(surfaced.contains("new surface(s)"), "{surfaced}");

        let tip = g(&["rev-parse", "HEAD"]);
        let carrier = crate::gate::design_carrier_note(repo, &tip);
        for text in [&empty, &surfaced] {
            assert!(text.contains(carrier), "sends a reader nowhere: {text}");
            for stale in crate::notes::CR_SCHEMAS.iter().filter(|s| **s != carrier) {
                assert!(!text.contains(*stale), "names {stale}, not {carrier}: {text}");
            }
            assert!(
                !text.contains("rulebook"),
                "nothing pins the rulebook any more, so nothing asks for one (DEC-53): {text}"
            );
        }
    }

    // The usage line is the command's contract as a reader meets it, before
    // running anything. It went on promising the pin for a whole Working Unit
    // after DEC-53 removed it from the output, so the one place a reader is told
    // what to expect was the one place nothing checked.
    #[test]
    fn the_usage_line_promises_only_what_design_scope_prints() {
        let line = super::USAGE
            .lines()
            .find(|line| line.contains("design-scope"))
            .expect("design-scope is named in the usage");
        assert!(
            !line.contains("rulebook"),
            "the usage promises a rulebook pin design-scope no longer writes: {line}"
        );
    }

    // The artifact block, with the pin gone from it: the reviewer is told what to
    // judge and where the verdict goes, and nothing else.
    #[test]
    fn design_scope_lists_the_artifacts_and_pins_no_rulebook() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| crate::git::git(repo, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "base"]);
        g(&["branch", "epic/x"]);

        let design = "docs/work/components/status-pill.html";
        std::fs::create_dir_all(repo.join("docs/work/components")).unwrap();
        std::fs::write(repo.join(design), "<html><body>one verdict</body></html>\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "design the pill"]);

        let printed = super::design_scope(repo, "epic/x", "HEAD").unwrap().1;
        assert!(printed.contains("1 design artifact(s) to judge:"), "{printed}");
        assert!(printed.contains(design), "{printed}");
        assert!(!printed.contains("rulebook"), "{printed}");
    }

    #[test]
    fn wu_branch_names_infer_their_epic() {
        assert_eq!(infer_wu_target("wu/capture/pagination").as_deref(), Some("epic/capture"));
        assert_eq!(infer_wu_target("wu/workflow/gate").as_deref(), Some("epic/workflow"));
        assert_eq!(infer_wu_target("feature/x"), None);
        assert_eq!(infer_wu_target("1a2b3c"), None);
    }

    #[test]
    fn reflog_action_yields_merge_tips() {
        assert_eq!(super::tips_from_reflog("merge wu/x/one"), vec!["wu/x/one"]);
        assert_eq!(
            super::tips_from_reflog("merge wu/x/one wu/x/two"),
            vec!["wu/x/one", "wu/x/two"]
        );
        assert!(super::tips_from_reflog("pull origin main").is_empty());
        assert!(super::tips_from_reflog("").is_empty());
    }

    #[test]
    fn flag_values_parse_positionally() {
        assert_eq!(flag_value(&["--target", "epic/x"], "--target").as_deref(), Some("epic/x"));
        assert_eq!(flag_value(&["--target"], "--target"), None);
        assert_eq!(flag_value(&[], "--target"), None);
    }

    // The WU hook must skip a back-merge of main into an epic (main is landed,
    // gated history — not a net-new WU seam, DEC-8) while still gating a forward WU
    // tip carrying epic-only work. is_ancestor(tip, main) is the discriminator.
    #[test]
    fn wu_hook_skips_back_merge_of_landed_history_but_gates_forward_work() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path();
        let g = |args: &[&str]| super::git(repo, args).unwrap();
        let commit = |name: &str, msg: &str| {
            std::fs::write(repo.join(name), msg).unwrap();
            g(&["add", "-A"]);
            g(&["commit", "-q", "-m", msg]);
            g(&["rev-parse", "HEAD"])
        };

        g(&["init", "-q", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "commit.gpgsign", "false"]);
        commit("base", "base");
        g(&["branch", "epic"]);

        let main_tip = commit("m", "main advances past the fork");

        g(&["checkout", "-q", "epic"]);
        let forward_wu_tip = commit("w", "epic-only WU work, absent from main");

        assert!(
            super::is_ancestor(repo, &main_tip, "main"),
            "main's own tip is landed history — the WU hook skips a back-merge of it"
        );
        assert!(
            !super::is_ancestor(repo, &forward_wu_tip, "main"),
            "a forward WU tip carries work not on main — the WU hook still gates it"
        );

        // Drive hook() itself, not just the discriminator: a MERGE_HEAD pointing at
        // main (a back-merge) is skipped clean at the WU level, proving the
        // lvl == Level::Wu arm fires end to end.
        let git_dir = super::git(repo, &["rev-parse", "--git-dir"]).unwrap();
        std::fs::write(repo.join(&git_dir).join("MERGE_HEAD"), format!("{main_tip}\n")).unwrap();
        let (code, msg) = super::hook(repo, "wu").unwrap();
        assert_eq!(code, 0, "back-merge of landed history must not fail the WU hook");
        assert!(msg.contains("back-merge of landed history"), "got: {msg}");
    }
}
