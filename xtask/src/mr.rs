use std::path::Path;

use crate::git::{git, note, write_note};

pub const TRAILER_KEY: &str = "Reviewed-By";

pub const NOT_AUTHORISED: &str = "\
A HUMAN must review this and confirm. The AI agent MUST NOT self-authorise.
             After the user has reviewed and said yes:
               cargo xtask mr commit -m \"<msg>\" --as <email|username>      # doc commit
               cargo xtask mr confirm <wu|epic> <tip> --as <email|username>  # WU / epic
             The identity must be listed in .mr-allowed.";

/// A `<tier>: <value>` line from `.mr-allowed`. Only `email`/`username` this epic;
/// E-MRSIG adds `key`.
pub struct Allowed {
    pub tier: String,
    pub value: String,
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
    allowed.iter().find(|a| a.value == identity).map(|a| a.tier.clone())
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

fn mr_commit(repo: &Path, message: &str, identity: &str) -> Result<(i32, String), String> {
    if message.trim().is_empty() {
        return Ok((1, "the commit message (-m) is empty".to_string()));
    }
    let Some(tier) = match_tier(&read_allowed(repo), identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let body = format!("{}\n\n{TRAILER_KEY}: {tier} {identity}\n", message.trim_end());
    git(repo, &["commit", "-m", &body, "--cleanup=verbatim"])?;
    Ok((0, format!("committed with {TRAILER_KEY}: {tier} {identity}")))
}

fn mr_confirm(
    repo: &Path,
    level: &str,
    tip_ref: &str,
    identity: &str,
) -> Result<(i32, String), String> {
    let level = match level {
        "wu" => "working-unit",
        "epic" => "epic",
        _ => return Err(crate::USAGE.to_string()),
    };
    let Some(tier) = match_tier(&read_allowed(repo), identity) else {
        return Ok((1, format!("{identity} is not listed in .mr-allowed\n{NOT_AUTHORISED}")));
    };
    let tip = git(repo, &["rev-parse", tip_ref])?;
    write_note(repo, "mr", &tip, &note_body(level, &tip, &tier, identity))?;
    Ok((0, format!("mr/v1 note written on {} ({tier} {identity})", crate::gate::short(&tip))))
}

/// Is there a valid `mr/v1` note on `tip` with a currently-listed signer?
pub fn tip_authorised(repo: &Path, tip: &str) -> bool {
    let Some(raw) = note(repo, "mr", tip) else { return false };
    let Ok(mr) = serde_yaml::from_str::<crate::notes::MrNote>(&raw) else { return false };
    mr.schema == "mr/v1" && match_tier(&read_allowed(repo), &mr.signer).is_some()
}

/// Does `message` carry a `Reviewed-By` trailer naming a currently-listed
/// identity? The single trailer-auth predicate for both the commit-msg hook and
/// the epic walk. E-MRSIG hardens this one function.
pub fn message_authorised(repo: &Path, message: &str) -> bool {
    trailer_of(message).is_some_and(|(_, id)| match_tier(&read_allowed(repo), &id).is_some())
}

/// The `commit-msg` hook: if the staged diff touches docs, the message must
/// carry a `Reviewed-By` trailer naming a listed identity, else the commit is
/// refused (exit 1).
pub fn commit_msg_hook(repo: &Path, msg_file: &str) -> Result<(i32, String), String> {
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

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["commit", rest @ ..] => {
            let message = crate::flag_value(rest, "-m").ok_or(crate::USAGE.to_string())?;
            let identity = crate::flag_value(rest, "--as").ok_or(crate::USAGE.to_string())?;
            mr_commit(repo, &message, &identity)
        }
        ["confirm", level, tip, rest @ ..] => {
            let identity = crate::flag_value(rest, "--as").ok_or(crate::USAGE.to_string())?;
            mr_confirm(repo, level, tip, &identity)
        }
        _ => Err(crate::USAGE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{match_tier, trailer_of, Allowed};

    fn allowed() -> Vec<Allowed> {
        vec![
            Allowed { tier: "email".into(), value: "a@b.co".into() },
            Allowed { tier: "username".into(), value: "vu".into() },
        ]
    }

    #[test]
    fn matches_a_listed_identity_to_its_tier() {
        assert_eq!(match_tier(&allowed(), "a@b.co").as_deref(), Some("email"));
        assert_eq!(match_tier(&allowed(), "vu").as_deref(), Some("username"));
        assert_eq!(match_tier(&allowed(), "someone@else.co"), None);
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
