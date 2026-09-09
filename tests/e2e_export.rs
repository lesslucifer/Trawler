#![allow(clippy::unwrap_used)]
//! E-EXPORT end to end: real traffic through the running proxy, paged out of the
//! listing, exported as HAR, cleared, and read back — over ephemeral ports and a
//! local origin, never the public network.

mod common;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use serde_json::Value;
use tempfile::{tempdir, TempDir};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use trawler::{serve, Config, RunningServer};

use common::tls_origin;

const BROWSER_CURL: &str = concat!(
    "curl 'http://api.example.com/v1/things?page=2' \\\n",
    "  -X POST \\\n",
    "  -H 'content-type: application/json' \\\n",
    "  -H 'accept: */*' \\\n",
    "  --data-raw '{\"name\":\"widget\"}' \\\n",
    "  --compressed"
);

async fn start_origin() -> (SocketAddr, JoinHandle<()>) {
    async fn ok() -> &'static str {
        "hello"
    }
    async fn echo(req: Request) -> Bytes {
        axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap()
    }
    async fn sse() -> impl IntoResponse {
        ([(CONTENT_TYPE, "text/event-stream")], "data: 1\n\ndata: 2\n\n")
    }

    let app =
        Router::new().route("/ok", get(ok)).route("/echo", post(echo)).route("/sse", get(sse));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, task)
}

fn start_https_origin() -> (String, u16, JoinHandle<()>) {
    let (ca_pem, server_config) = tls_origin(&["http/1.1"]);
    let app = Router::new().route("/secure", get(|| async { "top-secret" }));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let tls = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(server_config));
    let task = tokio::spawn(async move {
        let _ = axum_server::from_tcp_rustls(listener, tls).serve(app.into_make_service()).await;
    });
    (ca_pem, port, task)
}

struct Harness {
    origin: SocketAddr,
    https_port: u16,
    api_addr: SocketAddr,
    proxy: reqwest::Client,
    api: reqwest::Client,
    server: Option<RunningServer>,
    _ca_dir: TempDir,
    _origins: Vec<JoinHandle<()>>,
}

impl Harness {
    async fn setup() -> Self {
        trawler::init_crypto();
        let (origin, origin_task) = start_origin().await;
        let (upstream_ca_pem, https_port, https_task) = start_https_origin();

        let ca_dir = tempdir().unwrap();
        let config = Config {
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            api_addr: "127.0.0.1:0".parse().unwrap(),
            ca_dir: ca_dir.path().to_path_buf(),
            upstream_extra_root_pems: vec![upstream_ca_pem],
            ..Default::default()
        };
        let server = serve(config).await.unwrap();

        let api = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        let trawler_ca_pem = api
            .get(format!("http://{}/ca.crt", server.api_addr))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let proxy = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", server.proxy_addr)).unwrap())
            .add_root_certificate(reqwest::Certificate::from_pem(&trawler_ca_pem).unwrap())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        Self {
            origin,
            https_port,
            api_addr: server.api_addr,
            proxy,
            api,
            server: Some(server),
            _ca_dir: ca_dir,
            _origins: vec![origin_task, https_task],
        }
    }

    fn origin_url(&self, path: &str) -> String {
        format!("http://{}{}", self.origin, path)
    }

    fn api_url(&self, path: &str) -> String {
        format!("http://{}{}", self.api_addr, path)
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        self.api.get(self.api_url(path)).send().await.unwrap()
    }

    async fn get_json(&self, path: &str) -> Value {
        serde_json::from_str(&self.get(path).await.text().await.unwrap()).unwrap()
    }

    async fn post(&self, path: &str, body: Vec<u8>) -> (reqwest::StatusCode, String) {
        let response = self.api.post(self.api_url(path)).body(body).send().await.unwrap();
        (response.status(), response.text().await.unwrap())
    }

    async fn delete(&self, path: &str) -> Value {
        let body = self.api.delete(self.api_url(path)).send().await.unwrap().text().await.unwrap();
        serde_json::from_str(&body).unwrap()
    }

