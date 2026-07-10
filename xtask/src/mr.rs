use std::path::Path;
use std::process::Command;

use crate::gate::short;
use crate::git::{diff_sha256, git, note, ssh_sign, ssh_verify, write_note};

/// SSH-signature namespace for WU/epic note signatures (`ssh-keygen -Y -n`).
/// Doc-commit signatures ride git's native `git` namespace instead.
pub const NOTE_NAMESPACE: &str = "trawler-mr";

pub const TRAILER_KEY: &str = "Reviewed-By";

pub const NOT_AUTHORISED: &str = "\
A HUMAN must review this and confirm. The AI agent MUST NOT self-authorise.
             After the user has reviewed and said yes:
               cargo xtask mr commit -m \"<msg>\" --as <email|username>      # doc commit
               cargo xtask mr confirm <wu|epic> <tip> --as <email|username>  # WU / epic
             The identity must be listed in .mr-allowed.";

/// A `<tier>: <value>` line from `.mr-allowed`. `email`/`username` values are a
/// bare identity; a `key` value is `<identity> <ssh-pubkey>` — an unforgeable
/// entry E-MRSIG authorises by verifying a signature, not by matching a string.
pub struct Allowed {
    pub tier: String,
    pub value: String,
}

impl Allowed {
    /// The identity this entry authorises: the whole value for identity tiers,
    /// or the first field (`<identity>`) of a `key` entry.
    pub fn identity(&self) -> &str {
        if self.tier == "key" {
            self.value.split_whitespace().next().unwrap_or("")
        } else {
            &self.value
        }
    }

    /// The `<ssh-pubkey>` half of a `key` entry, if this is one.
    fn pubkey(&self) -> Option<&str> {
        (self.tier == "key")
            .then(|| self.value.split_once(char::is_whitespace))?
            .map(|(_, k)| k.trim())
    }
}

/// An `allowed_signers` file body for `ssh-keygen -Y verify`: one
/// `<identity> <ssh-pubkey>` line per `key` entry that names `identity`.
fn allowed_signers(allowed: &[Allowed], identity: &str) -> String {
    allowed
        .iter()
        .filter(|a| a.identity() == identity)
        .filter_map(|a| a.pubkey().map(|k| format!("{identity} {k}\n")))
        .collect()
}

/// Does `sig` verify over `data` in `namespace` as `identity`, against a `key`
/// entry listed for `identity`? The unforgeable half of the auth check.
fn signature_authorised(
    allowed: &[Allowed],
    identity: &str,
    namespace: &str,
    data: &[u8],
    sig: &str,
) -> bool {
    let signers = allowed_signers(allowed, identity);
    !signers.is_empty() && ssh_verify(identity, &signers, namespace, data, sig)
}

pub fn read_allowed(repo: &Path) -> Vec<Allowed> {
    let text = std::fs::read_to_string(repo.join(".mr-allowed")).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(':'))
        .map(|(tier, value)| Allowed {
            tier: tier.trim().to_string(),
            value: value.trim().to_string(),
        })
        .filter(|a| !a.tier.is_empty() && !a.value.is_empty())
        .collect()
}

/// The tier whose value equals `identity`, if any. This is the whole auth check
/// this epic — E-MRSIG replaces it with a signature verify.
pub fn match_tier(allowed: &[Allowed], identity: &str) -> Option<String> {
    allowed.iter().find(|a| a.identity() == identity).map(|a| a.tier.clone())
}

/// Parse a `Reviewed-By: <tier> <value>` trailer out of a commit message body.
/// Returns the `(tier, value)` of the last such trailer.
pub fn trailer_of(message: &str) -> Option<(String, String)> {
    message
        .lines()
        .filter_map(|l| l.strip_prefix(TRAILER_KEY))
        .filter_map(|rest| rest.trim_start().strip_prefix(':'))
        .filter_map(|rest| rest.trim().split_once(char::is_whitespace))
        .map(|(tier, value)| (tier.trim().to_string(), value.trim().to_string()))
        .last()
}

fn note_body(level: &str, tip: &str, tier: &str, signer: &str) -> String {
    format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\ntier: {tier}\nsigner: {signer}\ndate: {}\n",
        crate::gate::today()
    )
}

