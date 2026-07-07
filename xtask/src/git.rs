use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

// Git exports GIT_DIR (and friends) to hook processes; inherited, it overrides -C and
// silently redirects every call to the enclosing repo instead of `repo`.
fn cmd(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.arg("-C").arg(repo);
    cmd
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = cmd(repo)
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim_end()))
    }
}

pub fn note(repo: &Path, note_ref: &str, tip: &str) -> Option<String> {
    let ref_arg = format!("--ref={note_ref}");
    git(repo, &["notes", &ref_arg, "show", tip]).ok()
}

pub fn write_note(repo: &Path, note_ref: &str, tip: &str, body: &str) -> Result<(), String> {
    let ref_arg = format!("--ref={note_ref}");
    git(repo, &["notes", &ref_arg, "add", "-f", "-m", body, tip]).map(|_| ())
}

pub fn diff_names(repo: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    let range = format!("{base}..{tip}");
    Ok(git(repo, &["diff", "--name-only", "--no-renames", &range])?
        .lines()
        .map(str::to_string)
        .collect())
}

pub fn diff_sha256(repo: &Path, base: &str, tip: &str) -> Result<String, String> {
    let range = format!("{base}..{tip}");
    let diff = cmd(repo)
        .args(["diff", "--no-color", "--no-ext-diff", &range])
        .output()
        .map_err(|e| format!("failed to spawn git diff: {e}"))?;
    if !diff.status.success() {
        return Err(format!(
            "git diff {range}: {}",
            String::from_utf8_lossy(&diff.stderr).trim_end()
        ));
    }
    sha256_hex(&diff.stdout)
}

// A content hash of everything a `cargo` build sees: HEAD, tracked changes
// (staged or not), and untracked non-ignored files. git stash create looked
// like the primitive for this but silently drops untracked files, so the parts
// are hashed explicitly. Over-invalidates on a stray untracked file (safe
// direction: a needless rebuild, never a false skip).
// ponytail: untracked-but-not-ignored files invalidate the cache; tighten only
// if scratch files in the tree prove to churn it noisily.
pub fn worktree_key(repo: &Path) -> Result<String, String> {
    let head = git(repo, &["rev-parse", "HEAD"])?;
    let diff = cmd(repo)
        .args(["diff", "HEAD", "--no-color", "--no-ext-diff"])
        .output()
        .map_err(|e| format!("failed to spawn git diff HEAD: {e}"))?;
    if !diff.status.success() {
        return Err(format!("git diff HEAD: {}", String::from_utf8_lossy(&diff.stderr).trim_end()));
    }
    let untracked = git(repo, &["ls-files", "--others", "--exclude-standard"])?;
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&diff.stdout);
    for path in untracked.lines() {
        bytes.extend_from_slice(path.as_bytes());
        bytes.push(0);
        if let Ok(content) = std::fs::read(repo.join(path)) {
            bytes.extend_from_slice(&content);
        }
    }
    sha256_hex(&bytes)
}

fn sha256_hex(bytes: &[u8]) -> Result<String, String> {
    let tools: [&[&str]; 2] = [&["shasum", "-a", "256"], &["sha256sum"]];
    for tool in tools {
        let Ok(mut child) = Command::new(tool[0])
            .args(&tool[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(bytes).is_err() {
                continue;
            }
        }
        let Ok(out) = child.wait_with_output() else {
            continue;
        };
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(hex) = text.split_whitespace().next() {
                return Ok(hex.to_string());
            }
        }
    }
    Err("no sha256 tool available (need shasum or sha256sum on PATH)".to_string())
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"").expect("sha tool present"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc").expect("sha tool present"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
