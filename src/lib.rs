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

use std::future::IntoFuture;
use std::sync::Arc;

use hudsucker::certificate_authority::RcgenAuthority;
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::Proxy;
use tokio::net::TcpListener;
use tracing::{error, info};

pub use config::Config;
pub use event::FlowEvent;
pub use flow::{Flow, FlowId};
pub use recorder::Recorder;
pub use store::Store;

/// Boot the capture engine and the control API, then run until either stops or
/// the process is interrupted.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let _ = aws_lc_rs::default_provider().install_default();

    let ca_material = ca::load_or_create(&config);
    let ca = RcgenAuthority::new(
        ca_material.issuer,
        config.cert_cache_size,
        aws_lc_rs::default_provider(),
    );

    let store = Arc::new(Store::new(config.ring_max_bytes, config.broadcast_capacity));
    let recorder = Recorder::new(Arc::clone(&store), config.body_cap);

    let proxy = Proxy::builder()
        .with_addr(config.proxy_addr)
        .with_ca(ca)
        .with_rustls_connector(aws_lc_rs::default_provider())
        .with_http_handler(recorder)
        .build()?;

    let app = api::router(Arc::clone(&store), Arc::new(ca_material.cert_pem));
    let listener = TcpListener::bind(config.api_addr).await?;

    info!("proxy listening on http://{}", config.proxy_addr);
    info!("control API listening on http://{}", config.api_addr);
    info!("CA download at http://{}/ca.crt", config.api_addr);

    tokio::select! {
        result = proxy.start() => {
            if let Err(e) = result {
                error!("proxy stopped: {e}");
            }
        }
        result = axum::serve(listener, app).into_future() => {
            if let Err(e) = result {
                error!("control API stopped: {e}");
            }
        }
        _ = shutdown_signal() => {
            info!("shutting down");
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
