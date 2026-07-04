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
