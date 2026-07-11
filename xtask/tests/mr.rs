//! E-NMWF.W1 — the authoriser record & writer commands (`cargo xtask mr ...`).
//! E-MRSIG.W1 — the key-backed signature-verify predicates.
#![allow(clippy::unwrap_used)] // test code - panicking on setup failure is the assertion

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;
use xtask::mr;

struct Repo {
    _dir: TempDir,
    path: PathBuf,
}

impl Repo {
    fn new(allowed: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Self { _dir: dir, path };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "gate@test"]);
        repo.git(&["config", "user.name", "gate"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.write(".mr-allowed", allowed);
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "init"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        xtask::git::git(&self.path, args).unwrap_or_else(|e| panic!("{e}"))
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.path.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    fn p(&self) -> &Path {
        &self.path
    }

    /// Generate a software ed25519 key under the repo; return its public-key line.
    fn keygen(&self, name: &str) -> String {
        let key = self.path.join(name);
        assert!(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", name, "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        std::fs::read_to_string(self.path.join(format!("{name}.pub"))).unwrap().trim().to_string()
    }

    fn ssh_sign(&self, key_name: &str, namespace: &str, data: &[u8]) -> String {
        let mut child = Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", namespace, "-f"])
            .arg(self.path.join(key_name))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(data).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "ssh-keygen sign failed");
        String::from_utf8(out.stdout).unwrap()
    }
}

const ALLOWED: &str = "email: a@b.co\nusername: vu\n";

#[test]
fn confirm_writes_a_well_formed_note_for_a_listed_identity() {
    let repo = Repo::new(ALLOWED);
    let (code, _) = mr::run(&["confirm", "epic", "HEAD", "--as", "a@b.co"], repo.p()).unwrap();
    assert_eq!(code, 0);

    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("schema: mr/v1"), "{body}");
    assert!(body.contains("level: epic"), "{body}");
    assert!(body.contains("tier: email"), "{body}");
    assert!(body.contains("signer: a@b.co"), "{body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn confirm_maps_wu_level_and_records_the_username_tier() {
    let repo = Repo::new(ALLOWED);
    let (code, _) = mr::run(&["confirm", "wu", "HEAD", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0);
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("level: working-unit"), "{body}");
    assert!(body.contains("tier: username"), "{body}");
}

#[test]
fn unlisted_identity_is_refused_and_writes_nothing() {
    let repo = Repo::new(ALLOWED);
    let (code, out) =
        mr::run(&["confirm", "epic", "HEAD", "--as", "ghost@x.co"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("not listed in .mr-allowed"));
    assert!(out.contains("MUST NOT self-authorise"));
    assert!(!mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn commit_appends_the_reviewed_by_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, _) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0);

    let body = repo.git(&["log", "-1", "--format=%B"]);
    assert_eq!(mr::trailer_of(&body), Some(("username".into(), "vu".into())));
}

#[test]
fn commit_with_an_empty_message_is_refused() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "   ", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("empty"));
}

fn write_msg(repo: &Repo, body: &str) -> std::path::PathBuf {
    let f = repo.path.join("COMMIT_MSG");
    std::fs::write(&f, body).unwrap();
    f
}

#[test]
fn commit_msg_hook_ignores_a_non_doc_commit() {
    let repo = Repo::new(ALLOWED);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "feat: no docs here\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn commit_msg_hook_refuses_a_doc_commit_without_a_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n");
    let (code, out) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("no valid Reviewed-By trailer"));
    assert!(out.contains("MUST NOT self-authorise"));
}

