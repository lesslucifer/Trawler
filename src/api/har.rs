//! HAR 1.2 projection of captured flows — the format Trawler exchanges captures
//! in. Every field a HAR reader requires is present; every fact Trawler does not
//! hold is HAR's own sentinel (`-1`, `""`, `{}`) rather than a fabricated value,
//! because a reader cannot tell an invented number from a measured one.

use std::time::SystemTime;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::view::rfc3339;
use crate::flow::{
    header_value, BodyRec, Flow, Header, NotCapturedReason, RequestRec, ResponseRec,
};

const HAR_VERSION: &str = "1.2";
const CREATOR_NAME: &str = "Trawler";

/// HAR's "not available" for any numeric field. Distinct from `0`, which claims a
/// measurement of zero.
const UNKNOWN: i64 = -1;

#[derive(Serialize)]
pub struct HarDocument {
    pub log: HarLog,
}

#[derive(Serialize)]
pub struct HarLog {
    pub version: &'static str,
    pub creator: HarCreator,
    pub entries: Vec<HarEntry>,
}

#[derive(Serialize)]
pub struct HarCreator {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarEntry {
    pub started_date_time: String,
    pub time: i64,
    pub request: HarRequest,
    pub response: HarResponse,
    pub cache: HarCache,
    pub timings: HarTimings,
}

#[derive(Serialize)]
pub struct HarCache {}

/// Trawler measures one span — request sent to response complete — not the phase
/// breakdown HAR anticipates. `wait` carries the whole span and the two phases it
/// cannot separate are `0`, so the block sums to `time` as HAR requires whenever
/// the span was measured. When it was not, `time` is HAR's `-1` and the phases
/// are all `0`: the phases may not be negative, so the sum cannot be honoured
/// there, and zeroed phases beside an explicitly unknown `time` claim less than
/// a negative phase no reader accepts.
#[derive(Serialize)]
pub struct HarTimings {
    pub send: i64,
    pub wait: i64,
    pub receive: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarRequest {
    pub method: String,
    pub url: String,
    pub http_version: String,
    pub cookies: Vec<HarCookie>,
    pub headers: Vec<HarHeader>,
    pub query_string: Vec<HarQueryParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_data: Option<HarPostData>,
    pub headers_size: i64,
    pub body_size: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarResponse {
    pub status: u16,
    pub status_text: String,
    pub http_version: String,
    pub cookies: Vec<HarCookie>,
    pub headers: Vec<HarHeader>,
    pub content: HarContent,
    // HAR spells it `redirectURL`, which camelCase alone cannot produce.
    #[serde(rename = "redirectURL")]
    pub redirect_url: String,
    pub headers_size: i64,
    pub body_size: i64,
}

#[derive(Serialize)]
pub struct HarHeader {
    pub name: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct HarQueryParam {
    pub name: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct HarCookie {
    pub name: String,
    pub value: String,
}

/// A response body. `text` is absent whenever the bytes were never captured, so a
/// skipped body is structurally distinguishable from an empty one; `comment`
/// names which of the three non-capture states applies.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarContent {
    pub size: i64,
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarPostData {
    pub mime_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<&'static str>,
}

/// The reason token a non-captured body carries into the file, spelled as the
/// REST body view already spells the same state — so the two surfaces never name
/// one state two ways — and read back by the importer to restore the variant.
pub fn not_captured_comment(body: &BodyRec) -> Option<String> {
    let reason = match body {
        BodyRec::Streaming { .. } => "streaming",
        BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { .. } } => "too_large",
        BodyRec::NotCaptured { reason: NotCapturedReason::ReadError } => "read_error",
        BodyRec::Captured { .. } | BodyRec::Empty => return None,
    };
    Some(format!("trawler: body not captured ({reason})"))
}

/// Flows still awaiting a response are omitted: HAR requires `entry.response`
/// and Trawler holds nothing truthful to put in it.
pub fn har_document(flows: &[Flow]) -> HarDocument {
    HarDocument {
        log: HarLog {
            version: HAR_VERSION,
            creator: HarCreator { name: CREATOR_NAME, version: env!("CARGO_PKG_VERSION") },
            entries: flows.iter().filter_map(har_entry).collect(),
        },
    }
}

fn har_entry(flow: &Flow) -> Option<HarEntry> {
    let response = flow.response.as_ref()?;
    let time = flow.duration_ms.and_then(|ms| i64::try_from(ms).ok()).unwrap_or(UNKNOWN);
    Some(HarEntry {
        started_date_time: rfc3339(flow.started_at),
        time,
        request: har_request(&flow.request),
        response: har_response(response),
        cache: HarCache {},
        timings: har_timings(time),
    })
}

/// `time` may be HAR's `-1`, but `timings`' three phases may not: the spec
/// sanctions a negative only for the optional phases Trawler never reports. So
/// an unknown span is zeroed here rather than propagated as a negative `wait`.
fn har_timings(time: i64) -> HarTimings {
    HarTimings { send: 0, wait: time.max(0), receive: 0 }
}

fn har_request(request: &RequestRec) -> HarRequest {
    HarRequest {
        method: request.method.clone(),
        url: request.uri.clone(),
        http_version: request.version.clone(),
        cookies: request_cookies(&request.headers),
        headers: headers(&request.headers),
        query_string: query_string(&request.uri),
        post_data: post_data(&request.body, content_type(&request.headers)),
        headers_size: UNKNOWN,
        body_size: body_size(&request.body),
    }
}

fn har_response(response: &ResponseRec) -> HarResponse {
    HarResponse {
        status: response.status,
        status_text: status_text(response.status),
        http_version: response.version.clone(),
        cookies: response_cookies(&response.headers),
        headers: headers(&response.headers),
        content: content(&response.body, content_type(&response.headers)),
        redirect_url: header_value(&response.headers, "location").unwrap_or_default().to_owned(),
        headers_size: UNKNOWN,
        body_size: body_size(&response.body),
    }
}

fn headers(headers: &[Header]) -> Vec<HarHeader> {
    headers.iter().map(|h| HarHeader { name: h.name.clone(), value: h.value.clone() }).collect()
}

fn content_type(headers: &[Header]) -> String {
    header_value(headers, "content-type").unwrap_or("application/octet-stream").to_owned()
}

fn status_text(status: u16) -> String {
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
        .unwrap_or_default()
        .to_owned()
}

/// `-1` unless the size is genuinely known — which for a too-large body it is,
/// since the size is exactly why the bytes were dropped.
fn body_size(body: &BodyRec) -> i64 {
    match body {
        BodyRec::Captured { size, .. }
        | BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: size } } => {
            i64::try_from(*size).unwrap_or(i64::MAX)
        }
        BodyRec::Empty => 0,
        BodyRec::Streaming { .. }
        | BodyRec::NotCaptured { reason: NotCapturedReason::ReadError } => UNKNOWN,
    }
}

fn content(body: &BodyRec, mime_type: String) -> HarContent {
    let (text, encoding) = match body {
        BodyRec::Captured { bytes, .. } => {
            let (text, encoding) = encode_body(bytes, &mime_type);
            (Some(text), encoding)
        }
        _ => (None, None),
    };
    HarContent {
        size: body_size(body),
        mime_type,
        text,
        encoding,
        comment: not_captured_comment(body),
    }
}

/// Absent for every body that carries no bytes: HAR's `postData` has no shape for
/// "a body existed but was not captured", and an empty `text` would claim the
/// request had no body. A skipped request body is therefore visible only as
/// `bodySize: -1` with no `postData` — the honest reading of what HAR can express.
fn post_data(body: &BodyRec, mime_type: String) -> Option<HarPostData> {
    let BodyRec::Captured { bytes, .. } = body else {
        return None;
    };
    let (text, encoding) = encode_body(bytes, &mime_type);
    Some(HarPostData { mime_type, text, encoding })
}

/// Text stays text so the file is readable; everything else — and any text-typed
/// body whose bytes are not actually UTF-8 — is base64 under HAR's `encoding`
/// convention, because a lossy `to_string_lossy` would silently corrupt the
/// bytes a round-trip must return.
fn encode_body(bytes: &[u8], mime_type: &str) -> (String, Option<&'static str>) {
    if is_textual(mime_type) {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return (text.to_owned(), None);
        }
    }
    (STANDARD.encode(bytes), Some("base64"))
}

fn is_textual(mime_type: &str) -> bool {
    let mime_type = mime_type.to_ascii_lowercase();
    mime_type.starts_with("text/")
        || mime_type.contains("json")
        || mime_type.contains("xml")
        || mime_type.contains("javascript")
        || mime_type.contains("x-www-form-urlencoded")
}

fn query_string(uri: &str) -> Vec<HarQueryParam> {
    let Some(query) = uri.parse::<http::Uri>().ok().and_then(|u| u.query().map(str::to_owned))
    else {
        return Vec::new();
    };
    form_urlencoded::parse(query.as_bytes())
        .map(|(name, value)| HarQueryParam { name: name.into_owned(), value: value.into_owned() })
        .collect()
}

fn request_cookies(headers: &[Header]) -> Vec<HarCookie> {
    headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("cookie"))
        .flat_map(|h| h.value.split(';'))
        .filter_map(|pair| cookie_pair(pair.trim()))
        .collect()
}

