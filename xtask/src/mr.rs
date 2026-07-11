use std::path::Path;
use std::process::Command;

use crate::gate::short;
use crate::git::{diff_sha256, git, note, ssh_sign, ssh_verify, write_note};
use crate::wizard;

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

/// The auth-sheet / terminal line naming what a doc-commit signature approves.
fn commit_reason(message: &str, branch: &str, identity: &str) -> String {
    let title = message.lines().next().unwrap_or("").trim();
    format!("commit \"{title}\" on {branch} as {identity}")
}

/// The auth-sheet / terminal line naming what a confirm signature approves.
fn confirm_reason(tip_ref: &str, target: &str, tip: &str, identity: &str) -> String {
    format!("approve the merge of {tip_ref} into {target} (tip {}) as {identity}", short(tip))
}

fn current_branch(repo: &Path) -> String {
    git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "HEAD".to_string())
}

fn touch_hint(identity: &str, reason: &str) -> String {
    format!("about to {reason}\ntouch your security key / Touch ID to sign as {identity} now ...")
}

#[allow(clippy::print_stderr)]
fn prompt_touch(identity: &str, reason: &str) {
    eprintln!("{}", touch_hint(identity, reason));
}

/// The branch-name → level regex of the confirm wizard: `wu/…` → wu,
/// `epic/…` → epic, anything else asks the user.
fn infer_level(branch: &str) -> Option<&'static str> {
    if branch.starts_with("wu/") {
        Some("wu")
    } else if branch.starts_with("epic/") {
        Some("epic")
    } else {
        None
    }
}

/// The identity picker: the distinct identities already in `.mr-allowed`, never
/// typed blind. A single listed identity is taken as-is.
fn pick_identity(allowed: &[Allowed], input: &mut dyn std::io::BufRead) -> Result<String, String> {
    let mut identities: Vec<&str> = Vec::new();
    for entry in allowed {
        if !identities.contains(&entry.identity()) {
            identities.push(entry.identity());
        }
    }
    match identities.as_slice() {
        [] => {
            Err("no identities listed in .mr-allowed - run `cargo xtask mr setup` first"
                .to_string())
        }
        [only] => Ok((*only).to_string()),
        _ => {
            let options: Vec<wizard::MenuOption> = identities
                .iter()
                .map(|id| wizard::MenuOption {
                    id: (*id).to_string(),
                    label: (*id).to_string(),
                    note: String::new(),
                    unavailable: None,
                })
                .collect();
            let pick = wizard::choose(input, "sign as which identity?", &options)?;
            Ok(identities[pick].to_string())
        }
    }
}

