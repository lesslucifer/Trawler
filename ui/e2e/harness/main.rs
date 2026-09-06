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
    trawler_ca_pem: String,
    stop: Arc<Notify>,
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
            .route("/headers", get(long_headers_body));
        let _ = axum_server::from_tcp_rustls(origin, tls).serve(app.into_make_service()).await;
    });

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

    let stop = Arc::new(Notify::new());
    let control_state = Control {
        proxy_url: format!("http://{}", server.proxy_addr),
        origin_url: format!("https://localhost:{origin_port}/"),
        trawler_ca_pem,
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
        "origin": format!("https://localhost:{origin_port}/"),
        "control": format!("http://{control_addr}"),
    });
    println!("{ports}");

    stop.notified().await;
    server.shutdown().await;
}

// `/hit?p=json|br|img` proxies to that origin route; no param keeps the "/" hello.
// The origin is HTTPS (a decrypted flow is the only kind onboarding's Verify counts),
// so the client trusts Trawler's re-signing CA.
async fn hit(State(ctrl): State<Control>, Query(q): Query<HashMap<String, String>>) -> &'static str {
    let trawler_ca = reqwest::Certificate::from_pem(ctrl.trawler_ca_pem.as_bytes()).unwrap();
    let url = match q.get("p") {
        Some(p) => format!("{}{}", ctrl.origin_url, p.trim_start_matches('/')),
        None => ctrl.origin_url.clone(),
    };
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&ctrl.proxy_url).unwrap())
        .add_root_certificate(trawler_ca)
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

async fn stop_handler(State(ctrl): State<Control>) -> &'static str {
    ctrl.stop.notify_one();
    "stopping"
}