    async fn wait_for_flows(&self, expected: usize) -> Value {
        for _ in 0..200 {
            let flows = self.get_json("/flows").await;
            if flows.as_array().unwrap().len() >= expected {
                return flows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("only {:?} flows were ever captured", self.get_json("/flows").await);
    }

    async fn connections(&self) -> u64 {
        self.get_json("/stats").await["connections"].as_u64().unwrap()
    }

    async fn shutdown(mut self) {
        if let Some(server) = self.server.take() {
            server.shutdown().await;
        }
    }
}

/// The listing's own order is the walk's order, so the page sequence is compared
/// against it rather than against a set alone — a cursor that re-served or
/// skipped a row would still satisfy a set comparison of the right size.
async fn walk_pages(h: &Harness) -> Vec<String> {
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let query = match &cursor {
            Some(c) => format!("/flows?limit=1&cursor={c}"),
            None => "/flows?limit=1".to_owned(),
        };
        let page = h.get_json(&query).await;
        for flow in page["flows"].as_array().unwrap() {
            ids.push(flow["id"].as_str().unwrap().to_owned());
        }
        match page["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return ids,
        }
        assert!(ids.len() < 100, "the cursor never reached the end of the capture");
    }
}

fn entry_for<'a>(document: &'a Value, needle: &str) -> &'a Value {
    document["log"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["request"]["url"].as_str().unwrap().contains(needle))
        .unwrap_or_else(|| panic!("no exported entry for {needle}"))
}

fn summary_of<'a>(flows: &'a Value, needle: &str) -> &'a Value {
    flows
        .as_array()
        .unwrap()
        .iter()
        .find(|flow| flow["uri"].as_str().unwrap().contains(needle))
        .unwrap_or_else(|| panic!("no listed flow for {needle}"))
}

