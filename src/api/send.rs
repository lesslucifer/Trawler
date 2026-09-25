use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;
use http::header::{HeaderMap, HeaderName, HeaderValue, HOST};
use http::{Method, Uri};
use serde::Deserialize;

use crate::flow::{BodyRec, Flow, Header, NotCapturedReason};
use crate::sender::Outgoing;

/// The JSON both send doors take. Every field is optional here because on a
/// replay an absent field means "the seed's own value"; the compose door checks
/// for what it requires. An unknown field is refused, not ignored: a typo'd
/// `header` would otherwise silently send the seed's headers.
#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct RequestFields {
    method: Option<String>,
    url: Option<String>,
    headers: Option<Vec<HeaderField>>,
    body: Option<String>,
    body_base64: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct HeaderField {
    name: String,
    value: String,
}

/// Why nothing was sent. `Malformed` is the caller's request (400);
/// `SeedLacks` is a well-formed replay of a seed that cannot be sent as it
/// stands, so the caller has to supply what it lacks (422).
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Malformed(String),
    SeedLacks(String),
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        match self {
            Refusal::Malformed(reason) => (StatusCode::BAD_REQUEST, reason).into_response(),
            Refusal::SeedLacks(reason) => {
                (StatusCode::UNPROCESSABLE_ENTITY, reason).into_response()
            }
        }
    }
}

fn malformed(reason: impl Into<String>) -> Refusal {
    Refusal::Malformed(reason.into())
}

/// An empty body is `{}`, so `curl -X POST …/replay` re-sends the seed as it was.
/// The content type is not read, the import door's rule, so `curl -d` works.
pub fn parse_fields(body: &[u8]) -> Result<RequestFields, Refusal> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(RequestFields::default());
    }
    serde_json::from_slice(body).map_err(|e| malformed(format!("request body: {e}")))
}

/// The seed with the overrides laid over it. A given header list replaces the
/// seed's whole. The seed's own `Host` is dropped, so it follows the URL: only a
/// `Host` the caller wrote is "typed".
pub fn from_seed(seed: &Flow, fields: RequestFields) -> Result<Outgoing, Refusal> {
    let body = override_body(&fields)?;
    let method = match fields.method {
        Some(method) => parse_method(&method)?,
        None => Method::from_bytes(seed.request.method.as_bytes()).map_err(|_| {
            Refusal::SeedLacks(format!(
                "the seed's method {:?} is not an HTTP token; give a method to replace it",
                seed.request.method
            ))
        })?,
    };
    let uri = match fields.url {
        Some(url) => parse_url(&url)?,
        None => absolute_http(&seed.request.uri).ok_or_else(|| {
            Refusal::SeedLacks(
                "the seed's url is not absolute http(s); give a url to replace it".into(),
            )
        })?,
    };
    let headers = match fields.headers {
        Some(given) => header_map(given.into_iter().map(|h| (h.name, h.value)))?,
        None => seed_headers(&seed.request.headers)?,
    };
    let body = match body {
        Some(body) => body,
        None => seed_body(&seed.request.body)?,
    };
    Ok(Outgoing { method, uri, headers, body })
}

/// A request from nothing: a method and a URL are required, and absent headers
/// or body are none.
pub fn from_scratch(fields: RequestFields) -> Result<Outgoing, Refusal> {
    let body = override_body(&fields)?.unwrap_or_default();
    let method = parse_method(&fields.method.ok_or_else(|| malformed("method is required"))?)?;
    let uri = parse_url(&fields.url.ok_or_else(|| malformed("url is required"))?)?;
    let headers =
        header_map(fields.headers.unwrap_or_default().into_iter().map(|h| (h.name, h.value)))?;
    Ok(Outgoing { method, uri, headers, body })
}

fn override_body(fields: &RequestFields) -> Result<Option<Bytes>, Refusal> {
    match (&fields.body, &fields.body_base64) {
        (Some(_), Some(_)) => Err(malformed("give body or body_base64, not both")),
        (Some(text), None) => Ok(Some(Bytes::copy_from_slice(text.as_bytes()))),
        (None, Some(encoded)) => STANDARD
            .decode(encoded)
            .map(|bytes| Some(Bytes::from(bytes)))
            .map_err(|_| malformed("body_base64 is not base64")),
        (None, None) => Ok(None),
    }
}

/// A body that was never captured is never sent as an empty one — the
/// principle the export and the import already hold.
fn seed_body(body: &BodyRec) -> Result<Bytes, Refusal> {
    let absence = match body {
        BodyRec::Captured { bytes, .. } => return Ok(bytes.clone()),
        BodyRec::Empty => return Ok(Bytes::new()),
        BodyRec::Streaming { .. } => "streaming",
        BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { .. } } => "too_large",
        BodyRec::NotCaptured { reason: NotCapturedReason::ReadError } => "read_error",
    };
    Err(Refusal::SeedLacks(format!(
        "the seed's request body was not captured ({absence}); give a body to replace it"
    )))
}

