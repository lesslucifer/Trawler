//! Plain-stdin wizard primitives for `cargo xtask mr`: a provided argument
//! skips its prompt, a missing one is asked here. EOF (non-interactive with an
//! input missing) is a usage error naming the missing input.
use std::io::BufRead;

/// One entry of a numbered menu. `unavailable` carries the one-line reason a
/// method can't work on this device; such an entry is shown but not selectable.
pub struct MenuOption {
    pub id: String,
    pub label: String,
    pub note: String,
    pub unavailable: Option<String>,
}

/// Ask until a non-empty line arrives; EOF names the missing input.
#[allow(clippy::print_stderr)]
pub fn ask(input: &mut dyn BufRead, prompt: &str) -> Result<String, String> {
    loop {
        eprintln!("{prompt}:");
        let line = read_line(input, prompt)?;
        if !line.is_empty() {
            return Ok(line);
        }
    }
}

/// A yes/no step; empty answer means no (the skippable-step default).
#[allow(clippy::print_stderr)]
pub fn confirm(input: &mut dyn BufRead, prompt: &str) -> Result<bool, String> {
    eprintln!("{prompt} [y/N]:");
    let line = read_line(input, prompt)?;
    Ok(matches!(line.to_lowercase().as_str(), "y" | "yes"))
}

/// Show the menu and ask until an available option is picked (by number or id).
#[allow(clippy::print_stderr)]
pub fn choose(
    input: &mut dyn BufRead,
    title: &str,
    options: &[MenuOption],
) -> Result<usize, String> {
    eprintln!("{title}");
    for (i, option) in options.iter().enumerate() {
        match (&option.unavailable, option.note.is_empty()) {
            (Some(reason), _) => eprintln!("  -  {} (unavailable: {reason})", option.label),
            (None, true) => eprintln!("  {}) {}", i + 1, option.label),
            (None, false) => eprintln!("  {}) {} - {}", i + 1, option.label, option.note),
        }
    }
    loop {
        let answer = ask(input, "pick one (number or name)")?;
        if let Some(i) = parse_choice(&answer, options) {
            return Ok(i);
        }
        eprintln!("no such option: {answer}");
    }
}

/// A menu answer — the 1-based number or the option id — for available options.
pub fn parse_choice(answer: &str, options: &[MenuOption]) -> Option<usize> {
    let index = answer
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .filter(|i| *i < options.len())
        .or_else(|| options.iter().position(|o| o.id == answer))?;
    options[index].unavailable.is_none().then_some(index)
}

fn read_line(input: &mut dyn BufRead, prompt: &str) -> Result<String, String> {
    let mut line = String::new();
    let read = input.read_line(&mut line).map_err(|e| format!("cannot read input: {e}"))?;
    if read == 0 {
        return Err(format!(
            "missing input: {prompt} (pass it as an argument when non-interactive)"
        ));
    }
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{ask, choose, confirm, parse_choice, MenuOption};

    fn options() -> Vec<MenuOption> {
        vec![
            MenuOption {
                id: "touch-id".into(),
                label: "Touch ID".into(),
                note: "asks for Touch ID".into(),
                unavailable: None,
            },
            MenuOption {
                id: "software".into(),
                label: "software key".into(),
                note: String::new(),
                unavailable: None,
            },
            MenuOption {
                id: "fido2".into(),
                label: "FIDO2".into(),
                note: String::new(),
                unavailable: Some("no key plugged".into()),
            },
        ]
    }

    #[test]
    fn choices_parse_by_number_or_id_and_skip_unavailable() {
        let opts = options();
        assert_eq!(parse_choice("1", &opts), Some(0));
        assert_eq!(parse_choice("software", &opts), Some(1));
        assert_eq!(parse_choice("3", &opts), None, "unavailable is not selectable");
        assert_eq!(parse_choice("fido2", &opts), None);
        assert_eq!(parse_choice("0", &opts), None);
        assert_eq!(parse_choice("4", &opts), None);
        assert_eq!(parse_choice("nonsense", &opts), None);
    }

    #[test]
    fn ask_skips_blank_lines_and_names_the_missing_input_on_eof() {
        let mut input = Cursor::new(b"\n\nvuluu\n".to_vec());
        assert_eq!(ask(&mut input, "identity").unwrap(), "vuluu");

        let err = ask(&mut Cursor::new(Vec::new()), "identity").unwrap_err();
        assert!(err.contains("missing input: identity"), "{err}");
    }

    #[test]
    fn confirm_defaults_to_no_and_accepts_yes() {
        assert!(confirm(&mut Cursor::new(b"y\n".to_vec()), "passphrase?").unwrap());
        assert!(confirm(&mut Cursor::new(b"YES\n".to_vec()), "passphrase?").unwrap());
        assert!(!confirm(&mut Cursor::new(b"\n".to_vec()), "passphrase?").unwrap());
        assert!(!confirm(&mut Cursor::new(b"n\n".to_vec()), "passphrase?").unwrap());
        assert!(confirm(&mut Cursor::new(Vec::new()), "passphrase?").is_err());
    }

    #[test]
    fn choose_reprompts_until_a_valid_pick() {
        let opts = options();
        let mut input = Cursor::new(b"9\ntouch-id\n".to_vec());
        assert_eq!(choose(&mut input, "method?", &opts).unwrap(), 0);
    }
}
