// e2e core launcher: starts a local origin + a real Trawler core (serve(Config))
// on ephemeral ports, plus a tiny control server whose GET /hit sends one proxied
// request (so a flow appears) and GET /stop shuts everything down. Prints
// {"api","proxy","origin","control"} as one JSON line. Hermetic: no public
// network, ephemeral ports, a tempdir CA. The control server exists because
// wdio's launcher (which spawns this) and its test worker are separate processes
// — HTTP is the portable bridge between them.

use std::sync::Arc;

use axum::extract::State;
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
        let app = Router::new().route("/", get(|| async { "hello from origin" }));
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

async fn hit(State(ctrl): State<Control>) -> &'static str {
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&ctrl.proxy_url).unwrap())
        .build()
        .unwrap();
    let _ = client.get(&ctrl.origin_url).send().await;
    "hit-ok"
}

async fn stop_handler(State(ctrl): State<Control>) -> &'static str {
    ctrl.stop.notify_one();
    "stopping"
}