/// Real traffic on both schemes, one of it with a body and one of it streamed —
/// so the capture the rest of the test exports is captured, not fabricated.
async fn drive_real_traffic(h: &Harness) {
    let hello = h.proxy.get(h.origin_url("/ok")).send().await.unwrap().text().await.unwrap();
    assert_eq!(hello, "hello", "the client must receive the origin's bytes unaltered");
    let echoed = h
        .proxy
        .post(h.origin_url("/echo"))
        .header("content-type", "application/json")
        .body(r#"{"ping":true}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(echoed, r#"{"ping":true}"#);
    let sse = h.proxy.get(h.origin_url("/sse")).send().await.unwrap().text().await.unwrap();
    assert!(sse.contains("data: 1"), "the client must receive the streamed events");
    let secret = h
        .proxy
        .get(format!("https://localhost:{}/secure", h.https_port))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(secret, "top-secret", "the MITM leg must be transparent to the client");
}

/// W1 — the whole capture walked one row at a time, each row exactly once.
async fn walk_the_listing_a_row_at_a_time(h: &Harness, captured_ids: &[String]) {
    let paged = walk_pages(h).await;
    assert_eq!(paged, captured_ids, "paging must walk the listing's own order, once each");
    assert_eq!(
        paged.iter().collect::<HashSet<_>>().len(),
        paged.len(),
        "no row may be served twice"
    );
}

/// W2 — the capture leaves as a HAR 1.2 document, and its bytes are what W3 reads
/// back.
async fn export_reads_as_har(h: &Harness) -> Vec<u8> {
    let export = h.get("/flows/export").await;
    let disposition =
        export.headers().get("content-disposition").unwrap().to_str().unwrap().to_owned();
    assert!(disposition.contains(".har\""), "the export must name itself a .har attachment");
    let exported_bytes = export.bytes().await.unwrap().to_vec();
    let document: Value = serde_json::from_slice(&exported_bytes).unwrap();
    assert_eq!(document["log"]["version"], "1.2");
    assert_eq!(document["log"]["entries"].as_array().unwrap().len(), 4);

    let echo_entry = entry_for(&document, "/echo");
    assert_eq!(echo_entry["request"]["method"], "POST");
    assert_eq!(echo_entry["request"]["postData"]["text"], r#"{"ping":true}"#);
    assert_eq!(echo_entry["response"]["status"], 200);
    // The origin answers the echo untyped, so its bytes leave base64-encoded —
    // the branch a text-only assertion would never reach.
    assert_eq!(echo_entry["response"]["content"]["encoding"], "base64");
    assert_eq!(echo_entry["response"]["content"]["text"], "eyJwaW5nIjp0cnVlfQ==");
    assert!(echo_entry["request"]["httpVersion"].as_str().unwrap().starts_with("HTTP/"));
    assert!(echo_entry["timings"]["wait"].as_i64().unwrap() >= 0);
    assert!(!echo_entry["startedDateTime"].as_str().unwrap().is_empty());

    let secure_entry = entry_for(&document, "/secure");
    assert_eq!(secure_entry["response"]["content"]["text"], "top-secret");

    // A body the proxy never held must not read as a body that was empty.
    let sse_content = &entry_for(&document, "/sse")["response"]["content"];
    assert!(sse_content["text"].is_null(), "a streamed body must not be exported as text");
    assert_eq!(sse_content["size"], -1);
    assert!(sse_content["comment"].as_str().unwrap().contains("streaming"));

    // Export honours the listing's filters, because it reads them.
    let filtered: Value = h.get_json("/flows/export?q=/echo").await;
    assert_eq!(filtered["log"]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(entry_for(&filtered, "/echo")["request"]["method"], "POST");

    exported_bytes
}

/// W4 — the clear reaches an open subscriber before the listing is re-read, and
/// takes nothing with it that was never storage.
async fn clear_empties_and_announces(h: &Harness, captured_ids: &[String], connections: u64) {
    let mut stream = h.api.get(h.api_url("/stream")).send().await.unwrap();
    let removed = h.delete("/flows").await;
    assert_eq!(removed["removed"], 4);
    let cleared = tokio::time::timeout(Duration::from_secs(5), async {
        let mut seen = String::new();
        while let Some(chunk) = stream.chunk().await.unwrap() {
            seen.push_str(&String::from_utf8_lossy(&chunk));
            if seen.contains("event: cleared") {
                return seen;
            }
        }
        panic!("the stream ended without announcing the clear");
    })
    .await
    .expect("no cleared event reached the subscriber");
    for id in captured_ids {
        assert!(cleared.contains(id.as_str()), "the clear must name every flow it removed");
    }
    assert!(h.get_json("/flows").await.as_array().unwrap().is_empty());
    assert_eq!(h.connections().await, connections, "a cleared store is empty, not restarted");
}

/// W3 — the same bytes come back as flows, marked, and readable everywhere an
/// ordinary flow is.
async fn import_brings_it_back_marked(h: &Harness, captured: &Value, exported_bytes: Vec<u8>) {
    let (status, body) = h.post("/flows/import", exported_bytes.clone()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["imported"], 4);

    let imported = h.get_json("/flows").await;
    assert_eq!(imported.as_array().unwrap().len(), 4);
    for flow in imported.as_array().unwrap() {
        assert_eq!(flow["source"], "imported");
    }
    assert!(h.get_json("/flows?source=captured").await.as_array().unwrap().is_empty());
    assert_eq!(h.get_json("/flows?source=imported").await.as_array().unwrap().len(), 4);

    for needle in ["/ok", "/echo", "/sse", "/secure"] {
        let before = summary_of(captured, needle);
        let after = summary_of(&imported, needle);
        assert_eq!(before["method"], after["method"], "{needle} method");
        assert_eq!(before["uri"], after["uri"], "{needle} uri");
        assert_eq!(before["status"], after["status"], "{needle} status");
        assert_eq!(before["started_at"], after["started_at"], "{needle} start time");
    }

    let echo_id = summary_of(&imported, "/echo")["id"].as_str().unwrap().to_owned();
    let detail = h.get_json(&format!("/flows/{echo_id}")).await;
    assert_eq!(detail["source"], "imported");
    assert_eq!(detail["request_body"]["state"], "captured");
    let request_body = h.get(&format!("/flows/{echo_id}/request/body")).await.text().await.unwrap();
    assert_eq!(request_body, r#"{"ping":true}"#);
    let response_body =
        h.get(&format!("/flows/{echo_id}/response/body")).await.text().await.unwrap();
    assert_eq!(response_body, r#"{"ping":true}"#);

    let sse_id = summary_of(&imported, "/sse")["id"].as_str().unwrap().to_owned();
    assert_eq!(
        h.get_json(&format!("/flows/{sse_id}")).await["response_body"]["state"],
        "streaming"
    );

    // A truncated document is refused, and the store keeps what it had.
    let truncated = exported_bytes[..exported_bytes.len() / 2].to_vec();
    let (status, reason) = h.post("/flows/import", truncated).await;
    assert_eq!(status, 400);
    assert!(!reason.is_empty(), "a refusal must say why");
    assert_eq!(h.get_json("/flows").await.as_array().unwrap().len(), 4, "nothing partial landed");
}

/// W7 — one pasted curl command lands, and one the parser will not guess at does
/// not.
async fn a_pasted_curl_command_lands_and_an_unknown_flag_does_not(h: &Harness) {
    let (status, body) =
        h.post("/flows/import?format=curl", BROWSER_CURL.as_bytes().to_vec()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["imported"], 1);

    let with_pasted = h.get_json("/flows").await;
    let pasted = summary_of(&with_pasted, "/v1/things");
    assert_eq!(pasted["source"], "imported");
    assert_eq!(pasted["method"], "POST");
    assert!(pasted["status"].is_null(), "nothing was sent, so there is no response");
    assert!(pasted["response_body_state"].is_null());
    let pasted_id = pasted["id"].as_str().unwrap().to_owned();
    let pasted_detail = h.get_json(&format!("/flows/{pasted_id}")).await;
    assert_eq!(pasted_detail["uri"], "http://api.example.com/v1/things?page=2");
    let headers = pasted_detail["request_headers"].as_array().unwrap();
    let header = |name: &str| {
        headers
            .iter()
            .find(|h| h["name"].as_str().unwrap().eq_ignore_ascii_case(name))
            .map(|h| h["value"].as_str().unwrap().to_owned())
    };
    assert_eq!(header("content-type").as_deref(), Some("application/json"));
    assert!(header("accept-encoding").is_some(), "--compressed must expand into the header");
    let pasted_body =
        h.get(&format!("/flows/{pasted_id}/request/body")).await.text().await.unwrap();
    assert_eq!(pasted_body, r#"{"name":"widget"}"#);

    let (status, reason) =
        h.post("/flows/import?format=curl", b"curl 'http://x/' -F 'f=@a'".to_vec()).await;
    assert_eq!(status, 400);
    assert!(
        reason.contains("-F"),
        "the refusal must name the flag it did not understand: {reason}"
    );
    assert_eq!(h.get_json("/flows").await.as_array().unwrap().len(), 5);
}

/// One capture, all the way out and all the way back: the epic's e2e step table,
/// in the order the plan states it.
#[tokio::test]
async fn a_capture_pages_exports_clears_and_comes_back() {
    let h = Harness::setup().await;
    drive_real_traffic(&h).await;

    let captured = h.wait_for_flows(4).await;
    let captured_ids: Vec<String> = captured
        .as_array()
        .unwrap()
        .iter()
        .map(|flow| flow["id"].as_str().unwrap().to_owned())
        .collect();
    let connections = h.connections().await;

    walk_the_listing_a_row_at_a_time(&h, &captured_ids).await;
    let exported_bytes = export_reads_as_har(&h).await;
    clear_empties_and_announces(&h, &captured_ids, connections).await;
    import_brings_it_back_marked(&h, &captured, exported_bytes).await;
    a_pasted_curl_command_lands_and_an_unknown_flag_does_not(&h).await;

    h.shutdown().await;
}
