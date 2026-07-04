use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

const MEGABYTE: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Config {
    pub proxy_addr: SocketAddr,
    pub api_addr: SocketAddr,
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
            proxy_addr: SocketAddr::new(loopback, 3000),
            api_addr: SocketAddr::new(loopback, 3001),
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
        assert_eq!(c.proxy_addr.port(), 3000);
        assert_eq!(c.api_addr.port(), 3001);
        assert_eq!(c.body_cap, 10 * MEGABYTE);
        assert_eq!(c.ring_max_bytes, 256 * MEGABYTE);
        assert_eq!(c.broadcast_capacity, 1024);
        assert_eq!(c.cert_cache_size, 1000);
        assert_eq!(c.ca_dir, PathBuf::from("ca"));
    }

    #[test]
    fn ca_paths_derive_from_dir() {
        let c = Config { ca_dir: PathBuf::from("/tmp/foo"), ..Default::default() };
        assert_eq!(c.ca_cert_path(), PathBuf::from("/tmp/foo/trawler-ca.pem"));
        assert_eq!(c.ca_key_path(), PathBuf::from("/tmp/foo/trawler-ca.key"));
    }
}