/// The `mr/v1` note for a `key`-backed signer: carries the signed `base` and the
/// armored `SSH SIGNATURE` (a YAML block scalar) that [`tip_authorised`] verifies
/// over `sha256(git diff base..tip)`. `sig` is last so the block scalar can't
/// swallow a following key.
fn signed_note_body(level: &str, tip: &str, base: &str, signer: &str, sig: &str) -> String {
    let indented: String = sig.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n");
    format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\nbase: {base}\ntier: key\nsigner: {signer}\ndate: {}\nsig: |\n{indented}\n",
        crate::gate::today()
    )
}

/// Does a `key` entry (one carrying a public key) list `identity`? Signing paths
/// take the unforgeable route only for such an identity; an identity-only entry
/// keeps the E-NMWF plain-note path (coexistence).
fn has_key_entry(allowed: &[Allowed], identity: &str) -> bool {
    allowed.iter().any(|a| a.identity() == identity && a.pubkey().is_some())
}

/// The SSH key `git commit -S` / `ssh-keygen -Y sign` sign with, from git config
/// (`cargo xtask mr setup` writes it). Absent = setup was never run.
fn signing_key(repo: &Path) -> Result<String, String> {
    git(repo, &["config", "--get", "user.signingkey"]).map_err(|_| {
        "no signing key configured - run `cargo xtask mr setup --as <id>` first".to_string()
    })
}

/// The program for `-Y sign` calls: `gpg.ssh.program` when configured (the
/// bundled Secure-Enclave signer), stock `ssh-keygen` otherwise — the same
/// lookup git itself makes for `commit -S`.
fn signer_program(repo: &Path) -> String {
    git(repo, &["config", "--get", "gpg.ssh.program"]).unwrap_or_else(|_| "ssh-keygen".to_string())
}

/// The merge target whose merge-base with the tip is the signed range's `base` —
/// the same base the gate diffs against, so the signature binds the gated change.
fn resolve_target(
    repo: &Path,
    is_epic: bool,
    tip_ref: &str,
    flag: Option<String>,
) -> Option<String> {
    flag.or_else(|| is_epic.then(|| "main".to_string()))
        .or_else(|| crate::infer_wu_target(tip_ref))
        .or_else(|| {
            crate::infer_wu_target(&git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?)
        })
}

fn touch_hint(identity: &str) -> String {
    format!("touch your security key / Touch ID to sign as {identity} now ...")
}

#[allow(clippy::print_stderr)]
fn prompt_touch(identity: &str) {
    eprintln!("{}", touch_hint(identity));
}

