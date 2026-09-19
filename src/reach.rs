//! Reach — letting a device that is not this machine talk to Trawler.
//!
//! The loopback pair of listeners is never touched. Arming *adds* a second
//! proxy and a second control API on one interface address, so the app's own
//! client keeps the address it started on and disarming can never cost it its
//! connection. Nothing here is persisted: the listeners live in the process, so
//! an opt-in cannot outlive the session that confirmed it (DEC-55).

use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use hudsucker::certificate_authority::RcgenAuthority;
use hudsucker::rustls::crypto::aws_lc_rs;
use hudsucker::Proxy;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::config::Config;
use crate::store::Store;
use crate::{ca, upstream_connector, wait_for_shutdown, Recorder};

/// One address a second device could route to, with the interface it belongs to
/// so a user picking between three of them can tell which is the Wi-Fi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Interface {
    pub name: String,
    pub address: IpAddr,
}

/// Both sockets an armed session added, resolved — so an ephemeral port is
/// observable and the address reported to a user is the one that answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Addresses {
    pub proxy: SocketAddr,
    pub api: SocketAddr,
}

impl Addresses {
    /// Where a device that just typed the proxy address goes for the
    /// certificate. Built from the bound API socket rather than from the
    /// requested address, so it cannot name a port nothing is listening on.
    pub fn ca_cert_url(&self) -> String {
        format!("http://{}/ca.crt", self.api)
    }
}

#[derive(Debug)]
pub enum Error {
    /// `0.0.0.0` / `::` — every address the host holds, when the user was asked
    /// to name one. Refused rather than honoured: the screen has to print an
    /// address anyway, and a mistaken confirm should cost one network.
    Unspecified,
    Bind {
        address: SocketAddr,
        source: io::Error,
    },
    Proxy(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unspecified => {
                write!(f, "reach binds one interface address, not every address this machine holds")
            }
            Error::Bind { address, source } => write!(f, "could not listen on {address}: {source}"),
            Error::Proxy(detail) => write!(f, "could not start the proxy: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

/// The addresses a device elsewhere on the network could actually route to.
/// Loopback answers only this machine and a link-local address needs a zone
/// index nobody is going to type, so neither is offered as a choice.
pub fn interfaces() -> Vec<Interface> {
    let mut found: Vec<Interface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| routable(interface.ip()))
        .map(|interface| Interface { address: interface.ip(), name: interface.name })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.address.cmp(&b.address)));
    found
}

fn routable(address: IpAddr) -> bool {
    !address.is_loopback() && !address.is_unspecified() && !link_local(address)
}

fn link_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => is_unicast_link_local(v6),
    }
}

// `Ipv6Addr::is_unicast_link_local` is still unstable, so fe80::/10 is matched
// here rather than waiting for it.
fn is_unicast_link_local(address: Ipv6Addr) -> bool {
    address.segments()[0] & 0xffc0 == 0xfe80
}

/// Owns whether reach is on and what it bound. One per running engine; the
/// control API holds it so a user can arm without restarting the core, and
/// [`crate::serve`] holds it so shutdown takes the extra listeners with it.
pub struct Controller {
    config: Config,
    store: Arc<Store>,
    armed: Mutex<Option<Armed>>,
}

struct Armed {
    addresses: Addresses,
    shutdown: watch::Sender<bool>,
    proxy_task: JoinHandle<()>,
    api_task: JoinHandle<()>,
}

impl Controller {
    pub fn new(config: Config, store: Arc<Store>) -> Self {
        Self { config, store, armed: Mutex::new(None) }
    }

    pub async fn addresses(&self) -> Option<Addresses> {
        self.armed.lock().await.as_ref().map(|armed| armed.addresses)
    }

