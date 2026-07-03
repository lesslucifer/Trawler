//! Trawler — a native HTTP(S) debugging proxy.
//!
//! Boots a MITM proxy on 127.0.0.1:3000, generating (and persisting) a local
//! Certificate Authority on first run so HTTPS traffic can be decrypted once the
//! CA is trusted by the client. Every request and response is logged.

use std::net::SocketAddr;
use std::path::Path;

use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{Request, Response},
    rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose},
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use tracing::{error, info};

const LISTEN_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 3000);
const CA_DIR: &str = "ca";
const CA_CERT: &str = "ca/trawler-ca.pem";
const CA_KEY: &str = "ca/trawler-ca.key";

#[derive(Clone)]
struct LogHandler;

impl HttpHandler for LogHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        info!(method = %req.method(), uri = %req.uri(), "→ request");
        req.into()
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        info!(status = %res.status(), "← response");
        res
    }
}

/// Load the persisted CA, or generate and save a new one on first run.
fn load_or_create_ca() -> (KeyPair, hudsucker::rcgen::Certificate) {
    if Path::new(CA_CERT).exists() && Path::new(CA_KEY).exists() {
        let key_pem = std::fs::read_to_string(CA_KEY).expect("read CA key");
        let cert_pem = std::fs::read_to_string(CA_CERT).expect("read CA cert");
        let key_pair = KeyPair::from_pem(&key_pem).expect("parse CA key");
        let cert = CertificateParams::from_ca_cert_pem(&cert_pem)
            .expect("parse CA cert")
            .self_signed(&key_pair)
            .expect("re-sign CA cert");
        info!("loaded existing CA from {CA_DIR}/");
        return (key_pair, cert);
    }

    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params
        .distinguished_name
        .push(DnType::CommonName, "Trawler Proxy CA");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Trawler");

    let key_pair = KeyPair::generate().expect("generate CA key");
    let cert = params.self_signed(&key_pair).expect("self-sign CA");

    std::fs::create_dir_all(CA_DIR).expect("create ca dir");
    std::fs::write(CA_CERT, cert.pem()).expect("write CA cert");
    std::fs::write(CA_KEY, key_pair.serialize_pem()).expect("write CA key");
    info!("generated new CA at {CA_CERT} — trust this certificate to decrypt HTTPS");

    (key_pair, cert)
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("install ctrl-c handler");
    info!("shutting down");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("trawler=info".parse().unwrap()),
        )
        .init();

    let (key_pair, ca_cert) = load_or_create_ca();
    let ca = RcgenAuthority::new(key_pair, ca_cert, 1_000);

    let proxy = Proxy::builder()
        .with_addr(LISTEN_ADDR)
        .with_rustls_client()
        .with_ca(ca)
        .with_http_handler(LogHandler)
        .with_graceful_shutdown(shutdown_signal())
        .build();

    info!("Trawler listening on http://{LISTEN_ADDR}");
    if let Err(e) = proxy.start().await {
        error!("proxy error: {e}");
    }
}
