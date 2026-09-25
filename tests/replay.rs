#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tokio::task::JoinHandle;
use tower::ServiceExt;
use trawler::sender::Sender;
use trawler::{serve, Config, RunningServer, Store};

use common::tls_origin;

/// Everything that reached the origin, so a test asserts on the wire rather
/// than on what Trawler says it sent.
async fn echo(request: Request) -> Json<Value> {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let headers: Vec<Value> = parts
        .headers
        .iter()
        .map(|(name, value)| json!([name.as_str(), value.to_str().unwrap_or("<binary>")]))
        .collect();
    Json(json!({
        "method": parts.method.as_str(),
        "headers": headers,
        "body": String::from_utf8_lossy(&body),
    }))
}

fn origin_app() -> Router {
    Router::new()
        .route("/echo", any(echo))
        .route("/big", get(|| async { "x".repeat(2000) }))
        .route("/fixed", any(|| async { ([("content-type", "text/plain")], "fixed") }))
}

fn start_origin() -> (u16, JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, origin_app()).await;
    });
    (port, task)
}

fn start_tls_origin(alpn: &[&str]) -> (String, u16, JoinHandle<()>) {
    let (ca_pem, server_config) = tls_origin(alpn);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let tls = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(server_config));
    let task = tokio::spawn(async move {
        let _ = axum_server::from_tcp_rustls(listener, tls)
            .serve(origin_app().into_make_service())
            .await;
    });
    (ca_pem, port, task)
}

fn closed_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn echoed_header<'a>(echo: &'a Value, name: &str) -> Vec<&'a str> {
    echo["headers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str().unwrap())
        .collect()
}

struct Answer {
    status: StatusCode,
    location: Option<String>,
    text: String,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|_| panic!("not JSON: {}", self.text))
    }
}

struct Harness {
    server: RunningServer,
    api: reqwest::Client,
    proxy: reqwest::Client,
    port: u16,
    _ca_dir: TempDir,
    _origin: JoinHandle<()>,
}

impl Harness {
    async fn setup(body_cap: usize, extra_root_pems: Vec<String>) -> Self {
        let ca_dir = tempdir().unwrap();
        let config = Config {
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            api_addr: "127.0.0.1:0".parse().unwrap(),
            ca_dir: ca_dir.path().to_path_buf(),
            body_cap,
            upstream_extra_root_pems: extra_root_pems,
            ..Default::default()
        };
        let server = serve(config).await.unwrap();
        let (port, origin) = start_origin();
        let api = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
        let proxy = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", server.proxy_addr)).unwrap())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Self { server, api, proxy, port, _ca_dir: ca_dir, _origin: origin }
    }