    /// Bind the proxy and the control API on `address` as well, serving `api`
    /// there. Re-arming on a second address replaces the first rather than
    /// accumulating listeners, so what a user sees is what is reachable.
    ///
    /// An address this machine does not hold is refused by the bind itself, in
    /// the OS's own words — there is no list to check it against that the
    /// kernel does not already own.
    pub async fn arm(&self, address: IpAddr, api: Router) -> Result<Addresses, Error> {
        if address.is_unspecified() {
            return Err(Error::Unspecified);
        }
        let mut slot = self.armed.lock().await;
        if let Some(previous) = slot.take() {
            previous.stop().await;
        }

        let proxy_listener = bind(SocketAddr::new(address, self.config.proxy_addr.port())).await?;
        let api_listener = bind(SocketAddr::new(address, self.config.api_addr.port())).await?;
        let addresses = Addresses {
            proxy: proxy_listener.local_addr().map_err(|source| Error::Bind {
                address: SocketAddr::new(address, self.config.proxy_addr.port()),
                source,
            })?,
            api: api_listener.local_addr().map_err(|source| Error::Bind {
                address: SocketAddr::new(address, self.config.api_addr.port()),
                source,
            })?,
        };

        let (shutdown, stop) = watch::channel(false);
        let proxy_task = self.spawn_proxy(proxy_listener, stop.clone())?;
        let api_task = tokio::spawn(async move {
            let served = axum::serve(api_listener, api)
                .with_graceful_shutdown(wait_for_shutdown(stop))
                .await;
            if let Err(e) = served {
                error!("reachable control API stopped: {e}");
            }
        });

        info!("reachable at http://{} — control API on http://{}", addresses.proxy, addresses.api);
        *slot = Some(Armed { addresses, shutdown, proxy_task, api_task });
        Ok(addresses)
    }

    /// Stop answering anywhere but loopback. Idempotent: disarming something
    /// already off is the state the caller asked for.
    pub async fn disarm(&self) {
        if let Some(armed) = self.armed.lock().await.take() {
            armed.stop().await;
        }
    }

    // A second `Proxy` over the same store, so traffic a device sends arrives in
    // the same capture. The CA is re-read rather than shared because
    // `RcgenAuthority` consumes the issuer, and by the time anything can arm,
    // `serve` has already written both files.
    fn spawn_proxy(
        &self,
        listener: TcpListener,
        stop: watch::Receiver<bool>,
    ) -> Result<JoinHandle<()>, Error> {
        let material = ca::load_or_create(&self.config);
        let authority = RcgenAuthority::new(
            material.issuer,
            self.config.cert_cache_size,
            aws_lc_rs::default_provider(),
        );
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(authority)
            .with_http_connector(upstream_connector(&self.config.upstream_extra_root_pems))
            .with_http_handler(Recorder::new(Arc::clone(&self.store), self.config.body_cap))
            .with_graceful_shutdown(wait_for_shutdown(stop))
            .build()
            .map_err(|e| Error::Proxy(e.to_string()))?;
        Ok(tokio::spawn(async move {
            if let Err(e) = proxy.start().await {
                error!("reachable proxy stopped: {e}");
            }
        }))
    }
}

impl Armed {
    // The API task is aborted rather than awaited for the same reason
    // `RunningServer::shutdown` aborts its own: an open SSE `/stream` connection
    // never completes, so a graceful wait on it hangs forever.
    async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.proxy_task.await;
        self.api_task.abort();
        let _ = self.api_task.await;
    }
}

async fn bind(address: SocketAddr) -> Result<TcpListener, Error> {
    TcpListener::bind(address).await.map_err(|source| Error::Bind { address, source })
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn only_addresses_another_device_could_route_to_are_offered() {
        assert!(routable(v4(192, 168, 1, 20)));
        assert!(routable(v4(10, 0, 0, 5)));
        assert!(routable("2001:db8::1".parse().unwrap()));

        assert!(!routable(v4(127, 0, 0, 1)));
        assert!(!routable(v4(0, 0, 0, 0)));
        assert!(!routable(v4(169, 254, 3, 9)));
        assert!(!routable("::1".parse().unwrap()));
        assert!(!routable("fe80::1".parse().unwrap()));
    }

    #[test]
    fn enumerating_this_machine_offers_nothing_it_would_refuse_to_bind() {
        for interface in interfaces() {
            assert!(routable(interface.address), "offered {interface:?}");
            assert!(!interface.name.is_empty());
        }
    }

    #[test]
    fn the_certificate_url_names_the_port_that_answers() {
        let addresses = Addresses {
            proxy: "192.168.1.20:7420".parse().unwrap(),
            api: "192.168.1.20:7421".parse().unwrap(),
        };
        assert_eq!(addresses.ca_cert_url(), "http://192.168.1.20:7421/ca.crt");
    }

    #[test]
    fn an_ipv6_certificate_url_keeps_the_brackets_a_url_needs() {
        let addresses = Addresses {
            proxy: "[2001:db8::1]:7420".parse().unwrap(),
            api: "[2001:db8::1]:7421".parse().unwrap(),
        };
        assert_eq!(addresses.ca_cert_url(), "http://[2001:db8::1]:7421/ca.crt");
    }

    #[test]
    fn the_unspecified_address_is_refused_by_name() {
        let message = Error::Unspecified.to_string();
        assert!(message.contains("one interface address"), "{message}");
    }
}
