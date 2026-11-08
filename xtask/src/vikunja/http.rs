use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub total_pages: Option<u32>,
    pub body: String,
}

pub fn request(
    addr: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<Response, String> {
    let unreachable = |e: std::io::Error| format!("{method} http://{addr}{target}: {e}");
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(unreachable)?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(unreachable)?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(unreachable)?;
    let mut head = format!(
        "{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    head.push_str("\r\n");
    head.push_str(body);
    stream.write_all(head.as_bytes()).map_err(unreachable)?;
    let mut raw = Vec::new();
    // A server refusing before it reads the body (Vikunja's token-scope check)
    // answers and closes; the reset that follows must not discard its answer.
    let read = stream.read_to_end(&mut raw);
    match (parse_response(&raw), read) {
        (Ok(response), _) => Ok(response),
        (Err(_), Err(e)) => Err(unreachable(e)),
        (Err(e), Ok(_)) => Err(format!("{method} http://{addr}{target}: {e}")),
    }
}

pub fn parse_response(raw: &[u8]) -> Result<Response, String> {
    let split =
        raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("response has no header terminator")?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| "response head is not UTF-8")?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or("malformed status line")?;
    let mut total_pages = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("x-pagination-total-pages") {
            total_pages = value.parse().ok();
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }
    let payload = &raw[split + 4..];
    let bytes = if chunked { dechunk(payload)? } else { payload.to_vec() };
    let body = String::from_utf8(bytes).map_err(|_| "response body is not UTF-8")?;
    Ok(Response { status, total_pages, body })
}

fn dechunk(mut payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end =
            payload.windows(2).position(|w| w == b"\r\n").ok_or("truncated chunk size")?;
        let size_text = std::str::from_utf8(&payload[..line_end]).map_err(|_| "bad chunk size")?;
        let size_hex = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| "bad chunk size")?;
        payload = &payload[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        let chunk = payload.get(..size).ok_or("truncated chunk")?;
        out.extend_from_slice(chunk);
        payload = payload.get(size + 2..).ok_or("truncated chunk")?;
    }
}

pub fn percent_encode(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_page_count_and_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Pagination-Total-Pages: 3\r\nContent-Length: 2\r\n\r\n[]";
        let response = parse_response(raw).unwrap();
        assert_eq!(response, Response { status: 200, total_pages: Some(3), body: "[]".into() });
    }

    #[test]
    fn decodes_a_chunked_body() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n## A\r\n3;x=y\r\n\nb\n\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw).unwrap().body, "## A\nb\n");
        let truncated = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n9\r\nab";
        assert!(parse_response(truncated).is_err());
        let huge = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nab";
        assert!(parse_response(huge).is_err());
    }

    #[test]
    fn an_answer_sent_before_the_body_was_read_survives_the_reset() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut first = [0; 16];
            stream.read_exact(&mut first).unwrap();
            stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        });
        let response = request(addr, "PUT", "/api/v1/labels", &[], &"x".repeat(4096)).unwrap();
        server.join().unwrap();
        assert_eq!((response.status, response.body.as_str()), (401, "{}"));
    }

    #[test]
    fn refuses_a_malformed_response() {
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n").is_err());
        assert!(parse_response(b"garbage\r\n\r\n").is_err());
    }

    #[test]
    fn a_response_without_a_page_count_has_none() {
        let response = parse_response(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
        assert_eq!((response.status, response.total_pages), (404, None));
    }

    #[test]
    fn percent_encodes_each_reserved_byte_but_keeps_slashes() {
        assert_eq!(
            percent_encode("done = false && project = 3"),
            "done%20%3D%20false%20%26%26%20project%20%3D%203"
        );
        assert_eq!(percent_encode("a#b?c%—/"), "a%23b%3Fc%25%E2%80%94/");
    }
}
