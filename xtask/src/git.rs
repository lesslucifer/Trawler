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
    git_env(repo, args, &[])
}

/// [`git`] with extra environment variables on the spawned process — how a
/// sign reason reaches a `gpg.ssh.program` signer through `git commit -S`.
pub fn git_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let mut command = cmd(repo);
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command
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

pub fn added_names(repo: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    let range = format!("{base}..{tip}");
    Ok(git(repo, &["diff", "--name-only", "--no-renames", "--diff-filter=A", &range])?
        .lines()
        .map(str::to_string)
        .collect())
}

// Every path the range ever added under `prefix` - the commits, not the two
// endpoints. A file added by one commit and removed by another nets out of an
// endpoint diff, which is exactly how a design nobody built leaves no trace at
// an epic tip.
pub fn added_under(repo: &Path, base: &str, tip: &str, prefix: &str) -> Vec<String> {
    let range = format!("{base}..{tip}");
    let mut paths: Vec<String> = git(
        repo,
        &[
            "log",
            "--diff-filter=A",
            "--name-only",
            "--pretty=format:",
            "--no-renames",
            &range,
            "--",
            prefix,
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(str::to_string)
    .collect();
    paths.sort();
    paths.dedup();
    paths
}

pub fn blob_of(repo: &Path, rev: &str, path: &str) -> Option<String> {
    git(repo, &["rev-parse", &format!("{rev}:{path}")]).ok()
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
/// hardware key handle), running `program` — `ssh-keygen`, or a `gpg.ssh.program`
/// stand-in such as the bundled Secure-Enclave signer. Returns the armored
/// `SSH SIGNATURE` blob. For a hardware key this blocks on the physical touch —
/// the step an AI agent cannot perform. `reason` rides to the signer as
/// `TRAWLER_SIGN_REASON`, naming the authorised action on the auth sheet.
pub fn ssh_sign(
    program: &str,
    key: &str,
    namespace: &str,
    data: &[u8],
    reason: Option<&str>,
) -> Result<String, String> {
    let mut command = Command::new(program);
    if let Some(reason) = reason {
        command.env("TRAWLER_SIGN_REASON", reason);
    }
    let mut child = command
        .args(["-Y", "sign", "-n", namespace, "-f"])
        .arg(key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run {program} -Y sign: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data).map_err(|e| format!("failed to write signing payload: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("{program} -Y sign failed: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!("{program} -Y sign: {}", String::from_utf8_lossy(&out.stderr).trim_end()))
    }
}

/// The blob bytes of `path` at `rev`, exact — no trailing-whitespace trim, so a
/// hash over them matches a hash over the working-tree file byte for byte.
pub fn show_bytes(repo: &Path, rev: &str, path: &str) -> Result<Vec<u8>, String> {
    let spec = format!("{rev}:{path}");
    let out = cmd(repo)
        .args(["show", &spec])
        .output()
        .map_err(|e| format!("failed to spawn git show: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("git show {spec}: {}", String::from_utf8_lossy(&out.stderr).trim_end()))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> Result<String, String> {
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

        let sig =
            ssh_sign("ssh-keygen", key.to_str().unwrap(), "trawler-mr", data, None).expect("signs");
        assert!(ssh_verify("vu", &signers, "trawler-mr", data, &sig), "own signature verifies");
        assert!(!ssh_verify("vu", &signers, "trawler-mr", b"other", &sig), "tampered data fails");
        assert!(
            ssh_sign("ssh-keygen", "/no/such/key", "trawler-mr", data, None).is_err(),
            "missing key errors"
        );
    }

    #[test]
    #[cfg(unix)]
    fn ssh_sign_runs_the_given_signer_program() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let stub = dir.path().join("stub-signer");
        std::fs::write(&stub, "#!/bin/sh\ncat > /dev/null\nprintf '%s ' \"$@\"\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let out =
            ssh_sign(stub.to_str().unwrap(), "/some/key", "ns", b"payload", None).expect("runs");
        assert_eq!(out.trim_end(), "-Y sign -n ns -f /some/key");
    }

    #[test]
    #[cfg(unix)]
    fn ssh_sign_hands_the_reason_to_the_signer_environment() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let stub = dir.path().join("stub-signer");
        std::fs::write(&stub, "#!/bin/sh\ncat > /dev/null\nprintf '%s' \"$TRAWLER_SIGN_REASON\"\n")
            .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let reason = "approve the merge of wu/x/y into epic/x (tip abc1234) as vu";
        let out = ssh_sign(stub.to_str().unwrap(), "/some/key", "ns", b"payload", Some(reason))
            .expect("runs");
        assert_eq!(out, reason);
        let unset = ssh_sign(stub.to_str().unwrap(), "/some/key", "ns", b"payload", None).unwrap();
        assert_eq!(unset, "", "no reason means no variable, so the signer falls back");
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