    fn origin(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn api_url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.api_addr)
    }

    /// Posted the way `curl -d` posts: a form content type the door must not
    /// read.
    async fn post(&self, path: &str, body: &str) -> Answer {
        let response = self
            .api
            .post(self.api_url(path))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body.to_owned())
            .send()
            .await
            .unwrap();
        let status = StatusCode::from_u16(response.status().as_u16()).unwrap();
        let location = response.headers().get("location").map(|v| v.to_str().unwrap().to_owned());
        Answer { status, location, text: response.text().await.unwrap() }
    }

    async fn get_json(&self, path: &str) -> Value {
        let text = self.api.get(self.api_url(path)).send().await.unwrap().text().await.unwrap();
        serde_json::from_str(&text).unwrap()
    }

    async fn rows(&self, query: &str) -> Vec<Value> {
        self.get_json(&format!("/flows{query}")).await.as_array().unwrap().clone()
    }

    async fn response_body(&self, id: &str) -> Value {
        let url = self.api_url(&format!("/flows/{id}/response/body"));
        serde_json::from_slice(&self.api.get(url).send().await.unwrap().bytes().await.unwrap())
            .unwrap()
    }

    /// A seed the proxy captured, as a user's would be — waiting on a condition,
    /// never a fixed sleep.
    async fn captured_seed(&self, request: reqwest::RequestBuilder) -> String {
        request.send().await.unwrap().bytes().await.unwrap();
        for _ in 0..100 {
            let rows = self.rows("?source=captured").await;
            if let Some(row) = rows.iter().find(|row| !row["status"].is_null()) {
                return row["id"].as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the seed was never captured");
    }

    async fn post_seed(&self) -> String {
        let request = self
            .proxy
            .post(self.origin("/echo"))
            .header("authorization", "Bearer OLD")
            .header("content-type", "application/json")
            .body(r#"{"name":"a"}"#);
        self.captured_seed(request).await
    }

    async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

#[tokio::test]
async fn a_replay_with_an_edited_header_reaches_the_wire_and_lands_as_a_sent_flow() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    let seed = h.post_seed().await;

    let answer = h
        .post(
            &format!("/flows/{seed}/replay"),
            r#"{"headers":[{"name":"Authorization","value":"Bearer NEW"},
                           {"name":"Content-Type","value":"application/json"}]}"#,
        )
        .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.text);
    let flow = answer.json();
    let id = flow["id"].as_str().unwrap();
    assert_eq!(answer.location.as_deref(), Some(format!("/flows/{id}").as_str()));
    assert_eq!(flow["source"], "sent");
    assert_eq!(flow["seed"], seed.as_str());
    assert_eq!(flow["status"], 200);
    assert!(flow["request_headers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["name"] == "authorization" && h["value"] == "Bearer NEW"));

    let echo = h.response_body(id).await;
    assert_eq!(
        echoed_header(&echo, "authorization"),
        vec!["Bearer NEW"],
        "the edit reached the wire"
    );
    assert_eq!(echo["method"], "POST", "the seed's method, since none was given");
    assert_eq!(echo["body"], r#"{"name":"a"}"#, "the seed's body, since none was given");
    assert_eq!(h.get_json(&format!("/flows/{id}")).await["seed"], seed.as_str());

    h.shutdown().await;
}

#[tokio::test]
async fn an_empty_replay_resends_the_seed_and_an_edit_carries_its_own_host_and_length() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    let seed = h.post_seed().await;

    let resent = h.post(&format!("/flows/{seed}/replay"), "").await;
    assert_eq!(resent.status, StatusCode::CREATED, "{}", resent.text);
    let echo = h.response_body(resent.json()["id"].as_str().unwrap()).await;
    assert_eq!(echo["method"], "POST");
    assert_eq!(echo["body"], r#"{"name":"a"}"#);
    assert_eq!(echoed_header(&echo, "authorization"), vec!["Bearer OLD"]);
    assert_eq!(echoed_header(&echo, "content-type"), vec!["application/json"]);
    assert!(echoed_header(&echo, "proxy-connection").is_empty());

    let moved = format!("http://localhost:{}/echo", h.port);
    let edited = h
        .post(
            &format!("/flows/{seed}/replay"),
            &json!({ "url": moved, "body": r#"{"name":"alice"}"# }).to_string(),
        )
        .await;
    assert_eq!(edited.status, StatusCode::CREATED, "{}", edited.text);
    let echo = h.response_body(edited.json()["id"].as_str().unwrap()).await;
    assert_eq!(echoed_header(&echo, "host"), vec![format!("localhost:{}", h.port)]);
    assert_eq!(echoed_header(&echo, "content-length"), vec!["16"]);
    assert_eq!(
        echo["body"], r#"{"name":"alice"}"#,
        "the whole edited body, never cut to the old length"
    );

    let typed = h
        .post(
            &format!("/flows/{seed}/replay"),
            r#"{"headers":[{"name":"Host","value":"typed.test"}]}"#,
        )
        .await;
    assert_eq!(typed.status, StatusCode::CREATED, "{}", typed.text);
    let echo = h.response_body(typed.json()["id"].as_str().unwrap()).await;
    assert_eq!(echoed_header(&echo, "host"), vec!["typed.test"]);

    h.shutdown().await;
}

#[tokio::test]
async fn a_request_from_scratch_needs_a_method_and_a_url_and_nothing_else() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;

    let sent =
        h.post("/flows", &json!({ "method": "GET", "url": h.origin("/echo") }).to_string()).await;
    assert_eq!(sent.status, StatusCode::CREATED, "{}", sent.text);
    let flow = sent.json();
    assert_eq!(flow["source"], "sent");
    assert_eq!(flow["seed"], Value::Null);
    assert_eq!(h.response_body(flow["id"].as_str().unwrap()).await["method"], "GET");

    for (body, reason) in [
        (json!({ "url": h.origin("/echo") }), "method is required"),
        (json!({ "method": "GET" }), "url is required"),
        (json!({ "method": "GET", "url": "htp:/x" }), "url must be absolute http(s)"),
    ] {
        let refused = h.post("/flows", &body.to_string()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(refused.text, reason);
    }
    assert_eq!(h.rows("").await.len(), 1, "a refused send lands nothing");

    h.shutdown().await;
}

#[tokio::test]
async fn a_sent_flow_carries_its_mark_on_both_projections_and_is_selectable_by_it() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    let seed = h.post_seed().await;
    let replayed = h.post(&format!("/flows/{seed}/replay"), "{}").await.json();
    let composed = h
        .post("/flows", &json!({ "method": "GET", "url": h.origin("/echo") }).to_string())
        .await
        .json();

    let sent = h.rows("?source=sent").await;
    let ids: Vec<&str> = sent.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec![composed["id"].as_str().unwrap(), replayed["id"].as_str().unwrap()]);
    assert!(sent.iter().all(|row| row["source"] == "sent"));
    assert_eq!(sent[0]["seed"], Value::Null);
    assert_eq!(sent[1]["seed"], seed.as_str());

    let captured = h.rows("?source=captured").await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0]["seed"], Value::Null);

    let refused = h.api.get(h.api_url("/flows?source=bogus")).send().await.unwrap();
    assert_eq!(refused.status().as_u16(), 400);
    assert_eq!(refused.text().await.unwrap(), "source must be captured, imported or sent");

    h.shutdown().await;
}