/// Only the name/value pair: HAR's optional cookie attributes (`expires`,
/// `httpOnly`, …) are not parsed out of `Set-Cookie` here, and omitting an
/// optional field is honest where guessing it would not be.
fn response_cookies(headers: &[Header]) -> Vec<HarCookie> {
    headers
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|h| h.value.split(';').next())
        .filter_map(|pair| cookie_pair(pair.trim()))
        .collect()
}

fn cookie_pair(pair: &str) -> Option<HarCookie> {
    let (name, value) = pair.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(HarCookie { name: name.to_owned(), value: value.trim().to_owned() })
}

/// The read side of the format. Deliberately separate types from the `Serialize`
/// ones above: what Trawler writes is a fixed shape, what it reads is whatever a
/// conforming tool emitted — unknown fields ignored, every optional field
/// genuinely optional — and one struct serving both would make the export lax or
/// the import strict.
#[derive(Deserialize)]
struct InDocument {
    log: Option<InLog>,
}

#[derive(Deserialize)]
struct InLog {
    version: Option<String>,
    entries: Option<Vec<InEntry>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InEntry {
    started_date_time: Option<String>,
    time: Option<f64>,
    request: Option<InRequest>,
    response: Option<InResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InRequest {
    method: Option<String>,
    url: Option<String>,
    http_version: Option<String>,
    #[serde(default)]
    headers: Vec<InHeader>,
    post_data: Option<InPostData>,
    body_size: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InResponse {
    status: Option<i64>,
    http_version: Option<String>,
    #[serde(default)]
    headers: Vec<InHeader>,
    content: Option<InContent>,
}

#[derive(Deserialize)]
struct InHeader {
    name: Option<String>,
    #[serde(default)]
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InContent {
    size: Option<i64>,
    mime_type: Option<String>,
    text: Option<String>,
    encoding: Option<String>,
    comment: Option<String>,
}

/// `mimeType` is deliberately unread: the request's content type lives in the
/// `Content-Type` header, which is preserved verbatim, and `postData` only ever
/// restated it.
#[derive(Deserialize)]
struct InPostData {
    text: Option<String>,
    encoding: Option<String>,
}

/// Why a document was refused. Every variant names the field at fault and, for
/// an entry-level fault, its index — a caller with a 900-entry file cannot fix
/// what it cannot locate.
#[derive(Debug)]
pub enum ImportError {
    NotJson(String),
    MissingMember(&'static str),
    UnsupportedVersion(String),
    Entry { index: usize, problem: String },
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotJson(reason) => write!(f, "not a JSON document: {reason}"),
            ImportError::MissingMember(member) => write!(f, "missing {member}"),
            ImportError::UnsupportedVersion(found) => {
                write!(f, "unsupported HAR version {found:?} — only 1.2 is supported")
            }
            ImportError::Entry { index, problem } => {
                write!(f, "log.entries[{index}]: {problem}")
            }
        }
    }
}

/// The inverse of [`har_document`]: a HAR 1.2 document read back as flows, ready
/// to be inserted. Every fact HAR carries that Trawler has no home for is
/// dropped rather than smuggled into an adjacent field, and everything Trawler
/// derives — the host, the flow id — is re-derived rather than read.
///
/// The whole document parses before anything is returned, so a document with one
/// bad entry contributes nothing at all; "never a partially-populated store" is
/// then a property of the caller having a `Vec` or an `Err`, not of a cleanup
/// path.
///
/// Flows come back **oldest-first**: the export writes newest-first, so
/// preserving file order into a newest-first ring would land the capture
/// reversed.
pub fn flows_from_har(bytes: &[u8]) -> Result<Vec<Flow>, ImportError> {
    let document: InDocument =
        serde_json::from_slice(bytes).map_err(|e| ImportError::NotJson(e.to_string()))?;
    let log = document.log.ok_or(ImportError::MissingMember("log"))?;
    match log.version.as_deref() {
        Some(HAR_VERSION) => {}
        Some(found) => return Err(ImportError::UnsupportedVersion(found.to_owned())),
        None => return Err(ImportError::MissingMember("log.version")),
    }
    let entries = log.entries.ok_or(ImportError::MissingMember("log.entries"))?;

    let mut flows = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        let flow =
            flow_from_entry(entry).map_err(|problem| ImportError::Entry { index, problem })?;
        flows.push(flow);
    }
    flows.reverse();
    Ok(flows)
}

fn flow_from_entry(entry: InEntry) -> Result<Flow, String> {
    let request = entry.request.ok_or("missing request")?;
    let started_at = started_at(entry.started_date_time.as_deref())?;
    let mut flow = Flow::new(request_rec(request)?).imported();
    flow.started_at = started_at;
    flow.duration_ms = entry.time.and_then(duration_ms);
    flow.response = entry.response.map(response_rec).transpose()?;
    Ok(flow)
}

fn started_at(value: Option<&str>) -> Result<SystemTime, String> {
    let value = value.ok_or("missing startedDateTime")?;
    OffsetDateTime::parse(value, &Rfc3339)
        .map(SystemTime::from)
        .map_err(|e| format!("startedDateTime {value:?} is not RFC3339: {e}"))
}

/// HAR's `-1` — and any other negative, and a `time` a writer omitted — is the
/// absence the projection turned into a sentinel. The phases in `timings` are
/// never read: Trawler holds one span, and re-deriving the total from phases it
/// zeroes would contradict the `time` beside them.
fn duration_ms(time: f64) -> Option<u64> {
    if time < 0.0 || !time.is_finite() {
        return None;
    }
    // A duration is milliseconds; HAR types it as a number, so a fractional or
    // absurd value is clamped into the u64 the domain model holds rather than
    // wrapping.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    Some(if time >= u64::MAX as f64 { u64::MAX } else { time.round() as u64 })
}

fn request_rec(request: InRequest) -> Result<RequestRec, String> {
    let method = non_empty(request.method, "request.method")?;
    let uri = non_empty(request.url, "request.url")?;
    let headers = headers_from(request.headers, "request")?;
    let body = request_body(request.post_data, request.body_size)?;
    Ok(RequestRec { method, uri, version: version_of(request.http_version), headers, body })
}

fn response_rec(response: InResponse) -> Result<ResponseRec, String> {
    let status = response.status.ok_or("response.status is missing")?;
    let status = u16::try_from(status)
        .ok()
        .filter(|s| (100..=599).contains(s))
        .ok_or_else(|| format!("response.status {status} is not an HTTP status code"))?;
    let headers = headers_from(response.headers, "response")?;
    let body = response_body(response.content)?;
    Ok(ResponseRec { status, version: version_of(response.http_version), headers, body })
}

fn non_empty(value: Option<String>, field: &str) -> Result<String, String> {
    match value {
        Some(value) if !value.is_empty() => Ok(value),
        Some(_) => Err(format!("{field} is empty")),
        None => Err(format!("{field} is missing")),
    }
}

/// A writer that omits `httpVersion` is not describing a different protocol, it
/// simply did not record one — and `RequestRec::version` has no absence to
/// express, so the wire's default stands in.
fn version_of(version: Option<String>) -> String {
    version.filter(|v| !v.is_empty()).unwrap_or_else(|| "HTTP/1.1".to_owned())
}

fn headers_from(headers: Vec<InHeader>, leg: &str) -> Result<Vec<Header>, String> {
    headers
        .into_iter()
        .map(|header| {
            let name = header.name.ok_or_else(|| format!("{leg}.headers has an unnamed header"))?;
            Ok(Header { name, value: header.value })
        })
        .collect()
}

/// The response leg carries HAR's full four-state vocabulary, so it is read back
/// with it. The one judgement is a body with no `text` and no reason comment:
/// that is a foreign writer that did not keep the bytes, and calling it `Empty`
/// would assert a 4 MB stream was a 204 — the exact loss `BodyRec` exists to
/// prevent. Only a measured zero is empty.
fn response_body(content: Option<InContent>) -> Result<BodyRec, String> {
    let Some(content) = content else {
        return Ok(BodyRec::Empty);
    };
    if let Some(text) = content.text {
        return decode_body(&text, content.encoding.as_deref(), "response.content");
    }
    // With no bytes, the marker decides — and where it is absent, only a
    // measured zero is genuinely empty. A foreign writer's unkept body, or a
    // size claiming a body existed, is "the bytes are not in this file", never
    // `Empty`; and a `too_large` marker that lost its size keeps only that
    // coarser truth, since the size is what distinguished the two.
    let size = content.size.and_then(|size| usize::try_from(size).ok());
    Ok(match (reason_token(content.comment.as_deref()), size) {
        (Some("streaming"), _) => BodyRec::Streaming {
            content_type: content.mime_type.unwrap_or_else(|| "application/octet-stream".into()),
        },
        (Some("too_large"), Some(actual)) => {
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } }
        }
        (None, Some(0)) => BodyRec::Empty,
        _ => BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
    })
}

