use std::time::SystemTime;

use bytes::Bytes;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FlowId(pub Uuid);

impl FlowId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for FlowId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for FlowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where a flow came from. Until HAR import every flow was captured by the
/// proxy, so nothing recorded the fact; now the ring holds flows the proxy never
/// saw and a user must be able to tell them apart and select them separately.
/// `Sent` is an exchange Trawler's own client made (DEC-58).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowSource {
    Captured,
    Imported,
    Sent,
}

#[derive(Clone, Debug)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub enum NotCapturedReason {
    TooLarge { actual: usize },
    ReadError,
}

/// How a request/response body was handled by the capture path. A plain
/// `Option<Bytes>` cannot distinguish "empty", "skipped because streaming", and
/// "too large" — distinctions the interface must render differently.
#[derive(Clone, Debug)]
pub enum BodyRec {
    Captured { bytes: Bytes, size: usize },
    Streaming { content_type: String },
    NotCaptured { reason: NotCapturedReason },
    Empty,
}

impl BodyRec {
    /// Bytes physically retained in memory — the weight this body contributes to
    /// ring-buffer eviction. Streaming / not-captured / empty bodies hold nothing.
    pub fn retained_len(&self) -> usize {
        match self {
            BodyRec::Captured { size, .. } => *size,
            _ => 0,
        }
    }