fn mr_commit(repo: &Path, message: &str, identity: &str) -> Result<(i32, String), String> {
    if message.trim().is_empty() {
        return Ok((1, "the commit message (-m) is empty".to_string()));
    }
    let allowed = read_allowed(repo);
    let Some(tier) = match_tier(&allowed, identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let body = format!("{}\n\n{TRAILER_KEY}: {tier} {identity}\n", message.trim_end());
    // A `key` identity signs the commit natively (`gpgsig` header, verified at
    // pre-push + walk); the trailer still rides along for the commit-msg hook,
    // which fires *before* git signs and so can only presence-check the message.
    if has_key_entry(&allowed, identity) {
        prompt_touch(identity);
        git(repo, &["commit", "-S", "-m", &body, "--cleanup=verbatim"])?;
        Ok((0, format!("signed doc commit with {TRAILER_KEY}: {tier} {identity}")))
    } else {
        git(repo, &["commit", "-m", &body, "--cleanup=verbatim"])?;
        Ok((0, format!("committed with {TRAILER_KEY}: {tier} {identity}")))
    }
}

fn mr_confirm(
    repo: &Path,
    level: &str,
    tip_ref: &str,
    identity: &str,
    target: Option<String>,
) -> Result<(i32, String), String> {
    let level = match level {
        "wu" => "working-unit",
        "epic" => "epic",
        _ => return Err(crate::USAGE.to_string()),
    };
    let allowed = read_allowed(repo);
    let Some(tier) = match_tier(&allowed, identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let tip = git(repo, &["rev-parse", tip_ref])?;
    if !has_key_entry(&allowed, identity) {
        write_note(repo, "mr", &tip, &note_body(level, &tip, &tier, identity))?;
        return Ok((0, format!("mr/v1 note written on {} ({tier} {identity})", short(&tip))));
    }
    let Some(target) = resolve_target(repo, level == "epic", tip_ref, target) else {
        return Ok((
            1,
            "cannot infer the merge target; pass --target <epic-branch|main>".to_string(),
        ));
    };
    let base = git(repo, &["merge-base", &target, &tip])?;
    let hash = diff_sha256(repo, &base, &tip)?;
    let key = signing_key(repo)?;
    prompt_touch(identity);
    let sig = ssh_sign(&signer_program(repo), &key, NOTE_NAMESPACE, hash.as_bytes())?;
    write_note(repo, "mr", &tip, &signed_note_body(level, &tip, &base, identity, &sig))?;
    Ok((0, format!("mr/v1 signed note written on {} (key {identity})", short(&tip))))
}

/// Is there a valid `mr/v1` note on `tip` authorising it? A `key`-backed signer
/// authorises only when the note's `sig` verifies over the recomputed
/// `base..tip` change-hash; an identity-only signer authorises by being listed.
pub fn tip_authorised(repo: &Path, tip: &str) -> bool {
    let Some(raw) = note(repo, "mr", tip) else { return false };
    let Ok(mr) = serde_yaml::from_str::<crate::notes::MrNote>(&raw) else { return false };
    if mr.schema != "mr/v1" {
        return false;
    }
    let allowed = read_allowed(repo);
    if let (Some(base), Some(sig)) = (&mr.base, &mr.sig) {
        let Ok(hash) = diff_sha256(repo, base, tip) else { return false };
        return signature_authorised(&allowed, &mr.signer, NOTE_NAMESPACE, hash.as_bytes(), sig);
    }
    match_tier(&allowed, &mr.signer).is_some()
}

/// Does `message` carry a `Reviewed-By` trailer naming a currently-listed
/// identity? The single trailer-auth predicate for both the commit-msg hook and
/// the epic walk. Doc-commit *signatures* verify separately (see [`commit_signed`]).
pub fn message_authorised(repo: &Path, message: &str) -> bool {
    trailer_of(message).is_some_and(|(_, id)| match_tier(&read_allowed(repo), &id).is_some())
}

/// Is `commit` signed (git-native SSH `gpgsig`) by a listed `key` entry? Uses
/// `git verify-commit` against an `allowed_signers` derived from `.mr-allowed`.
/// The auth check for doc commits — unforgeable, offline, no device.
pub fn commit_signed(repo: &Path, commit: &str) -> bool {
    let allowed = read_allowed(repo);
    let signers: String = allowed
        .iter()
        .filter_map(|a| a.pubkey().map(|k| format!("{} {k}\n", a.identity())))
        .collect();
    if signers.is_empty() {
        return false;
    }
    let Ok(dir) = tempfile::tempdir() else { return false };
    let signers_path = dir.path().join("allowed_signers");
    if std::fs::write(&signers_path, &signers).is_err() {
        return false;
    }
    git(
        repo,
        &[
            "-c",
            "gpg.format=ssh",
            "-c",
            &format!("gpg.ssh.allowedSignersFile={}", signers_path.display()),
            "verify-commit",
            commit,
        ],
    )
    .is_ok()
}

/// The `commit-msg` hook: if the staged diff touches docs, the message must
/// carry a `Reviewed-By` trailer naming a listed identity, else the commit is
/// refused (exit 1).
pub fn commit_msg_hook(repo: &Path, msg_file: &str) -> Result<(i32, String), String> {
    // A merge authors no new content; its merged doc commits carry their own
    // trailers, re-verified by the epic walk. Requiring one on the merge commit
    // too would demand re-authorising already-signed history at every merge.
    if git(repo, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]).is_ok() {
        return Ok((0, String::new()));
    }
    let staged = git(repo, &["diff", "--cached", "--name-only", "--no-renames"])?;
    if !staged.lines().any(crate::notes::is_doc) {
        return Ok((0, String::new()));
    }
    let message = std::fs::read_to_string(msg_file)
        .map_err(|e| format!("cannot read commit message file {msg_file}: {e}"))?;
    if message_authorised(repo, &message) {
        Ok((0, String::new()))
    } else {
        Ok((
            1,
            format!(
                "this commit touches docs with no valid {TRAILER_KEY} trailer\n{NOT_AUTHORISED}"
            ),
        ))
    }
}

/// The `.mr-allowed` line registering `identity`'s public key.
fn key_line(identity: &str, pubkey: &str) -> String {
    format!("key: {identity} {pubkey}")
}

/// `existing` with `line` appended, idempotently — an already-present entry is
/// left untouched so re-running setup never duplicates a key.
fn append_key_line(existing: &str, line: &str) -> String {
    if existing.lines().any(|l| l.trim() == line) {
        return existing.to_string();
    }
    let sep = if existing.is_empty() || existing.ends_with('\n') { "" } else { "\n" };
    format!("{existing}{sep}{line}\n")
}

/// The public-key file for a `--key` value. A Secure-Enclave / Touch-ID key
/// (via an ssh-agent, e.g. Secretive) has *only* a public key on disk, so a
/// `.pub` path is taken as-is; a private-key stem gets `.pub` appended.
fn pub_path_for(key: &str) -> String {
    if key.ends_with(".pub") {
        key.to_string()
    } else {
        format!("{key}.pub")
    }
}

const SE_KEY_HEADER: &str = "trawler-se-v1";
const SE_SIGN_SOURCE: &str = include_str!("se_sign.swift");

/// A private-key file written by the bundled Secure-Enclave signer: header
/// line, then the `CryptoKit` blob only this machine's Enclave can use.
fn is_se_key(path: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.lines().next() == Some(SE_KEY_HEADER))
}

