// e2e core launcher: starts a local origin + a real Trawler core (serve(Config))
// on ephemeral ports, plus a tiny control server whose GET /hit sends one proxied
// request (so a flow appears) and GET /stop shuts everything down. Prints
// {"api","proxy","origin","control"} as one JSON line. Hermetic: no public
// network, ephemeral ports, a tempdir CA. The control server exists because
// wdio's launcher (which spawns this) and its test worker are separate processes
// — HTTP is the portable bridge between them.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use trawler::{serve, Config};

#[derive(Clone)]
struct Control {
    proxy_url: String,
    origin_url: String,
    stop: Arc<Notify>,
}

#[tokio::main]
async fn main() {
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_addr = origin.local_addr().unwrap();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/", get(|| async { "hello from origin" }))
            .route("/json", get(json_body))
            .route("/br", get(brotli_json_body))
            .route("/img", get(png_body));
        let _ = axum::serve(origin, app).await;
    });

    let ca_dir = tempdir().unwrap();
    let config = Config {
        proxy_addr: "127.0.0.1:0".parse().unwrap(),
        api_addr: "127.0.0.1:0".parse().unwrap(),
        ca_dir: ca_dir.path().to_path_buf(),
        ..Default::default()
    };
    let server = serve(config).await.unwrap();

    let stop = Arc::new(Notify::new());
    let control_state = Control {
        proxy_url: format!("http://{}", server.proxy_addr),
        origin_url: format!("http://{origin_addr}/"),
        stop: Arc::clone(&stop),
    };
    let control = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control.local_addr().unwrap();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/hit", get(hit))
            .route("/stop", get(stop_handler))
            .with_state(control_state);
        let _ = axum::serve(control, app).await;
    });

    let ports = serde_json::json!({
        "api": format!("http://{}", server.api_addr),
        "proxy": format!("http://{}", server.proxy_addr),
        "origin": format!("http://{origin_addr}/"),
        "control": format!("http://{control_addr}"),
    });
    println!("{ports}");

    stop.notified().await;
    server.shutdown().await;
}

// `/hit?p=json|br|img` proxies to that origin route; no param keeps the UC-4 "/".
async fn hit(State(ctrl): State<Control>, Query(q): Query<HashMap<String, String>>) -> &'static str {
    let url = match q.get("p") {
        Some(p) => format!("{}{}", ctrl.origin_url, p.trim_start_matches('/')),
        None => ctrl.origin_url.clone(),
    };
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&ctrl.proxy_url).unwrap())
        .build()
        .unwrap();
    let _ = client.get(&url).send().await;
    "hit-ok"
}

async fn json_body() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"greeting":"hello","numbers":[1,2,3]}"#,
    )
}

// Pre-compressed on the origin so the stored capture bytes are brotli — the UI
// must go through the server-side decode chain (W1) to show a JSON tree.
async fn brotli_json_body() -> impl IntoResponse {
    let mut compressed = Vec::new();
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
        writer.write_all(br#"{"compressed":"brotli","ok":true}"#).unwrap();
    }
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_ENCODING, "br"),
        ],
        compressed,
    )
}

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
    0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x78,
    0xe1, 0xab, 0x05, 0x00, 0x03, 0x80, 0x01, 0x60, 0x37, 0x6e, 0x7b, 0x87, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

async fn png_body() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], PNG_1X1)
}

async fn stop_handler(State(ctrl): State<Control>) -> &'static str {
    ctrl.stop.notify_one();
    "stopping"
}