/// `postData` is absent for every request body the projection could not carry,
/// so its absence plus `bodySize` is the whole signal — the asymmetry HAR's own
/// shape forces. A GET with no body is the common case and must stay `Empty`.
fn request_body(post_data: Option<InPostData>, body_size: Option<i64>) -> Result<BodyRec, String> {
    if let Some(post_data) = post_data {
        if let Some(text) = post_data.text {
            return decode_body(&text, post_data.encoding.as_deref(), "request.postData");
        }
    }
    Ok(match body_size {
        None | Some(0) => BodyRec::Empty,
        Some(_) => BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
    })
}

/// The bytes always win over the file's `size`: a `size` that disagrees is what
/// the epic calls "a body claiming a size it does not have", and the ring's byte
/// accounting has to weigh what it actually holds or eviction goes wrong.
fn decode_body(text: &str, encoding: Option<&str>, field: &str) -> Result<BodyRec, String> {
    let bytes = if encoding.is_some_and(|e| e.eq_ignore_ascii_case("base64")) {
        STANDARD.decode(text).map_err(|e| format!("{field} is not valid base64: {e}"))?
    } else {
        text.as_bytes().to_vec()
    };
    let size = bytes.len();
    Ok(BodyRec::Captured { bytes: Bytes::from(bytes), size })
}