fn seed_headers(headers: &[Header]) -> Result<HeaderMap, Refusal> {
    header_map(
        headers
            .iter()
            .filter(|h| !h.name.eq_ignore_ascii_case(HOST.as_str()))
            .map(|h| (h.name.clone(), h.value.clone())),
    )
    .map_err(|refusal| match refusal {
        Refusal::Malformed(reason) | Refusal::SeedLacks(reason) => {
            Refusal::SeedLacks(format!("the seed's {reason}; give headers to replace them"))
        }
    })
}

fn header_map(headers: impl Iterator<Item = (String, String)>) -> Result<HeaderMap, Refusal> {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| malformed(format!("header {name:?} is not a valid header name")))?;
        let header_value = HeaderValue::from_str(&value)
            .map_err(|_| malformed(format!("header {name:?} has a value the wire cannot carry")))?;
        map.append(header_name, header_value);
    }
    Ok(map)
}

fn parse_method(method: &str) -> Result<Method, Refusal> {
    Method::from_bytes(method.as_bytes()).map_err(|_| malformed("method must be an HTTP token"))
}

fn parse_url(url: &str) -> Result<Uri, Refusal> {
    absolute_http(url).ok_or_else(|| malformed("url must be absolute http(s)"))
}

fn absolute_http(url: &str) -> Option<Uri> {
    let uri = url.parse::<Uri>().ok()?;
    let scheme = uri.scheme_str()?;
    let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    (web && uri.authority().is_some()).then_some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{captured, FlowBuilder};

    fn fields(json: &str) -> RequestFields {
        parse_fields(json.as_bytes()).expect("fields")
    }

    fn seed() -> Flow {
        FlowBuilder::new()
            .method("POST")
            .uri("https://a.test/users")
            .req_header("Host", "a.test")
            .req_header("Content-Type", "application/json")
            .req_header("X-Dup", "one")
            .req_header("X-Dup", "two")
            .req_body(captured(br#"{"name":"a"}"#))
            .build()
    }

    fn header_values(outgoing: &Outgoing, name: &str) -> Vec<String> {
        outgoing.headers.get_all(name).iter().map(|v| v.to_str().unwrap().to_owned()).collect()
    }

    #[test]
    fn an_absent_override_is_the_seeds_own_value() {
        let outgoing = from_seed(&seed(), fields("{}")).expect("sendable");
        assert_eq!(outgoing.method, Method::POST);
        assert_eq!(outgoing.uri, "https://a.test/users");
        assert_eq!(header_values(&outgoing, "content-type"), vec!["application/json"]);
        assert_eq!(header_values(&outgoing, "x-dup"), vec!["one", "two"]);
        assert_eq!(&outgoing.body[..], br#"{"name":"a"}"#);
    }

    #[test]
    fn an_empty_request_body_is_an_empty_override() {
        let outgoing =
            from_seed(&seed(), parse_fields(b"  \n").expect("fields")).expect("sendable");
        assert_eq!(outgoing.uri, "https://a.test/users");
    }

    #[test]
    fn each_given_override_replaces_its_field() {
        let outgoing = from_seed(
            &seed(),
            fields(
                r#"{"method":"PUT","url":"https://b.test/users","body":"{\"name\":\"alice\"}",
                    "headers":[{"name":"Authorization","value":"Bearer NEW"}]}"#,
            ),
        )
        .expect("sendable");
        assert_eq!(outgoing.method, Method::PUT);
        assert_eq!(outgoing.uri, "https://b.test/users");
        assert_eq!(&outgoing.body[..], br#"{"name":"alice"}"#);
        assert_eq!(header_values(&outgoing, "authorization"), vec!["Bearer NEW"]);
        assert!(header_values(&outgoing, "content-type").is_empty(), "a given list replaces whole");
    }

    #[test]
    fn the_seeds_host_is_not_typed_but_a_given_one_is() {
        let untouched = from_seed(&seed(), fields("{}")).expect("sendable");
        assert!(header_values(&untouched, "host").is_empty(), "the seed's Host follows the URL");

        let typed = from_seed(&seed(), fields(r#"{"headers":[{"name":"Host","value":"a.test"}]}"#))
            .expect("sendable");
        assert_eq!(header_values(&typed, "host"), vec!["a.test"]);
    }

    #[test]
    fn a_body_comes_as_text_or_as_base64_never_both() {
        let text = from_seed(&seed(), fields(r#"{"body":""}"#)).expect("sendable");
        assert!(text.body.is_empty(), "an empty string is an empty body, not the seed's");

        let binary = from_seed(&seed(), fields(r#"{"body_base64":"//4="}"#)).expect("sendable");
        assert_eq!(&binary.body[..], &[0xff, 0xfe]);

        let null = from_seed(&seed(), fields(r#"{"body":null}"#)).expect("sendable");
        assert_eq!(&null.body[..], br#"{"name":"a"}"#, "null is absent");

        assert_eq!(
            from_seed(&seed(), fields(r#"{"body":"x","body_base64":"eA=="}"#)).err(),
            Some(malformed("give body or body_base64, not both"))
        );
        assert_eq!(
            from_seed(&seed(), fields(r#"{"body_base64":"not base64!"}"#)).err(),
            Some(malformed("body_base64 is not base64"))
        );
    }

    #[test]
    fn a_body_never_captured_is_refused_by_name_unless_one_is_given() {
        for (body, absence) in [
            (BodyRec::Streaming { content_type: "text/event-stream".into() }, "streaming"),
            (
                BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 9 } },
                "too_large",
            ),
            (BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }, "read_error"),
        ] {
            let seed = FlowBuilder::new().method("POST").req_body(body).build();
            assert_eq!(
                from_seed(&seed, fields("{}")).err(),
                Some(Refusal::SeedLacks(format!(
                    "the seed's request body was not captured ({absence}); give a body to replace it"
                )))
            );
            let replaced = from_seed(&seed, fields(r#"{"body":"x"}"#)).expect("an override sends");
            assert_eq!(&replaced.body[..], b"x");
        }

        let empty = FlowBuilder::new().build();
        assert!(from_seed(&empty, fields("{}")).expect("sendable").body.is_empty());
    }

    #[test]
    fn a_seed_whose_url_cannot_be_sent_needs_one_given() {
        let seed = FlowBuilder::new().uri("/only/path").build();
        assert_eq!(
            from_seed(&seed, fields("{}")).err(),
            Some(Refusal::SeedLacks(
                "the seed's url is not absolute http(s); give a url to replace it".into()
            ))
        );
        assert!(from_seed(&seed, fields(r#"{"url":"http://b.test/"}"#)).is_ok());
    }

    #[test]
    fn a_seed_header_the_wire_cannot_carry_needs_headers_given() {
        let seed = FlowBuilder::new().req_header("bad name", "x").build();
        assert!(matches!(from_seed(&seed, fields("{}")), Err(Refusal::SeedLacks(_))));
        assert!(from_seed(&seed, fields(r#"{"headers":[]}"#)).is_ok());
    }

    #[test]
    fn composing_requires_a_method_and_a_url_and_nothing_else() {
        let outgoing = from_scratch(fields(r#"{"method":"GET","url":"https://a.test/users"}"#))
            .expect("sendable");
        assert_eq!(outgoing.method, Method::GET);
        assert!(outgoing.headers.is_empty());
        assert!(outgoing.body.is_empty());

        assert_eq!(
            from_scratch(fields(r#"{"url":"https://a.test/"}"#)).err(),
            Some(malformed("method is required"))
        );
        assert_eq!(
            from_scratch(fields(r#"{"method":"GET"}"#)).err(),
            Some(malformed("url is required"))
        );
    }

    #[test]
    fn the_callers_own_values_are_refused_with_a_reason_naming_them() {
        for (json, reason) in [
            (r#"{"method":"GET","url":"htp:/x"}"#, "url must be absolute http(s)"),
            (r#"{"method":"GET","url":"/relative"}"#, "url must be absolute http(s)"),
            (r#"{"method":"GET","url":"ftp://a.test/"}"#, "url must be absolute http(s)"),
            (r#"{"method":"G T","url":"http://a.test/"}"#, "method must be an HTTP token"),
            (r#"{"method":"","url":"http://a.test/"}"#, "method must be an HTTP token"),
            (
                r#"{"method":"GET","url":"http://a.test/","headers":[{"name":"a b","value":"x"}]}"#,
                r#"header "a b" is not a valid header name"#,
            ),
            (
                r#"{"method":"GET","url":"http://a.test/","headers":[{"name":"x","value":"a\nb"}]}"#,
                r#"header "x" has a value the wire cannot carry"#,
            ),
        ] {
            assert_eq!(from_scratch(fields(json)).err(), Some(malformed(reason)), "{json}");
        }
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_ignored() {
        let Err(Refusal::Malformed(reason)) = parse_fields(br#"{"header":[]}"#) else {
            panic!("a typo'd field must be refused");
        };
        assert!(reason.contains("unknown field `header`"), "{reason}");

        let Err(Refusal::Malformed(reason)) = parse_fields(b"not json") else {
            panic!("not JSON must be refused");
        };
        assert!(reason.starts_with("request body: "), "{reason}");
    }

    #[test]
    fn a_refusal_answers_with_its_status_and_reason() {
        assert_eq!(malformed("x").into_response().status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            Refusal::SeedLacks("x".into()).into_response().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
