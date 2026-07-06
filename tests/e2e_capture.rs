#![allow(clippy::unwrap_used)]

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::Request;
use axum::http::header::{CONTENT_ENCODING, CONTENT_TYPE};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use serde_json::Value;
use tempfile::{tempdir, TempDir};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use trawler::{serve, Config, RunningServer};

use common::gzip;

async fn start_origin() -> (SocketAddr, JoinHandle<()>) {
    async fn ok() -> &'static str {
        "hello"
    }
    async fn echo(req: Request) -> Bytes {
        axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap()
    }
    async fn big() -> Vec<u8> {
        vec![b'x'; 1000]
    }
    async fn gzip_json() -> impl IntoResponse {
        ([(CONTENT_TYPE, "application/json"), (CONTENT_ENCODING, "gzip")], gzip(br#"{"msg":"hi"}"#))
    }
    async fn sse() -> impl IntoResponse {
        ([(CONTENT_TYPE, "text/event-stream")], "data: 1\n\ndata: 2\n\ndata: 3\n\n")
    }

    let app = Router::new()
        .route("/ok", get(ok))
        .route("/echo", post(echo))
        .route("/big", get(big))
        .route("/gzip", get(gzip_json))
        .route("/sse", get(sse));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, task)
}

struct Harness {
    origin: SocketAddr,
    api_addr: SocketAddr,
    proxy: reqwest::Client,
    api: reqwest::Client,
    server: Option<RunningServer>,
    _ca_dir: TempDir,
    _origin: JoinHandle<()>,
}

impl Harness {
    async fn setup(body_cap: usize) -> Self {
        let (origin, origin_task) = start_origin().await;
        let ca_dir = tempdir().unwrap();
        let config = Config {
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            api_addr: "127.0.0.1:0".parse().unwrap(),
            ca_dir: ca_dir.path().to_path_buf(),
            body_cap,
            ..Default::default()
        };
        let server = serve(config).await.unwrap();
        let proxy = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", server.proxy_addr)).unwrap())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let api = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
        Self {
            origin,
            api_addr: server.api_addr,
            proxy,
            api,
            server: Some(server),
            _ca_dir: ca_dir,
            _origin: origin_task,
        }
    }

    fn origin_url(&self, path: &str) -> String {
        format!("http://{}{}", self.origin, path)
    }

    async fn api_json(&self, path: &str) -> Value {
        let body = self
            .api
            .get(format!("http://{}{}", self.api_addr, path))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        serde_json::from_str(&body).unwrap()
    }