#[test]
fn commit_msg_hook_accepts_a_doc_commit_with_a_listed_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n\nReviewed-By: email a@b.co\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn commit_msg_hook_refuses_a_doc_commit_with_an_unlisted_trailer() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let msg = write_msg(&repo, "docs: change\n\nReviewed-By: email ghost@x.co\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn commit_msg_hook_skips_merge_commits() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let merge_head = repo.path.join(".git/MERGE_HEAD");
    std::fs::create_dir_all(merge_head.parent().unwrap()).unwrap();
    std::fs::write(&merge_head, format!("{head}\n")).unwrap();

    let msg = write_msg(&repo, "Merge branch x\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 0, "a merge commit needs no trailer of its own");
}

#[test]
fn commit_msg_hook_sees_a_doc_renamed_out_of_docs() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/movable.html", "content that is long enough to be a similar rename\n");
    repo.write("src/keep.rs", "// keep\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add doc"]);
    repo.git(&["mv", "docs/movable.html", "src/moved.rs"]);
    let msg = write_msg(&repo, "move doc out, no trailer\n");
    let (code, _) = mr::commit_msg_hook(repo.p(), msg.to_str().unwrap()).unwrap();
    assert_eq!(code, 1, "a rename out of docs/ must still require a trailer");
}

#[test]
fn commit_with_unlisted_identity_is_refused() {
    let repo = Repo::new(ALLOWED);
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) =
        mr::run(&["commit", "-m", "docs: change", "--as", "ghost"], repo.p()).unwrap();
    assert_eq!(code, 1);
    assert!(out.contains("not listed in .mr-allowed"));
}

// ── E-MRSIG.W1 — key-backed signature verification ──────────────────────────

/// A repo with one code commit past `base`, plus a `key` entry for `signer`.
/// Returns (base, tip, pubkey-line).
fn repo_with_key(signer: &str) -> (Repo, String, String, String) {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");
    repo.write(".mr-allowed", &format!("email: a@b.co\nkey: {signer} {pubkey}\n"));
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "add key"]);
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    (repo, base, tip, pubkey)
}

fn write_signed_note(repo: &Repo, level: &str, base: &str, tip: &str, signer: &str, sig: &str) {
    let body = format!(
        "schema: mr/v1\nlevel: {level}\ntip: {tip}\nbase: {base}\ntier: key\nsigner: {signer}\nsig: |\n{}\n",
        sig.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
    );
    xtask::git::write_note(repo.p(), "mr", tip, &body).unwrap();
}

#[test]
fn tip_authorised_accepts_a_correctly_signed_note() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    assert!(mr::tip_authorised(repo.p(), &tip));
}

#[test]
fn tip_authorised_rejects_a_note_whose_diff_changed() {
    let (repo, base, tip, _) = repo_with_key("vu");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("id", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    // Amend the tip → the signed change-hash no longer matches.
    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    write_signed_note(&repo, "epic", &base, &amended, "vu", &sig);
    assert!(!mr::tip_authorised(repo.p(), &amended), "amend must invalidate the signature");
}

#[test]
fn tip_authorised_rejects_a_signature_from_an_unlisted_key() {
    let (repo, base, tip, _) = repo_with_key("vu");
    repo.keygen("other");
    let hash = xtask::git::diff_sha256(repo.p(), &base, &tip).unwrap();
    let sig = repo.ssh_sign("other", mr::NOTE_NAMESPACE, hash.as_bytes());
    write_signed_note(&repo, "epic", &base, &tip, "vu", &sig);
    assert!(!mr::tip_authorised(repo.p(), &tip));
}

#[test]
fn tip_authorised_rejects_a_bare_note_from_an_unlisted_signer() {
    // No sig/base falls back to identity-match (E-NMWF coexistence); an unlisted
    // signer still fails. A listed signer's bare note authorises — the key is an
    // added option in W1, not yet a mandate (mandate lands with W3 enforcement).
    let (repo, _base, tip, _) = repo_with_key("vu");
    let body = "schema: mr/v1\nlevel: epic\ntip: x\ntier: email\nsigner: ghost@x.co\n";
    xtask::git::write_note(repo.p(), "mr", &tip, body).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &tip));
}

