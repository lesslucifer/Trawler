// e2e core launcher: starts a local HTTPS origin + a real Trawler core (serve(Config))
// on ephemeral ports, plus a tiny control server whose GET /hit sends one proxied
// HTTPS request (so a *decrypted* flow appears — the only kind onboarding's Verify
// step counts) and GET /stop shuts everything down. Prints
// {"api","proxy","origin","control"} as one JSON line. Hermetic: no public
// network, ephemeral ports, a tempdir CA, a self-signed origin CA. The control
// server exists because wdio's launcher (which spawns this) and its test worker are
// separate processes — HTTP is the portable bridge between them.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::Json;
use axum::Router;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use trawler::{serve, Config};

mod front;

#[derive(Clone)]
struct Control {
    proxy_url: String,
    origin_url: String,
    trawler_ca_pem: String,
    stop: Arc<Notify>,
    front: Arc<front::Front>,
}

// Self-signed CA + a localhost leaf for the origin. The core trusts the CA
// upstream (upstream_extra_root_pems) so it can complete the origin TLS and MITM
// it; the /hit client trusts Trawler's own CA so it accepts the re-signed leaf.
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

#[tokio::main]
async fn main() {
    trawler::init_crypto();
    let (origin_ca_pem, leaf_pem, leaf_key_pem) = gen_certs();

    let origin = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    let tls =
        axum_server::tls_rustls::RustlsConfig::from_pem(leaf_pem.into_bytes(), leaf_key_pem.into_bytes())
            .await
            .unwrap();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/", get(|| async { "hello from origin" }))
            .route("/json", get(json_body))
            .route("/br", get(brotli_json_body))
            .route("/img", get(png_body))
            .route("/xml", get(xml_body))
            .route("/big", get(oversized_text_body))
            .route("/headers", get(long_headers_body))
            .route("/missing", get(not_found_body))
            .route("/boom", get(server_error_body))
            .route("/echo", any(echo_headers));
        let _ = axum_server::from_tcp_rustls(origin, tls).serve(app.into_make_service()).await;
    });

    // Bound and released, so a send to it is refused and a spec can prove that a
    // failed send lands no flow.
    let closed_port =
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    let ca_dir = tempdir().unwrap();
    let config = Config {
        proxy_addr: "127.0.0.1:0".parse().unwrap(),
        api_addr: "127.0.0.1:0".parse().unwrap(),
        ca_dir: ca_dir.path().to_path_buf(),
        upstream_extra_root_pems: vec![origin_ca_pem],
        ..Default::default()
    };
    let server = serve(config).await.unwrap();

    let trawler_ca_pem = reqwest::get(format!("http://{}/ca.crt", server.api_addr))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let front = Arc::new(front::Front::new(server.api_addr, server.proxy_addr.port()));
    let front_addr = front::serve(Arc::clone(&front)).await;

    let stop = Arc::new(Notify::new());
    let control_state = Control {
        proxy_url: format!("http://{}", server.proxy_addr),
        origin_url: format!("https://localhost:{origin_port}/"),
        trawler_ca_pem,
        stop: Arc::clone(&stop),
        front,
    };
    let control = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let control_addr = control.local_addr().unwrap();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/hit", get(hit))
            .route("/stop", get(stop_handler))
            .route("/kill", get(kill_handler))
            .route("/front", get(front_mode))
            .route("/front/reach", post(front_reach))
            .with_state(control_state);
        let _ = axum::serve(control, app).await;
    });

    let ports = serde_json::json!({
        "api": format!("http://{}", server.api_addr),
        "proxy": format!("http://{}", server.proxy_addr),
        "origin": format!("https://localhost:{origin_port}/"),
        "control": format!("http://{control_addr}"),
        "closed": format!("http://127.0.0.1:{closed_port}/"),
        "front": format!("http://{front_addr}"),
    });
    println!("{ports}");

    stop.notified().await;
    server.shutdown().await;
}