#[tokio::test]
async fn a_seed_body_never_captured_is_refused_by_name_until_one_is_given() {
    let h = Harness::setup(512, Vec::new()).await;
    let seed = h.captured_seed(h.proxy.post(h.origin("/echo")).body("y".repeat(2000))).await;
    let before = h.rows("").await.len();

    let refused = h.post(&format!("/flows/{seed}/replay"), "{}").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        refused.text,
        "the seed's request body was not captured (too_large); give a body to replace it"
    );
    assert_eq!(h.rows("").await.len(), before, "nothing was sent, so nothing landed");

    let replaced = h.post(&format!("/flows/{seed}/replay"), r#"{"body":"short"}"#).await;
    assert_eq!(replaced.status, StatusCode::CREATED, "{}", replaced.text);
    assert_eq!(h.response_body(replaced.json()["id"].as_str().unwrap()).await["body"], "short");

    h.shutdown().await;
}

#[tokio::test]
async fn a_send_that_reaches_nobody_answers_with_the_reason_and_lands_nothing() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    h.post_seed().await;
    let before = h.rows("").await.len();

    let url = format!("http://127.0.0.1:{}/", closed_port());
    let failed = h.post("/flows", &json!({ "method": "GET", "url": url }).to_string()).await;
    assert_eq!(failed.status, StatusCode::BAD_GATEWAY);
    assert!(failed.text.starts_with("send failed: "), "{}", failed.text);
    assert!(failed.text.to_lowercase().contains("refused"), "the reason in words: {}", failed.text);
    assert_eq!(h.rows("").await.len(), before, "no flow for an exchange that never happened");

    h.shutdown().await;
}

// The shipped deadline is 30 s, which no test can wait out, so this one drives
// the same router with a sender whose deadline it can.
#[tokio::test]
async fn an_upstream_that_never_answers_is_a_gateway_timeout_and_lands_nothing() {
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = silent.local_addr().unwrap().port();
    let _held = tokio::spawn(async move {
        let (_connection, _) = silent.accept().await.unwrap();
        std::future::pending::<()>().await;
    });

    let store = Arc::new(Store::new(1_000_000, 64));
    let sender = Sender::new(&[], 1024).with_deadline(Duration::from_secs(1));
    let router = trawler::test_support::api_router_with_sender(Arc::clone(&store), "PEM", sender);
    let body = json!({ "method": "GET", "url": format!("http://127.0.0.1:{port}/") }).to_string();
    let request =
        axum::http::Request::builder().method("POST").uri("/flows").body(Body::from(body)).unwrap();
    let response = router.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    let text = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&text[..], b"no response within 1 s");
    assert!(store.list(&trawler::store::FlowFilter::default()).is_empty());
}

