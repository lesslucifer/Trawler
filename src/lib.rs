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
pub mod recorder;
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

pub use config::Config;
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
    shutdown_tx: watch::Sender<bool>,
    proxy_task: JoinHandle<()>,
    api_task: JoinHandle<()>,
}

impl RunningServer {
    /// Signal both servers to stop and wait for them to wind down.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.proxy_task.await;
        let _ = self.api_task.await;
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
    let app = api::router(Arc::clone(&store), Arc::new(ca_material.cert_pem));
    let api_task = tokio::spawn(async move {
        let served = axum::serve(api_listener, app)
            .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
            .await;
        if let Err(e) = served {
            error!("control API stopped: {e}");
        }
    });

    Ok(RunningServer { proxy_addr, api_addr, shutdown_tx, proxy_task, api_task })
}

async fn wait_for_shutdown(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|stop| *stop).await;
}

/// Build the proxy-to-origin TLS connector: webpki roots plus any extra CA PEMs
/// from config (default none reproduces standard system-trust behaviour).
fn upstream_connector(extra_root_pems: &[String]) -> HttpsConnector<HttpConnector> {
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
    HttpsConnectorBuilder::new().with_tls_config(tls).https_or_http().enable_http1().build()
}

/// Boot the engine and run until interrupted.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let server = serve(config).await?;
    info!("proxy listening on http://{}", server.proxy_addr);
    info!("control API listening on http://{}", server.api_addr);
    info!("CA download at http://{}/ca.crt", server.api_addr);

    shutdown_signal().await;
    info!("shutting down");
    server.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
