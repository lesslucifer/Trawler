use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
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

#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    use std::process::Stdio;

    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    let listing = Command::new("tasklist")
        .args(["/NH", "/FO", "CSV", "/FI", &format!("PID eq {pid}")])
        .output()
        .expect("tasklist runs");
    String::from_utf8_lossy(&listing.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(unix)]
pub fn kill_hard(pid: u32) -> bool {
    Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success()
}

#[cfg(windows)]
pub fn kill_hard(pid: u32) -> bool {
    Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).status().unwrap().success()
}

#[cfg(unix)]
pub const HARD_KILLED: trawler_ui_lib::supervisor::CoreState =
    trawler_ui_lib::supervisor::CoreState::Exited { code: None, signal: Some(9) };

#[cfg(windows)]
pub const HARD_KILLED: trawler_ui_lib::supervisor::CoreState =
    trawler_ui_lib::supervisor::CoreState::Exited { code: Some(1), signal: None };

#[cfg(not(windows))]
pub const LOCAL_URL: &str = "tauri://localhost";

#[cfg(windows)]
pub const LOCAL_URL: &str = "http://tauri.localhost";

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
