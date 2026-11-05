use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use serde_json::Value;
use tempfile::{tempdir, TempDir};
use tokio::task::JoinHandle;
use trawler::{serve, Config, RunningServer};

use crate::common::tls_origin;

fn start_tls_origin(alpn: &[&str]) -> (String, u16, JoinHandle<()>) {
    async fn hello() -> &'static str {
        "hello-tls"
    }
    async fn echo(req: Request) -> Bytes {
        axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap()
    }
    let (ca_pem, server_config) = tls_origin(alpn);
    let app = Router::new().route("/ok", get(hello)).route("/echo", post(echo));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let tls = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(server_config));
    let task = tokio::spawn(async move {
        let _ = axum_server::from_tcp_rustls(listener, tls).serve(app.into_make_service()).await;
    });
    (ca_pem, port, task)
}

struct Harness {
    proxy_addr: std::net::SocketAddr,
    api_addr: std::net::SocketAddr,
    trawler_ca: reqwest::Certificate,
    api: reqwest::Client,
    server: RunningServer,
    _ca_dir: TempDir,
    _origin: JoinHandle<()>,
}

impl Harness {
    async fn setup(upstream_ca_pem: String, origin_task: JoinHandle<()>) -> Self {
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
        let pem = api
            .get(format!("http://{}/ca.crt", server.api_addr))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let trawler_ca = reqwest::Certificate::from_pem(&pem).unwrap();
        Self {
            proxy_addr: server.proxy_addr,
            api_addr: server.api_addr,
            trawler_ca,
            api,
            server,
            _ca_dir: ca_dir,
            _origin: origin_task,
        }
    }

    fn client_builder(&self) -> reqwest::ClientBuilder {
        reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", self.proxy_addr)).unwrap())
            .add_root_certificate(self.trawler_ca.clone())
            .timeout(Duration::from_secs(5))
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

    /// Poll until `count` flows matching `path` all have a recorded response,
    /// then return their full `FlowView`s. Polling a real condition, never a
    /// fixed sleep.
    async fn wait_completed_flows(&self, path: &str, count: usize) -> Vec<Value> {
        for _ in 0..100 {
            let flows = self.api_json(&format!("/flows?q={path}")).await;
            let arr = flows.as_array().unwrap();
            if arr.len() == count && arr.iter().all(|f| !f["status"].is_null()) {
                let mut views = Vec::new();
                for summary in arr {
                    let id = summary["id"].as_str().unwrap();
                    views.push(self.api_json(&format!("/flows/{id}")).await);
                }
                return views;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{count} completed flow(s) for {path} were never captured");
    }

    async fn wait_flow(&self, path: &str) -> Value {
        self.wait_completed_flows(path, 1).await.pop().unwrap()
    }

    async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

fn assert_versions(flow: &Value, request: &str, response: &str) {
    assert_eq!(flow["request_http_version"], request, "client-leg version");
    assert_eq!(flow["response_http_version"], response, "upstream-leg version");
}

#[tokio::test]
async fn h2_client_and_h2_origin_speak_h2_on_both_legs() {
    let (ca_pem, port, origin) = start_tls_origin(&["h2", "http/1.1"]);
    let h = Harness::setup(ca_pem, origin).await;
    let client = h.client_builder().build().unwrap();

    let body = client
        .get(format!("https://localhost:{port}/ok"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "hello-tls", "body must round-trip intact through the h2 MITM");

    let flow = h.wait_flow("/ok").await;
    assert_versions(&flow, "HTTP/2.0", "HTTP/2.0");

    h.shutdown().await;
}

#[tokio::test]
async fn h2_client_against_h1_only_origin_records_mixed_legs() {
    let (ca_pem, port, origin) = start_tls_origin(&["http/1.1"]);
    let h = Harness::setup(ca_pem, origin).await;
    let client = h.client_builder().build().unwrap();

    let echoed = client
        .post(format!("https://localhost:{port}/echo"))
        .body("downgrade-leg")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(echoed, "downgrade-leg", "h2->h1 adaptation must not corrupt the body");

    let flow = h.wait_flow("/echo").await;
    assert_versions(&flow, "HTTP/2.0", "HTTP/1.1");

    h.shutdown().await;
}

#[tokio::test]
async fn h1_client_against_h2_origin_records_mixed_legs() {
    let (ca_pem, port, origin) = start_tls_origin(&["h2", "http/1.1"]);
    let h = Harness::setup(ca_pem, origin).await;
    let client = h.client_builder().http1_only().build().unwrap();

    let echoed = client
        .post(format!("https://localhost:{port}/echo"))
        .body("upgrade-leg")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(echoed, "upgrade-leg", "h1->h2 adaptation must not corrupt the body");

    let flow = h.wait_flow("/echo").await;
    assert_versions(&flow, "HTTP/1.1", "HTTP/2.0");

    h.shutdown().await;
}

#[tokio::test]
async fn concurrent_h2_streams_correlate_each_response_to_its_request() {
    let (ca_pem, port, origin) = start_tls_origin(&["h2", "http/1.1"]);
    let h = Harness::setup(ca_pem, origin).await;
    let client = h.client_builder().build().unwrap();

    let warmup =
        client.get(format!("https://localhost:{port}/ok")).send().await.unwrap().text().await;
    assert!(warmup.is_ok(), "warm-up request establishes the single h2 connection to multiplex");

    let stream_count = 5;
    let mut requests = Vec::new();
    for n in 1..=stream_count {
        let client = client.clone();
        let url = format!("https://localhost:{port}/echo");
        let body = "x".repeat(n);
        requests.push(tokio::spawn(async move {
            let echoed =
                client.post(url).body(body.clone()).send().await.unwrap().text().await.unwrap();
            assert_eq!(echoed, body, "each stream must receive its own echo");
        }));
    }
    for request in requests {
        request.await.unwrap();
    }

    let flows = h.wait_completed_flows("/echo", stream_count).await;
    let mut sizes = Vec::new();
    for flow in &flows {
        assert_versions(flow, "HTTP/2.0", "HTTP/2.0");
        assert_eq!(
            flow["request_body"]["data_base64"], flow["response_body"]["data_base64"],
            "a flow paired a response with a foreign request under multiplexing"
        );
        sizes.push(flow["request_body"]["size"].as_u64().unwrap());
    }
    sizes.sort_unstable();
    let expected: Vec<u64> = (1..=stream_count as u64).collect();
    assert_eq!(sizes, expected, "every concurrent stream must be captured exactly once");

    h.shutdown().await;
}
