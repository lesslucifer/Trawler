//! A pasted `curl` command read back as one request-only flow — the inverse of
//! the app's copy-as-curl. The parser accepts what a browser's "Copy as cURL"
//! and Trawler's own serializer emit, expands the flags that *are* headers
//! spelled differently, consumes the flags that never reach the wire, and
//! refuses everything else naming the token it did not understand.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;

use crate::flow::{BodyRec, Flow, Header, RequestRec};

const COMPRESSED_ACCEPT_ENCODING: &str = "deflate, gzip, br, zstd";

/// Why a command was refused. Every variant names the token at fault, because
/// the reason is rendered verbatim on the paste target and a user edits the
/// command rather than retyping it.
#[derive(Debug, PartialEq, Eq)]
pub enum CurlError {
    Empty,
    NotCurl(String),
    NotUtf8(String),
    UnterminatedQuote,
    UnsupportedFlag(String),
    MissingValue(String),
    NoUrl,
    SecondUrl(String),
    NotAbsoluteUrl(String),
    ReadsFile(String),
    UserWithoutPassword,
    HeaderWithoutColon(String),
    HeaderWithoutName(String),
}

impl std::fmt::Display for CurlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurlError::Empty => write!(f, "empty command"),
            CurlError::NotCurl(word) => write!(f, "not a curl command: starts with {word:?}"),
            CurlError::NotUtf8(what) => write!(f, "{what} is not UTF-8"),
            CurlError::UnterminatedQuote => write!(f, "unterminated quote"),
            CurlError::UnsupportedFlag(flag) => write!(f, "unsupported flag {flag}"),
            CurlError::MissingValue(flag) => write!(f, "{flag} needs a value"),
            CurlError::NoUrl => write!(f, "no URL"),
            CurlError::SecondUrl(url) => {
                write!(f, "a second URL: {url:?} — one request per command")
            }
            CurlError::NotAbsoluteUrl(url) => {
                write!(f, "URL {url:?} is not an absolute http:// or https:// URL")
            }
            CurlError::ReadsFile(argument) => {
                write!(f, "{argument} reads a file, whose bytes are not in the command")
            }
            CurlError::UserWithoutPassword => {
                write!(f, "-u has no password — curl would prompt for one")
            }
            CurlError::HeaderWithoutColon(header) => write!(f, "header {header:?} has no colon"),
            CurlError::HeaderWithoutName(header) => write!(f, "header {header:?} has no name"),
        }
    }
}

/// One command in, one flow out: the request curl would have sent, marked as
/// imported, with no response because nothing was sent.
pub fn flow_from_curl(command: &[u8]) -> Result<Flow, CurlError> {
    let words = tokenize(command)?;
    let mut words = words.into_iter();
    let program = words.next().ok_or(CurlError::Empty)?;
    let program = utf8(program, "the command")?;
    if !is_curl(&program) {
        return Err(CurlError::NotCurl(program));
    }
    let parsed = Command::walk(words)?;
    Ok(Flow::new(parsed.into_request()?).imported())
}

fn is_curl(program: &str) -> bool {
    let name = program.rsplit('/').next().unwrap_or(program);
    name == "curl" || name == "curl.exe"
}

fn utf8(bytes: Vec<u8>, what: &str) -> Result<String, CurlError> {
    String::from_utf8(bytes).map_err(|_| CurlError::NotUtf8(what.to_owned()))
}

/// The flags this parser knows, and what each does to the request. The two
/// tables in the WU plan — accepted and ignored — are exactly the arms of
/// [`Flag::lookup`]; a name it does not return is refused, never guessed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flag {
    Method,
    Url,
    Header,
    Data { raw: bool },
    Cookie,
    User,
    Compressed,
    Ignored { takes_value: bool },
}

impl Flag {
    fn lookup(name: &str) -> Option<Self> {
        Some(match name {
            "-X" | "--request" => Flag::Method,
            "--url" => Flag::Url,
            "-H" | "--header" => Flag::Header,
            "--data-raw" => Flag::Data { raw: true },
            "-d" | "--data" | "--data-binary" | "--data-ascii" => Flag::Data { raw: false },
            "-b" | "--cookie" => Flag::Cookie,
            "-u" | "--user" => Flag::User,
            "--compressed" => Flag::Compressed,
            "-s"
            | "--silent"
            | "-S"
            | "--show-error"
            | "-k"
            | "--insecure"
            | "-i"
            | "--include"
            | "-v"
            | "--verbose"
            | "-L"
            | "--location"
            | "-f"
            | "--fail"
            | "-#"
            | "--progress-bar"
            | "--no-progress-meter"
            | "-g"
            | "--globoff"
            | "-N"
            | "--no-buffer"
            | "-0"
            | "--http1.0"
            | "--http1.1"
            | "--http2"
            | "--http2-prior-knowledge"
            | "--http3"
            | "--tlsv1.2"
            | "--tlsv1.3"
            | "-4"
            | "--ipv4"
            | "-6"
            | "--ipv6"
            | "--proxy-insecure"
            | "--no-keepalive" => Flag::Ignored { takes_value: false },
            "-o" | "--output" | "-w" | "--write-out" | "-m" | "--max-time"
            | "--connect-timeout" | "--retry" | "--retry-delay" | "-x" | "--proxy" | "--cacert"
            | "--capath" | "--cert" | "--key" | "--resolve" | "--connect-to" | "--max-redirs"
            | "--limit-rate" | "--interface" => Flag::Ignored { takes_value: true },
            _ => return None,
        })
    }

