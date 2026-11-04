use std::path::Path;

use crate::git::{git, git_outcome, is_executable};

pub const TRACKED_DIR: &str = ".cargo-husky/hooks";
pub const INSTALL: &str = "cargo xtask hooks install";

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["install"] => install(repo),
        _ => Err(crate::USAGE.to_string()),
    }
}

pub fn install(repo: &Path) -> Result<(i32, String), String> {
    if !repo.join(TRACKED_DIR).is_dir() {
        return Ok((1, format!("hooks install refused - this tree has no {TRACKED_DIR}/")));
    }
    let stranded = stranded_worktrees(repo)?;
    if !stranded.is_empty() {
        return Ok((
            1,
            refused(
                &format!(
                    "the setting is repo-wide, and git would silently skip these worktrees' non-executable hooks:\n{}",
                    stranded.join("\n")
                ),
                &format!("make them executable in each listed worktree (or back-merge the branch that does), then re-run {INSTALL}"),
            ),
        ));
    }
    git(repo, &["config", "--local", "core.hooksPath", TRACKED_DIR])?;
    match refusal(repo)? {
        Some(refusal) => Ok((1, refusal)),
        None => Ok((
            0,
            format!("hooks installed - core.hooksPath = {TRACKED_DIR}: every worktree runs the hooks in its own checkout"),
        )),
    }
}

// A relative value is the point, not a convenience: git resolves it against each
// worktree's own root, so a worktree runs the hooks its branch carries. An
// absolute path would pin every worktree to one checkout's copy - the drift this
// replaces.
/// Why a gate must not run in this checkout, or `None`. A tree that ships no
/// tracked hooks has nothing to drift from.
pub fn refusal(repo: &Path) -> Result<Option<String>, String> {
    let dir = repo.join(TRACKED_DIR);
    if !dir.is_dir() {
        return Ok(None);
    }
    let configured = git_outcome(repo, &["config", "--get", "core.hooksPath"], &[])?;
    let configured = configured.success.then_some(configured.stdout);
    if configured.as_deref().map(|path| path.trim_end_matches('/')) != Some(TRACKED_DIR) {
        let found = configured.map_or_else(|| "unset".to_string(), |path| format!("'{path}'"));
        return Ok(Some(refused(
            &format!("core.hooksPath is {found}, so git does not run the tracked {TRACKED_DIR}/"),
            INSTALL,
        )));
    }
    let skipped = non_executable(&dir)?;
    if skipped.is_empty() {
        return Ok(None);
    }
    let paths = skipped.iter().map(|name| format!("{TRACKED_DIR}/{name}")).collect::<Vec<_>>();
    Ok(Some(refused(
        &format!("git skips a hook that is not executable: {}", paths.join(", ")),
        &format!("chmod +x {0} && git update-index --chmod=+x {0}", paths.join(" ")),
    )))
}

fn refused(why: &str, fix: &str) -> String {
    format!("hooks REFUSED - {why}\n      fix -> {fix}")
}

fn stranded_worktrees(repo: &Path) -> Result<Vec<String>, String> {
    let listing = git(repo, &["worktree", "list", "--porcelain"])?;
    let mut stranded = Vec::new();
    for checkout in listing.lines().filter_map(|line| line.strip_prefix("worktree ")) {
        let dir = Path::new(checkout).join(TRACKED_DIR);
        if !dir.is_dir() {
            continue;
        }
        let skipped = non_executable(&dir)?;
        if !skipped.is_empty() {
            stranded.push(format!("        {checkout}: {}", skipped.join(", ")));
        }
    }
    Ok(stranded)
}