/// Reads back the token [`not_captured_comment`] wrote, and only that: a comment
/// a foreign tool put there for its own reasons must not be mistaken for
/// Trawler's marker.
fn reason_token(comment: Option<&str>) -> Option<&str> {
    comment?.strip_prefix("trawler: body not captured (")?.strip_suffix(')')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{captured, FlowBuilder};
    use serde_json::{json, Value};

    fn document(flows: &[Flow]) -> Value {
        serde_json::to_value(har_document(flows)).expect("serialize")
    }

    fn entry(flow: Flow) -> Value {
        document(&[flow])["log"]["entries"][0].clone()
    }

    fn full_flow() -> Flow {
        FlowBuilder::new()
            .method("POST")
            .uri("http://api.example.com/users?page=2")
            .req_header("Content-Type", "application/json")
            .req_header("Cookie", "sid=abc; theme=dark")
            .req_body(captured(br#"{"name":"x"}"#))
            .status(201)
            .resp_header("Content-Type", "application/json")
            .resp_header("Set-Cookie", "sid=new; Path=/; HttpOnly")
            .resp_body(captured(br#"{"id":7}"#))
            .build()
    }

    #[test]
    fn document_envelope_names_the_format_and_its_writer() {
        let doc = document(&[full_flow()]);
        assert_eq!(doc["log"]["version"], "1.2");
        assert_eq!(doc["log"]["creator"]["name"], "Trawler");
        assert_eq!(doc["log"]["creator"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(doc["log"]["entries"].as_array().expect("entries").len(), 1);
    }

    #[test]
    fn entries_keep_the_order_they_were_given() {
        let flows: Vec<Flow> = ["http://a/", "http://b/", "http://c/"]
            .iter()
            .map(|uri| FlowBuilder::new().uri(uri).status(200).build())
            .collect();
        let doc = document(&flows);
        let urls: Vec<&str> = doc["log"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e["request"]["url"].as_str().expect("url"))
            .collect();
        assert_eq!(urls, vec!["http://a/", "http://b/", "http://c/"]);
    }

    #[test]
    fn request_and_response_fields_map_across() {
        let e = entry(full_flow());
        assert_eq!(e["request"]["method"], "POST");
        assert_eq!(e["request"]["url"], "http://api.example.com/users?page=2");
        assert_eq!(e["request"]["httpVersion"], "HTTP/1.1");
        assert_eq!(e["response"]["status"], 201);
        assert_eq!(e["response"]["statusText"], "Created");
        assert_eq!(e["response"]["httpVersion"], "HTTP/1.1");
        assert_eq!(e["time"], 1);
        assert_eq!(e["timings"], json!({"send": 0, "wait": 1, "receive": 0}));
        assert_eq!(e["cache"], json!({}));
        assert!(e["startedDateTime"].as_str().expect("timestamp").contains('T'));
    }

    #[test]
    fn a_flow_still_awaiting_its_response_is_omitted() {
        let doc = document(&[FlowBuilder::new().build()]);
        assert_eq!(doc["log"]["entries"].as_array().expect("entries").len(), 0);

        let mixed = document(&[FlowBuilder::new().uri("http://pending/").build(), full_flow()]);
        let entries = mixed["log"]["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["request"]["url"], "http://api.example.com/users?page=2");
    }

    #[test]
    fn an_unmeasured_duration_is_hars_unknown_not_a_zero() {
        let mut flow = full_flow();
        flow.duration_ms = None;
        let e = entry(flow);
        assert_eq!(e["time"], -1);
    }

    // HAR sanctions -1 for `time`, but requires the three phases of `timings` to
    // be non-negative — so an unmeasured duration is unknown at the entry and
    // zero across the phases, never a negative phase no reader accepts. An
    // imported entry carrying `time: -1` reaches exactly this state (W3).
    #[test]
    fn an_unmeasured_duration_never_puts_a_negative_number_in_timings() {
        let mut flow = full_flow();
        flow.duration_ms = None;
        let e = entry(flow);
        assert_eq!(e["timings"], json!({"send": 0, "wait": 0, "receive": 0}));
    }

    #[test]
    fn header_lists_preserve_order_and_duplicates() {
        let e = entry(
            FlowBuilder::new()
                .req_header("Accept", "text/html")
                .req_header("Accept", "application/json")
                .status(200)
                .resp_header("Vary", "Accept")
                .build(),
        );
        assert_eq!(
            e["request"]["headers"],
            json!([
                {"name": "Accept", "value": "text/html"},
                {"name": "Accept", "value": "application/json"},
            ])
        );
        assert_eq!(e["response"]["headers"], json!([{"name": "Vary", "value": "Accept"}]));
    }

    #[test]
    fn header_and_body_sizes_are_unknown_unless_measured() {
        let e = entry(full_flow());
        assert_eq!(e["request"]["headersSize"], -1);
        assert_eq!(e["response"]["headersSize"], -1);
        assert_eq!(e["request"]["bodySize"], 12);
        assert_eq!(e["response"]["bodySize"], 8);
    }

    #[test]
    fn query_string_is_parsed_and_percent_decoded() {
        let params = |uri: &str| {
            entry(FlowBuilder::new().uri(uri).status(200).build())["request"]["queryString"].clone()
        };
        assert_eq!(params("http://x/path"), json!([]));
        assert_eq!(params("http://x/path?a=1"), json!([{"name": "a", "value": "1"}]));
        assert_eq!(
            params("http://x/path?tag=a&tag=b"),
            json!([{"name": "tag", "value": "a"}, {"name": "tag", "value": "b"}])
        );
        assert_eq!(
            params("http://x/path?q=hello%20world%26more"),
            json!([{"name": "q", "value": "hello world&more"}])
        );
        assert_eq!(params("http://x/path?flag="), json!([{"name": "flag", "value": ""}]));
    }

    #[test]
    fn cookies_come_from_the_headers_that_carry_them() {
        let e = entry(full_flow());
        assert_eq!(
            e["request"]["cookies"],
            json!([{"name": "sid", "value": "abc"}, {"name": "theme", "value": "dark"}])
        );
        assert_eq!(e["response"]["cookies"], json!([{"name": "sid", "value": "new"}]));

        let none = entry(FlowBuilder::new().status(200).build());
        assert_eq!(none["request"]["cookies"], json!([]));
        assert_eq!(none["response"]["cookies"], json!([]));
    }

    #[test]
    fn multiple_set_cookie_headers_each_yield_a_cookie() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_header("Set-Cookie", "a=1; Path=/")
                .resp_header("set-cookie", "b=2")
                .build(),
        );
        assert_eq!(
            e["response"]["cookies"],
            json!([{"name": "a", "value": "1"}, {"name": "b", "value": "2"}])
        );
    }

    #[test]
    fn redirect_url_is_the_location_header_or_empty() {
        let with = entry(
            FlowBuilder::new().status(302).resp_header("Location", "http://elsewhere/").build(),
        );
        assert_eq!(with["response"]["redirectURL"], "http://elsewhere/");
        assert_eq!(entry(full_flow())["response"]["redirectURL"], "");
    }

    #[test]
    fn status_text_is_canonical_and_empty_for_an_unknown_code() {
        let text = |status: u16| {
            entry(FlowBuilder::new().status(status).build())["response"]["statusText"].clone()
        };
        assert_eq!(text(200), "OK");
        assert_eq!(text(404), "Not Found");
        assert_eq!(text(599), "");
    }

    #[test]
    fn a_textual_body_stays_readable_text() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_header("Content-Type", "text/plain; charset=utf-8")
                .resp_body(captured(b"hello"))
                .build(),
        );
        assert_eq!(e["response"]["content"]["text"], "hello");
        assert_eq!(e["response"]["content"]["size"], 5);
        assert_eq!(e["response"]["content"]["mimeType"], "text/plain; charset=utf-8");
        assert!(e["response"]["content"].get("encoding").is_none());
        assert!(e["response"]["content"].get("comment").is_none());
    }

    #[test]
    fn a_binary_body_is_base64_under_hars_encoding_field() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_header("Content-Type", "image/png")
                .resp_body(captured(&[0x89, 0x50, 0x4e, 0x47]))
                .build(),
        );
        assert_eq!(e["response"]["content"]["encoding"], "base64");
        assert_eq!(e["response"]["content"]["text"], STANDARD.encode([0x89, 0x50, 0x4e, 0x47]));
        assert_eq!(e["response"]["content"]["size"], 4);
    }

    // A text content type is a claim about the bytes, not a guarantee; base64 is
    // what keeps the bytes recoverable when the claim is false.
    #[test]
    fn a_text_typed_body_that_is_not_utf8_falls_back_to_base64() {
        let invalid = [0xff, 0xfe, 0x00];
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_header("Content-Type", "text/plain")
                .resp_body(captured(&invalid))
                .build(),
        );
        assert_eq!(e["response"]["content"]["encoding"], "base64");
        assert_eq!(e["response"]["content"]["text"], STANDARD.encode(invalid));
    }

    #[test]
    fn an_empty_body_is_a_measured_zero_with_no_reason() {
        let e = entry(FlowBuilder::new().status(204).build());
        assert_eq!(e["response"]["content"]["size"], 0);
        assert!(e["response"]["content"].get("text").is_none());
        assert!(e["response"]["content"].get("comment").is_none());
        assert_eq!(e["response"]["bodySize"], 0);
    }

    #[test]
    fn a_streaming_body_is_reported_unavailable_and_names_its_reason() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_header("Content-Type", "text/event-stream")
                .resp_body(BodyRec::Streaming { content_type: "text/event-stream".into() })
                .build(),
        );
        assert_eq!(e["response"]["content"]["size"], -1);
        assert!(e["response"]["content"].get("text").is_none());
        assert_eq!(e["response"]["content"]["comment"], "trawler: body not captured (streaming)");
        assert_eq!(e["response"]["bodySize"], -1);
    }

    // The size is known — it is why the body was dropped — so it is reported,
    // while the absent `text` says the bytes are not here.
    #[test]
    fn a_too_large_body_reports_the_size_it_had_without_the_bytes() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_body(BodyRec::NotCaptured {
                    reason: NotCapturedReason::TooLarge { actual: 9_000_000 },
                })
                .build(),
        );
        assert_eq!(e["response"]["content"]["size"], 9_000_000);
        assert!(e["response"]["content"].get("text").is_none());
        assert_eq!(e["response"]["content"]["comment"], "trawler: body not captured (too_large)");
    }

    #[test]
    fn a_read_error_body_reports_neither_bytes_nor_size() {
        let e = entry(
            FlowBuilder::new()
                .status(200)
                .resp_body(BodyRec::NotCaptured { reason: NotCapturedReason::ReadError })
                .build(),
        );
        assert_eq!(e["response"]["content"]["size"], -1);
        assert!(e["response"]["content"].get("text").is_none());
        assert_eq!(e["response"]["content"]["comment"], "trawler: body not captured (read_error)");
    }

    #[test]
    fn post_data_carries_a_captured_request_body_and_nothing_else() {
        let e = entry(full_flow());
        assert_eq!(e["request"]["postData"]["text"], r#"{"name":"x"}"#);
        assert_eq!(e["request"]["postData"]["mimeType"], "application/json");
        assert!(e["request"]["postData"].get("encoding").is_none());

        let binary = entry(
            FlowBuilder::new()
                .req_header("Content-Type", "application/octet-stream")
                .req_body(captured(&[0x00, 0x01]))
                .status(200)
                .build(),
        );
        assert_eq!(binary["request"]["postData"]["encoding"], "base64");
        assert_eq!(binary["request"]["postData"]["text"], STANDARD.encode([0x00, 0x01]));
    }

    // HAR's postData cannot say "a body existed but was not captured", so the
    // field is absent and `bodySize: -1` carries the fact instead.
    #[test]
    fn an_uncaptured_request_body_omits_post_data_rather_than_claiming_emptiness() {
        for body in [
            BodyRec::Empty,
            BodyRec::Streaming { content_type: "application/grpc".into() },
            BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
        ] {
            let e = entry(FlowBuilder::new().req_body(body).status(200).build());
            assert!(e["request"].get("postData").is_none(), "postData must be absent: {e}");
        }
    }

    #[test]
    fn a_missing_content_type_falls_back_to_octet_stream() {
        let e = entry(FlowBuilder::new().status(200).resp_body(captured(b"x")).build());
        assert_eq!(e["response"]["content"]["mimeType"], "application/octet-stream");
        assert_eq!(e["response"]["content"]["encoding"], "base64");
    }

    #[test]
    fn not_captured_comment_only_speaks_for_the_states_that_lost_bytes() {
        assert!(not_captured_comment(&captured(b"x")).is_none());
        assert!(not_captured_comment(&BodyRec::Empty).is_none());
        assert!(not_captured_comment(&BodyRec::Streaming { content_type: "x".into() })
            .expect("reason")
            .contains("streaming"));
        assert!(not_captured_comment(&BodyRec::NotCaptured {
            reason: NotCapturedReason::TooLarge { actual: 1 }
        })
        .expect("reason")
        .contains("too_large"));
        assert!(not_captured_comment(&BodyRec::NotCaptured {
            reason: NotCapturedReason::ReadError
        })
        .expect("reason")
        .contains("read_error"));
    }

    mod import {
        use super::*;
        use crate::flow::FlowSource;

        fn har(entries: &Value) -> Vec<u8> {
            serde_json::to_vec(&json!({
                "log": { "version": "1.2", "creator": {"name": "Other", "version": "9"},
                         "entries": entries }
            }))
            .expect("serialize")
        }

        fn import(entries: &Value) -> Vec<Flow> {
            flows_from_har(&har(entries)).expect("import")
        }

        fn refusal(entries: &Value) -> String {
            flows_from_har(&har(entries)).expect_err("refusal").to_string()
        }

        fn minimal_entry() -> Value {
            json!({
                "startedDateTime": "2026-09-04T10:00:00Z",
                "time": 12,
                "request": {
                    "method": "GET", "url": "http://example.com/", "httpVersion": "HTTP/1.1",
                    "headers": [], "queryString": [], "headersSize": -1, "bodySize": 0
                },
                "response": {
                    "status": 200, "statusText": "OK", "httpVersion": "HTTP/1.1",
                    "headers": [], "content": {"size": 0, "mimeType": "text/plain"},
                    "redirectURL": "", "headersSize": -1, "bodySize": 0
                },
                "cache": {},
                "timings": {"send": 0, "wait": 12, "receive": 0}
            })
        }

        fn one(entry: &Value) -> Flow {
            import(&json!([entry])).into_iter().next().expect("one flow")
        }

        // The document round-trips through Trawler's own writer, so the parser
        // is asserted against the projection it inverts rather than against a
        // hand-written fixture that could drift from what the export emits.
        fn reimport(flows: &[Flow]) -> Vec<Flow> {
            let document = serde_json::to_vec(&har_document(flows)).expect("serialize");
            flows_from_har(&document).expect("reimport")
        }

        #[test]
        fn an_entry_becomes_a_flow_marked_imported() {
            let flow = one(&minimal_entry());
            assert_eq!(flow.source, FlowSource::Imported);
            assert_eq!(flow.request.method, "GET");
            assert_eq!(flow.request.uri, "http://example.com/");
            assert_eq!(flow.request.version, "HTTP/1.1");
            assert_eq!(flow.host(), Some("example.com"), "the host is re-derived, never read");
            assert_eq!(flow.duration_ms, Some(12));
            let response = flow.response.expect("response");
            assert_eq!(response.status, 200);
            assert_eq!(response.version, "HTTP/1.1");
        }

        #[test]
        fn the_captured_instant_is_preserved_and_the_id_is_fresh() {
            let flow = one(&minimal_entry());
            assert_eq!(rfc3339(flow.started_at), "2026-09-04T10:00:00Z");
            let again = one(&minimal_entry());
            assert_ne!(flow.id, again.id, "an id is this store's handle, never the file's");
            assert_eq!(rfc3339(again.started_at), rfc3339(flow.started_at));
        }

        // The export writes newest-first, so file order must be reversed to land
        // in a newest-first ring the way the file read.
        #[test]
        fn entries_come_back_oldest_first_so_the_ring_reads_them_in_file_order() {
            let mut entries = Vec::new();
            for url in ["http://newest/", "http://middle/", "http://oldest/"] {
                let mut entry = minimal_entry();
                entry["request"]["url"] = json!(url);
                entries.push(entry);
            }
            let uris: Vec<String> =
                import(&json!(entries)).iter().map(|f| f.request.uri.clone()).collect();
            assert_eq!(uris, ["http://oldest/", "http://middle/", "http://newest/"]);
        }

        #[test]
        fn an_unmeasured_duration_comes_back_as_the_absence_it_was() {
            for time in [json!(-1), json!(-0.5), Value::Null] {
                let mut entry = minimal_entry();
                entry["time"] = time.clone();
                assert_eq!(one(&entry).duration_ms, None, "time {time}");
            }
            let mut entry = minimal_entry();
            entry.as_object_mut().expect("entry").remove("time");
            assert_eq!(one(&entry).duration_ms, None, "an absent time");
        }

        // The phases are zeroed beside a `-1` total, so reading them back would
        // contradict the total they sit next to.
        #[test]
        fn the_timing_phases_are_never_read_back_as_the_duration() {
            let mut entry = minimal_entry();
            entry["time"] = json!(-1);
            entry["timings"] = json!({"send": 3, "wait": 40, "receive": 5});
            assert_eq!(one(&entry).duration_ms, None);
        }

        #[test]
        fn a_fractional_duration_rounds_rather_than_truncating() {
            let mut entry = minimal_entry();
            entry["time"] = json!(12.6);
            assert_eq!(one(&entry).duration_ms, Some(13));
        }

        #[test]
        fn headers_keep_their_order_their_duplicates_and_their_empty_values() {
            let mut entry = minimal_entry();
            entry["request"]["headers"] = json!([
                {"name": "Accept", "value": "text/html"},
                {"name": "X-Trace", "value": "1"},
                {"name": "Accept", "value": "application/json"},
                {"name": "X-Empty"},
            ]);
            let flow = one(&entry);
            let headers: Vec<(&str, &str)> =
                flow.request.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect();
            assert_eq!(
                headers,
                [
                    ("Accept", "text/html"),
                    ("X-Trace", "1"),
                    ("Accept", "application/json"),
                    ("X-Empty", ""),
                ]
            );
        }

        #[test]
        fn an_entry_with_no_response_is_a_request_only_flow() {
            let mut entry = minimal_entry();
            entry.as_object_mut().expect("entry").remove("response");
            let flow = one(&entry);
            assert!(flow.response.is_none());
            assert_eq!(flow.request.method, "GET");
        }

        #[test]
        fn a_missing_http_version_falls_back_to_the_wire_default() {
            let mut entry = minimal_entry();
            entry["request"].as_object_mut().expect("request").remove("httpVersion");
            entry["response"]["httpVersion"] = json!("");
            let flow = one(&entry);
            assert_eq!(flow.request.version, "HTTP/1.1");
            assert_eq!(flow.response.expect("response").version, "HTTP/1.1");
        }

        // Every one of these was derived from something the flow already carries,
        // so reading it back would duplicate a fact rather than restore one.
        #[test]
        fn the_derived_and_unretained_fields_are_ignored_without_error() {
            let mut entry = minimal_entry();
            entry["request"]["queryString"] = json!([{"name": "ignored", "value": "yes"}]);
            entry["request"]["cookies"] = json!([{"name": "sid", "value": "abc"}]);
            entry["request"]["headersSize"] = json!(4096);
            entry["response"]["cookies"] = json!([{"name": "sid", "value": "new"}]);
            entry["response"]["statusText"] = json!("Totally Fine");
            entry["response"]["redirectURL"] = json!("http://elsewhere/");
            entry["cache"] = json!({"beforeRequest": {"eTag": "x"}});
            entry["serverIPAddress"] = json!("127.0.0.1");
            entry["connection"] = json!("443");
            let flow = one(&entry);
            assert!(flow.request.headers.is_empty(), "cookies never become headers");
            let response = flow.response.expect("response");
            assert!(response.headers.is_empty(), "redirectURL never becomes a Location");
            assert_eq!(response.status, 200);
        }

        #[test]
        fn a_textual_body_comes_back_as_its_bytes() {
            let mut entry = minimal_entry();
            entry["response"]["content"] =
                json!({"size": 13, "mimeType": "application/json", "text": "{\"id\":\"seven\"}"});
            let body = one(&entry).response.expect("response").body;
            assert_eq!(body.bytes().expect("bytes").as_ref(), br#"{"id":"seven"}"#);
        }

        #[test]
        fn a_base64_body_comes_back_as_the_bytes_it_encoded() {
            let mut entry = minimal_entry();
            entry["response"]["content"] = json!({
                "size": 4, "mimeType": "image/png", "text": "AAECAw==", "encoding": "base64"
            });
            let body = one(&entry).response.expect("response").body;
            assert_eq!(body.bytes().expect("bytes").as_ref(), &[0, 1, 2, 3]);
        }

        // The `size` is a claim about the bytes; the ring weighs what it actually
        // holds, so a disagreeing size cannot be allowed to set the weight.
        #[test]
        fn the_bytes_outrank_a_size_that_disagrees_with_them() {
            let mut entry = minimal_entry();
            entry["response"]["content"] =
                json!({"size": 9_999_999, "mimeType": "text/plain", "text": "four"});
            let flow = one(&entry);
            assert_eq!(flow.retained_bytes(), 4);
            match flow.response.expect("response").body {
                BodyRec::Captured { size, .. } => assert_eq!(size, 4),
                other => panic!("expected a captured body, got {other:?}"),
            }
        }

        #[test]
        fn each_uncaptured_body_comes_back_in_the_state_the_file_stated() {
            let cases = [
                (
                    json!({"size": -1, "mimeType": "text/event-stream",
                           "comment": "trawler: body not captured (streaming)"}),
                    "streaming",
                ),
                (
                    json!({"size": 4096, "mimeType": "application/zip",
                           "comment": "trawler: body not captured (too_large)"}),
                    "not_captured",
                ),
                (
                    json!({"size": -1, "mimeType": "text/plain",
                           "comment": "trawler: body not captured (read_error)"}),
                    "not_captured",
                ),
                (json!({"size": 0, "mimeType": "text/plain"}), "empty"),
            ];
            for (content, state) in cases {
                let mut entry = minimal_entry();
                entry["response"]["content"] = content.clone();
                let body = one(&entry).response.expect("response").body;
                assert_eq!(body.state(), state, "content {content}");
            }
        }

        #[test]
        fn a_streamed_body_keeps_the_content_type_it_was_streaming() {
            let mut entry = minimal_entry();
            entry["response"]["content"] = json!({
                "size": -1, "mimeType": "text/event-stream",
                "comment": "trawler: body not captured (streaming)"
            });
            match one(&entry).response.expect("response").body {
                BodyRec::Streaming { content_type } => {
                    assert_eq!(content_type, "text/event-stream");
                }
                other => panic!("expected a streaming body, got {other:?}"),
            }
        }

        #[test]
        fn a_too_large_body_keeps_the_size_that_got_it_dropped() {
            let mut entry = minimal_entry();
            entry["response"]["content"] = json!({
                "size": 4096, "mimeType": "application/zip",
                "comment": "trawler: body not captured (too_large)"
            });
            match one(&entry).response.expect("response").body {
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } } => {
                    assert_eq!(actual, 4096);
                }
                other => panic!("expected a too-large body, got {other:?}"),
            }
        }

        // The size is the only thing separating too-large from a read error, so a
        // marker that lost it keeps the coarser truth rather than inventing one.
        #[test]
        fn a_too_large_marker_without_a_size_degrades_to_a_read_error() {
            for size in [json!(-1), Value::Null] {
                let mut entry = minimal_entry();
                entry["response"]["content"] = json!({
                    "size": size, "mimeType": "application/zip",
                    "comment": "trawler: body not captured (too_large)"
                });
                match one(&entry).response.expect("response").body {
                    BodyRec::NotCaptured { reason: NotCapturedReason::ReadError } => {}
                    other => panic!("expected a read error for size {size}, got {other:?}"),
                }
            }
        }

        // The foreign-HAR case: calling this `Empty` would assert a 4 MB response
        // was a 204 — the exact loss `BodyRec` exists to prevent.
        #[test]
        fn a_foreign_body_the_file_does_not_carry_is_not_captured_never_empty() {
            for size in [json!(-1), json!(4_000_000), Value::Null] {
                let mut entry = minimal_entry();
                entry["response"]["content"] = json!({"size": size, "mimeType": "text/html"});
                let body = one(&entry).response.expect("response").body;
                assert_eq!(body.state(), "not_captured", "size {size}");
            }
        }

        // A comment a foreign tool wrote for its own reasons is not Trawler's
        // marker: it never restores a marked state, and it never overrides what
        // the size and text beside it already say.
        #[test]
        fn a_foreign_comment_is_not_mistaken_for_the_marker() {
            let foreign = "compressed by Charles";
            let mut streamish = minimal_entry();
            streamish["response"]["content"] =
                json!({"size": -1, "mimeType": "text/event-stream", "comment": foreign});
            assert_eq!(
                one(&streamish).response.expect("response").body.state(),
                "not_captured",
                "a foreign comment never restores `streaming`"
            );

            let mut sized = minimal_entry();
            sized["response"]["content"] =
                json!({"size": 4096, "mimeType": "application/zip", "comment": foreign});
            match one(&sized).response.expect("response").body {
                BodyRec::NotCaptured { reason: NotCapturedReason::ReadError } => {}
                other => panic!("a foreign comment never restores `too_large`: {other:?}"),
            }

            let mut measured_zero = minimal_entry();
            measured_zero["response"]["content"] =
                json!({"size": 0, "mimeType": "text/plain", "comment": foreign});
            assert_eq!(
                one(&measured_zero).response.expect("response").body.state(),
                "empty",
                "a measured zero stays empty whatever prose sits beside it"
            );
        }

        #[test]
        fn a_missing_content_object_is_an_empty_body() {
            let mut entry = minimal_entry();
            entry["response"].as_object_mut().expect("response").remove("content");
            assert_eq!(one(&entry).response.expect("response").body.state(), "empty");
        }

        #[test]
        fn a_request_body_comes_back_from_post_data() {
            let mut entry = minimal_entry();
            entry["request"]["postData"] =
                json!({"mimeType": "application/json", "text": "{\"a\":1}"});
            entry["request"]["bodySize"] = json!(7);
            let body = one(&entry).request.body;
            assert_eq!(body.bytes().expect("bytes").as_ref(), br#"{"a":1}"#);
        }

        // `postData` has no shape for an uncaptured body, so its absence plus
        // `bodySize` is the whole signal — and a GET with no body must stay empty.
        #[test]
        fn an_absent_post_data_is_read_through_the_body_size_beside_it() {
            let cases = [
                (json!(0), "empty"),
                (Value::Null, "empty"),
                (json!(-1), "not_captured"),
                (json!(512), "not_captured"),
            ];
            for (body_size, state) in cases {
                let mut entry = minimal_entry();
                entry["request"]["bodySize"] = body_size.clone();
                assert_eq!(one(&entry).request.body.state(), state, "bodySize {body_size}");
            }
        }

        #[test]
        fn a_document_that_is_not_json_is_refused_as_such() {
            let error = flows_from_har(b"<html>nope</html>").expect_err("refusal").to_string();
            assert!(error.contains("not a JSON document"), "{error}");
        }

        #[test]
        fn a_document_missing_its_members_names_the_one_it_wants() {
            for (document, member) in [
                (json!({}), "log"),
                (json!({"log": {"entries": []}}), "log.version"),
                (json!({"log": {"version": "1.2"}}), "log.entries"),
            ] {
                let bytes = serde_json::to_vec(&document).expect("serialize");
                let error = flows_from_har(&bytes).expect_err("refusal").to_string();
                assert_eq!(error, format!("missing {member}"), "{document}");
            }
        }

        #[test]
        fn a_version_this_epic_does_not_speak_is_refused_naming_it() {
            let bytes =
                serde_json::to_vec(&json!({"log": {"version": "1.3", "entries": []}})).unwrap();
            let error = flows_from_har(&bytes).expect_err("refusal").to_string();
            assert!(error.contains("1.3"), "{error}");
            assert!(error.contains("1.2"), "the refusal says what is supported: {error}");
        }

        #[test]
        fn an_empty_entry_list_is_a_document_that_imported_nothing() {
            assert!(import(&json!([])).is_empty());
        }

        // A caller with a 900-entry file cannot fix what it cannot locate.
        #[test]
        fn an_entry_level_fault_names_its_index_and_its_field() {
            let mut broken = minimal_entry();
            broken["request"]["url"] = json!("");
            let error = refusal(&json!([minimal_entry(), broken]));
            assert!(error.starts_with("log.entries[1]:"), "{error}");
            assert!(error.contains("request.url"), "{error}");
        }

        #[test]
        fn every_malformation_is_refused_with_a_reason() {
            let cases: Vec<(Value, &str)> = vec![
                (
                    {
                        let mut e = minimal_entry();
                        e.as_object_mut().expect("entry").remove("request");
                        e
                    },
                    "missing request",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["request"].as_object_mut().expect("request").remove("method");
                        e
                    },
                    "request.method is missing",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["request"]["method"] = json!("");
                        e
                    },
                    "request.method is empty",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["request"].as_object_mut().expect("request").remove("url");
                        e
                    },
                    "request.url is missing",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e.as_object_mut().expect("entry").remove("startedDateTime");
                        e
                    },
                    "missing startedDateTime",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["startedDateTime"] = json!("yesterday");
                        e
                    },
                    "not RFC3339",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["response"].as_object_mut().expect("response").remove("status");
                        e
                    },
                    "response.status is missing",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["response"]["status"] = json!(99);
                        e
                    },
                    "not an HTTP status code",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["response"]["status"] = json!(600);
                        e
                    },
                    "not an HTTP status code",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["request"]["headers"] = json!([{"value": "orphan"}]);
                        e
                    },
                    "unnamed header",
                ),
                (
                    {
                        let mut e = minimal_entry();
                        e["response"]["content"] = json!({
                            "size": 4, "mimeType": "image/png", "text": "!!not base64!!",
                            "encoding": "base64"
                        });
                        e
                    },
                    "not valid base64",
                ),
            ];
            for (entry, expected) in cases {
                let error = refusal(&json!([entry]));
                assert!(error.contains(expected), "expected {expected:?} in {error:?}");
            }
        }

        #[test]
        fn a_full_exchange_survives_export_and_reimport() {
            let original = full_flow();
            let back = reimport(std::slice::from_ref(&original));
            assert_eq!(back.len(), 1);
            let back = &back[0];
            assert_eq!(back.request.method, original.request.method);
            assert_eq!(back.request.uri, original.request.uri);
            assert_eq!(back.request.version, original.request.version);
            assert_eq!(back.request.body.bytes(), original.request.body.bytes());
            assert_eq!(rfc3339(back.started_at), rfc3339(original.started_at));
            assert_eq!(back.duration_ms, original.duration_ms);
            assert_eq!(back.source, FlowSource::Imported);
            let (before, after) =
                (original.response.expect("response"), back.response.as_ref().expect("response"));
            assert_eq!(after.status, before.status);
            assert_eq!(after.body.bytes(), before.body.bytes());
            let names: Vec<&str> = after.headers.iter().map(|h| h.name.as_str()).collect();
            assert_eq!(names, ["Content-Type", "Set-Cookie"]);
        }
    }

    mod properties {
        use super::*;
        use crate::flow::FlowSource;
        use proptest::prelude::*;

        fn body() -> impl Strategy<Value = BodyRec> {
            prop_oneof![
                proptest::collection::vec(any::<u8>(), 0..32).prop_map(|b| captured(&b)),
                Just(BodyRec::Empty),
                Just(BodyRec::Streaming { content_type: "text/event-stream".into() }),
                (0usize..9_999_999).prop_map(|actual| BodyRec::NotCaptured {
                    reason: NotCapturedReason::TooLarge { actual }
                }),
                Just(BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }),
            ]
        }

        fn content_type() -> impl Strategy<Value = &'static str> {
            prop_oneof![
                Just("text/plain"),
                Just("application/json"),
                Just("image/png"),
                Just("application/octet-stream"),
            ]
        }

        fn headers() -> impl Strategy<Value = Vec<(&'static str, &'static str)>> {
            proptest::collection::vec(
                (
                    prop_oneof![Just("Accept"), Just("X-Trace"), Just("Authorization")],
                    prop_oneof![Just("one"), Just("two"), Just("")],
                ),
                0..4,
            )
        }

        fn flow() -> impl Strategy<Value = Flow> {
            (
                prop_oneof![Just("GET"), Just("POST"), Just("DELETE")],
                prop_oneof![
                    Just("http://x/path"),
                    Just("http://x/path?a=1&a=2"),
                    Just("http://x/"),
                    Just("not a uri"),
                ],
                body(),
                body(),
                prop_oneof![Just(None), (100u16..600).prop_map(Some)],
                prop_oneof![Just(None), (0u64..10_000).prop_map(Some)],
                (content_type(), content_type()),
                (headers(), headers()),
            )
                .prop_map(
                    |(
                        method,
                        uri,
                        req_body,
                        resp_body,
                        status,
                        duration,
                        (req_type, resp_type),
                        (req_headers, resp_headers),
                    )| {
                        let mut builder = FlowBuilder::new().method(method).uri(uri);
                        builder = builder.req_header("Content-Type", req_type);
                        for (name, value) in req_headers {
                            builder = builder.req_header(name, value);
                        }
                        builder = builder.req_body(req_body);
                        if let Some(status) = status {
                            builder = builder.status(status).resp_header("Content-Type", resp_type);
                            for (name, value) in resp_headers {
                                builder = builder.resp_header(name, value);
                            }
                            builder = builder.resp_body(resp_body);
                        }
                        let mut flow = builder.build();
                        if flow.response.is_some() {
                            flow.duration_ms = duration;
                        }
                        flow
                    },
                )
        }

        // The load-bearing claim: whatever the ring holds, the file Trawler
        // writes is valid HAR 1.2 — every required key present, every unknown a
        // sentinel, and never a body claiming bytes it does not carry.
        proptest! {
            #[test]
            fn every_entry_carries_the_har_1_2_required_fields(flows in proptest::collection::vec(flow(), 0..8)) {
                let doc = serde_json::to_value(har_document(&flows)).expect("serialize");
                prop_assert_eq!(&doc["log"]["version"], "1.2");
                let entries = doc["log"]["entries"].as_array().expect("entries");
                prop_assert_eq!(
                    entries.len(),
                    flows.iter().filter(|f| f.response.is_some()).count()
                );

                for e in entries {
                    for key in ["startedDateTime", "time", "request", "response", "cache", "timings"] {
                        prop_assert!(e.get(key).is_some(), "entry missing {}: {}", key, e);
                    }
                    for key in ["method", "url", "httpVersion", "cookies", "headers", "queryString", "headersSize", "bodySize"] {
                        prop_assert!(e["request"].get(key).is_some(), "request missing {}", key);
                    }
                    for key in ["status", "statusText", "httpVersion", "cookies", "headers", "content", "redirectURL", "headersSize", "bodySize"] {
                        prop_assert!(e["response"].get(key).is_some(), "response missing {}", key);
                    }

                    let time = e["time"].as_i64().expect("time");
                    let t = &e["timings"];
                    for phase in ["send", "wait", "receive"] {
                        let value = t[phase].as_i64();
                        prop_assert!(value.is_some(), "timings missing {}", phase);
                        prop_assert!(
                            value.expect("phase") >= 0,
                            "HAR requires a non-negative {}: {}", phase, t
                        );
                    }
                    if time >= 0 {
                        let sum: i64 = ["send", "wait", "receive"]
                            .iter()
                            .map(|p| t[p].as_i64().expect("phase"))
                            .sum();
                        prop_assert_eq!(sum, time, "timings must sum to time: {}", e);
                    }

                    // A body that carries text must report the size of what it
                    // carries; a body without text must never claim a size it
                    // cannot back — except a too-large body, whose whole point is
                    // that the size is known and the bytes are not.
                    let content = &e["response"]["content"];
                    prop_assert!(content.get("size").is_some(), "content missing size");
                    prop_assert!(content.get("mimeType").is_some(), "content missing mimeType");
                    match content.get("text") {
                        Some(_) => prop_assert!(
                            content["size"].as_i64().expect("size") >= 0,
                            "text present with unknown size: {}", content
                        ),
                        None => prop_assert!(
                            content.get("comment").is_some() || content["size"] == 0,
                            "a bodyless content must say why or be a measured zero: {}", content
                        ),
                    }
                    if let Some(post) = e["request"].get("postData") {
                        prop_assert!(post.get("text").is_some(), "postData missing text");
                        prop_assert!(post.get("mimeType").is_some(), "postData missing mimeType");
                    }
                }
            }
        }

        // The WU's central claim, and an all-inputs one: whatever the ring can
        // hold, exported and read back, compares equal on exactly what DEC-30
        // says the format preserves — and the property names the unpreserved
        // facts too, so the boundary is documented rather than assumed.
        proptest! {
            #[test]
            fn a_flow_survives_export_and_reimport(flows in proptest::collection::vec(flow(), 0..6)) {
                let document = serde_json::to_vec(&har_document(&flows)).expect("serialize");
                let back = flows_from_har(&document).expect("a document Trawler wrote is one it reads");

                // The export omits a flow still awaiting its response, and the
                // import reverses the newest-first file into ring order.
                let exported: Vec<&Flow> = flows.iter().filter(|f| f.response.is_some()).collect();
                let expected: Vec<&&Flow> = exported.iter().rev().collect();
                prop_assert_eq!(back.len(), expected.len());

                for (before, after) in expected.iter().zip(&back) {
                    prop_assert_eq!(&after.request.method, &before.request.method);
                    prop_assert_eq!(&after.request.uri, &before.request.uri);
                    prop_assert_eq!(&after.request.version, &before.request.version);
                    prop_assert_eq!(rfc3339(after.started_at), rfc3339(before.started_at));
                    prop_assert_eq!(after.duration_ms, before.duration_ms);
                    prop_assert_eq!(after.source, FlowSource::Imported);
                    prop_assert_eq!(after.host(), before.host(), "the host is re-derived from the URI");

                    prop_assert_eq!(
                        header_pairs(&after.request.headers),
                        header_pairs(&before.request.headers),
                        "headers keep their order and their duplicates"
                    );
                    assert_body_survived(&before.request.body, &after.request.body, Leg::Request)?;

                    let before_res = before.response.as_ref().expect("exported flows have responses");
                    let after_res = after.response.as_ref().expect("a response round-trips");
                    prop_assert_eq!(after_res.status, before_res.status);
                    prop_assert_eq!(&after_res.version, &before_res.version);
                    prop_assert_eq!(
                        header_pairs(&after_res.headers),
                        header_pairs(&before_res.headers)
                    );
                    assert_body_survived(&before_res.body, &after_res.body, Leg::Response)?;
                }
            }
        }

        fn header_pairs(headers: &[Header]) -> Vec<(&str, &str)> {
            headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect()
        }

        #[derive(Clone, Copy)]
        enum Leg {
            Request,
            Response,
        }

        /// Body *bytes* round-trip exactly; body *state* round-trips too, with the
        /// one exception HAR's shape forces — `postData` cannot express an
        /// uncaptured request body, so every uncaptured state on the request leg
        /// collapses to the coarsest honest one. Asserted rather than tolerated,
        /// so the collapse cannot silently widen.
        fn assert_body_survived(
            before: &BodyRec,
            after: &BodyRec,
            leg: Leg,
        ) -> Result<(), TestCaseError> {
            prop_assert_eq!(after.bytes(), before.bytes(), "captured bytes are preserved exactly");
            match (before, after) {
                (BodyRec::Captured { size: a, .. }, BodyRec::Captured { size: b, .. }) => {
                    prop_assert_eq!(a, b);
                }
                (
                    BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: a } },
                    BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: b } },
                ) => prop_assert_eq!(a, b, "the size that got a body dropped is preserved"),
                (BodyRec::Empty, BodyRec::Empty)
                | (BodyRec::Streaming { .. }, BodyRec::Streaming { .. })
                | (
                    BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
                    BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
                ) => {}
                (before, BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }) => {
                    prop_assert!(
                        matches!(leg, Leg::Request),
                        "only the request leg collapses an uncaptured state, got {:?}",
                        before
                    );
                    prop_assert!(
                        !matches!(before, BodyRec::Captured { .. } | BodyRec::Empty),
                        "a captured or empty request body never collapses, got {:?}",
                        before
                    );
                }
                (before, after) => prop_assert!(
                    false,
                    "body state changed across the round trip: {:?} -> {:?}",
                    before,
                    after
                ),
            }
            Ok(())
        }

        // Import is the first endpoint taking untrusted structured input, so
        // "never a panic" is an all-inputs claim. Random bytes mostly fail at the
        // JSON gate, so the interesting half is documents shaped almost right.
        proptest! {
            #[test]
            fn the_parser_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
                let _ = flows_from_har(&bytes);
            }

            #[test]
            fn the_parser_never_panics_on_a_near_miss_document(document in near_miss_document()) {
                let bytes = serde_json::to_vec(&document).expect("serialize");
                let _ = flows_from_har(&bytes);
            }
        }

        /// A HAR-shaped document whose every member may be the wrong type, out of
        /// range, or missing — the shapes a hostile or merely broken writer emits.
        fn near_miss_document() -> impl Strategy<Value = serde_json::Value> {
            let scalar = prop_oneof![
                Just(serde_json::Value::Null),
                Just(serde_json::json!(-1)),
                Just(serde_json::json!(0)),
                Just(serde_json::json!(i64::MIN)),
                Just(serde_json::json!(f64::MAX)),
                Just(serde_json::json!("")),
                Just(serde_json::json!("1.2")),
                Just(serde_json::json!([])),
                Just(serde_json::json!({})),
                Just(serde_json::json!("!!not base64!!")),
            ];
            let entry = proptest::collection::hash_map(
                prop_oneof![
                    Just("startedDateTime"),
                    Just("time"),
                    Just("request"),
                    Just("response"),
                ],
                scalar.clone(),
                0..4,
            );
            (scalar, proptest::collection::vec(entry, 0..4)).prop_map(|(version, entries)| {
                serde_json::json!({ "log": { "version": version, "entries": entries } })
            })
        }
    }
}
