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

/// What git actually did: its exit disposition and both streams, kept apart —
/// for a caller whose verdict is not git's exit code, which [`git_env`]'s
/// `Result` cannot express.
pub struct Outcome {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    /// Git's own account of what happened: its stderr, its stdout when stderr
    /// is empty (`commit` reports "nothing to commit" on stdout), and a named
    /// silence when git wrote to neither — never `""`, which would leave a
    /// caller's sentence ending on a bare colon.
    pub fn said(&self) -> &str {
        match (self.stderr.as_str(), self.stdout.as_str()) {
            ("", "") => "git printed nothing",
            ("", stdout) => stdout,
            (stderr, _) => stderr,
        }
    }
}

/// [`git_env`]'s spawn and capture, without the verdict. Errs only when git
/// could not be run at all — a non-zero exit is an outcome, not an error.
pub fn git_outcome(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Outcome, String> {
    let mut command = cmd(repo);
    for (key, value) in env {
        command.env(key, value);
    }
    let out = command
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn git {}: {e}", args.join(" ")))?;
    Ok(Outcome {
        success: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    })
}

/// [`git`] with extra environment variables on the spawned process — how a
/// sign reason reaches a `gpg.ssh.program` signer through `git commit -S`.
pub fn git_env(repo: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<String, String> {
    let out = git_outcome(repo, args, env)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(format!("git {}: {}", args.join(" "), out.stderr))
    }
}

pub fn note(repo: &Path, note_ref: &str, tip: &str) -> Option<String> {
    let ref_arg = format!("--ref={note_ref}");
    git(repo, &["notes", &ref_arg, "show", tip]).ok()
}

/// A tip's note parsed into its schema type; `None` when absent or unparseable —
/// an unreadable note is treated as no note, so a malformed one never authorises.
pub fn parsed_note<T: serde::de::DeserializeOwned>(
    repo: &Path,
    note_ref: &str,
    tip: &str,
) -> Option<T> {
    note(repo, note_ref, tip).and_then(|raw| serde_yaml::from_str(&raw).ok())
}

pub fn write_note(repo: &Path, note_ref: &str, tip: &str, body: &str) -> Result<(), String> {
    let ref_arg = format!("--ref={note_ref}");
    git(repo, &["notes", &ref_arg, "add", "-f", "-m", body, tip]).map(|_| ())
}

fn names_in(repo: &Path, base: &str, tip: &str, filter: &[&str]) -> Result<Vec<String>, String> {
    let range = format!("{base}..{tip}");
    let mut args = vec!["diff", "--name-only", "--no-renames"];
    args.extend_from_slice(filter);
    args.push(&range);
    Ok(git(repo, &args)?.lines().map(str::to_string).collect())
}

pub fn diff_names(repo: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    names_in(repo, base, tip, &[])
}

// Renames are NOT detected here, and that is a deliberate refusal rather than an
// oversight. A rename's destination looks exactly like a new surface, and git can
// only pair it with a source by content similarity - which knows nothing about
// surfaces. Two review rounds found that heuristic letting genuinely new screens
// through: one scaffolded out of a deleted filler file, one copied from a live
// component while an unrelated surface happened to retire in the same diff. The
// gate therefore fails CLOSED: a rename of a built component is refused, because
// its design was already retired and there is nothing left to cite. That is a
// documented limitation with a reproduction, owned by W9 - not a hole.
pub fn added_names(repo: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    names_in(repo, base, tip, &["--diff-filter=A"])
}

// What the range leaves standing - added and modified, never deleted. Lower-case
// `d` excludes deletions inside the diff itself, so nothing downstream has to ask
// git whether each path still resolves at the tip: one process, not one per path.
pub fn changed_names(repo: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    names_in(repo, base, tip, &["--diff-filter=d"])
}

// Every path the range ever added under `prefix` - the commits, not the two
// endpoints. A file added by one commit and removed by another nets out of an
// endpoint diff, which is exactly how a design nobody built leaves no trace at
// an epic tip.
//
// `--full-history` is the load-bearing flag, not boilerplate. With a pathspec,
// git's default history simplification walks only the first parent past a merge
// whose tree is TREESAME for that path - and a design added and `git rm`'d inside
// one Working Unit branch makes its merge exactly that. Without the flag the add
// commit is never visited and the epic greens on a design nobody built, which is
// the one case this function exists to catch.
pub fn added_under(repo: &Path, base: &str, tip: &str, prefix: &str) -> Vec<String> {
    let range = format!("{base}..{tip}");
    let mut paths: Vec<String> = git(
        repo,
        &[
            "log",
            "--full-history",
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

/// The newest commit reachable from `tip` whose tree holds `path`, with the blob
/// it holds there — a design's last state before DEC-16 retired it, which is
/// where DEC-28 resolves a citation the tip no longer ships. `None` when nothing
/// reachable from `tip` ever held the path. `--full-history` for the reason
/// [`added_under`] records: the add-then-remove inside one branch is the shape
/// this exists to find, and default simplification skips it.
pub fn last_state(repo: &Path, tip: &str, path: &str) -> Option<(String, String)> {
    git(repo, &["log", "--full-history", "--format=%H", tip, "--", path])
        .ok()?
        .lines()
        .find_map(|commit| blob_of(repo, commit, path).map(|blob| (commit.to_string(), blob)))
}

pub fn diff_sha256(repo: &Path, base: &str, tip: &str) -> Result<String, String> {
    diff_sha256_paths(repo, base, tip, &[])
}

// The same hash over a subset of the range: the review binds the code paths it
// judged and the design step binds the artifacts it read, so a document the
// review never opened cannot move the bytes its PASS is checked against. Both
// sides of each binding call this one function, so what is hashed is the same
// bytes on both.
pub fn diff_sha256_paths(
    repo: &Path,
    base: &str,
    tip: &str,
    paths: &[String],
) -> Result<String, String> {
    let range = format!("{base}..{tip}");
    let diff = cmd(repo)
        .args(["diff", "--no-color", "--no-ext-diff", &range, "--"])
        .args(paths)
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

/// The two listings whose union is every path a `cargo` build may read: the
/// index (so a tracked file deleted from the worktree still has an entry to
/// record as absent) and untracked non-ignored files.
const BUILD_INPUT_LISTINGS: [&[&str]; 2] =
    [&["ls-files", "-z"], &["ls-files", "--others", "--exclude-standard", "-z"]];

fn build_inputs(repo: &Path) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for args in BUILD_INPUT_LISTINGS {
        let listing = cmd(repo)
            .args(args)
            .output()
            .map_err(|e| format!("failed to spawn git {}: {e}", args.join(" ")))?;
        if !listing.status.success() {
            return Err(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&listing.stderr).trim_end()
            ));
        }
        // A path the key cannot address is an error, never a silently dropped
        // entry: dropping one would be a false skip, the one direction a cache
        // must never fail in.
        let listed = String::from_utf8(listing.stdout).map_err(|_| {
            format!("git {} listed a non-UTF-8 path; the key cannot address it", args.join(" "))
        })?;
        paths.extend(listed.split('\0').filter(|p| !p.is_empty()).map(str::to_string));
    }
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Every field is length-prefixed so no file's contents can imitate the framing
/// of a following entry.
fn push_field(bytes: &mut Vec<u8>, tag: u8, field: &[u8]) {
    bytes.push(tag);
    bytes.extend_from_slice(field.len().to_string().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(field);
}

/// What a build would read at `path`: a symlink's target, a file's contents plus
/// its executable bit, or the fact that nothing is there. The old key got
/// deletions and mode flips for free out of `git diff HEAD`; reading the
/// worktree does not, so each is recorded as itself.
fn push_entry(bytes: &mut Vec<u8>, repo: &Path, path: &str) {
    push_field(bytes, b'p', path.as_bytes());
    let full = repo.join(path);
    match std::fs::symlink_metadata(&full) {
        Err(_) => bytes.push(b'0'),
        Ok(meta) if meta.is_symlink() => {
            let target = std::fs::read_link(&full).unwrap_or_default();
            push_field(bytes, b'l', target.to_string_lossy().as_bytes());
        }
        Ok(meta) => {
            bytes.push(if is_executable(&meta) { b'x' } else { b'-' });
            match std::fs::read(&full) {
                Ok(content) => push_field(bytes, b'f', &content),
                Err(_) => bytes.push(b'?'),
            }
        }
    }
}

/// A content hash of everything a `cargo` build reads: every tracked and
/// untracked non-ignored path, its contents, its executable bit, and the fact
/// of a deletion. Deliberately **not** `HEAD` or `git diff HEAD` — identical
/// content under a new commit is the same tree to every compiler, and hashing
/// history made each commit a false miss (DEC-54; DEC-11 keyed the old shape).
/// The index is not hashed either: fmt, clippy and the tests read the worktree,
/// so the key describes what the compiler sees. Over-invalidates on a stray
/// untracked file (safe direction: a needless rebuild, never a false skip).
// ponytail: untracked-but-not-ignored files invalidate the cache; tighten only
// if scratch files in the tree prove to churn it noisily.
pub fn worktree_key(repo: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    for path in build_inputs(repo)? {
        push_entry(&mut bytes, repo, &path);
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

/// The same blob as text, for the readers that judge source rather than hash it.
/// Absent, unreadable and not-UTF-8 all collapse to `None` — every caller treats
/// a path it cannot read as a path that declares nothing.
pub fn read_utf8_at(repo: &Path, rev: &str, path: &str) -> Option<String> {
    String::from_utf8(show_bytes(repo, rev, path).ok()?).ok()
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

    use super::{git, git_env, git_outcome, last_state, sha256_hex, ssh_sign, ssh_verify};

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

    struct History {
        _dir: tempfile::TempDir,
        repo: std::path::PathBuf,
    }

    impl History {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().to_path_buf();
            let g = |args: &[&str]| git(&repo, args).unwrap();
            g(&["init", "-q", "-b", "main"]);
            g(&["config", "user.email", "t@t"]);
            g(&["config", "user.name", "t"]);
            g(&["config", "commit.gpgsign", "false"]);
            let history = Self { _dir: dir, repo };
            history.write("README", "base\n");
            history.commit("base");
            history
        }

        fn git(&self, args: &[&str]) -> String {
            git(&self.repo, args).unwrap()
        }

        fn write(&self, rel: &str, body: &str) {
            let full = self.repo.join(rel);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, body).unwrap();
        }

        fn remove(&self, rel: &str) {
            std::fs::remove_file(self.repo.join(rel)).unwrap();
        }

        fn commit(&self, msg: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", msg]);
            self.git(&["rev-parse", "HEAD"])
        }

        fn blob(&self, rev: &str, path: &str) -> String {
            self.git(&["rev-parse", &format!("{rev}:{path}")])
        }
    }

    const DESIGN: &str = "docs/work/components/badge.html";

    #[test]
    fn last_state_is_the_blob_the_path_held_just_before_it_left_the_tree() {
        let h = History::new();
        h.write(DESIGN, "v1\n");
        let added = h.commit("add the design");
        let first = h.blob(&added, DESIGN);
        h.write(DESIGN, "v2\n");
        let amended = h.commit("amend the design");
        let last = h.blob(&amended, DESIGN);
        h.remove(DESIGN);
        let tip = h.commit("retire the design");

        assert_ne!(first, last, "the amendment moved the bytes");
        assert_eq!(
            last_state(&h.repo, &tip, DESIGN),
            Some((amended, last)),
            "the last state, not the first - an amended design is cited as it left"
        );
        assert_eq!(last_state(&h.repo, &tip, "docs/work/components/never.html"), None);
    }

    #[test]
    fn last_state_reaches_only_the_history_the_tip_stands_on() {
        let h = History::new();
        h.git(&["checkout", "-q", "-b", "elsewhere"]);
        h.write(DESIGN, "on another branch\n");
        h.commit("a design the tip never reaches");
        h.git(&["checkout", "-q", "main"]);
        h.write("README", "moved on\n");
        let tip = h.commit("main moves on without it");

        assert_eq!(last_state(&h.repo, &tip, DESIGN), None, "unreachable history is not history");
    }

    #[test]
    fn last_state_sees_through_a_merge_that_added_and_removed_the_path_in_one_branch() {
        let h = History::new();
        h.git(&["checkout", "-q", "-b", "wu/x/one"]);
        h.write(DESIGN, "designed and built here\n");
        let added = h.commit("design and build");
        let blob = h.blob(&added, DESIGN);
        h.remove(DESIGN);
        h.commit("retire it, DEC-16");
        h.git(&["checkout", "-q", "main"]);
        h.git(&["merge", "--no-ff", "-q", "-m", "merge wu/x/one", "wu/x/one"]);
        let tip = h.git(&["rev-parse", "HEAD"]);

        assert_eq!(
            last_state(&h.repo, &tip, DESIGN),
            Some((added, blob)),
            "the merge is TREESAME for the path - only --full-history visits the add"
        );
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

    #[test]
    fn an_outcome_keeps_the_exit_disposition_and_both_streams_apart() {
        let h = History::new();
        let ok = git_outcome(&h.repo, &["rev-parse", "HEAD"], &[]).expect("git runs");
        assert!(ok.success);
        assert_eq!(ok.stdout, h.git(&["rev-parse", "HEAD"]));
        assert_eq!(ok.stderr, "", "a succeeding query says nothing on stderr");

        let bad = git_outcome(&h.repo, &["rev-parse", "no-such-ref"], &[]).expect("git runs");
        assert!(!bad.success, "a non-zero exit is an outcome, not an error");
        assert!(bad.stderr.contains("no-such-ref"), "git's own line survives: {}", bad.stderr);
        assert_eq!(bad.said(), bad.stderr, "stderr is what git said when it is non-empty");
    }

    #[test]
    fn an_outcome_falls_back_to_stdout_for_what_git_said() {
        // `git commit` with nothing staged reports on stdout, not stderr, so
        // the one useful line is not on the stream git failed on.
        let h = History::new();
        let out = git_outcome(&h.repo, &["commit", "-m", "nothing staged"], &[]).expect("git runs");
        assert!(!out.success);
        assert!(out.stderr.is_empty(), "git said it on stdout, not stderr: {}", out.stderr);
        assert!(out.said().contains("nothing to commit"), "got: {}", out.said());
    }

    #[test]
    fn a_silent_git_is_reported_as_silence_not_as_an_empty_quote() {
        let silent =
            super::Outcome { success: false, stdout: String::new(), stderr: String::new() };
        assert!(!silent.said().is_empty(), "a caller quoting this would trail off");
        assert_eq!(silent.said(), "git printed nothing");
    }

    #[test]
    fn git_env_reports_a_failure_exactly_as_it_always_has() {
        let h = History::new();
        let args = ["rev-parse", "no-such-ref"];
        let err = git_env(&h.repo, &args, &[]).expect_err("a bad ref errs");
        let raw = git_outcome(&h.repo, &args, &[]).expect("git runs");
        assert_eq!(err, format!("git {}: {}", args.join(" "), raw.stderr));
    }

    #[test]
    fn git_outcome_errs_only_when_git_cannot_run() {
        let missing = std::path::Path::new("/no/such/repo/anywhere");
        assert!(
            git_outcome(missing, &["status"], &[]).is_ok(),
            "git ran and refused - that is an outcome"
        );
    }

    // The key frames every variable-length field with its length, so no file's
    // contents can imitate the start of the next entry - the one way a pure
    // delimiter scheme could hash two different trees alike.
    #[test]
    fn a_files_contents_cannot_imitate_the_next_entry() {
        let mut framed = Vec::new();
        super::push_field(&mut framed, b'p', b"a");
        super::push_field(&mut framed, b'f', b"\0p\x001\0b");
        let mut two_entries = Vec::new();
        super::push_field(&mut two_entries, b'p', b"a");
        super::push_field(&mut two_entries, b'f', b"");
        super::push_field(&mut two_entries, b'p', b"b");
        assert_ne!(framed, two_entries);
    }

    #[test]
    fn an_empty_field_is_not_an_absent_one() {
        let mut empty = Vec::new();
        super::push_field(&mut empty, b'f', b"");
        assert_ne!(empty, vec![b'0'], "an empty file must not hash as a missing one");
    }

    #[test]
    fn the_field_tag_separates_a_link_target_from_the_same_bytes_as_content() {
        let mut link = Vec::new();
        super::push_field(&mut link, b'l', b"target");
        let mut content = Vec::new();
        super::push_field(&mut content, b'f', b"target");
        assert_ne!(link, content);
    }
}
