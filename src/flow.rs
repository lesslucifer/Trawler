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
    Truncated { prefix: Bytes, captured: usize, total_hint: Option<usize> },
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
            BodyRec::Truncated { captured, .. } => *captured,
            _ => 0,
        }
    }

    pub fn state(&self) -> &'static str {
        match self {
            BodyRec::Captured { .. } => "captured",
            BodyRec::Truncated { .. } => "truncated",
            BodyRec::Streaming { .. } => "streaming",
            BodyRec::NotCaptured { .. } => "not_captured",
            BodyRec::Empty => "empty",
        }
    }

    pub fn bytes(&self) -> Option<&Bytes> {
        match self {
            BodyRec::Captured { bytes, .. } => Some(bytes),
            BodyRec::Truncated { prefix, .. } => Some(prefix),
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
}

impl Flow {
    pub fn new(request: RequestRec) -> Self {
        Self {
            id: FlowId::new(),
            started_at: SystemTime::now(),
            request,
            response: None,
            duration_ms: None,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.request.body.retained_len()
            + self.response.as_ref().map_or(0, |r| r.body.retained_len())
    }

    pub fn host(&self) -> Option<String> {
        self.request.uri.parse::<http::Uri>().ok().and_then(|u| u.host().map(str::to_owned))
    }

    fn header_value<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
        headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
    }

    pub fn request_content_type(&self) -> Option<&str> {
        Self::header_value(&self.request.headers, "content-type")
    }

    pub fn response_content_type(&self) -> Option<&str> {
        self.response.as_ref().and_then(|r| Self::header_value(&r.headers, "content-type"))
    }
}
