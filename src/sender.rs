use std::time::{Duration, Instant, SystemTime};

use bytes::Bytes;
use http::header::{self, HeaderMap, HeaderValue};
use http::{Method, Request, Uri};
use http_body_util::Full;
use hudsucker::hyper_util::client::legacy::connect::HttpConnector;
use hudsucker::hyper_util::client::legacy::Client;
use hudsucker::hyper_util::rt::TokioExecutor;
use hudsucker::Body;
use hyper_rustls::HttpsConnector;

use crate::capture;
use crate::flow::{
    header_list, version_token, BodyRec, Flow, FlowId, NotCapturedReason, RequestRec, ResponseRec,
};

/// How long a send waits for the response head, and then again for its body.
pub const RESPONSE_DEADLINE: Duration = Duration::from_secs(30);

/// The headers a connection owns. HTTP/2 refuses every one of them outright, so
/// a request carrying one could not reach an h2 upstream at all; the client
/// sets its own on HTTP/1.
const CONNECTION_HEADERS: &[&str] =
    &["connection", "keep-alive", "proxy-connection", "transfer-encoding", "upgrade", "te"];

/// One request, already validated into wire types, before the header rules
/// run. A `Host` in `headers` is the caller's own and is sent as given.
pub struct Outgoing {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    NoResponse(Duration),
    Transport(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::NoResponse(deadline) => {
                write!(f, "no response within {} s", deadline.as_secs())
            }
            SendError::Transport(cause) => write!(f, "send failed: {cause}"),
        }
    }
}

/// Trawler's own client — the outbound path that is not the proxy. It connects
/// through the proxy's `upstream_connector`, so an upstream is verified exactly
/// as a proxied request's would be, and it records bodies under the same cap.
/// It hands back the finished flow rather than storing it: an exchange that
/// never happened must never reach the ring (DEC-58).
#[derive(Clone)]
pub struct Sender {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    body_cap: usize,
    deadline: Duration,
}

impl Sender {
    pub fn new(extra_root_pems: &[String], body_cap: usize) -> Self {
        let client =
            Client::builder(TokioExecutor::new()).build(crate::upstream_connector(extra_root_pems));
        Self { client, body_cap, deadline: RESPONSE_DEADLINE }
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    pub async fn send(&self, outgoing: Outgoing, seed: Option<FlowId>) -> Result<Flow, SendError> {
        let started_at = SystemTime::now();
        let start = Instant::now();
        let sent_body = held(outgoing.body.clone(), self.body_cap);
        let request = wire_request(outgoing);
        let sent = RequestRec {
            method: request.method().as_str().to_owned(),
            uri: request.uri().to_string(),
            version: String::new(),
            headers: header_list(request.headers()),
            body: sent_body,
        };

        let response = tokio::time::timeout(self.deadline, self.client.request(request))
            .await
            .map_err(|_| SendError::NoResponse(self.deadline))?
            .map_err(|e| SendError::Transport(cause_chain(&e)))?;

        let (parts, body) = response.into_parts();
        let captured = capture::capture(&parts.headers, Body::from(body), self.body_cap);
        let body = match tokio::time::timeout(self.deadline, captured).await {
            Ok((body, _unread)) => body,
            Err(_) => BodyRec::NotCaptured { reason: NotCapturedReason::ReadError },
        };

        let version = version_token(parts.version);
        let mut flow = Flow::new(RequestRec { version: version.clone(), ..sent }).sent(seed);
        flow.started_at = started_at;
        flow.response = Some(ResponseRec {
            status: parts.status.as_u16(),
            version,
            headers: header_list(&parts.headers),
            body,
        });
        flow.duration_ms = Some(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
        Ok(flow)
    }
}

/// The header rules. A length is the body's, never the list's, because a stale
/// one truncates an edited body. `Host` follows the URL unless the caller typed
/// one, and is written explicitly so the recorded request is the wire's on both
/// protocols (on h2 it equals `:authority`). Cookies join into one header, as
/// the proxy's own normalization does, because an h2 seed may split them and an
/// HTTP/1.1 server takes one. The version is left at HTTP/1.1 for the
/// connector's ALPN to upgrade, as the proxy's is.
fn wire_request(outgoing: Outgoing) -> Request<Full<Bytes>> {
    let Outgoing { method, uri, mut headers, body } = outgoing;
    for name in CONNECTION_HEADERS {
        headers.remove(*name);
    }
    headers.remove(header::CONTENT_LENGTH);
    if !body.is_empty() {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
    }
    if !headers.contains_key(header::HOST) {
        if let Some(authority) =
            uri.authority().and_then(|a| HeaderValue::from_str(a.as_str()).ok())
        {
            headers.insert(header::HOST, authority);
        }
    }
    join_cookies(&mut headers);

    let mut request = Request::new(Full::new(body));
    *request.method_mut() = method;
    *request.uri_mut() = uri;
    *request.headers_mut() = headers;
    request
}

fn join_cookies(headers: &mut HeaderMap) {
    let cookies: Vec<&[u8]> =
        headers.get_all(header::COOKIE).iter().map(HeaderValue::as_bytes).collect();
    if cookies.len() < 2 {
        return;
    }
    if let Ok(joined) = HeaderValue::from_bytes(&cookies.join(&b"; "[..])) {
        headers.insert(header::COOKIE, joined);
    }
}

/// What a sent body is recorded as. The cap bounds what the ring holds, never
/// what goes out.
fn held(bytes: Bytes, cap: usize) -> BodyRec {
    match bytes.len() {
        0 => BodyRec::Empty,
        size if size <= cap => BodyRec::Captured { bytes, size },
        actual => BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual } },
    }
}