#[test]
fn commit_signed_accepts_a_commit_signed_by_a_listed_key() {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");
    repo.write(".mr-allowed", &format!("key: vu {pubkey}\n"));
    repo.git(&["config", "gpg.format", "ssh"]);
    repo.git(&["config", "user.signingkey", repo.p().join("id").to_str().unwrap()]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-S", "-m", "signed"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(mr::commit_signed(repo.p(), &tip));
}

#[test]
fn commit_signed_rejects_an_unsigned_commit() {
    let (repo, _base, tip, _) = repo_with_key("vu");
    assert!(!mr::commit_signed(repo.p(), &tip), "the tip commit was never signed");
}

#[test]
fn commit_signed_rejects_a_commit_signed_by_an_unlisted_key() {
    let repo = Repo::new("email: a@b.co\n");
    repo.keygen("id");
    let other = repo.keygen("other");
    repo.write(".mr-allowed", &format!("key: vu {other}\n"));
    repo.git(&["config", "gpg.format", "ssh"]);
    repo.git(&["config", "user.signingkey", repo.p().join("id").to_str().unwrap()]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-S", "-m", "signed by id, listed is other"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(!mr::commit_signed(repo.p(), &tip));
}

// ── E-MRSIG.W2 — setup + signing UX ──────────────────────────────────────────

/// Run `mr setup --as <id> --key <path>` over a software key; return its pubkey.
fn setup_with_software_key(repo: &Repo, identity: &str) -> String {
    let pubkey = repo.keygen("id");
    let key_path = repo.p().join("id");
    let (code, _) =
        mr::run(&["setup", "--as", identity, "--key", key_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    pubkey
}

#[test]
fn setup_registers_the_key_and_configures_signing() {
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = setup_with_software_key(&repo, "vu");

    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vu {pubkey}")), "{allowed}");
    assert!(allowed.contains("email: a@b.co"), "existing entries are preserved: {allowed}");
    assert_eq!(repo.git(&["config", "--get", "gpg.format"]), "ssh");
    assert_eq!(
        repo.git(&["config", "--get", "user.signingkey"]),
        repo.p().join("id").to_str().unwrap()
    );

    // Idempotent: re-running setup against the same key does not duplicate it.
    let key_path = repo.p().join("id");
    mr::run(&["setup", "--as", "vu", "--key", key_path.to_str().unwrap()], repo.p()).unwrap();
    let again = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert_eq!(again.matches("key: vu").count(), 1, "{again}");
}

#[test]
fn confirm_signs_a_note_that_the_w1_predicate_accepts() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let (code, out) =
        mr::run(&["confirm", "wu", &tip, "--as", "vu", "--target", &base], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");

    let body = repo.git(&["notes", "--ref=mr", "show", &tip]);
    assert!(body.contains("tier: key"), "{body}");
    assert!(body.contains("sig: |"), "the signature rides in the note: {body}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the signed note authorises the tip");
}

#[test]
fn confirm_signature_breaks_when_the_tip_is_amended() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    mr::run(&["confirm", "epic", &tip, "--as", "vu", "--target", &base], repo.p()).unwrap();
    assert!(mr::tip_authorised(repo.p(), &tip));

    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    let old_note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    xtask::git::write_note(repo.p(), "mr", &amended, &old_note).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &amended), "amending invalidates the signature");
}

#[test]
fn confirm_falls_back_to_a_plain_note_for_an_identity_only_signer() {
    let repo = Repo::new("email: a@b.co\nusername: vu\n");
    // No key entry and no --target: the plain-note path (E-NMWF coexistence).
    let (code, _) = mr::run(&["confirm", "epic", "HEAD", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0);
    let body = repo.git(&["notes", "--ref=mr", "show", "HEAD"]);
    assert!(body.contains("tier: username"), "{body}");
    assert!(!body.contains("sig:"), "an identity-only note carries no signature: {body}");
    assert!(mr::tip_authorised(repo.p(), "HEAD"));
}

#[test]
fn mr_commit_signs_a_doc_commit_a_listed_key_verifies() {
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);

    let (code, out) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert_eq!(
        mr::trailer_of(&repo.git(&["log", "-1", "--format=%B", &tip])),
        Some(("key".into(), "vu".into())),
        "the trailer still rides along for the commit-msg hook"
    );
    assert!(mr::commit_signed(repo.p(), &tip), "the doc commit is natively SSH-signed");
}

#[test]
fn setup_accepts_a_public_key_path_for_a_secure_enclave_key() {
    // A Secure-Enclave / Touch-ID key (via an ssh-agent like Secretive) exposes
    // only a public key on disk. Passing the .pub directly must register it and
    // point user.signingkey at the .pub (the agent holds the private half) -
    // not at a nonexistent <path>.pub.pub.
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("se");
    std::fs::remove_file(repo.p().join("se")).unwrap();
    let pub_path = repo.p().join("se.pub");

    let (code, _) =
        mr::run(&["setup", "--as", "vuluu", "--key", pub_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vuluu {pubkey}")), "{allowed}");
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), pub_path.to_str().unwrap());
}

#[test]
fn confirm_signs_when_user_signingkey_is_a_public_key_path() {
    // user.signingkey set to a .pub (the Secure-Enclave shape) must still drive
    // `ssh-keygen -Y sign -f <pub>` to a note W1 accepts. Hermetic proxy for the
    // agent: the private half sits on disk next to the .pub (no agent/Touch ID
    // to reproduce), so this exercises the .pub signing-key code path end to end.
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("se");
    let pub_path = repo.p().join("se.pub");
    mr::run(&["setup", "--as", "vuluu", "--key", pub_path.to_str().unwrap()], repo.p()).unwrap();
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), pub_path.to_str().unwrap());
    assert!(pubkey.starts_with("ssh-ed25519"));

    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let (code, out) =
        mr::run(&["confirm", "wu", &tip, "--as", "vuluu", "--target", &base], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(repo.git(&["notes", "--ref=mr", "show", &tip]).contains("sig: |"));
    assert!(mr::tip_authorised(repo.p(), &tip), "signed via a .pub signing key");
}