    /// Poll the control API until the flow for `path` is recorded, returning its
    /// full `FlowView`. Polling a real condition, never a fixed sleep.
    async fn wait_flow(&self, path: &str) -> Value {
        for _ in 0..100 {
            let flows = self.api_json(&format!("/flows?q={path}")).await;
            if let Some(first) = flows.as_array().and_then(|a| a.first()) {
                let id = first["id"].as_str().unwrap().to_string();
                return self.api_json(&format!("/flows/{id}")).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("flow for {path} was never captured");
    }

    async fn shutdown(mut self) {
        if let Some(server) = self.server.take() {
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn plain_http_get_and_post_are_captured_transparently() {
    let h = Harness::setup(100).await;

    let body = h.proxy.get(h.origin_url("/ok")).send().await.unwrap().text().await.unwrap();
    assert_eq!(body, "hello", "client must receive the origin's bytes unaltered");
    let flow = h.wait_flow("/ok").await;
    assert_eq!(flow["method"], "GET");
    assert_eq!(flow["status"], 200);
    assert_eq!(flow["response_body"]["state"], "captured");

    let echoed = h
        .proxy
        .post(h.origin_url("/echo"))
        .body("ping")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(echoed, "ping");
    let flow = h.wait_flow("/echo").await;
    assert_eq!(flow["method"], "POST");
    assert_eq!(flow["request_body"]["state"], "captured");

    h.shutdown().await;
}

#[tokio::test]
async fn sse_streams_and_over_cap_body_forwarded_in_full() {
    let h = Harness::setup(100).await;

    let sse = h.proxy.get(h.origin_url("/sse")).send().await.unwrap().text().await.unwrap();
    assert!(
        sse.contains("data: 1") && sse.contains("data: 3"),
        "client must receive all SSE events"
    );
    let flow = h.wait_flow("/sse").await;
    assert_eq!(flow["response_body"]["state"], "streaming", "SSE must never be buffered");

    let big = h.proxy.get(h.origin_url("/big")).send().await.unwrap().bytes().await.unwrap();
    assert_eq!(big.len(), 1000, "client must receive the full over-cap body");
    let flow = h.wait_flow("/big").await;
    assert_eq!(flow["response_body"]["state"], "not_captured");
    assert_eq!(flow["response_body"]["reason"], "too_large");

    h.shutdown().await;
}

fn gen_certs() -> (String, String, String) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    };
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let ca_pem = ca_cert.pem();
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_key = KeyPair::generate().unwrap();
    let mut leaf_params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();
    (ca_pem, leaf_cert.pem(), leaf_key.serialize_pem())
}

async fn start_https_origin(leaf_pem: String, leaf_key_pem: String) -> (u16, JoinHandle<()>) {
    use axum_server::tls_rustls::RustlsConfig;
    let app = Router::new().route("/secure", get(|| async { "top-secret" }));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config =
        RustlsConfig::from_pem(leaf_pem.into_bytes(), leaf_key_pem.into_bytes()).await.unwrap();
    let task = tokio::spawn(async move {
        let _ = axum_server::from_tcp_rustls(listener, config).serve(app.into_make_service()).await;
    });
    (port, task)
}

#[tokio::test]
async fn https_mitm_decrypts_and_captures() {
    trawler::init_crypto();
    let (ca_pem, leaf_pem, leaf_key_pem) = gen_certs();
    let (port, _origin) = start_https_origin(leaf_pem, leaf_key_pem).await;

    let ca_dir = tempdir().unwrap();
    let config = Config {
        proxy_addr: "127.0.0.1:0".parse().unwrap(),
        api_addr: "127.0.0.1:0".parse().unwrap(),
        ca_dir: ca_dir.path().to_path_buf(),
        upstream_extra_root_pems: vec![ca_pem],
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
    let trawler_ca = reqwest::Certificate::from_pem(&trawler_ca_pem).unwrap();

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{}", server.proxy_addr)).unwrap())
        .add_root_certificate(trawler_ca)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let url = format!("https://localhost:{port}/secure");
    let body = client.get(&url).send().await.unwrap().text().await.unwrap();
    assert_eq!(body, "top-secret", "client must receive decrypted origin bytes through the MITM");

    let flows_text = api
        .get(format!("http://{}/flows?q=/secure", server.api_addr))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let flows: Value = serde_json::from_str(&flows_text).unwrap();
    let arr = flows.as_array().unwrap();
    assert!(!arr.is_empty(), "decrypted HTTPS request was not captured");
    assert_eq!(arr[0]["method"], "GET");
    assert!(
        arr.iter().all(|f| f["method"] != "CONNECT"),
        "a bogus CONNECT flow leaked into capture"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn ui_shell_serves_live_filterable_flow_list() {
    let h = Harness::setup(100).await;

    h.proxy.get(h.origin_url("/ok")).send().await.unwrap();
    h.wait_flow("/ok").await;

    let page = h.api.get(format!("http://{}/", h.api_addr)).send().await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    let html = page.text().await.unwrap();
    for wired in ["/flows", "/stream", "EventSource"] {
        assert!(html.contains(wired), "shell page must wire {wired}");
    }

    let matching = h.api_json("/flows?host=127.0.0.1").await;
    assert!(!matching.as_array().unwrap().is_empty(), "host filter must return the proxied flow");
    let none = h.api_json("/flows?host=no-such-host").await;
    assert!(none.as_array().unwrap().is_empty(), "non-matching host filter must return nothing");

    let mut sse = h.api.get(format!("http://{}/stream", h.api_addr)).send().await.unwrap();
    h.proxy.get(h.origin_url("/big")).send().await.unwrap();
    let mut seen = String::new();
    tokio::time::timeout(Duration::from_secs(4), async {
        while let Some(chunk) = sse.chunk().await.unwrap() {
            seen.push_str(&String::from_utf8_lossy(&chunk));
            if seen.contains("event: request") && seen.contains("/big") {
                return;
            }
        }
        panic!("SSE stream ended without the /big event");
    })
    .await
    .expect("no SSE event for the new flow before timeout");

    // `sse` is still open on purpose: shutdown must not hang on a live SSE stream
    h.shutdown().await;
}

#[tokio::test]
async fn gzip_capture_raw_decode_on_view_and_ca_download() {
    let h = Harness::setup(100).await;

    let client_bytes =
        h.proxy.get(h.origin_url("/gzip")).send().await.unwrap().bytes().await.unwrap();
    assert_eq!(&client_bytes[..2], &[0x1f, 0x8b], "client received the compressed bytes as-is");

    let flow = h.wait_flow("/gzip").await;
    let id = flow["id"].as_str().unwrap();

    let raw = h
        .api
        .get(format!("http://{}/flows/{}/response/body", h.api_addr, id))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(&raw[..2], &[0x1f, 0x8b], "stored body is the raw compressed bytes");

    let decoded = h
        .api
        .get(format!("http://{}/flows/{}/response/body?decode=1", h.api_addr, id))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let parsed: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(parsed["msg"], "hi", "decode endpoint reverses gzip + pretty-prints JSON");

    let ca = h.api.get(format!("http://{}/ca.crt", h.api_addr)).send().await.unwrap();
    assert_eq!(ca.headers()["content-type"], "application/x-x509-ca-cert");
    assert!(ca.text().await.unwrap().contains("BEGIN CERTIFICATE"));

    h.shutdown().await;
}