    pub fn state(&self) -> &'static str {
        match self {
            BodyRec::Captured { .. } => "captured",
            BodyRec::Streaming { .. } => "streaming",
            BodyRec::NotCaptured { .. } => "not_captured",
            BodyRec::Empty => "empty",
        }
    }

    pub fn bytes(&self) -> Option<&Bytes> {
        match self {
            BodyRec::Captured { bytes, .. } => Some(bytes),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestRec {
    pub method: String,
    pub uri: String,
    pub version: String,
    pub headers: Vec<Header>,
    pub body: BodyRec,
}

#[derive(Clone, Debug)]
pub struct ResponseRec {
    pub status: u16,
    pub version: String,
    pub headers: Vec<Header>,
    pub body: BodyRec,
}

/// The top-level unit of capture — one per HTTP exchange. `response` is an
/// `Option` because a flow exists the moment the request is seen, before any
/// reply lands.
#[derive(Clone, Debug)]
pub struct Flow {
    pub id: FlowId,
    pub started_at: SystemTime,
    pub request: RequestRec,
    pub response: Option<ResponseRec>,
    pub duration_ms: Option<u64>,
    pub source: FlowSource,
    /// The flow a sent one was replayed from. It may name a flow the ring has
    /// since let go: the link records where the request came from, not a
    /// reference the store keeps alive.
    pub seed: Option<FlowId>,
    host: Option<String>,
}

/// The version spelling every flow carries, whichever side of Trawler made the
/// exchange.
pub(crate) fn version_token(version: http::Version) -> String {
    format!("{version:?}")
}

pub(crate) fn header_list(map: &http::HeaderMap) -> Vec<Header> {
    map.iter()
        .map(|(name, value)| Header {
            name: name.as_str().to_owned(),
            value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
        })
        .collect()
}

pub(crate) fn header_value<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

impl Flow {
    pub fn new(request: RequestRec) -> Self {
        let host = request.uri.parse::<http::Uri>().ok().and_then(|u| u.host().map(str::to_owned));
        Self {
            id: FlowId::new(),
            started_at: SystemTime::now(),
            request,
            response: None,
            duration_ms: None,
            source: FlowSource::Captured,
            seed: None,
            host,
        }
    }

    /// The same flow, marked as having arrived from a file rather than the wire.
    #[must_use]
    pub fn imported(mut self) -> Self {
        self.source = FlowSource::Imported;
        self
    }

    /// The same flow, marked as sent by Trawler itself, from `seed` when it was
    /// a replay and from nothing when it was composed.
    #[must_use]
    pub fn sent(mut self, seed: Option<FlowId>) -> Self {
        self.source = FlowSource::Sent;
        self.seed = seed;
        self
    }

    pub fn retained_bytes(&self) -> usize {
        self.request.body.retained_len()
            + self.response.as_ref().map_or(0, |r| r.body.retained_len())
    }

    /// Derived once at construction, so filter scans under the store lock never
    /// re-parse the URI.
    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    pub fn request_content_type(&self) -> Option<&str> {
        header_value(&self.request.headers, "content-type")
    }

    pub fn response_content_type(&self) -> Option<&str> {
        self.response.as_ref().and_then(|r| header_value(&r.headers, "content-type"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{captured, FlowBuilder};

    #[test]
    fn retained_len_by_variant() {
        assert_eq!(captured(b"abc").retained_len(), 3);
        assert_eq!(BodyRec::Streaming { content_type: "x".into() }.retained_len(), 0);
        assert_eq!(
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 99 } }
                .retained_len(),
            0
        );
        assert_eq!(BodyRec::Empty.retained_len(), 0);
    }

    #[test]
    fn state_by_variant() {
        assert_eq!(captured(b"").state(), "captured");
        assert_eq!(BodyRec::Streaming { content_type: "x".into() }.state(), "streaming");
        assert_eq!(
            BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }.state(),
            "not_captured"
        );
        assert_eq!(BodyRec::Empty.state(), "empty");
    }

    #[test]
    fn bytes_present_only_when_retained() {
        assert_eq!(captured(b"hi").bytes().map(std::convert::AsRef::as_ref), Some(&b"hi"[..]));
        assert!(BodyRec::Empty.bytes().is_none());
        assert!(BodyRec::Streaming { content_type: "x".into() }.bytes().is_none());
        assert!(BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }.bytes().is_none());
    }

    #[test]
    fn retained_bytes_sums_request_and_response() {
        assert_eq!(FlowBuilder::new().req_body(captured(b"12345")).build().retained_bytes(), 5);
        let both =
            FlowBuilder::new().req_body(captured(b"12345")).resp_body(captured(b"abc")).build();
        assert_eq!(both.retained_bytes(), 8);
    }

    #[test]
    fn host_parsing() {
        let host = |uri: &str| FlowBuilder::new().uri(uri).build().host().map(str::to_owned);
        assert_eq!(host("http://example.com/x").as_deref(), Some("example.com"));
        assert_eq!(host("http://example.com:8080/x").as_deref(), Some("example.com"));
        assert_eq!(host("http://127.0.0.1:3000/").as_deref(), Some("127.0.0.1"));
        assert!(host("http://[::1]:8080/").unwrap().contains("::1"));
        assert_eq!(host("/only/path"), None);
        assert_eq!(host("not a uri"), None);
    }

    #[test]
    fn a_captured_flow_is_the_default_and_carries_no_seed() {
        let flow = FlowBuilder::new().build();
        assert_eq!(flow.source, FlowSource::Captured);
        assert_eq!(flow.seed, None);
        assert_eq!(FlowBuilder::new().build().imported().source, FlowSource::Imported);
    }

    #[test]
    fn a_sent_flow_is_marked_and_names_its_seed_when_it_had_one() {
        let seed = FlowBuilder::new().build();
        let replayed = FlowBuilder::new().build().sent(Some(seed.id));
        assert_eq!(replayed.source, FlowSource::Sent);
        assert_eq!(replayed.seed, Some(seed.id));

        let composed = FlowBuilder::new().build().sent(None);
        assert_eq!(composed.source, FlowSource::Sent);
        assert_eq!(composed.seed, None);
    }

    #[test]
    fn version_renders_api_wire_format() {
        assert_eq!(version_token(http::Version::HTTP_11), "HTTP/1.1");
        assert_eq!(version_token(http::Version::HTTP_2), "HTTP/2.0");
        assert_eq!(version_token(http::Version::HTTP_10), "HTTP/1.0");
    }

    #[test]
    fn header_list_preserves_duplicates() {
        let mut map = http::HeaderMap::new();
        map.append("x-dup", "one".parse().unwrap());
        map.append("x-dup", "two".parse().unwrap());
        map.insert("content-type", "text/plain".parse().unwrap());
        let out = header_list(&map);
        let dup: Vec<_> =
            out.iter().filter(|h| h.name == "x-dup").map(|h| h.value.as_str()).collect();
        assert_eq!(dup, vec!["one", "two"]);
        assert!(out.iter().any(|h| h.name == "content-type" && h.value == "text/plain"));
    }

    #[test]
    fn header_list_is_lossy_on_non_utf8_values() {
        let mut map = http::HeaderMap::new();
        map.insert("x-bin", http::HeaderValue::from_bytes(&[0xff, 0xfe]).unwrap());
        let out = header_list(&map);
        assert!(out.iter().find(|h| h.name == "x-bin").unwrap().value.contains('\u{FFFD}'));
    }

    #[test]
    fn content_type_lookup_case_insensitive_first_wins() {
        let flow = FlowBuilder::new()
            .req_header("Content-Type", "application/json")
            .req_header("content-type", "text/plain")
            .status(200)
            .resp_header("CONTENT-TYPE", "text/html")
            .build();
        assert_eq!(flow.request_content_type(), Some("application/json"));
        assert_eq!(flow.response_content_type(), Some("text/html"));
        assert_eq!(FlowBuilder::new().build().response_content_type(), None);
    }
}