// ── E-MRSIG.W7 — native Secure-Enclave signer ────────────────────────────────

/// Compile the bundled Secure-Enclave signer from its embedded source. `None`
/// skips the test where `swiftc` is absent (non-mac CI); a failed compile of
/// present source is a real failure. The Enclave paths themselves (keygen +
/// Touch-ID sign) need hardware + a human and stay a deployment waiver; these
/// tests prove the helper builds and its ssh-keygen passthrough delegates
/// correctly for every non-Enclave key.
fn compiled_se_helper(dir: &Path) -> Option<PathBuf> {
    let src = dir.join("trawler-signer.swift");
    let bin = dir.join("trawler-signer");
    std::fs::write(&src, include_str!("../src/trawler_signer.swift")).unwrap();
    let out = Command::new("swiftc").arg("-O").arg(&src).arg("-o").arg(&bin).output().ok()?;
    assert!(out.status.success(), "swiftc failed: {}", String::from_utf8_lossy(&out.stderr));
    Some(bin)
}

#[test]
fn se_helper_passes_software_keys_through_to_ssh_keygen() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    let pubkey = repo.keygen("id");

    let mut child = Command::new(&helper)
        .args(["-Y", "sign", "-n", "trawler-mr", "-f"])
        .arg(repo.p().join("id"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"payload").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "helper sign failed");
    let sig = String::from_utf8(out.stdout).unwrap();
    assert!(sig.starts_with("-----BEGIN SSH SIGNATURE-----"), "{sig}");
    let signers = format!("vu {pubkey}\n");
    assert!(
        xtask::git::ssh_verify("vu", &signers, "trawler-mr", b"payload", &sig),
        "a passthrough signature verifies with stock ssh-keygen"
    );
}

#[test]
fn se_helper_signs_and_verifies_git_commits_via_gpg_ssh_program() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.git(&["config", "gpg.ssh.program", helper.to_str().unwrap()]);

    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "docs: change", "--as", "vu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(
        mr::commit_signed(repo.p(), &tip),
        "git signs and verifies through the helper (gpg.ssh.program both ways)"
    );
}

#[test]
fn confirm_signs_through_the_configured_signer_program() {
    let tools = TempDir::new().unwrap();
    let Some(helper) = compiled_se_helper(tools.path()) else { return };
    let repo = Repo::new("email: a@b.co\n");
    setup_with_software_key(&repo, "vu");
    repo.git(&["config", "gpg.ssh.program", helper.to_str().unwrap()]);

    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);

    let (code, out) =
        mr::run(&["confirm", "wu", &tip, "--as", "vu", "--target", &base], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the note signed via the helper authorises");
}

