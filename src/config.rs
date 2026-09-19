use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

const MEGABYTE: usize = 1024 * 1024;

/// How far the two servers listen. `Local` binds loopback alone — what Trawler
/// has always done, and what it still does until a user decides otherwise.
/// `Interface` keeps that bind and adds a second pair on one address a device
/// elsewhere on the network can route to. Arming it is an authorization act,
/// never a setting: see DEC-55.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reach {
    #[default]
    Local,
    Interface(IpAddr),
}

impl Reach {
    pub fn address(self) -> Option<IpAddr> {
        match self {
            Reach::Local => None,
            Reach::Interface(address) => Some(address),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The local bind — loopback in every shipped configuration. `reach` adds
    /// to this pair; it never replaces it, so the app's own client keeps the
    /// address it started on whatever the user arms.
    pub proxy_addr: SocketAddr,
    pub api_addr: SocketAddr,
    /// Where a second device may reach these servers. `Local` by default;
    /// a headless core can boot armed, a UI arms it through the control API.
    pub reach: Reach,
    pub ca_dir: PathBuf,
    pub body_cap: usize,
    pub ring_max_bytes: usize,
    pub broadcast_capacity: usize,
    pub cert_cache_size: u64,
    /// Extra CA certificates (PEM) the proxy-to-origin connector trusts, on top
    /// of the webpki roots. Empty by default; lets callers reach an origin whose
    /// cert chains to a private/test CA.
    pub upstream_extra_root_pems: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        Self {
            proxy_addr: SocketAddr::new(loopback, 7420),
            api_addr: SocketAddr::new(loopback, 7421),
            reach: Reach::Local,
            ca_dir: PathBuf::from("ca"),
            body_cap: 10 * MEGABYTE,
            ring_max_bytes: 256 * MEGABYTE,
            broadcast_capacity: 1024,
            cert_cache_size: 1_000,
            upstream_extra_root_pems: Vec::new(),
        }
    }
}

impl Config {
    pub fn ca_cert_path(&self) -> PathBuf {
        self.ca_dir.join("trawler-ca.pem")
    }

    pub fn ca_key_path(&self) -> PathBuf {
        self.ca_dir.join("trawler-ca.key")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!(c.proxy_addr.port(), 7420);
        assert_eq!(c.api_addr.port(), 7421);
        assert_eq!(c.body_cap, 10 * MEGABYTE);
        assert_eq!(c.ring_max_bytes, 256 * MEGABYTE);
        assert_eq!(c.broadcast_capacity, 1024);
        assert_eq!(c.cert_cache_size, 1000);
        assert_eq!(c.ca_dir, PathBuf::from("ca"));
    }

    #[test]
    fn nothing_is_reachable_until_something_says_so() {
        let c = Config::default();
        assert!(c.proxy_addr.ip().is_loopback());
        assert!(c.api_addr.ip().is_loopback());
        assert_eq!(c.reach, Reach::Local);
        assert_eq!(c.reach.address(), None);
    }

    #[test]
    fn an_armed_config_names_the_address_without_moving_the_local_bind() {
        let address = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
        let c = Config { reach: Reach::Interface(address), ..Default::default() };
        assert_eq!(c.reach.address(), Some(address));
        assert!(c.proxy_addr.ip().is_loopback());
    }

    #[test]
    fn ca_paths_derive_from_dir() {
        let c = Config { ca_dir: PathBuf::from("/tmp/foo"), ..Default::default() };
        assert_eq!(c.ca_cert_path(), PathBuf::from("/tmp/foo/trawler-ca.pem"));
        assert_eq!(c.ca_key_path(), PathBuf::from("/tmp/foo/trawler-ca.key"));
    }
}