    fn takes_value(self) -> bool {
        !matches!(self, Flag::Compressed | Flag::Ignored { takes_value: false })
    }
}

#[derive(Default)]
struct Command {
    method: Option<String>,
    url: Option<String>,
    headers: Vec<Header>,
    cookies: Vec<String>,
    data: Vec<Vec<u8>>,
}

impl Command {
    fn walk(mut words: impl Iterator<Item = Vec<u8>>) -> Result<Self, CurlError> {
        let mut command = Command::default();
        let mut flags_ended = false;
        while let Some(word) = words.next() {
            if flags_ended || !word.starts_with(b"-") || word.len() == 1 {
                command.set_url(word)?;
            } else if word == b"--" {
                flags_ended = true;
            } else if word.starts_with(b"--") {
                let name = utf8(word, "a flag")?;
                let flag =
                    Flag::lookup(&name).ok_or_else(|| CurlError::UnsupportedFlag(name.clone()))?;
                let value = flag.takes_value().then(|| words.next()).flatten();
                command.apply(flag, &name, value)?;
            } else {
                command.walk_cluster(&word[1..], &mut words)?;
            }
        }
        Ok(command)
    }

    /// `-sS`, `-XPOST`: each letter is a flag of its own, and the first that
    /// takes a value consumes the rest of the cluster — or the next word when
    /// the cluster ends with it — exactly as curl reads it.
    fn walk_cluster(
        &mut self,
        letters: &[u8],
        words: &mut impl Iterator<Item = Vec<u8>>,
    ) -> Result<(), CurlError> {
        let letters =
            std::str::from_utf8(letters).map_err(|_| CurlError::NotUtf8("a flag".to_owned()))?;
        for (offset, letter) in letters.char_indices() {
            let name = format!("-{letter}");
            let flag =
                Flag::lookup(&name).ok_or_else(|| CurlError::UnsupportedFlag(name.clone()))?;
            if !flag.takes_value() {
                self.apply(flag, &name, None)?;
                continue;
            }
            let attached = &letters[offset + letter.len_utf8()..];
            let value =
                if attached.is_empty() { words.next() } else { Some(attached.as_bytes().to_vec()) };
            return self.apply(flag, &name, value);
        }
        Ok(())
    }

    fn apply(&mut self, flag: Flag, name: &str, value: Option<Vec<u8>>) -> Result<(), CurlError> {
        let value = match (flag.takes_value(), value) {
            (false, _) => Vec::new(),
            (true, Some(value)) => value,
            (true, None) => return Err(CurlError::MissingValue(name.to_owned())),
        };
        match flag {
            Flag::Method if value.is_empty() => {
                return Err(CurlError::MissingValue(name.to_owned()))
            }
            Flag::Method => self.method = Some(utf8(value, name)?),
            Flag::Url => self.set_url(value)?,
            Flag::Header => self.add_header(utf8(value, name)?, name)?,
            Flag::Data { raw } => {
                if !raw && value.starts_with(b"@") {
                    return Err(CurlError::ReadsFile(spelled(name, &value)));
                }
                self.data.push(value);
            }
            Flag::Cookie => {
                if !value.contains(&b'=') {
                    return Err(CurlError::ReadsFile(spelled(name, &value)));
                }
                self.cookies.push(utf8(value, name)?);
            }
            Flag::User => {
                let credentials = utf8(value, name)?;
                if !credentials.contains(':') {
                    return Err(CurlError::UserWithoutPassword);
                }
                self.headers.push(Header {
                    name: "Authorization".into(),
                    value: format!("Basic {}", STANDARD.encode(credentials)),
                });
            }
            Flag::Compressed => self.headers.push(Header {
                name: "Accept-Encoding".into(),
                value: COMPRESSED_ACCEPT_ENCODING.into(),
            }),
            Flag::Ignored { .. } => {}
        }
        Ok(())
    }

    fn set_url(&mut self, word: Vec<u8>) -> Result<(), CurlError> {
        let url = utf8(word, "the URL")?;
        if self.url.is_some() {
            return Err(CurlError::SecondUrl(url));
        }
        self.url = Some(url);
        Ok(())
    }

    /// curl's three spellings: `Name: value` sends it, `Name;` sends it empty,
    /// and `Name:` with nothing after the colon is the instruction to send no
    /// such header at all — so it adds nothing here either.
    fn add_header(&mut self, header: String, flag: &str) -> Result<(), CurlError> {
        if header.starts_with('@') {
            return Err(CurlError::ReadsFile(format!("{flag} {header}")));
        }
        let (name, value) = match header.split_once(':') {
            Some((name, value)) => (name, value.trim()),
            None => match header.strip_suffix(';') {
                Some(name) => (name, ""),
                None => return Err(CurlError::HeaderWithoutColon(header)),
            },
        };
        if name.trim().is_empty() {
            return Err(CurlError::HeaderWithoutName(header));
        }
        let sends_nothing = value.is_empty() && !header.ends_with(';');
        if !sends_nothing {
            self.headers.push(Header { name: name.to_owned(), value: value.to_owned() });
        }
        Ok(())
    }