fn mr_commit(
    repo: &Path,
    message: Option<String>,
    identity: Option<String>,
    input: &mut dyn std::io::BufRead,
) -> Result<(i32, String), String> {
    let message = match message {
        Some(m) => m,
        None => wizard::ask(input, "commit message")?,
    };
    let message = message.as_str();
    if message.trim().is_empty() {
        return Ok((1, "the commit message (-m) is empty".to_string()));
    }
    let allowed = read_allowed(repo);
    let identity = match identity {
        Some(id) => id,
        None => pick_identity(&allowed, input)?,
    };
    let identity = identity.as_str();
    let Some(tier) = match_tier(&allowed, identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let body = format!("{}\n\n{TRAILER_KEY}: {tier} {identity}\n", message.trim_end());
    // A `key` identity signs the commit natively (`gpgsig` header, verified at
    // pre-push + walk); the trailer still rides along for the commit-msg hook,
    // which fires *before* git signs and so can only presence-check the message.
    if has_key_entry(&allowed, identity) {
        let reason = commit_reason(message, &current_branch(repo), identity);
        prompt_touch(identity, &reason);
        crate::git::git_env(
            repo,
            &["commit", "-S", "-m", &body, "--cleanup=verbatim"],
            &[("TRAWLER_SIGN_REASON", &reason)],
        )?;
        Ok((0, format!("signed doc commit with {TRAILER_KEY}: {tier} {identity}")))
    } else {
        git(repo, &["commit", "-m", &body, "--cleanup=verbatim"])?;
        Ok((0, format!("committed with {TRAILER_KEY}: {tier} {identity}")))
    }
}

fn mr_confirm(
    repo: &Path,
    level: Option<&str>,
    tip_ref: Option<&str>,
    identity: Option<String>,
    target: Option<String>,
    input: &mut dyn std::io::BufRead,
) -> Result<(i32, String), String> {
    let level_word = match level {
        Some(l) => l.to_string(),
        None => match infer_level(&current_branch(repo)) {
            Some(inferred) => inferred.to_string(),
            None => wizard::ask(input, "level (wu or epic)")?,
        },
    };
    let level = match level_word.as_str() {
        "wu" => "working-unit",
        "epic" => "epic",
        _ => return Err(crate::USAGE.to_string()),
    };
    let tip_ref = tip_ref.unwrap_or("HEAD");
    let allowed = read_allowed(repo);
    let identity = match identity {
        Some(id) => id,
        None => pick_identity(&allowed, input)?,
    };
    let identity = identity.as_str();
    let Some(tier) = match_tier(&allowed, identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let tip = git(repo, &["rev-parse", tip_ref])?;
    if !has_key_entry(&allowed, identity) {
        write_note(repo, "mr", &tip, &note_body(level, &tip, &tier, identity))?;
        return Ok((0, format!("mr/v1 note written on {} ({tier} {identity})", short(&tip))));
    }
    let target = match resolve_target(repo, level == "epic", tip_ref, target) {
        Some(t) => t,
        None => wizard::ask(input, "merge target branch (e.g. epic/<slug> or main)")?,
    };
    let base = git(repo, &["merge-base", &target, &tip])?;
    let hash = diff_sha256(repo, &base, &tip)?;
    let key = signing_key(repo)?;
    let named_tip = if tip_ref == "HEAD" { current_branch(repo) } else { tip_ref.to_string() };
    let reason = confirm_reason(&named_tip, &target, &tip, identity);
    prompt_touch(identity, &reason);
    let sig =
        ssh_sign(&signer_program(repo), &key, NOTE_NAMESPACE, hash.as_bytes(), Some(&reason))?;
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
const SE_SIGN_SOURCE: &str = include_str!("trawler_signer.swift");

/// A private-key file written by the bundled Secure-Enclave signer: header
/// line, then the `CryptoKit` blob only this machine's Enclave can use.
fn is_se_key(path: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.lines().next() == Some(SE_KEY_HEADER))
}

/// Compile the bundled Secure-Enclave signer (`trawler_signer.swift`) with the
/// system `swiftc`, caching binary + source under `~/.config/trawler`;
/// recompiles only when the embedded source changed. The binary is named
/// `trawler-signer` because the macOS auth sheet shows the executable name.
fn build_se_helper(home: &str) -> Result<std::path::PathBuf, String> {
    let dir = Path::new(home).join(".config").join("trawler");
    let src = dir.join("trawler-signer.swift");
    let bin = dir.join("trawler-signer");
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

/// Can the Secure-Enclave method work here? `Ok(biometry?)` from a
/// `trawler-signer check` run — the definitive answer only `CryptoKit` can give —
/// or `Err(one-line reason)` when unreachable (off-macOS, no `swiftc`, no
/// Enclave). Probed cheapest-first, before any menu is shown.
fn se_capability(home: &str) -> Result<bool, String> {
    if !cfg!(target_os = "macos") {
        return Err("Secure Enclave signing is macOS-only".to_string());
    }
    let helper = build_se_helper(home)?;
    let out = Command::new(&helper)
        .arg("check")
        .output()
        .map_err(|e| format!("cannot run trawler-signer check: {e}"))?;
    parse_se_check(&String::from_utf8_lossy(&out.stdout))
}

/// `enclave=yes|no biometry=yes|no` → `Ok(biometry)` when an Enclave exists.
fn parse_se_check(report: &str) -> Result<bool, String> {
    if !report.contains("enclave=yes") {
        return Err("no Secure Enclave on this hardware".to_string());
    }
    Ok(report.contains("biometry=yes"))
}

/// The setup menu, built from the probe: the Secure-Enclave row is labelled by
/// what its `.userPresence` gate will actually ask (Touch ID, or the login
/// password on a biometry-less Mac) and greyed with its reason when unreachable.
fn setup_methods(se: &Result<bool, String>) -> Vec<wizard::MenuOption> {
    let (se_label, se_note) = match se {
        Ok(true) | Err(_) => {
            ("Touch ID (Secure Enclave)", "each signature asks for Touch ID on this Mac")
        }
        Ok(false) => (
            "System password (Secure Enclave)",
            "each signature asks for your login password (no biometry here)",
        ),
    };
    vec![
        wizard::MenuOption {
            id: "touch-id".to_string(),
            label: se_label.to_string(),
            note: se_note.to_string(),
            unavailable: se.as_ref().err().cloned(),
        },
        wizard::MenuOption {
            id: "fido2".to_string(),
            label: "FIDO2 security key".to_string(),
            note: "each signature asks for a touch on the key (needs one plugged in now)"
                .to_string(),
            unavailable: None,
        },
        wizard::MenuOption {
            id: "software".to_string(),
            label: "new software key".to_string(),
            note: "asks for its passphrase if you set one; possession-only otherwise".to_string(),
            unavailable: None,
        },
        wizard::MenuOption {
            id: "existing".to_string(),
            label: "existing key (path)".to_string(),
            note: "whatever that key itself asks (agent, passphrase, or nothing)".to_string(),
            unavailable: None,
        },
        wizard::MenuOption {
            id: "identity".to_string(),
            label: "identity only (no key)".to_string(),
            note: "nothing - a string match, the weakest tier".to_string(),
            unavailable: None,
        },
    ]
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

#[allow(clippy::print_stderr)]
fn generate_software_key(stem: &str, with_passphrase: bool) -> Result<(), String> {
    if let Some(parent) = Path::new(stem).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let mut command = Command::new("ssh-keygen");
    command.args(["-q", "-t", "ed25519", "-C", "trawler-mr", "-f", stem]);
    if with_passphrase {
        eprintln!("choose the passphrase in ssh-keygen's own prompt (asked per signature) ...");
    } else {
        command.args(["-N", ""]);
    }
    let ok = command.status().map_err(|e| format!("failed to run ssh-keygen: {e}"))?.success();
    if ok {
        Ok(())
    } else {
        Err(format!("ssh-keygen failed to generate the software key at {stem}"))
    }
}

/// Register a signing key: append its `.mr-allowed` entry (never overwrite) and
/// make it the active signer (`user.signingkey` — the last setup wins, earlier
/// keys stay verifiable). A Secure-Enclave blob also installs the bundled
/// signer as `gpg.ssh.program`.
fn register_key(
    repo: &Path,
    identity: &str,
    key_path: &str,
    pubkey: &str,
) -> Result<(i32, String), String> {
    append_allowed_line(repo, &key_line(identity, pubkey))?;
    git(repo, &["config", "gpg.format", "ssh"])?;
    git(repo, &["config", "user.signingkey", key_path])?;
    let asks = if is_se_key(key_path) {
        let home =
            std::env::var("HOME").map_err(|_| "HOME unset; cannot locate signer".to_string())?;
        let helper = build_se_helper(&home)?;
        git(repo, &["config", "gpg.ssh.program", &helper.display().to_string()])?;
        "Touch ID / your login password (Secure Enclave, this machine only)"
    } else if pubkey.starts_with("sk-") {
        "a touch on the plugged-in security key"
    } else {
        "the key's passphrase if it has one; nothing otherwise (possession-only)"
    };
    Ok((0, setup_summary(repo, identity, asks)))
}

/// The closing setup output (P4): what was registered, what each signature will
/// ask for, which `.mr-allowed` entries this identity now holds, and which key
/// is the active signer.
fn setup_summary(repo: &Path, identity: &str, asks: &str) -> String {
    let entries: Vec<String> = read_allowed(repo)
        .iter()
        .filter(|a| a.identity() == identity)
        .map(|a| a.tier.clone())
        .collect();
    let active = git(repo, &["config", "--get", "user.signingkey"])
        .unwrap_or_else(|_| "none (identity-only)".to_string());
    format!(
        "registered {identity} in .mr-allowed (entries: {})\n\
         each signature asks for: {asks}\n\
         active signer (user.signingkey): {active}\n\
         doc commits: cargo xtask mr commit --as {identity}",
        entries.join(", ")
    )
}

fn append_allowed_line(repo: &Path, line: &str) -> Result<(), String> {
    let allowed_path = repo.join(".mr-allowed");
    let existing = std::fs::read_to_string(&allowed_path).unwrap_or_default();
    std::fs::write(&allowed_path, append_key_line(&existing, line))
        .map_err(|e| format!("cannot write .mr-allowed: {e}"))
}

fn mr_setup(
    repo: &Path,
    identity: Option<String>,
    key: Option<String>,
    input: &mut dyn std::io::BufRead,
) -> Result<(i32, String), String> {
    let identity = match identity {
        Some(id) => id,
        None => wizard::ask(input, "identity (email or username)")?,
    };
    if let Some(key_path) = key {
        let pubkey = read_pubkey(&pub_path_for(&key_path))?;
        return register_key(repo, &identity, &key_path, &pubkey);
    }
    let home = std::env::var("HOME").map_err(|_| "HOME unset; pass --key <path>".to_string())?;
    setup_wizard(repo, &identity, &home, &se_capability(&home), input)
}

/// The capability-probed setup menu. `se` is injected so hermetic tests drive
/// the full wizard without real hardware or `$HOME`; `mr_setup` passes the
/// live probe.
pub fn setup_wizard(
    repo: &Path,
    identity: &str,
    home: &str,
    se: &Result<bool, String>,
    input: &mut dyn std::io::BufRead,
) -> Result<(i32, String), String> {
    let methods = setup_methods(se);
    let pick = wizard::choose(input, "how should your signatures be authorised?", &methods)?;
    match methods[pick].id.as_str() {
        "touch-id" => {
            let stem = provision_se_key(home, identity)?;
            register_key(repo, identity, &stem, &read_pubkey(&pub_path_for(&stem))?)
        }
        "fido2" => {
            let stem = format!("{home}/.ssh/id_trawler_mrsig");
            if !Path::new(&pub_path_for(&stem)).exists() {
                generate_key(&stem)?;
            }
            register_key(repo, identity, &stem, &read_pubkey(&pub_path_for(&stem))?)
        }
        "software" => {
            let stem = format!("{home}/.ssh/id_trawler_{identity}");
            if !Path::new(&pub_path_for(&stem)).exists() {
                let with_passphrase =
                    wizard::confirm(input, "protect it with a passphrase? (skippable)")?;
                generate_software_key(&stem, with_passphrase)?;
            }
            register_key(repo, identity, &stem, &read_pubkey(&pub_path_for(&stem))?)
        }
        "existing" => {
            let path = wizard::ask(input, "path to the key (.pub or private-key stem)")?;
            register_key(repo, identity, &path, &read_pubkey(&pub_path_for(&path))?)
        }
        _ => {
            let tier = if identity.contains('@') { "email" } else { "username" };
            append_allowed_line(repo, &format!("{tier}: {identity}"))?;
            Ok((0, setup_summary(repo, identity, "nothing - a string match, the weakest tier")))
        }
    }
}

/// Positional (non-flag) arguments; every `-…` token consumes the value after it.
fn split_positionals<'a>(rest: &[&'a str]) -> Vec<&'a str> {
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        if rest[i].starts_with('-') {
            i += 2;
        } else {
            positionals.push(rest[i]);
            i += 1;
        }
    }
    positionals
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    run_with_input(args, repo, &mut std::io::stdin().lock())
}

