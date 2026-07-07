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

/// Verify an SSH `SIGNATURE` blob over `data` in `namespace`, against an
/// `allowed_signers` file whose lines are `<principal> <ssh-pubkey>`. Returns
/// true only if `ssh-keygen -Y verify` accepts it for `principal`. Offline: no
/// device, no network — just the public keys and the data.
pub fn ssh_verify(
    principal: &str,
    allowed_signers: &str,
    namespace: &str,
    data: &[u8],
    sig: &str,
) -> bool {
    let Ok(dir) = tempfile::tempdir() else { return false };
    let signers = dir.path().join("allowed_signers");
    let sig_file = dir.path().join("data.sig");
    if std::fs::write(&signers, allowed_signers).is_err() || std::fs::write(&sig_file, sig).is_err()
    {
        return false;
    }
    let Ok(mut child) = Command::new("ssh-keygen")
        .args(["-Y", "verify", "-f"])
        .arg(&signers)
        .args(["-I", principal, "-n", namespace, "-s"])
        .arg(&sig_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(data);
    }
    child.wait().is_ok_and(|s| s.success())
}

/// Sign `data` in `namespace` with the SSH key at `key` (a private-key path or a
/// hardware key handle). Returns the armored `SSH SIGNATURE` blob. For a hardware
/// key this blocks on the physical touch — the step an AI agent cannot perform.
pub fn ssh_sign(key: &str, namespace: &str, data: &[u8]) -> Result<String, String> {
    let mut child = Command::new("ssh-keygen")
        .args(["-Y", "sign", "-n", namespace, "-f"])
        .arg(key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run ssh-keygen -Y sign: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data).map_err(|e| format!("failed to write signing payload: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("ssh-keygen -Y sign failed: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!("ssh-keygen -Y sign: {}", String::from_utf8_lossy(&out.stderr).trim_end()))
    }
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
    use std::process::Command;

    use super::{sha256_hex, ssh_sign, ssh_verify};

    #[test]
    fn ssh_sign_then_verify_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("id");
        assert!(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "test", "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        let pubkey = std::fs::read_to_string(dir.path().join("id.pub")).unwrap().trim().to_string();
        let signers = format!("vu {pubkey}\n");
        let data = b"sha256-of-the-change";

        let sig = ssh_sign(key.to_str().unwrap(), "trawler-mr", data).expect("signs");
        assert!(ssh_verify("vu", &signers, "trawler-mr", data, &sig), "own signature verifies");
        assert!(!ssh_verify("vu", &signers, "trawler-mr", b"other", &sig), "tampered data fails");
        assert!(ssh_sign("/no/such/key", "trawler-mr", data).is_err(), "missing key errors");
    }

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