    fn into_request(mut self) -> Result<RequestRec, CurlError> {
        let uri = self.url.ok_or(CurlError::NoUrl)?;
        if !is_absolute_http(&uri) {
            return Err(CurlError::NotAbsoluteUrl(uri));
        }
        if !self.cookies.is_empty() {
            self.headers.push(Header { name: "Cookie".into(), value: self.cookies.join("; ") });
        }
        let body = self.data.join(&b'&');
        let method = self
            .method
            .unwrap_or_else(|| if self.data.is_empty() { "GET" } else { "POST" }.to_owned());
        Ok(RequestRec {
            method,
            uri,
            version: "HTTP/1.1".to_owned(),
            headers: self.headers,
            body: if body.is_empty() {
                BodyRec::Empty
            } else {
                BodyRec::Captured { size: body.len(), bytes: Bytes::from(body) }
            },
        })
    }
}

fn spelled(flag: &str, value: &[u8]) -> String {
    format!("{flag} {}", String::from_utf8_lossy(value))
}

fn is_absolute_http(url: &str) -> bool {
    url.parse::<http::Uri>()
        .is_ok_and(|uri| matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some())
}

/// POSIX-shell word splitting over bytes — whitespace, `'…'`, `"…"` with its
/// four escapes, a bare backslash, backslash-newline continuation and bash's
/// `$'…'` — with nothing expanded: a `$VAR` stays `$VAR`, because the command
/// is data, not a script. Bytes rather than chars so a `$'\xff'` body byte
/// survives; every non-body field is checked for UTF-8 where it is read.
fn tokenize(command: &[u8]) -> Result<Vec<Vec<u8>>, CurlError> {
    let mut words = Vec::new();
    let mut word: Option<Vec<u8>> = None;
    let mut bytes = command.iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' => words.extend(word.take()),
            b'\\' => match bytes.next() {
                Some(b'\n') => {}
                Some(b'\r') => {
                    bytes.next_if_eq(&b'\n');
                }
                Some(escaped) => word.get_or_insert_default().push(escaped),
                None => word.get_or_insert_default().push(b'\\'),
            },
            b'\'' => single_quoted(&mut bytes, word.get_or_insert_default())?,
            b'"' => double_quoted(&mut bytes, word.get_or_insert_default())?,
            b'$' if bytes.peek() == Some(&b'\'') => {
                bytes.next();
                ansi_c_quoted(&mut bytes, word.get_or_insert_default())?;
            }
            other => word.get_or_insert_default().push(other),
        }
    }
    words.extend(word);
    Ok(words)
}

type ByteStream<'a> = std::iter::Peekable<std::iter::Copied<std::slice::Iter<'a, u8>>>;

fn single_quoted(bytes: &mut ByteStream<'_>, word: &mut Vec<u8>) -> Result<(), CurlError> {
    for byte in bytes.by_ref() {
        if byte == b'\'' {
            return Ok(());
        }
        word.push(byte);
    }
    Err(CurlError::UnterminatedQuote)
}

fn double_quoted(bytes: &mut ByteStream<'_>, word: &mut Vec<u8>) -> Result<(), CurlError> {
    while let Some(byte) = bytes.next() {
        match byte {
            b'"' => return Ok(()),
            b'\\' => match bytes.next() {
                Some(escaped @ (b'"' | b'\\' | b'$' | b'`')) => word.push(escaped),
                Some(b'\n') => {}
                Some(other) => word.extend([b'\\', other]),
                None => return Err(CurlError::UnterminatedQuote),
            },
            other => word.push(other),
        }
    }
    Err(CurlError::UnterminatedQuote)
}

/// bash's ANSI-C quoting, which Chrome emits for any body carrying a control
/// character, a `'` or a `!`: `\n \r \t \\ \' \"` and `\uHHHH` are what it
/// writes; `\xHH`, octal and the remaining C escapes are read for completeness.
/// An escape bash does not know stays literal, as bash keeps it.
fn ansi_c_quoted(bytes: &mut ByteStream<'_>, word: &mut Vec<u8>) -> Result<(), CurlError> {
    while let Some(byte) = bytes.next() {
        match byte {
            b'\'' => return Ok(()),
            b'\\' => match bytes.next() {
                None => return Err(CurlError::UnterminatedQuote),
                Some(b'n') => word.push(b'\n'),
                Some(b'r') => word.push(b'\r'),
                Some(b't') => word.push(b'\t'),
                Some(b'a') => word.push(0x07),
                Some(b'b') => word.push(0x08),
                Some(b'f') => word.push(0x0c),
                Some(b'v') => word.push(0x0b),
                Some(b'e' | b'E') => word.push(0x1b),
                Some(escaped @ (b'\\' | b'\'' | b'"' | b'?')) => word.push(escaped),
                Some(b'x') => match take_radix(bytes, 16, 2) {
                    (Some(value), _) => word.push(low_byte(value)),
                    (None, _) => word.extend(b"\\x"),
                },
                Some(b'u') => push_code_point(word, take_radix(bytes, 16, 4), b"\\u"),
                Some(b'U') => push_code_point(word, take_radix(bytes, 16, 8), b"\\U"),
                Some(digit @ b'0'..=b'7') => {
                    let mut value = u32::from(digit - b'0');
                    for _ in 0..2 {
                        match bytes.next_if(|b| (b'0'..=b'7').contains(b)) {
                            Some(next) => value = value * 8 + u32::from(next - b'0'),
                            None => break,
                        }
                    }
                    word.push(low_byte(value));
                }
                Some(other) => word.extend([b'\\', other]),
            },
            other => word.push(other),
        }
    }
    Err(CurlError::UnterminatedQuote)
}

