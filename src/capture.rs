use http::{header, HeaderMap};
use http_body_util::{BodyExt, Full};
use hudsucker::Body;

use crate::flow::{BodyRec, NotCapturedReason};

const STREAMING_CONTENT_TYPES: &[&str] = &["text/event-stream"];

fn streaming_content_type(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let base = raw.split(';').next().unwrap_or(raw).trim();
    STREAMING_CONTENT_TYPES.iter().any(|s| base.eq_ignore_ascii_case(s)).then(|| raw.to_string())
}

/// Drain a body up to `cap`, decide how it was captured, and return a fresh body
/// to forward — the read is destructive, so the body is always rebuilt, even on
/// the cap-exceeded path, so a capture decision never breaks the connection.
///
/// Streaming content types (SSE and friends) are forwarded untouched: a body
/// that never ends must never be buffered, or the exchange stalls forever.
pub async fn capture(headers: &HeaderMap, body: Body, cap: usize) -> (BodyRec, Body) {
    if let Some(content_type) = streaming_content_type(headers) {
        return (BodyRec::Streaming { content_type }, body);
    }

    match body.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            let size = bytes.len();
            let rec = if size == 0 {
                BodyRec::Empty
            } else if size > cap {
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: size } }
            } else {
                BodyRec::Captured { bytes: bytes.clone(), size }
            };
            (rec, Body::from(Full::new(bytes)))
        }
        Err(_) => (BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }, Body::empty()),
    }
}