// ── E-MRSIG e2e — the full signed manual-review flow ─────────────────────────

/// End-to-end over one temp repo: `mr setup` registers a key + signing config →
/// a doc commit signed via `mr commit` verifies → a WU/epic note signed via
/// `mr confirm` authorises → tamper (amend) breaks the signature → an unsigned
/// commit / unlisted signer is rejected. Uses a software key: the hardware-touch
/// half (FIDO2 / Secure-Enclave Touch ID) is a deployment property not
/// reproducible in a hermetic run — this proves the crypto half end to end.
#[test]
fn e2e_signed_manual_review_flow_with_a_software_key() {
    let repo = Repo::new("email: a@b.co\n");

    // 1. setup: register the key + configure SSH signing in one step.
    let pubkey = repo.keygen("id");
    let key_path = repo.p().join("id");
    let (code, _) =
        mr::run(&["setup", "--as", "vuluu", "--key", key_path.to_str().unwrap()], repo.p())
            .unwrap();
    assert_eq!(code, 0);
    let allowed = std::fs::read_to_string(repo.p().join(".mr-allowed")).unwrap();
    assert!(allowed.contains(&format!("key: vuluu {pubkey}")), "{allowed}");
    assert_eq!(repo.git(&["config", "--get", "gpg.format"]), "ssh");
    assert_eq!(repo.git(&["config", "--get", "user.signingkey"]), key_path.to_str().unwrap());

    // 2. a doc commit signed via `mr commit` verifies (the unsigned negative is step 5).
    repo.write("docs/x.html", "x");
    repo.git(&["add", "-A"]);
    let (code, out) = mr::run(&["commit", "-m", "docs: x", "--as", "vuluu"], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    let doc_tip = repo.git(&["rev-parse", "HEAD"]);
    assert!(mr::commit_signed(repo.p(), &doc_tip), "the signed doc commit verifies");
    assert_eq!(
        mr::trailer_of(&repo.git(&["log", "-1", "--format=%B", &doc_tip])),
        Some(("key".into(), "vuluu".into())),
        "the trailer rides along for the commit-msg hook"
    );

    // 3. a WU/epic tip confirmed via `mr confirm` authorises over the signed range.
    let base = repo.git(&["rev-parse", "HEAD"]);
    repo.write("src/lib.rs", "pub fn f() {}\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "the change"]);
    let tip = repo.git(&["rev-parse", "HEAD"]);
    let (code, out) =
        mr::run(&["confirm", "epic", &tip, "--as", "vuluu", "--target", &base], repo.p()).unwrap();
    assert_eq!(code, 0, "{out}");
    assert!(mr::tip_authorised(repo.p(), &tip), "the signed note authorises the tip");

    // 4. tamper-evidence: amending the tip breaks the signed change-hash.
    repo.write("src/lib.rs", "pub fn f() { let _ = 1; }\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--amend", "--no-edit"]);
    let amended = repo.git(&["rev-parse", "HEAD"]);
    let note = repo.git(&["notes", "--ref=mr", "show", &tip]);
    xtask::git::write_note(repo.p(), "mr", &amended, &note).unwrap();
    assert!(!mr::tip_authorised(repo.p(), &amended), "amend invalidates the signature");

    // 5. negatives: an unsigned commit and a bare note from an unlisted signer fail.
    let root = repo.git(&["rev-list", "--max-parents=0", "HEAD"]);
    assert!(!mr::commit_signed(repo.p(), &root), "the unsigned init commit is not signed");
    let forged = "schema: mr/v1\nlevel: epic\ntip: x\ntier: email\nsigner: ghost@x.co\n";
    xtask::git::write_note(repo.p(), "mr", &root, forged).unwrap();
    assert!(
        !mr::tip_authorised(repo.p(), &root),
        "an unlisted signer's bare note never authorises"
    );
}