/// [`run`] with the wizard's answers read from `input` — a provided argument
/// skips its prompt, a missing one is asked. Integration tests drive this with
/// piped answers.
pub fn run_with_input(
    args: &[&str],
    repo: &Path,
    input: &mut dyn std::io::BufRead,
) -> Result<(i32, String), String> {
    match args {
        ["setup", rest @ ..] => {
            mr_setup(repo, crate::flag_value(rest, "--as"), crate::flag_value(rest, "--key"), input)
        }
        ["commit", rest @ ..] => {
            mr_commit(repo, crate::flag_value(rest, "-m"), crate::flag_value(rest, "--as"), input)
        }
        ["confirm", rest @ ..] => {
            let (level, tip) = match split_positionals(rest).as_slice() {
                [] => (None, None),
                [level] => (Some(*level), None),
                [level, tip] => (Some(*level), Some(*tip)),
                _ => return Err(crate::USAGE.to_string()),
            };
            mr_confirm(
                repo,
                level,
                tip,
                crate::flag_value(rest, "--as"),
                crate::flag_value(rest, "--target"),
                input,
            )
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
        pub_path_for, short, signature_authorised, signed_note_body, signer_program, touch_hint,
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
    fn touch_hint_names_the_signer_and_the_action() {
        let hint = touch_hint("vu", "approve the merge of wu/x/y into epic/x (tip abc1234) as vu");
        assert!(hint.contains("vu"));
        assert!(hint.to_lowercase().contains("touch"));
        assert!(hint.contains("approve the merge of wu/x/y into epic/x"));
    }

    #[test]
    fn commit_reason_names_the_title_branch_and_signer() {
        let reason = super::commit_reason("docs: x\n\nlong body\n", "epic/mrsig", "vuluu");
        assert_eq!(reason, "commit \"docs: x\" on epic/mrsig as vuluu");
    }

    #[test]
    fn confirm_reason_names_both_branches_and_the_tip() {
        let tip = "0a77604aaaaaaaaa";
        let reason = super::confirm_reason("wu/mrsig/ux", "epic/mrsig", tip, "vuluu");
        assert_eq!(
            reason,
            format!(
                "approve the merge of wu/mrsig/ux into epic/mrsig (tip {}) as vuluu",
                short(tip)
            )
        );
    }

    #[test]
    fn level_is_inferred_from_the_branch_name_by_a_fixed_regex() {
        assert_eq!(super::infer_level("wu/mrsig/ux"), Some("wu"));
        assert_eq!(super::infer_level("epic/mrsig"), Some("epic"));
        assert_eq!(super::infer_level("main"), None);
        assert_eq!(super::infer_level("feature/wu"), None);
        assert_eq!(super::infer_level("HEAD"), None, "detached HEAD asks the user");
    }

    #[test]
    fn se_check_reports_parse_to_biometry_or_a_reason() {
        assert_eq!(super::parse_se_check("enclave=yes biometry=yes\n"), Ok(true));
        assert_eq!(super::parse_se_check("enclave=yes biometry=no\n"), Ok(false));
        assert!(super::parse_se_check("enclave=no biometry=no\n").is_err());
        assert!(super::parse_se_check("garbage").is_err());
    }

    #[test]
    fn setup_menu_labels_follow_the_capability_probe() {
        let touch_id = super::setup_methods(&Ok(true));
        assert_eq!(touch_id[0].label, "Touch ID (Secure Enclave)");
        assert!(touch_id[0].unavailable.is_none());

        let password = super::setup_methods(&Ok(false));
        assert_eq!(password[0].label, "System password (Secure Enclave)");
        assert!(password[0].note.contains("login password"));

        let absent = super::setup_methods(&Err("no swiftc".to_string()));
        assert_eq!(absent[0].unavailable.as_deref(), Some("no swiftc"));

        for methods in [&touch_id, &password, &absent] {
            assert_eq!(methods.len(), 5);
            assert!(
                methods[1..].iter().all(|m| m.unavailable.is_none()),
                "only the Enclave row is capability-gated"
            );
        }
    }

    #[test]
    fn positionals_split_off_flag_value_pairs() {
        assert_eq!(
            super::split_positionals(&["wu", "HEAD", "--as", "vu", "--target", "main"]),
            vec!["wu", "HEAD"]
        );
        assert_eq!(super::split_positionals(&["--as", "vu"]), Vec::<&str>::new());
        assert_eq!(super::split_positionals(&["epic"]), vec!["epic"]);
    }

    #[test]
    fn allowed_entries_accumulate_across_methods() {
        let after_key = append_key_line("username: vu\n", &key_line("vu", "ssh-ed25519 AAAA"));
        let after_email = append_key_line(&after_key, "email: vu@x.co");
        assert_eq!(after_email, "username: vu\nkey: vu ssh-ed25519 AAAA\nemail: vu@x.co\n");
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
