pub mod debug;
pub mod doc;
pub mod gate;
pub mod git;
pub mod mr;
pub mod notes;

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
  cargo xtask mr commit -m <msg> --as <email|username>       (doc commit: write the Reviewed-By trailer)
  cargo xtask mr confirm <wu|epic> <tip> --as <email|username> (write the mr/v1 note on the tip)
  cargo xtask dod list [--epic <id>]
  cargo xtask debug serve [--port <n>]                        (workflow-notes view, default :48027)";

pub fn repo_root() -> Result<PathBuf, String> {
    git(Path::new("."), &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn run(args: &[String], repo: &Path) -> Result<(i32, String), String> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["verify-dod", rest @ ..] => verify_dod(rest, repo),
        ["verify-doc", paths @ ..] if !paths.is_empty() => doc::run_paths(repo, paths),
        ["mr", rest @ ..] => mr::run(rest, repo),
        ["dod", "list", rest @ ..] => Ok(dod_list(rest, repo)),
        ["debug", "serve", rest @ ..] => debug::serve(rest, repo),
        _ => Err(USAGE.to_string()),
    }
}

fn verify_dod(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["wu", tip, rest @ ..] => {
            let target = target_flag(rest)
                .or_else(|| infer_wu_target(tip))
                .ok_or("cannot infer the epic from the branch name; pass --target <epic-branch>")?;
            gate_run(repo, Level::Wu, tip, &target)
        }
        ["epic", tip, rest @ ..] => {
            let target = target_flag(rest).unwrap_or_else(|| "main".to_string());
            gate_run(repo, Level::Epic, tip, &target)
        }
        ["commit", sha] => {
            let parent = format!("{sha}^");
            let report = Gate { repo }.verify(Level::Commit, sha, &parent)?;
            Ok((i32::from(!report.green()), format_report(&report)))
        }
        ["hook", level] => hook(repo, level),
        ["commit-msg", file] => mr::commit_msg_hook(repo, file),
        ["range", old, new] => range(repo, old, new),
        _ => Err(USAGE.to_string()),
    }
}

fn gate_run(repo: &Path, level: Level, tip: &str, target: &str) -> Result<(i32, String), String> {
    let gate = Gate { repo };
    let report = gate.verify(level, tip, target)?;
    if report.exempt {
        return Ok((0, format_report(&report)));
    }
    if report.green() {
        gate.write_receipt(&report)?;
        Ok((0, format!("{} - dod/v1 receipt written", format_report(&report))))
    } else {
        Ok((1, format_report(&report)))
    }
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
        [tip] => gate_run(repo, lvl, tip, "HEAD"),
        _ => Ok((
            1,
            "octopus merge refused - the gate verifies one branch per merge; merge WUs one at a time"
                .to_string(),
        )),
    }
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
            let report = gate.verify(Level::Commit, commit, first)?;
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

fn infer_wu_target(tip: &str) -> Option<String> {
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
}
