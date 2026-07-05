use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
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
    Ok(git(repo, &["diff", "--name-only", &range])?.lines().map(str::to_string).collect())
}

pub fn diff_sha256(repo: &Path, base: &str, tip: &str) -> Result<String, String> {
    let range = format!("{base}..{tip}");
    let diff = Command::new("git")
        .arg("-C")
        .arg(repo)
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
