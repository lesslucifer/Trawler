use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use trawler_ui_lib::supervisor::Launch;

// macOS scans a freshly linked binary before it first runs: 16-22s for this
// one, and `cargo test` relinks it before a run. The core itself is ready in
// milliseconds, so the patience is sized for the scan, not for the core.
pub const PATIENCE: Duration = Duration::from_secs(60);

pub fn launch(ca_dir: &Path, proxy_port: u16, api_port: u16) -> Launch {
    Launch {
        program: PathBuf::from(env!("CARGO_BIN_EXE_trawler-ui")),
        leading_args: vec![trawler_ui_lib::CORE_FLAG.into()],
        ca_dir: ca_dir.to_path_buf(),
        proxy_port,
        api_port,
        ready_timeout: PATIENCE,
        grace: Duration::from_secs(5),
    }
}

pub fn flows_status(api: SocketAddr) -> String {
    let mut stream = TcpStream::connect(api).expect("the API accepts");
    write!(stream, "GET /flows HTTP/1.1\r\nHost: {api}\r\nConnection: close\r\n\r\n")
        .expect("a request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("a response");
    response.lines().next().unwrap_or_default().to_string()
}

pub fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

pub fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

pub fn bindable(addr: SocketAddr) -> bool {
    TcpListener::bind(addr).is_ok()
}