fn non_executable(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let meta = entry.metadata().map_err(|e| format!("stat {}: {e}", entry.path().display()))?;
        if meta.is_file() && !(cfg!(not(unix)) || is_executable(&meta)) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::{install, refusal, INSTALL, TRACKED_DIR};
    use crate::git::git;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn repo() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        git(&path, &["init", "-q"]).unwrap();
        (dir, path)
    }

    fn tracked_hook(repo: &Path, name: &str, mode: u32) {
        let dir = repo.join(TRACKED_DIR);
        fs::create_dir_all(&dir).unwrap();
        let hook = dir.join(name);
        fs::write(&hook, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn hooks_path(repo: &Path, value: &str) {
        git(repo, &["config", "--local", "core.hooksPath", value]).unwrap();
    }

    #[test]
    fn a_tree_without_tracked_hooks_has_nothing_to_refuse() {
        let (_d, repo) = repo();
        assert_eq!(refusal(&repo).unwrap(), None);
    }

    #[test]
    fn an_unset_hooks_path_is_refused_with_the_install_line() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        let said = refusal(&repo).unwrap().expect("unset must refuse");
        assert!(said.contains(INSTALL), "{said}");
    }

    #[test]
    fn a_hooks_path_pointing_elsewhere_is_refused() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        hooks_path(&repo, ".git/hooks");
        let said = refusal(&repo).unwrap().expect("another dir must refuse");
        assert!(said.contains("'.git/hooks'"), "{said}");
        assert!(said.contains(INSTALL), "{said}");
    }

    // An absolute path to this very dir still ties every other worktree to this
    // checkout's hooks.
    #[test]
    fn an_absolute_path_to_the_tracked_dir_is_refused() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        hooks_path(&repo, repo.join(TRACKED_DIR).to_str().unwrap());
        assert!(refusal(&repo).unwrap().is_some());
    }

    #[test]
    fn the_relative_tracked_dir_passes_with_or_without_a_trailing_slash() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        hooks_path(&repo, TRACKED_DIR);
        assert_eq!(refusal(&repo).unwrap(), None);
        hooks_path(&repo, &format!("{TRACKED_DIR}/"));
        assert_eq!(refusal(&repo).unwrap(), None);
    }

    #[test]
    fn a_non_executable_hook_is_refused_by_name_with_the_chmod_fix() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        tracked_hook(&repo, "commit-msg", 0o644);
        hooks_path(&repo, TRACKED_DIR);
        let said = refusal(&repo).unwrap().expect("a 644 hook must refuse");
        assert!(said.contains(&format!("{TRACKED_DIR}/commit-msg")), "{said}");
        assert!(!said.contains("pre-commit"), "only the skipped hook is named: {said}");
        assert!(said.contains("git update-index --chmod=+x"), "{said}");
    }

    #[test]
    fn install_writes_the_relative_path_and_verifies_green() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        let (code, said) = install(&repo).unwrap();
        assert_eq!(code, 0, "{said}");
        assert_eq!(git(&repo, &["config", "--get", "core.hooksPath"]).unwrap(), TRACKED_DIR);
        assert_eq!(refusal(&repo).unwrap(), None);
    }

    #[test]
    fn install_still_refuses_a_non_executable_hook() {
        let (_d, repo) = repo();
        tracked_hook(&repo, "pre-merge-commit", 0o644);
        let (code, said) = install(&repo).unwrap();
        assert_eq!(code, 1, "{said}");
        assert!(said.contains("pre-merge-commit"), "{said}");
        assert!(
            git(&repo, &["config", "--get", "core.hooksPath"]).is_err(),
            "refused before writing"
        );
    }

    // The setting reaches every worktree at once; a sibling still on a branch
    // with a 644 hook would lose that gate without a word.
    #[test]
    fn install_refuses_while_a_linked_worktree_would_lose_a_hook() {
        let (dir, repo) = repo();
        tracked_hook(&repo, "pre-commit", 0o755);
        let linked = dir.path().join("linked");
        git(&repo, &["worktree", "add", "-q", "--orphan", "-b", "side", linked.to_str().unwrap()])
            .unwrap();
        tracked_hook(&linked, "commit-msg", 0o644);
        let (code, said) = install(&repo).unwrap();
        assert_eq!(code, 1, "{said}");
        assert!(said.contains(&format!("{}: commit-msg", linked.display())), "{said}");
        assert!(
            git(&repo, &["config", "--get", "core.hooksPath"]).is_err(),
            "refused before writing"
        );
    }

    #[test]
    fn install_refuses_a_tree_without_tracked_hooks() {
        let (_d, repo) = repo();
        let (code, _) = install(&repo).unwrap();
        assert_eq!(code, 1);
        assert!(git(&repo, &["config", "--get", "core.hooksPath"]).is_err());
    }

    #[test]
    fn every_tracked_hook_in_this_repo_is_executable() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(TRACKED_DIR);
        let hooks: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        assert!(!hooks.is_empty(), "no tracked hooks found in {}", dir.display());
        for hook in hooks {
            let mode = fs::metadata(&hook).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "{} is not executable: git skips it", hook.display());
        }
    }
}