/// The whole cause chain in words: hyper's outermost message only says which
/// stage failed, and the reason a person can act on is at the bottom.
fn cause_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut causes: Vec<String> = Vec::new();
    let mut next = Some(error);
    while let Some(cause) = next {
        let text = cause.to_string();
        if causes.last() != Some(&text) {
            causes.push(text);
        }
        next = cause.source();
    }
    causes.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outgoing(uri: &str, headers: &[(&str, &str)], body: &[u8]) -> Outgoing {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.append(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        Outgoing {
            method: Method::POST,
            uri: uri.parse().unwrap(),
            headers: map,
            body: Bytes::copy_from_slice(body),
        }
    }

    fn values<'a>(request: &'a Request<Full<Bytes>>, name: &str) -> Vec<&'a str> {
        request.headers().get_all(name).iter().map(|v| v.to_str().unwrap()).collect()
    }

    #[test]
    fn the_length_is_the_body_sent_never_the_one_listed() {
        let request = wire_request(outgoing(
            "http://b.test/",
            &[("Content-Length", "12")],
            b"{\"name\":\"alice\"}",
        ));
        assert_eq!(values(&request, "content-length"), vec!["16"]);

        let empty = wire_request(outgoing("http://b.test/", &[("Content-Length", "12")], b""));
        assert!(values(&empty, "content-length").is_empty());
    }

    #[test]
    fn host_follows_the_url_unless_one_was_typed() {
        let request = wire_request(outgoing("http://b.test:8080/users", &[], b""));
        assert_eq!(values(&request, "host"), vec!["b.test:8080"]);

        let typed = wire_request(outgoing("http://b.test/", &[("Host", "a.test")], b""));
        assert_eq!(values(&typed, "host"), vec!["a.test"]);
    }

    #[test]
    fn the_connection_owns_its_own_headers() {
        let listed: Vec<(&str, &str)> =
            CONNECTION_HEADERS.iter().map(|name| (*name, "x")).collect();
        let request = wire_request(outgoing("http://b.test/", &listed, b"body"));
        for name in CONNECTION_HEADERS {
            assert!(values(&request, name).is_empty(), "{name} must not reach the wire");
        }
    }

    #[test]
    fn split_cookies_join_into_one_header() {
        let request =
            wire_request(outgoing("http://b.test/", &[("Cookie", "a=1"), ("Cookie", "b=2")], b""));
        assert_eq!(values(&request, "cookie"), vec!["a=1; b=2"]);

        let single = wire_request(outgoing("http://b.test/", &[("Cookie", "a=1")], b""));
        assert_eq!(values(&single, "cookie"), vec!["a=1"]);
    }

    #[test]
    fn every_other_header_goes_as_given_with_its_duplicates() {
        let request = wire_request(outgoing(
            "http://b.test/",
            &[("Authorization", "Bearer NEW"), ("X-Dup", "one"), ("X-Dup", "two")],
            b"",
        ));
        assert_eq!(values(&request, "authorization"), vec!["Bearer NEW"]);
        assert_eq!(values(&request, "x-dup"), vec!["one", "two"]);
        assert_eq!(request.version(), http::Version::HTTP_11);
    }

    #[test]
    fn a_sent_body_is_held_under_the_cap_and_sent_whatever_its_size() {
        assert!(matches!(held(Bytes::new(), 4), BodyRec::Empty));
        assert!(matches!(held(Bytes::from_static(b"abcd"), 4), BodyRec::Captured { size: 4, .. }));
        assert!(matches!(
            held(Bytes::from_static(b"abcde"), 4),
            BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 5 } }
        ));
    }

    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);

    impl std::fmt::Display for Layer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Layer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref().map(|l| l as &(dyn std::error::Error + 'static))
        }
    }

    #[test]
    fn the_reason_reaches_the_bottom_of_the_chain_without_repeating_itself() {
        let error = Layer(
            "client error (Connect)",
            Some(Box::new(Layer(
                "tcp connect error",
                Some(Box::new(Layer(
                    "tcp connect error",
                    Some(Box::new(Layer("Connection refused", None))),
                ))),
            ))),
        );
        assert_eq!(
            cause_chain(&error),
            "client error (Connect): tcp connect error: Connection refused"
        );
    }

    #[test]
    fn the_errors_say_what_happened_in_words() {
        assert_eq!(SendError::NoResponse(RESPONSE_DEADLINE).to_string(), "no response within 30 s");
        assert_eq!(
            SendError::Transport("connection refused".into()).to_string(),
            "send failed: connection refused"
        );
    }

    #[tokio::test]
    async fn an_answered_send_is_a_finished_flow_marked_sent_and_linked() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = axum::Router::new()
            .route("/users", axum::routing::post(|| async { (http::StatusCode::CREATED, "made") }));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let seed = FlowId::new();
        let flow = Sender::new(&[], 1024)
            .send(outgoing(&format!("http://127.0.0.1:{port}/users"), &[], b"{}"), Some(seed))
            .await
            .expect("an answered send");

        assert_eq!(flow.source, crate::flow::FlowSource::Sent);
        assert_eq!(flow.seed, Some(seed));
        assert_eq!(flow.request.method, "POST");
        assert_eq!(flow.request.uri, format!("http://127.0.0.1:{port}/users"));
        assert_eq!(flow.request.version, "HTTP/1.1");
        assert!(matches!(flow.request.body, BodyRec::Captured { size: 2, .. }));
        let response = flow.response.expect("a response");
        assert_eq!(response.status, 201);
        assert_eq!(response.version, "HTTP/1.1");
        assert_eq!(response.body.bytes().map(|b| b.to_vec()), Some(b"made".to_vec()));
        assert!(flow.duration_ms.is_some());
    }

    #[tokio::test]
    async fn a_silent_upstream_is_no_response_and_no_flow() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (_held_open, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });

        let deadline = Duration::from_millis(100);
        let result = Sender::new(&[], 1024)
            .with_deadline(deadline)
            .send(outgoing(&format!("http://127.0.0.1:{port}/"), &[], b""), None)
            .await;
        assert_eq!(result.err(), Some(SendError::NoResponse(deadline)));
    }
}