/// The number the next digits spell, and the digits themselves — kept so an
/// escape that names no character can be put back exactly as it was typed.
fn take_radix(bytes: &mut ByteStream<'_>, radix: u32, max_digits: usize) -> (Option<u32>, Vec<u8>) {
    let mut digits = Vec::new();
    for _ in 0..max_digits {
        let Some(digit) = bytes.next_if(|b| (*b as char).is_digit(radix)) else { break };
        digits.push(digit);
    }
    let value = std::str::from_utf8(&digits)
        .ok()
        .and_then(|spelled| u32::from_str_radix(spelled, radix).ok());
    (value, digits)
}

/// bash keeps the low byte of an over-long `\NNN`, so `$'\777'` is `0xff`.
fn low_byte(value: u32) -> u8 {
    u8::try_from(value & 0xff).unwrap_or(u8::MAX)
}

/// A surrogate or an out-of-range `\U` names no character; bash keeps the whole
/// escape literal, digits included, and so does this.
fn push_code_point(word: &mut Vec<u8>, (code, digits): (Option<u32>, Vec<u8>), marker: &[u8]) {
    if let Some(c) = code.and_then(char::from_u32) {
        word.extend(c.to_string().as_bytes());
    } else {
        word.extend(marker);
        word.extend(digits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowSource;

    fn parse(command: &str) -> Flow {
        flow_from_curl(command.as_bytes()).unwrap_or_else(|e| panic!("{command:?}: {e}"))
    }

    fn refusal(command: &str) -> CurlError {
        flow_from_curl(command.as_bytes()).err().unwrap_or_else(|| panic!("{command:?} parsed"))
    }

    fn headers(flow: &Flow) -> Vec<(&str, &str)> {
        flow.request.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect()
    }

    fn body(flow: &Flow) -> &[u8] {
        flow.request.body.bytes().map_or(&[], |b| b.as_ref())
    }

    fn words(command: &str) -> Vec<String> {
        tokenize(command.as_bytes())
            .expect("tokenizes")
            .into_iter()
            .map(|w| String::from_utf8(w).expect("utf8"))
            .collect()
    }

    mod tokenizer {
        use super::*;

        #[test]
        fn words_split_on_whitespace_and_quotes_join_them() {
            assert_eq!(words("curl  a\tb\n'c d'"), ["curl", "a", "b", "c d"]);
        }

        #[test]
        fn a_single_quoted_word_is_literal_and_the_idiom_embeds_a_quote() {
            assert_eq!(words(r"'it'\''s' '\n$x'"), ["it's", r"\n$x"]);
        }

        #[test]
        fn double_quotes_honour_their_four_escapes_and_expand_nothing() {
            assert_eq!(words(r#""a \"b\" \\ \$x \` $HOME \n""#), [r#"a "b" \ $x ` $HOME \n"#]);
        }

        #[test]
        fn a_bare_backslash_escapes_the_next_byte() {
            assert_eq!(words(r"a\ b \'c\'"), ["a b", "'c'"]);
        }

        #[test]
        fn backslash_newline_continues_the_line_inside_and_outside_quotes() {
            assert_eq!(words("curl \\\n  -H \\\r\n 'x' \"a\\\nb\""), ["curl", "-H", "x", "ab"]);
        }

        #[test]
        fn adjacent_quoted_segments_form_one_word() {
            assert_eq!(words(r#"a'b'"c"d$'e'"#), ["abcde"]);
        }

        #[test]
        fn ansi_c_quoting_reads_every_escape_and_keeps_an_unknown_one() {
            let got = tokenize(r#"$'a\nb\tc\\d\'e\"f\x41\101é\U0001F600\q\e'"#.as_bytes())
                .expect("tokenizes");
            assert_eq!(got.len(), 1);
            assert_eq!(got[0], "a\nb\tc\\d'e\"fAAé😀\\q\x1b".as_bytes());
        }

        #[test]
        fn ansi_c_quoting_carries_a_raw_byte_and_an_escape_with_no_digits_stays_literal() {
            let got = tokenize(br"$'\xff\x'").expect("tokenizes");
            assert_eq!(got[0], [0xff, b'\\', b'x']);
        }

        #[test]
        fn an_escape_naming_no_character_stays_literal_with_its_digits() {
            assert_eq!(words(r"$'\uD800 \U00110000 \uzz'"), [r"\uD800 \U00110000 \uzz"]);
        }

        #[test]
        fn an_empty_quoted_word_is_a_word() {
            assert_eq!(words("-X ''"), ["-X", ""]);
        }

        #[test]
        fn every_unterminated_quote_is_refused() {
            for command in ["curl 'x", "curl \"x", "curl $'x", "curl \"x\\", "curl $'x\\"] {
                assert_eq!(
                    tokenize(command.as_bytes()),
                    Err(CurlError::UnterminatedQuote),
                    "{command}"
                );
            }
        }

        #[test]
        fn a_trailing_backslash_is_kept() {
            assert_eq!(words(r"a\"), [r"a\"]);
        }
    }

    mod flags {
        use super::*;

        #[test]
        fn the_bare_url_and_the_url_flag_both_set_the_uri() {
            assert_eq!(parse("curl http://a/").request.uri, "http://a/");
            assert_eq!(parse("curl --url http://a/").request.uri, "http://a/");
        }

        #[test]
        fn the_method_defaults_to_get_then_post_with_a_body_and_x_wins() {
            assert_eq!(parse("curl http://a/").request.method, "GET");
            assert_eq!(parse("curl http://a/ -d x").request.method, "POST");
            assert_eq!(parse("curl http://a/ -d x -X PUT").request.method, "PUT");
            assert_eq!(parse("curl --request patch http://a/").request.method, "patch");
        }

        #[test]
        fn an_empty_data_value_still_makes_it_a_post_with_no_body() {
            let flow = parse("curl http://a/ -d ''");
            assert_eq!(flow.request.method, "POST");
            assert!(matches!(flow.request.body, BodyRec::Empty));
        }

        #[test]
        fn headers_keep_their_order_and_values_lose_the_space_after_the_colon() {
            let flow = parse("curl http://a/ -H 'B: 2' --header 'A:1' -H 'C:  x: y '");
            assert_eq!(headers(&flow), [("B", "2"), ("A", "1"), ("C", "x: y")]);
        }

        #[test]
        fn a_header_ending_in_a_semicolon_is_empty_and_one_ending_in_a_colon_sends_nothing() {
            let flow = parse("curl http://a/ -H 'Empty;' -H 'Gone:' -H 'Also-Gone:   '");
            assert_eq!(headers(&flow), [("Empty", "")]);
        }

        #[test]
        fn several_data_flags_join_with_an_ampersand_under_every_spelling() {
            let flow = parse(
                "curl http://a/ -d a=1 --data b=2 --data-raw c=3 --data-binary d=4 --data-ascii e=5",
            );
            assert_eq!(body(&flow), b"a=1&b=2&c=3&d=4&e=5");
            assert!(matches!(flow.request.body, BodyRec::Captured { size: 19, .. }));
        }

        #[test]
        fn data_raw_keeps_an_at_sign_literal_where_the_others_would_read_a_file() {
            assert_eq!(body(&parse("curl http://a/ --data-raw @x")), b"@x");
            for flag in ["-d", "--data", "--data-binary", "--data-ascii"] {
                assert_eq!(
                    refusal(&format!("curl http://a/ {flag} @body.json")),
                    CurlError::ReadsFile(format!("{flag} @body.json"))
                );
            }
        }

        #[test]
        fn cookies_become_one_cookie_header_and_a_jar_is_refused() {
            let flow = parse("curl http://a/ -b 'a=1; b=2' --cookie c=3");
            assert_eq!(headers(&flow), [("Cookie", "a=1; b=2; c=3")]);
            assert_eq!(
                refusal("curl http://a/ -b jar.txt"),
                CurlError::ReadsFile("-b jar.txt".into())
            );
        }

        #[test]
        fn user_expands_to_basic_authorization_and_needs_a_password() {
            let flow = parse("curl -u alice:secret http://a/");
            assert_eq!(headers(&flow), [("Authorization", "Basic YWxpY2U6c2VjcmV0")]);
            assert_eq!(refusal("curl --user alice http://a/"), CurlError::UserWithoutPassword);
        }

        #[test]
        fn compressed_expands_to_accept_encoding() {
            let flow = parse("curl --compressed http://a/");
            assert_eq!(headers(&flow), [("Accept-Encoding", "deflate, gzip, br, zstd")]);
        }

        #[test]
        fn expansions_keep_their_place_among_typed_headers_and_cookies_come_last() {
            let flow = parse("curl -H 'A: 1' -u u:p -b x=1 --compressed -H 'B: 2' http://a/");
            assert_eq!(
                headers(&flow).iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                ["A", "Authorization", "Accept-Encoding", "B", "Cookie"]
            );
        }

        #[test]
        fn none_of_curls_runtime_defaults_are_invented() {
            let flow = parse("curl http://a/ -d 'x=1'");
            assert!(headers(&flow).is_empty(), "{:?}", headers(&flow));
        }

        #[test]
        fn a_double_dash_ends_the_flags() {
            let flow = parse("curl -s -- http://a/");
            assert_eq!(flow.request.uri, "http://a/");
            assert_eq!(refusal("curl -- -H http://a/"), CurlError::SecondUrl("http://a/".into()));
        }

        #[test]
        fn short_clusters_and_attached_values_read_as_curl_reads_them() {
            let flow = parse("curl -sSkL -XPOST -HX-A:1 -d'a=1' http://a/");
            assert_eq!(flow.request.method, "POST");
            assert_eq!(headers(&flow), [("X-A", "1")]);
            assert_eq!(body(&flow), b"a=1");
            assert_eq!(refusal("curl -sF x http://a/"), CurlError::UnsupportedFlag("-F".into()));
        }

        #[test]
        fn every_ignored_flag_is_consumed_with_its_value_when_it_takes_one() {
            let plain = parse("curl http://a/ -H 'A: 1'");
            let noisy = parse(
                "curl -s --silent -S --show-error -k --insecure -i --include -v --verbose -L \
                 --location -f --fail -# --progress-bar --no-progress-meter -g --globoff -N \
                 --no-buffer -0 --http1.0 --http1.1 --http2 --http2-prior-knowledge --http3 \
                 --tlsv1.2 --tlsv1.3 -4 --ipv4 -6 --ipv6 --proxy-insecure --no-keepalive \
                 -o /dev/null --output out -w '%{http_code}' --write-out x -m 5 --max-time 5 \
                 --connect-timeout 5 --retry 3 --retry-delay 1 -x http://proxy:1 --proxy p \
                 --cacert ca.pem --capath /etc --cert c.pem --key k.pem --resolve a:80:1.2.3.4 \
                 --connect-to a:80:b:80 --max-redirs 3 --limit-rate 1k --interface eth0 \
                 http://a/ -H 'A: 1'",
            );
            assert_eq!(noisy.request.method, plain.request.method);
            assert_eq!(noisy.request.uri, plain.request.uri);
            assert_eq!(headers(&noisy), headers(&plain));
            assert_eq!(body(&noisy), body(&plain));
        }

        #[test]
        fn an_ignored_flags_value_is_never_mistaken_for_the_url() {
            assert_eq!(parse("curl -o http://not-the-url/ http://a/").request.uri, "http://a/");
        }
    }

    mod refusals {
        use super::*;

        #[test]
        fn an_empty_command_is_refused_as_such() {
            for command in ["", "   \n\t"] {
                assert_eq!(refusal(command), CurlError::Empty);
            }
            assert_eq!(CurlError::Empty.to_string(), "empty command");
        }

        #[test]
        fn the_first_word_must_be_curl() {
            assert_eq!(refusal("wget http://a/"), CurlError::NotCurl("wget".into()));
            assert_eq!(
                CurlError::NotCurl("wget".into()).to_string(),
                "not a curl command: starts with \"wget\""
            );
            for program in ["curl", "/usr/bin/curl", "./curl", "curl.exe"] {
                assert_eq!(parse(&format!("{program} http://a/")).request.uri, "http://a/");
            }
        }

        #[test]
        fn an_unknown_flag_is_refused_by_name_under_both_spellings() {
            for flag in [
                "-F",
                "-T",
                "--form-string",
                "--data-urlencode",
                "-G",
                "-I",
                "--json",
                "-A",
                "-e",
                "--bogus",
            ] {
                assert_eq!(
                    refusal(&format!("curl http://a/ {flag} x")),
                    CurlError::UnsupportedFlag(flag.into()),
                    "{flag}"
                );
            }
            assert_eq!(CurlError::UnsupportedFlag("-F".into()).to_string(), "unsupported flag -F");
        }

        #[test]
        fn a_value_taking_flag_at_the_end_needs_a_value() {
            for flag in ["-H", "--header", "-X", "-d", "-b", "-u", "--url", "-o"] {
                assert_eq!(
                    refusal(&format!("curl http://a/ {flag}")),
                    CurlError::MissingValue(flag.into()),
                    "{flag}"
                );
            }
            assert_eq!(refusal("curl http://a/ -X ''"), CurlError::MissingValue("-X".into()));
            assert_eq!(CurlError::MissingValue("-H".into()).to_string(), "-H needs a value");
        }

        #[test]
        fn no_url_and_a_second_url_are_each_named() {
            assert_eq!(refusal("curl -s"), CurlError::NoUrl);
            assert_eq!(
                refusal("curl http://a/ http://other/"),
                CurlError::SecondUrl("http://other/".into())
            );
            assert_eq!(
                CurlError::SecondUrl("http://other/".into()).to_string(),
                "a second URL: \"http://other/\" — one request per command"
            );
        }

        #[test]
        fn a_url_that_is_not_absolute_http_is_refused_naming_it() {
            for url in ["example.com/x", "/relative", "ftp://a/", "http://", "not a url"] {
                assert_eq!(
                    refusal(&format!("curl '{url}'")),
                    CurlError::NotAbsoluteUrl(url.into()),
                    "{url}"
                );
            }
            assert_eq!(
                CurlError::NotAbsoluteUrl("example.com/x".into()).to_string(),
                "URL \"example.com/x\" is not an absolute http:// or https:// URL"
            );
        }

        #[test]
        fn a_header_read_from_a_file_or_without_a_colon_or_name_is_refused() {
            assert_eq!(
                refusal("curl http://a/ -H @headers"),
                CurlError::ReadsFile("-H @headers".into())
            );
            assert_eq!(
                refusal("curl http://a/ -H 'no colon here'"),
                CurlError::HeaderWithoutColon("no colon here".into())
            );
            assert_eq!(
                refusal("curl http://a/ -H ': x'"),
                CurlError::HeaderWithoutName(": x".into())
            );
            assert_eq!(
                CurlError::ReadsFile("-d @body.json".into()).to_string(),
                "-d @body.json reads a file, whose bytes are not in the command"
            );
            assert_eq!(
                CurlError::HeaderWithoutColon("no colon here".into()).to_string(),
                "header \"no colon here\" has no colon"
            );
        }

        #[test]
        fn a_non_utf8_field_is_refused_naming_the_field_while_a_body_may_carry_any_byte() {
            assert_eq!(
                flow_from_curl(b"curl $'http://\\xff/'").err(),
                Some(CurlError::NotUtf8("the URL".into()))
            );
            assert_eq!(
                flow_from_curl(b"curl http://a/ -H $'\\xff: 1'").err(),
                Some(CurlError::NotUtf8("-H".into()))
            );
            let flow =
                flow_from_curl(b"curl http://a/ --data-binary $'\\xff\\xfe'").expect("parses");
            assert_eq!(body(&flow), [0xff, 0xfe]);
            assert_eq!(CurlError::NotUtf8("-H".into()).to_string(), "-H is not UTF-8");
        }

        #[test]
        fn a_windows_cmd_shaped_command_is_refused_rather_than_half_read() {
            assert_eq!(
                refusal("curl \"http://a/\" ^\n -H \"A: 1\""),
                CurlError::SecondUrl("^".into())
            );
        }
    }

    mod shapes {
        use super::*;

        #[test]
        fn a_chrome_bash_command_parses_whole() {
            let flow = parse(
                "curl 'https://api.example.com/v1/orders?page=2' \\\n  \
                 -H 'accept: application/json' \\\n  \
                 -H 'content-type: application/json' \\\n  \
                 -H 'cookie: sid=abc; theme=dark' \\\n  \
                 -H 'user-agent: Mozilla/5.0' \\\n  \
                 --data-raw $'{\"note\":\"it\\'s here\",\"n\":1}' \\\n  \
                 --compressed",
            );
            assert_eq!(flow.request.method, "POST");
            assert_eq!(flow.request.uri, "https://api.example.com/v1/orders?page=2");
            assert_eq!(flow.host(), Some("api.example.com"));
            assert_eq!(
                headers(&flow),
                [
                    ("accept", "application/json"),
                    ("content-type", "application/json"),
                    ("cookie", "sid=abc; theme=dark"),
                    ("user-agent", "Mozilla/5.0"),
                    ("Accept-Encoding", "deflate, gzip, br, zstd"),
                ]
            );
            assert_eq!(body(&flow), br#"{"note":"it's here","n":1}"#);
        }

        #[test]
        fn a_firefox_command_parses_whole() {
            let flow = parse(
                "curl 'https://api.example.com/v1/orders' -X POST \
                 -H 'User-Agent: Mozilla/5.0 (Macintosh)' -H 'Accept: */*' \
                 -H 'Content-Type: application/json' -H 'Origin: https://app.example.com' \
                 --data-raw '{\"a\":1}'",
            );
            assert_eq!(flow.request.method, "POST");
            assert_eq!(headers(&flow).len(), 4);
            assert_eq!(headers(&flow)[3], ("Origin", "https://app.example.com"));
            assert_eq!(body(&flow), br#"{"a":1}"#);
        }

        #[test]
        fn a_safari_command_parses_whole() {
            let flow = parse(
                "curl 'https://api.example.com/v1/orders' \\\n-X 'POST' \\\n\
                 -H 'Accept: application/json' \\\n-H 'Content-Type: application/json' \\\n\
                 --data-binary '{\"a\":1}'",
            );
            assert_eq!(flow.request.method, "POST");
            assert_eq!(
                headers(&flow),
                [("Accept", "application/json"), ("Content-Type", "application/json")]
            );
            assert_eq!(body(&flow), br#"{"a":1}"#);
        }

        // The literal outputs ui/src/flow.test.ts pins for toCurl, so the two
        // serializers — the TS one and the test mirror below — are held to the
        // same bytes.
        #[test]
        fn the_pinned_to_curl_outputs_parse_back() {
            let flow = parse("curl 'http://example.com/a?x=1' -H 'Accept: application/json'");
            assert_eq!(flow.request.method, "GET");
            assert_eq!(flow.request.uri, "http://example.com/a?x=1");
            assert_eq!(headers(&flow), [("Accept", "application/json")]);
            assert!(matches!(flow.request.body, BodyRec::Empty));

            let flow = parse("curl -X POST 'http://example.com/a?x=1' --data-raw '{\"a\":1}'");
            assert_eq!(flow.request.method, "POST");
            assert_eq!(body(&flow), br#"{"a":1}"#);

            let flow = parse("curl -X POST 'http://example.com/a?x=1' --data-raw 'it'\\''s'");
            assert_eq!(body(&flow), b"it's");
        }

        #[test]
        fn the_flow_is_request_only_marked_imported_and_stamped_now() {
            let before = std::time::SystemTime::now();
            let flow = parse("curl http://a/b");
            assert_eq!(flow.source, FlowSource::Imported);
            assert!(flow.response.is_none());
            assert!(flow.duration_ms.is_none());
            assert_eq!(flow.request.version, "HTTP/1.1");
            assert!(flow.started_at >= before);
        }
    }

    mod properties {
        use super::*;
        use crate::test_support::{captured, FlowBuilder};
        use proptest::prelude::*;

        fn shell_quote(s: &str) -> String {
            format!("'{}'", s.replace('\'', "'\\''"))
        }

        /// A byte-for-byte mirror of `toCurl` in `ui/src/flow.ts`, anchored by
        /// `the_pinned_to_curl_outputs_parse_back` to the outputs the TS test
        /// file pins.
        fn to_curl(flow: &Flow) -> String {
            let mut parts = vec!["curl".to_owned()];
            if flow.request.method != "GET" {
                parts.push("-X".to_owned());
                parts.push(flow.request.method.clone());
            }
            parts.push(shell_quote(&flow.request.uri));
            for header in &flow.request.headers {
                parts.push("-H".to_owned());
                parts.push(shell_quote(&format!("{}: {}", header.name, header.value)));
            }
            let text = flow.request.body.bytes().map(|b| String::from_utf8_lossy(b).into_owned());
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                parts.push("--data-raw".to_owned());
                parts.push(shell_quote(&text));
            }
            parts.join(" ")
        }

        #[test]
        fn the_mirror_reproduces_the_outputs_the_ts_test_file_pins() {
            let flow = FlowBuilder::new()
                .uri("http://example.com/a?x=1")
                .req_header("Accept", "application/json")
                .build();
            assert_eq!(
                to_curl(&flow),
                "curl 'http://example.com/a?x=1' -H 'Accept: application/json'"
            );
            let flow = FlowBuilder::new()
                .method("POST")
                .uri("http://example.com/a?x=1")
                .req_body(captured(b"it's"))
                .build();
            assert_eq!(
                to_curl(&flow),
                "curl -X POST 'http://example.com/a?x=1' --data-raw 'it'\\''s'"
            );
        }

        // The boundary of the inverse, stated by the generator: toCurl itself
        // is lossy for a header whose value is blank (curl reads `Name: ` as
        // "send no such header") and for a GET carrying a body (curl would
        // POST it), so neither shape is generated — the parser agrees with
        // curl in both cases, and the property is about everything else.
        fn request_flow() -> impl Strategy<Value = Flow> {
            let header_value = "[^\\s'][^\\r\\n]{0,20}[^\\s']|[^\\s'\\r\\n]";
            let header = (
                prop_oneof![Just("Accept"), Just("X-Trace"), Just("Cookie"), Just("Content-Type")],
                header_value,
            );
            let uri = prop_oneof![
                Just("http://example.com/".to_owned()),
                "https://[a-z]{1,6}\\.test/[a-z0-9/]{0,8}(\\?[a-z]=[a-z0-9%]{1,4})?",
            ];
            let body = prop_oneof![
                Just(None),
                "[^\\x00]{1,40}".prop_map(Some),
                Just(Some("it's {\"quoted\": 'and' \\ escaped}\n".to_owned())),
            ];
            (
                prop_oneof![Just("GET"), Just("POST"), Just("PUT"), Just("DELETE")],
                uri,
                proptest::collection::vec(header, 0..4),
                body,
            )
                .prop_map(|(method, uri, headers, body)| {
                    let mut builder = FlowBuilder::new().method(method).uri(&uri);
                    for (name, value) in &headers {
                        builder = builder.req_header(name, value);
                    }
                    if let Some(text) = body.filter(|_| method != "GET") {
                        builder = builder.req_body(captured(text.as_bytes()));
                    }
                    builder.build()
                })
        }

        proptest! {
            #[test]
            fn a_flow_rendered_by_to_curl_parses_back_to_itself(flow in request_flow()) {
                let command = to_curl(&flow);
                let back = flow_from_curl(command.as_bytes())
                    .unwrap_or_else(|e| panic!("{command:?}: {e}"));
                prop_assert_eq!(&back.request.method, &flow.request.method);
                prop_assert_eq!(&back.request.uri, &flow.request.uri);
                prop_assert_eq!(headers(&back), headers(&flow));
                prop_assert_eq!(body(&back), body(&flow));
                prop_assert_eq!(back.source, FlowSource::Imported);
                prop_assert!(back.response.is_none());
            }

            #[test]
            fn the_parser_never_panics_on_arbitrary_text(text in any::<String>()) {
                let _ = flow_from_curl(text.as_bytes());
            }

            #[test]
            fn the_parser_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
                let _ = flow_from_curl(&bytes);
            }

            // Random text mostly fails at the first word, so this generator
            // speaks the parser's own vocabulary with values missing, doubled,
            // attached, quoted and malformed — every branch past the tokenizer.
            #[test]
            fn the_parser_never_panics_on_a_curl_shaped_near_miss(
                words in proptest::collection::vec(near_miss_word(), 0..12)
            ) {
                let command = format!("curl {}", words.join(" "));
                let _ = flow_from_curl(command.as_bytes());
            }
        }

        fn near_miss_word() -> impl Strategy<Value = String> {
            prop_oneof![
                prop_oneof![
                    Just("-X"),
                    Just("-H"),
                    Just("-d"),
                    Just("--data-raw"),
                    Just("-b"),
                    Just("-u"),
                    Just("--compressed"),
                    Just("-sS"),
                    Just("-XPOST"),
                    Just("-o"),
                    Just("--"),
                    Just("-"),
                    Just("--bogus"),
                    Just("-F"),
                    Just("--url"),
                ]
                .prop_map(str::to_owned),
                prop_oneof![
                    Just("http://a/"),
                    Just("'http://b/x?y=1'"),
                    Just("example.com"),
                    Just("''"),
                    Just("'A: 1'"),
                    Just("'A;'"),
                    Just("'A:'"),
                    Just("':x'"),
                    Just("nocolon"),
                    Just("@file"),
                    Just("u:p"),
                    Just("u"),
                    Just("a=1"),
                    Just("$'\\n\\xff\\u00e9'"),
                    Just("\"a \\\" b\""),
                    Just("'unterminated"),
                    Just("\\"),
                    Just("^"),
                ]
                .prop_map(str::to_owned),
                "[ -~]{0,8}",
            ]
        }
    }
}
