//! HAR 1.2 projection of captured flows — the format Trawler exchanges captures
//! in. Every field a HAR reader requires is present; every fact Trawler does not
//! hold is HAR's own sentinel (`-1`, `""`, `{}`) rather than a fabricated value,
//! because a reader cannot tell an invented number from a measured one.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;

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

    mod properties {
        use super::*;
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
            )
                .prop_map(|(method, uri, req_body, resp_body, status, duration)| {
                    let mut builder = FlowBuilder::new().method(method).uri(uri).req_body(req_body);
                    if let Some(status) = status {
                        builder = builder.status(status).resp_body(resp_body);
                    }
                    let mut flow = builder.build();
                    if flow.response.is_some() {
                        flow.duration_ms = duration;
                    }
                    flow
                })
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
    }
}
