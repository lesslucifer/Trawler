//! Trawler core — a UI-agnostic HTTP(S) capture engine.
//!
//! The proxy handler is a dumb producer; a central [`Store`] owns the canonical
//! flows; every interface is a subscriber over a broadcast bus. Live-tailing and
//! filtering fall out of that shape rather than being bolted on. The product
//! surface is a local control API — any UI (TUI, web, `curl`) is just a client.

pub mod api;
pub mod ca;
pub mod capture;
pub mod config;
pub mod event;
pub mod flow;
pub mod reach;
pub mod recorder;
pub mod sender;
pub mod store;

use std::net::SocketAddr;
use std::sync::Arc;

use hudsucker::certificate_authority::RcgenAuthority;
use hudsucker::hyper_util::client::legacy::connect::HttpConnector;
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::rustls::{ClientConfig, RootCertStore};
use hudsucker::Proxy;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info};

pub use config::{Config, Reach};
pub use event::FlowEvent;
pub use flow::{Flow, FlowId};
pub use recorder::Recorder;
pub use store::Store;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;

/// Install the process-global rustls crypto provider exactly once. Wrapped in a
/// `Once` so repeated `serve()` calls — notably many e2e tests in one process —
/// never race or double-install.
pub fn init_crypto() {
    static CRYPTO: std::sync::Once = std::sync::Once::new();
    CRYPTO.call_once(|| {
        let _ = aws_lc_rs::default_provider().install_default();
    });
}

/// A running engine: the addresses both servers actually bound (resolved, so an
/// ephemeral `:0` port is observable) plus a handle to stop them.
pub struct RunningServer {
    pub proxy_addr: SocketAddr,
    pub api_addr: SocketAddr,
    reach: Arc<reach::Controller>,
    shutdown_tx: watch::Sender<bool>,
    proxy_task: JoinHandle<()>,
    api_task: JoinHandle<()>,
}

impl RunningServer {
    /// Signal both servers to stop and wait for them to wind down. The proxy
    /// drains gracefully; the API task is aborted after the signal because open
    /// SSE `/stream` connections never complete, so a graceful wait on them
    /// hangs shutdown forever (e.g. Ctrl-C with the UI shell open).
    pub async fn shutdown(self) {
        self.reach.disarm().await;
        let _ = self.shutdown_tx.send(true);
        let _ = self.proxy_task.await;
        self.api_task.abort();
        let _ = self.api_task.await;
    }

    /// What an armed session bound, or `None` while Trawler answers only this
    /// machine. Resolved, so a test that armed an ephemeral port can find it.
    pub async fn reachable_at(&self) -> Option<reach::Addresses> {
        self.reach.addresses().await
    }
}

/// Boot the capture engine and control API without blocking, returning the
/// resolved bound addresses and a shutdown handle. `run()` is this plus a wait
/// on the interrupt signal — this split lets tests drive the real servers on
/// ephemeral ports and shut them down deterministically.
pub async fn serve(config: Config) -> Result<RunningServer, Box<dyn std::error::Error>> {
    init_crypto();

    let ca_material = ca::load_or_create(&config);
    let ca = RcgenAuthority::new(
        ca_material.issuer,
        config.cert_cache_size,
        aws_lc_rs::default_provider(),
    );

    let store = Arc::new(Store::new(config.ring_max_bytes, config.broadcast_capacity));
    let recorder = Recorder::new(Arc::clone(&store), config.body_cap);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let proxy_listener = TcpListener::bind(config.proxy_addr).await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy = Proxy::builder()
        .with_listener(proxy_listener)
        .with_ca(ca)
        .with_http_connector(upstream_connector(&config.upstream_extra_root_pems))
        .with_http_handler(recorder)
        .with_graceful_shutdown(wait_for_shutdown(shutdown_rx.clone()))
        .build()?;
    let proxy_task = tokio::spawn(async move {
        if let Err(e) = proxy.start().await {
            error!("proxy stopped: {e}");
        }
    });

    let api_listener = TcpListener::bind(config.api_addr).await?;
    let api_addr = api_listener.local_addr()?;
    let reach = Arc::new(reach::Controller::new(
        config.clone(),
        Arc::clone(&store),
        ca_material.pems.clone(),
    ));
    let sender = sender::Sender::new(&config.upstream_extra_root_pems, config.body_cap);
    let app = api::router(
        Arc::clone(&store),
        Arc::new(ca_material.pems.cert),
        Arc::clone(&reach),
        sender,
    );
    // A config that boots armed is how a headless core reaches a device without
    // a UI to confirm at; the arming a user does goes through the control API.
    if let Some(address) = config.reach.address() {
        reach.arm(address, app.clone()).await?;
    }
    let api_task = tokio::spawn(async move {
        let served = axum::serve(api_listener, app)
            .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
            .await;
        if let Err(e) = served {
            error!("control API stopped: {e}");
        }
    });

    Ok(RunningServer { proxy_addr, api_addr, reach, shutdown_tx, proxy_task, api_task })
}