/// Compile the bundled Secure-Enclave signer (`se_sign.swift`) with the system
/// `swiftc`, caching binary + source under `~/.config/trawler`; recompiles only
/// when the embedded source changed.
fn build_se_helper(home: &str) -> Result<std::path::PathBuf, String> {
    let dir = Path::new(home).join(".config").join("trawler");
    let src = dir.join("se-sign.swift");
    let bin = dir.join("se-sign");
    if bin.exists() && std::fs::read_to_string(&src).ok().as_deref() == Some(SE_SIGN_SOURCE) {
        return Ok(bin);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    std::fs::write(&src, SE_SIGN_SOURCE)
        .map_err(|e| format!("cannot write {}: {e}", src.display()))?;
    let out = Command::new("swiftc")
        .arg("-O")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .map_err(|e| format!("swiftc unavailable: {e}"))?;
    if out.status.success() {
        Ok(bin)
    } else {
        Err(format!("swiftc failed: {}", String::from_utf8_lossy(&out.stderr).trim_end()))
    }
}

/// Mint (or reuse) a Touch-ID-gated Secure-Enclave key for `identity` — the
/// zero-install macOS path. Fails cleanly off-macOS, without `swiftc`, or on
/// Enclave-less hardware; the caller falls back to FIDO2.
fn provision_se_key(home: &str, identity: &str) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err("Secure Enclave signing is macOS-only".to_string());
    }
    let helper = build_se_helper(home)?;
    let stem = format!("{home}/.ssh/id_trawler_se_{identity}");
    if !Path::new(&stem).exists() {
        let out = Command::new(&helper)
            .args(["keygen", &stem])
            .output()
            .map_err(|e| format!("cannot run the Secure-Enclave signer: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "Secure Enclave keygen failed: {}",
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
    }
    Ok(stem)
}

/// Resolve the signing key. `--key <path>` is used verbatim (a private-key stem,
/// a `.pub` for an agent-backed key, or a Secure-Enclave blob from an earlier
/// setup). Otherwise, on a Secure-Enclave Mac a Touch-ID key is provisioned
/// natively via the bundled signer — the Enclave→OpenSSH bridge stock macOS
/// lacks (`sc_auth` keys are unreachable by `ssh-keygen -Y sign`; see
/// `se_sign.swift`). Failing that, a FIDO2 `ed25519-sk` key is generated at the
/// default path (needs a security key + touch). Returns `(signingkey, pubkey)`.
#[allow(clippy::print_stderr)]
fn resolve_key(identity: &str, key: Option<String>) -> Result<(String, String), String> {
    if let Some(key_path) = key {
        let pubkey = read_pubkey(&pub_path_for(&key_path))?;
        return Ok((key_path, pubkey));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME unset; pass --key <path>".to_string())?;
    match provision_se_key(&home, identity) {
        Ok(stem) => {
            let pubkey = read_pubkey(&pub_path_for(&stem))?;
            return Ok((stem, pubkey));
        }
        Err(reason) => eprintln!("no Secure-Enclave path ({reason}); trying FIDO2 ..."),
    }
    let stem = format!("{home}/.ssh/id_trawler_mrsig");
    if !Path::new(&pub_path_for(&stem)).exists() {
        generate_key(&stem)?;
    }
    Ok((stem.clone(), read_pubkey(&pub_path_for(&stem))?))
}

fn read_pubkey(pub_path: &str) -> Result<String, String> {
    std::fs::read_to_string(pub_path)
        .map_err(|e| format!("cannot read public key {pub_path}: {e}"))
        .map(|s| s.trim().to_string())
}

#[allow(clippy::print_stderr)]
fn generate_key(path: &str) -> Result<(), String> {
    eprintln!(
        "generating a hardware-backed ed25519-sk key; touch your security key when it prompts ..."
    );
    let ok = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519-sk", "-N", "", "-C", "trawler-mr", "-f", path])
        .status()
        .map_err(|e| format!("failed to run ssh-keygen: {e}"))?
        .success();
    if ok {
        Ok(())
    } else {
        Err("no signing key provisioned. Pick one and re-run with --key <path>:\n  \
             * Touch ID  - a Secure-Enclave key via an ssh-agent bridge (e.g. Secretive), --key its .pub\n  \
             * FIDO2     - attach a security key and re-run `mr setup` (no --key) to generate one\n  \
             * software  - ssh-keygen -t ed25519 -f ~/.ssh/mrsig (NOT hardware-backed), --key ~/.ssh/mrsig"
            .to_string())
    }
}

fn mr_setup(repo: &Path, identity: &str, key: Option<String>) -> Result<(i32, String), String> {
    let (key_path, pubkey) = resolve_key(identity, key)?;
    let allowed_path = repo.join(".mr-allowed");
    let existing = std::fs::read_to_string(&allowed_path).unwrap_or_default();
    let updated = append_key_line(&existing, &key_line(identity, &pubkey));
    std::fs::write(&allowed_path, updated).map_err(|e| format!("cannot write .mr-allowed: {e}"))?;
    git(repo, &["config", "gpg.format", "ssh"])?;
    git(repo, &["config", "user.signingkey", &key_path])?;
    let mut touch_note = "";
    if is_se_key(&key_path) {
        let home =
            std::env::var("HOME").map_err(|_| "HOME unset; cannot locate signer".to_string())?;
        let helper = build_se_helper(&home)?;
        git(repo, &["config", "gpg.ssh.program", &helper.display().to_string()])?;
        touch_note = " Every signature will prompt Touch ID (Secure Enclave, this machine only).";
    }
    Ok((
        0,
        format!(
            "registered key for {identity} in .mr-allowed and configured SSH signing \
             (user.signingkey={key_path}). Doc commits: cargo xtask mr commit --as \
             {identity}{touch_note}"
        ),
    ))
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["setup", rest @ ..] => {
            let identity = crate::flag_value(rest, "--as").ok_or(crate::USAGE.to_string())?;
            mr_setup(repo, &identity, crate::flag_value(rest, "--key"))
        }
        ["commit", rest @ ..] => {
            let message = crate::flag_value(rest, "-m").ok_or(crate::USAGE.to_string())?;
            let identity = crate::flag_value(rest, "--as").ok_or(crate::USAGE.to_string())?;
            mr_commit(repo, &message, &identity)
        }
        ["confirm", level, tip, rest @ ..] => {
            let identity = crate::flag_value(rest, "--as").ok_or(crate::USAGE.to_string())?;
            mr_confirm(repo, level, tip, &identity, crate::flag_value(rest, "--target"))
        }
        _ => Err(crate::USAGE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use super::{
        allowed_signers, append_key_line, has_key_entry, is_se_key, key_line, match_tier,
        pub_path_for, signature_authorised, signed_note_body, signer_program, touch_hint,
        trailer_of, Allowed, NOTE_NAMESPACE, SE_KEY_HEADER,
    };

    fn allowed() -> Vec<Allowed> {
        vec![
            Allowed { tier: "email".into(), value: "a@b.co".into() },
            Allowed { tier: "username".into(), value: "vu".into() },
        ]
    }

    /// A software ed25519 key in `dir`; returns (public-key-line, sign-fn).
    fn keygen(dir: &Path) -> (String, impl Fn(&str, &[u8]) -> String + '_) {
        let key = dir.join("id");
        assert!(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "test", "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        let pubkey = std::fs::read_to_string(dir.join("id.pub")).unwrap().trim().to_string();
        let sign = move |namespace: &str, data: &[u8]| {
            let mut child = Command::new("ssh-keygen")
                .args(["-Y", "sign", "-n", namespace, "-f"])
                .arg(&key)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(data).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success());
            String::from_utf8(out.stdout).unwrap()
        };
        (pubkey, sign)
    }

    #[test]
    fn matches_a_listed_identity_to_its_tier() {
        assert_eq!(match_tier(&allowed(), "a@b.co").as_deref(), Some("email"));
        assert_eq!(match_tier(&allowed(), "vu").as_deref(), Some("username"));
        assert_eq!(match_tier(&allowed(), "someone@else.co"), None);
    }

    #[test]
    fn a_key_entry_is_listed_by_its_identity() {
        let a = vec![Allowed { tier: "key".into(), value: "vu ssh-ed25519 AAAAdummy".into() }];
        assert_eq!(match_tier(&a, "vu").as_deref(), Some("key"));
        assert_eq!(match_tier(&a, "ssh-ed25519"), None, "the pubkey is not an identity");
    }

    #[test]
    fn allowed_signers_renders_only_matching_key_entries() {
        let a = vec![
            Allowed { tier: "email".into(), value: "vu".into() },
            Allowed { tier: "key".into(), value: "vu ssh-ed25519 AAAAvu".into() },
            Allowed { tier: "key".into(), value: "bo ssh-ed25519 AAAAbo".into() },
        ];
        assert_eq!(allowed_signers(&a, "vu"), "vu ssh-ed25519 AAAAvu\n");
        assert_eq!(allowed_signers(&a, "nobody"), "");
    }

    #[test]
    fn signature_roundtrips_and_rejects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let (pubkey, sign) = keygen(dir.path());
        let listed = vec![Allowed { tier: "key".into(), value: format!("vu {pubkey}") }];
        let data = b"sha256-of-the-change";
        let sig = sign(NOTE_NAMESPACE, data);

        assert!(signature_authorised(&listed, "vu", NOTE_NAMESPACE, data, &sig), "valid sig");
        assert!(
            !signature_authorised(&listed, "vu", NOTE_NAMESPACE, b"other data", &sig),
            "tampered data"
        );
        assert!(
            !signature_authorised(&listed, "vu", "wrong-namespace", data, &sig),
            "wrong namespace"
        );
        assert!(
            !signature_authorised(&listed, "vu", NOTE_NAMESPACE, data, "not a sig blob"),
            "malformed blob"
        );

        let (other_pub, _) = keygen(tempfile::tempdir().unwrap().path());
        let unlisted = vec![Allowed { tier: "key".into(), value: format!("vu {other_pub}") }];
        assert!(
            !signature_authorised(&unlisted, "vu", NOTE_NAMESPACE, data, &sig),
            "signed by a key not listed for vu"
        );

        let identity_only = allowed();
        assert!(
            !signature_authorised(&identity_only, "vu", NOTE_NAMESPACE, data, &sig),
            "an identity-only entry has no key to verify against"
        );
    }

    #[test]
    fn se_key_detection_matches_only_the_blob_header() {
        let dir = tempfile::tempdir().unwrap();
        let se = dir.path().join("se");
        std::fs::write(&se, format!("{SE_KEY_HEADER}\nQUFBQQ==\n")).unwrap();
        assert!(is_se_key(se.to_str().unwrap()));

        let software = dir.path().join("id");
        std::fs::write(&software, "-----BEGIN OPENSSH PRIVATE KEY-----\n").unwrap();
        assert!(!is_se_key(software.to_str().unwrap()));
        assert!(!is_se_key(dir.path().join("missing").to_str().unwrap()));
    }

    #[test]
    fn signer_program_reads_gpg_ssh_program_config() {
        let dir = tempfile::tempdir().unwrap();
        crate::git::git(dir.path(), &["init", "-q"]).unwrap();
        crate::git::git(dir.path(), &["config", "gpg.ssh.program", "/opt/se-sign"]).unwrap();
        assert_eq!(signer_program(dir.path()), "/opt/se-sign");
    }

    #[test]
    fn key_line_appends_idempotently() {
        assert_eq!(key_line("vu", "ssh-ed25519 AAAA"), "key: vu ssh-ed25519 AAAA");
        let line = key_line("vu", "ssh-ed25519 AAAA");
        assert_eq!(append_key_line("", &line), "key: vu ssh-ed25519 AAAA\n");
        assert_eq!(
            append_key_line("email: a@b.co\n", &line),
            "email: a@b.co\nkey: vu ssh-ed25519 AAAA\n"
        );
        assert_eq!(
            append_key_line("email: a@b.co", &line),
            "email: a@b.co\nkey: vu ssh-ed25519 AAAA\n"
        );
        let once = append_key_line("email: a@b.co\n", &line);
        assert_eq!(append_key_line(&once, &line), once, "a present key is never duplicated");
    }

    #[test]
    fn pub_path_takes_a_dot_pub_as_is_else_appends() {
        assert_eq!(pub_path_for("/home/u/.ssh/id"), "/home/u/.ssh/id.pub");
        assert_eq!(pub_path_for("/home/u/.ssh/se.pub"), "/home/u/.ssh/se.pub");
    }

    #[test]
    fn has_key_entry_only_for_a_pubkey_carrying_entry() {
        let a = vec![
            Allowed { tier: "email".into(), value: "vu".into() },
            Allowed { tier: "key".into(), value: "bo ssh-ed25519 AAAAbo".into() },
        ];
        assert!(has_key_entry(&a, "bo"));
        assert!(!has_key_entry(&a, "vu"), "an identity-only entry is not a key");
        assert!(!has_key_entry(&a, "nobody"));
    }

    #[test]
    fn signed_note_body_parses_as_mr_v1_with_the_sig_intact() {
        let sig = "-----BEGIN SSH SIGNATURE-----\nAAAAdummyblob\n-----END SSH SIGNATURE-----\n";
        let body = signed_note_body("epic", "tttt", "bbbb", "vu", sig);
        let note: crate::notes::MrNote = serde_yaml::from_str(&body).expect("valid mr/v1 yaml");
        assert_eq!(note.tier, "key");
        assert_eq!(note.base.as_deref(), Some("bbbb"));
        assert!(note.sig.as_deref().unwrap().contains("BEGIN SSH SIGNATURE"), "{body}");
        assert!(note.sig.as_deref().unwrap().contains("AAAAdummyblob"), "{body}");
    }

    #[test]
    fn touch_hint_names_the_signer() {
        assert!(touch_hint("vu").contains("vu"));
        assert!(touch_hint("vu").to_lowercase().contains("touch"));
    }

    #[test]
    fn parses_last_reviewed_by_trailer() {
        let msg = "docs: x\n\nReviewed-By: email a@b.co\n";
        assert_eq!(trailer_of(msg), Some(("email".into(), "a@b.co".into())));
        assert_eq!(trailer_of("no trailer here"), None);
        let two = "m\n\nReviewed-By: email old@x.co\nReviewed-By: username vu\n";
        assert_eq!(trailer_of(two), Some(("username".into(), "vu".into())));
    }
}
