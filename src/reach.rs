//! Reach — letting a device that is not this machine talk to Trawler.
//!
//! The loopback pair of listeners is never touched. Arming *adds* a second
//! proxy and a second control API on one interface address, so the app's own
//! client keeps the address it started on and disarming can never cost it its
//! connection. Nothing here is persisted: the listeners live in the process, so
//! an opt-in cannot outlive the session that confirmed it (DEC-55).

use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, UdpSocket};
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

use crate::ca::{self, CaPems};
use crate::config::Config;
use crate::store::Store;
use crate::{upstream_connector, wait_for_shutdown, Recorder};

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
    Ca(String),
    Proxy(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unspecified => {
                write!(f, "reach binds one interface address, not every address this machine holds")
            }
            Error::Bind { address, source } => write!(f, "could not listen on {address}: {source}"),
            Error::Ca(detail) => write!(f, "could not build a certificate authority: {detail}"),
            Error::Proxy(detail) => write!(f, "could not start the proxy: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

/// The addresses a device elsewhere on the network could actually route to,
/// best first. Loopback answers only this machine and a link-local address
/// needs a zone index nobody is going to type, so neither is offered as a
/// choice. What is offered is ordered for the person who has to type one of
/// these into a phone by hand, because the kernel's order answers a different
/// question. The kernel's device *names* are left as they are: what a user
/// calls `en1` is something only the host can say and only the shell may ask
/// (DEC-13), so the screen renames them where it can.
pub fn interfaces() -> Vec<Interface> {
    let routed = routed_from();
    let mut found: Vec<Interface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| routable(interface.ip()))
        .map(|interface| Interface { address: interface.ip(), name: interface.name })
        .collect();
    order(&mut found, routed);
    found
}

fn order(found: &mut [Interface], routed: Option<IpAddr>) {
    found.sort_by(|a, b| {
        precedence(a.address, routed)
            .cmp(&precedence(b.address, routed))
            .then(a.name.cmp(&b.name))
            .then(a.address.cmp(&b.address))
    });
}

/// Lower is offered first: the address this host actually sends from, then any
/// other IPv4 because a phone's proxy field takes one, then IPv6.
///
/// IPv6 is ranked by nothing finer, and that is deliberate. The two addresses a
/// dual-stack host holds are a stable one and a rotating privacy address, and
/// which is which is a flag `if_addrs` does not carry and this crate may not
/// shell out to read (DEC-13). Ranking by the routing table would answer with
/// the rotating one, since that is exactly what a privacy address is for - so
/// an IPv6-only host is offered a stable order it did not choose rather than a
/// preference this crate cannot honestly compute.
fn precedence(address: IpAddr, routed: Option<IpAddr>) -> u8 {
    match address {
        IpAddr::V4(_) if Some(address) == routed => 0,
        IpAddr::V4(_) => 1,
        IpAddr::V6(_) => 2,
    }
}

/// Which address the routing table would send from, asked without sending
/// anything and without asking the OS for a tool: a connected UDP socket picks
/// the route and binds a source, and the destination is TEST-NET-3, which
/// routes nowhere. No route, no answer - and then the rest of the ordering
/// decides.
fn routed_from() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("203.0.113.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip())
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
    ca: CaPems,
    armed: Mutex<Option<Armed>>,
}

struct Armed {
    addresses: Addresses,
    shutdown: watch::Sender<bool>,
    proxy_task: JoinHandle<()>,
    api_task: JoinHandle<()>,
}

impl Controller {
    pub fn new(config: Config, store: Arc<Store>, ca: CaPems) -> Self {
        Self { config, store, ca, armed: Mutex::new(None) }
    }

    pub async fn addresses(&self) -> Option<Addresses> {
        self.armed.lock().await.as_ref().map(|armed| armed.addresses)
    }

    /// Bind the proxy and the control API on `address` as well, serving `api`
    /// there.
    ///
    /// Arming the address already armed changes nothing and reports what is
    /// already listening. Arming a different one brings the new pair up
    /// *before* the old pair goes down, so a refusal cannot cost a user the
    /// exposure they confirmed — a device routing through it would lose its
    /// route to a request that bound nothing.
    ///
    /// An address this machine does not hold is refused by the bind itself, in
    /// the OS's own words — there is no list to check it against that the
    /// kernel does not already own.
    pub async fn arm(&self, address: IpAddr, api: Router) -> Result<Addresses, Error> {
        if address.is_unspecified() {
            return Err(Error::Unspecified);
        }
        // The critical section is the swap, and the bind that must not race a
        // second arm — never the outgoing session's drain, which waits on a
        // device's open exchange and would hold every other reader of the state
        // behind it, including the one trying to revoke the exposure.
        let (addresses, previous) = {
            let mut slot = self.armed.lock().await;
            if let Some(current) =
                slot.as_ref().filter(|armed| armed.addresses.proxy.ip() == address)
            {
                return Ok(current.addresses);
            }
            let next = self.listen_on(address, api).await?;
            (next.addresses, slot.replace(next))
        };

        if let Some(previous) = previous {
            previous.stop().await;
        }
        info!("reachable at http://{} — control API on http://{}", addresses.proxy, addresses.api);
        Ok(addresses)
    }

    // Everything that can fail, done before anything that cannot be undone.
    async fn listen_on(&self, address: IpAddr, api: Router) -> Result<Armed, Error> {
        let (proxy_listener, proxy) =
            bind(SocketAddr::new(address, self.config.proxy_addr.port())).await?;
        let (api_listener, api_addr) =
            bind(SocketAddr::new(address, self.config.api_addr.port())).await?;

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

        Ok(Armed { addresses: Addresses { proxy, api: api_addr }, shutdown, proxy_task, api_task })
    }

    /// Stop answering anywhere but loopback. Idempotent: disarming something
    /// already off is the state the caller asked for.
    pub async fn disarm(&self) {
        // Taken in a statement of its own so the guard is gone before the
        // drain: an `if let` on the lock would hold it for the whole body.
        let armed = self.armed.lock().await.take();
        if let Some(armed) = armed {
            armed.stop().await;
        }
    }

    // A second `Proxy` over the same store, so traffic a device sends arrives in
    // the same capture. It needs an issuer of its own because `RcgenAuthority`
    // consumes the one it is given — rebuilt from the PEMs held since boot, so
    // arming a live process never reads the disk on a runtime thread.
    fn spawn_proxy(
        &self,
        listener: TcpListener,
        stop: watch::Receiver<bool>,
    ) -> Result<JoinHandle<()>, Error> {
        let issuer = ca::issuer(&self.ca).map_err(Error::Ca)?;
        let authority =
            RcgenAuthority::new(issuer, self.config.cert_cache_size, aws_lc_rs::default_provider());
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

/// The listener and the socket it actually got — resolved here so an ephemeral
/// port is never reported as the `:0` that was asked for.
async fn bind(address: SocketAddr) -> Result<(TcpListener, SocketAddr), Error> {
    let listener =
        TcpListener::bind(address).await.map_err(|source| Error::Bind { address, source })?;
    let bound = listener.local_addr().map_err(|source| Error::Bind { address, source })?;
    Ok((listener, bound))
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

    fn on(name: &str, address: &str) -> Interface {
        Interface { name: name.to_owned(), address: address.parse().expect("address") }
    }

    fn ordered(found: &[Interface], routed: Option<&str>) -> Vec<String> {
        let mut found = found.to_vec();
        order(&mut found, routed.map(|address| address.parse().expect("address")));
        found.into_iter().map(|interface| interface.address.to_string()).collect()
    }

    // The failure this ordering exists to prevent: the screen offered the first
    // address the kernel listed, which was a rotating IPv6 on an interface named
    // en1 - four choices deep, and wrong in a phone's proxy field.
    #[test]
    fn the_address_this_host_sends_from_is_offered_first() {
        let found = [
            on("Wi-Fi", "2001:db8::c0d"),
            on("Thunderbolt Bridge", "10.9.9.1"),
            on("Wi-Fi", "192.168.0.105"),
        ];
        assert_eq!(
            ordered(&found, Some("192.168.0.105")),
            ["192.168.0.105", "10.9.9.1", "2001:db8::c0d"]
        );
    }

    #[test]
    fn a_host_with_no_route_still_offers_its_ipv4_before_its_ipv6() {
        let found = [on("Wi-Fi", "2001:db8::1"), on("Wi-Fi", "192.168.0.105")];

        assert_eq!(ordered(&found, None), ["192.168.0.105", "2001:db8::1"]);
    }

    // Two IPv6 addresses cannot be told apart from inside this crate - the flag
    // that says which one rotates is the host's to print, and DEC-13 keeps this
    // crate from asking it - so what is asserted is the order they do have:
    // stable, and below every IPv4.
    #[test]
    fn ipv6_is_offered_below_every_ipv4_and_in_a_stable_order() {
        let found = [
            on("Wi-Fi", "2001:db8::c0d"),
            on("Wi-Fi", "2001:db8::1"),
            on("Thunderbolt Bridge", "10.9.9.1"),
        ];

        assert_eq!(ordered(&found, None), ["10.9.9.1", "2001:db8::1", "2001:db8::c0d"]);
        assert_eq!(ordered(&found, None), ordered(&found, None));
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

    // The refusal matters because the PEMs are held since boot: a controller
    // built without real ones (the shared API-test router) must fail the arm
    // rather than take the process down with it.
    #[tokio::test]
    async fn a_controller_without_a_usable_ca_refuses_to_listen_rather_than_panicking() {
        let controller = Controller::new(
            Config {
                proxy_addr: "127.0.0.1:0".parse().expect("addr"),
                api_addr: "127.0.0.1:0".parse().expect("addr"),
                ..Config::default()
            },
            Arc::new(Store::new(1024, 8)),
            CaPems { cert: "not a certificate".into(), key: String::new() },
        );

        let refusal = controller
            .arm("127.0.0.1".parse().expect("addr"), Router::new())
            .await
            .expect_err("a controller with no CA cannot serve HTTPS");

        assert!(matches!(refusal, Error::Ca(_)), "{refusal}");
        assert_eq!(controller.addresses().await, None);
    }

    #[test]
    fn the_unspecified_address_is_refused_by_name() {
        let message = Error::Unspecified.to_string();
        assert!(message.contains("one interface address"), "{message}");
    }
}
