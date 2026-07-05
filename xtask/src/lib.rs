pub mod gate;
pub mod git;
pub mod notes;

use std::path::{Path, PathBuf};

use gate::{format_report, Gate, Level};
use git::{diff_names, git, note};
use notes::DodReceipt;

pub const USAGE: &str = "\
usage:
  cargo xtask verify-dod wu <tip-ref> [--target <epic-branch>]
  cargo xtask verify-dod epic <tip-ref> [--target <ref>]      (target defaults to main)
  cargo xtask verify-dod commit <sha>
  cargo xtask verify-dod hook <wu|epic>                       (tip = MERGE_HEAD, target = HEAD)
  cargo xtask verify-dod range <old> <new>
  cargo xtask dod list [--epic <id>]";

pub fn repo_root() -> Result<PathBuf, String> {
    git(Path::new("."), &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn run(args: &[String], repo: &Path) -> Result<(i32, String), String> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["verify-dod", rest @ ..] => verify_dod(rest, repo),
        ["dod", "list", rest @ ..] => dod_list(rest, repo),
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
        ["range", old, new] => range(repo, old, new),
        _ => Err(USAGE.to_string()),
    }
}

fn gate_run(repo: &Path, level: Level, tip: &str, target: &str) -> Result<(i32, String), String> {
    let gate = Gate { repo };
    let report = gate.verify(level, tip, target)?;
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
    let Ok(tip) = git(repo, &["rev-parse", "MERGE_HEAD"]) else {
        return Ok((0, "no MERGE_HEAD - not a merge, gate not required".to_string()));
    };
    let base = git(repo, &["merge-base", "HEAD", &tip])?;
    let names = diff_names(repo, &base, &tip)?;
    if !names.iter().any(|p| notes::is_code(p)) {
        return Ok((0, "docs/infra-only merge - gate not required".to_string()));
    }
    gate_run(repo, lvl, &tip, "HEAD")
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

fn dod_list(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    use std::fmt::Write as _;
    let filter = target_value(args, "--epic");
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
        Ok((0, "no dod/v1 receipts found".to_string()))
    } else {
        Ok((0, out.trim_end().to_string()))
    }
}

fn target_flag(rest: &[&str]) -> Option<String> {
    target_value(rest, "--target")
}

fn target_value(rest: &[&str], flag: &str) -> Option<String> {
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
    use super::{infer_wu_target, target_value};

    #[test]
    fn wu_branch_names_infer_their_epic() {
        assert_eq!(infer_wu_target("wu/capture/pagination").as_deref(), Some("epic/capture"));
        assert_eq!(infer_wu_target("wu/workflow/gate").as_deref(), Some("epic/workflow"));
        assert_eq!(infer_wu_target("feature/x"), None);
        assert_eq!(infer_wu_target("1a2b3c"), None);
    }

    #[test]
    fn flag_values_parse_positionally() {
        assert_eq!(target_value(&["--target", "epic/x"], "--target").as_deref(), Some("epic/x"));
        assert_eq!(target_value(&["--target"], "--target"), None);
        assert_eq!(target_value(&[], "--target"), None);
    }
}