pub(crate) async fn wait_for_shutdown(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|stop| *stop).await;
}

/// Build the proxy-to-origin TLS connector: webpki roots plus any extra CA PEMs
/// from config (default none reproduces standard system-trust behaviour).
pub(crate) fn upstream_connector(extra_root_pems: &[String]) -> HttpsConnector<HttpConnector> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for pem in extra_root_pems {
        let mut reader = pem.as_bytes();
        for cert in rustls_pemfile::certs(&mut reader).flatten() {
            let _ = roots.add(cert);
        }
    }
    let tls = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("tls protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    HttpsConnectorBuilder::new().with_tls_config(tls).https_or_http().enable_all_versions().build()
}

/// Boot the engine and run until interrupted.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let server = serve(config).await?;
    info!("proxy listening on http://{}", server.proxy_addr);
    info!("control API listening on http://{}", server.api_addr);
    info!("CA download at http://{}/ca.crt", server.api_addr);
    if let Some(addresses) = server.reachable_at().await {
        info!(
            "reachable from other devices at http://{} — certificate at {}",
            addresses.proxy,
            addresses.ca_cert_url()
        );
    } else {
        info!("loopback only — other devices cannot reach this Trawler");
    }

    shutdown_signal().await;
    info!("shutting down");
    server.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::upstream_connector;
    use bytes::Bytes;
    use http_body_util::Empty;
    use hudsucker::hyper_util::client::legacy::Client;
    use hudsucker::hyper_util::rt::TokioExecutor;

    async fn version_negotiated_with_origin(alpn: &[&str]) -> http::Version {
        let (ca_pem, server_config) = crate::test_support::tls_origin(alpn);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let app = axum::Router::new().route("/", axum::routing::get(|| async { "ok" }));
        let tls =
            axum_server::tls_rustls::RustlsConfig::from_config(std::sync::Arc::new(server_config));
        tokio::spawn(async move {
            let _ =
                axum_server::from_tcp_rustls(listener, tls).serve(app.into_make_service()).await;
        });

        let client = Client::builder(TokioExecutor::new())
            .build::<_, Empty<Bytes>>(upstream_connector(&[ca_pem]));
        let uri = format!("https://localhost:{port}/").parse().expect("uri");
        client.get(uri).await.expect("request through connector").version()
    }

    #[tokio::test]
    async fn upstream_connector_negotiates_h2_when_origin_offers_it() {
        assert_eq!(
            version_negotiated_with_origin(&["h2", "http/1.1"]).await,
            http::Version::HTTP_2
        );
    }

    #[tokio::test]
    async fn upstream_connector_keeps_h1_against_h1_only_origin() {
        assert_eq!(version_negotiated_with_origin(&["http/1.1"]).await, http::Version::HTTP_11);
    }
}