#[tokio::test]
async fn an_https_upstream_is_verified_as_the_proxy_verifies_one_and_h2_is_recorded() {
    let (h1_ca, h1_port, _h1) = start_tls_origin(&["http/1.1"]);
    let (h2_ca, h2_port, _h2) = start_tls_origin(&["h2", "http/1.1"]);
    let (_untrusted_ca, untrusted_port, _untrusted) = start_tls_origin(&["http/1.1"]);
    let h = Harness::setup(1024 * 1024, vec![h1_ca, h2_ca]).await;

    let compose =
        |port: u16| json!({ "method": "GET", "url": format!("https://localhost:{port}/echo") });

    let h1 = h.post("/flows", &compose(h1_port).to_string()).await;
    assert_eq!(h1.status, StatusCode::CREATED, "{}", h1.text);
    assert_eq!(h1.json()["request_http_version"], "HTTP/1.1");
    assert_eq!(h1.json()["response_http_version"], "HTTP/1.1");

    let h2 = h.post("/flows", &compose(h2_port).to_string()).await;
    assert_eq!(h2.status, StatusCode::CREATED, "{}", h2.text);
    assert_eq!(
        h2.json()["request_http_version"],
        "HTTP/2.0",
        "the version negotiated, not assumed"
    );
    assert_eq!(h2.json()["response_http_version"], "HTTP/2.0");
    assert_eq!(h.response_body(h2.json()["id"].as_str().unwrap()).await["method"], "GET");

    let before = h.rows("").await.len();
    let untrusted = h.post("/flows", &compose(untrusted_port).to_string()).await;
    assert_eq!(untrusted.status, StatusCode::BAD_GATEWAY);
    assert!(untrusted.text.contains("certificate"), "{}", untrusted.text);
    assert_eq!(h.rows("").await.len(), before);

    h.shutdown().await;
}

#[tokio::test]
async fn a_sent_flow_lives_by_the_rings_rules_like_any_other() {
    let h = Harness::setup(16, Vec::new()).await;

    let over_cap =
        h.post("/flows", &json!({ "method": "GET", "url": h.origin("/big") }).to_string()).await;
    assert_eq!(over_cap.status, StatusCode::CREATED, "{}", over_cap.text);
    let body = &over_cap.json()["response_body"];
    assert_eq!(body["state"], "not_captured");
    assert_eq!(body["reason"], "too_large");

    let seed = h.captured_seed(h.proxy.get(h.origin("/echo"))).await;
    let removed = h.api.delete(h.api_url(&format!("/flows/{seed}"))).send().await.unwrap();
    assert_eq!(removed.status().as_u16(), 200);
    let gone = h.post(&format!("/flows/{seed}/replay"), "{}").await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.text, format!("no flow {seed} in the ring"));

    h.shutdown().await;
}

#[tokio::test]
async fn a_sent_flow_is_announced_on_the_stream_as_it_lands() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    let mut stream = h.api.get(h.api_url("/stream")).send().await.unwrap();

    let sent =
        h.post("/flows", &json!({ "method": "GET", "url": h.origin("/echo") }).to_string()).await;
    let id = sent.json()["id"].as_str().unwrap().to_owned();

    let mut seen = String::new();
    let announced = tokio::time::timeout(Duration::from_secs(5), async {
        while !(seen.contains("event: request") && seen.contains("event: response")) {
            let chunk = stream.chunk().await.unwrap().expect("the stream stays open");
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
    })
    .await;
    assert!(announced.is_ok(), "both events for the sent flow, got: {seen}");
    assert!(seen.find("event: request") < seen.find("event: response"), "{seen}");
    assert!(seen.contains(&id), "{seen}");

    h.shutdown().await;
}

#[tokio::test]
async fn a_sent_flow_view_shape_is_locked() {
    let h = Harness::setup(1024 * 1024, Vec::new()).await;
    let sent = h
        .post(
            "/flows",
            &json!({
                "method": "POST",
                "url": h.origin("/fixed"),
                "headers": [{ "name": "X-Trace", "value": "1" }],
                "body": "hi",
            })
            .to_string(),
        )
        .await;
    assert_eq!(sent.status, StatusCode::CREATED, "{}", sent.text);

    let port = h.port.to_string();
    let mut view = sent.json();
    view["id"] = json!("<id>");
    view["started_at"] = json!("<started>");
    view["duration_ms"] = json!("<duration>");
    view["uri"] = json!(view["uri"].as_str().unwrap().replace(&port, "<port>"));
    for header in view["request_headers"].as_array_mut().unwrap() {
        header["value"] = json!(header["value"].as_str().unwrap().replace(&port, "<port>"));
    }
    for header in view["response_headers"].as_array_mut().unwrap() {
        if header["name"] == "date" {
            header["value"] = json!("<date>");
        }
    }
    insta::assert_json_snapshot!(view);

    h.shutdown().await;
}