// `/hit?p=json|br|img` proxies to that origin route; no param keeps the "/" hello.
// `h=name:value` adds one request header, for a spec that edits it on replay.
// The origin is HTTPS (a decrypted flow is the only kind onboarding's Verify counts),
// so the client trusts Trawler's re-signing CA — unless `untrusted` is given, which
// is a device that has not installed it: its tunnel opens and nothing decrypts.
async fn hit(State(ctrl): State<Control>, Query(q): Query<HashMap<String, String>>) -> &'static str {
    let trawler_ca = reqwest::Certificate::from_pem(ctrl.trawler_ca_pem.as_bytes()).unwrap();
    let url = match q.get("p") {
        Some(p) => format!("{}{}", ctrl.origin_url, p.trim_start_matches('/')),
        None => ctrl.origin_url.clone(),
    };
    let mut client = reqwest::Client::builder().proxy(reqwest::Proxy::all(&ctrl.proxy_url).unwrap());
    if !q.contains_key("untrusted") {
        client = client.add_root_certificate(trawler_ca);
    }
    let client = client.build().unwrap();
    let mut request = client.get(&url);
    if let Some((name, value)) = q.get("h").and_then(|h| h.split_once(':')) {
        request = request.header(name.trim(), value.trim());
    }
    let _ = request.send().await;
    "hit-ok"
}

// Answers with the request headers it received, so a spec reads an edit off the
// wire rather than off the flow. The pause is what lets a spec see the composer
// waiting: the core answers only once the reply exists.
async fn echo_headers(headers: axum::http::HeaderMap) -> impl IntoResponse {
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let echoed: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or_default().into()))
        .collect();
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::Value::Object(echoed).to_string(),
    )
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

// The only non-2xx traffic the harness produces. A status-class filter is
// answerable only against a capture holding more than one class, and the 5xx
// beside the 4xx is what stops "not 2xx" from passing for "is 4xx".
async fn not_found_body() -> impl IntoResponse {
    (axum::http::StatusCode::NOT_FOUND, "no such thing")
}

async fn server_error_body() -> impl IntoResponse {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "it broke")
}

async fn xml_body() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/xml")],
        r#"<order id="9f3c"><item sku="A-1" qty="2"/></order>"#,
    )
}

// Past the UI's 128 KiB render cap and far under the 10 MiB capture cap, so the
// body is captured whole and the pane must state what it left out.
async fn oversized_text_body() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain")],
        "trawler ".repeat(25_000),
    )
}

// A response whose headers dwarf its body: the shape real traffic has (session
// cookies, a bearer-sized token, a long CSP), so the inspector's layout can be
// checked against headers that would otherwise push the body off screen.
async fn long_headers_body() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    for i in 0..3 {
        let cookie = format!("session{i}={}; Path=/; HttpOnly; Secure", "x".repeat(900));
        headers.append(header::SET_COOKIE, cookie.parse().unwrap());
    }
    headers.insert("x-token", format!("Bearer {}", "eyJhbGciOiJIUzI1NiJ9.".repeat(40)).parse().unwrap());
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self' https://cdn.example.com; img-src * data:; connect-src 'self' https://api.example.com wss://live.example.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".parse().unwrap(),
    );
    for i in 0..12 {
        headers.insert(
            axum::http::HeaderName::from_bytes(format!("x-trace-{i}").as_bytes()).unwrap(),
            format!("{i:08x}-{}", "ab".repeat(16)).parse().unwrap(),
        );
    }
    (headers, r#"{"greeting":"hello","under":"the headers"}"#)
}

// `/front?mode=up|down|slow&ms=<n>`: how the window's core answers from now on.
async fn front_mode(
    State(ctrl): State<Control>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<&'static str, (axum::http::StatusCode, String)> {
    let mode = match q.get("mode").map(String::as_str) {
        Some("up") => front::Mode::Up,
        Some("down") => front::Mode::Down,
        Some("slow") => {
            let ms = q.get("ms").and_then(|ms| ms.parse().ok()).unwrap_or(10_000);
            front::Mode::Slow(std::time::Duration::from_millis(ms))
        }
        other => {
            return Err((axum::http::StatusCode::BAD_REQUEST, format!("no front mode {other:?}")))
        }
    };
    ctrl.front.set_mode(mode);
    Ok("front-ok")
}

async fn front_reach(
    State(ctrl): State<Control>,
    Json(interfaces): Json<Vec<front::Interface>>,
) -> &'static str {
    ctrl.front.set_interfaces(interfaces);
    "front-ok"
}

async fn stop_handler(State(ctrl): State<Control>) -> &'static str {
    ctrl.stop.notify_one();
    "stopping"
}

// "The core died under an open window" needs the process gone, not drained: a
// graceful shutdown waits for in-flight requests, and an SSE stream never
// finishes — it would hold open the very connection the assertion is about.
// The delay is only so this response reaches the caller before the exit.
async fn kill_handler() -> &'static str {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        std::process::exit(0);
    });
    "killing"
}
