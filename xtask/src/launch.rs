use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const THRESHOLD: Duration = Duration::from_secs(2);
pub const ACCEPT_SLOW: &str = "TRAWLER_ACCEPT_SLOW_LAUNCH";

#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    Fast,
    Slow,
}

pub fn slow_accepted() -> bool {
    std::env::var_os(ACCEPT_SLOW).is_some_and(|value| value == "1")
}

fn cache_file_from(xdg_cache: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let root = match (xdg_cache.filter(|dir| !dir.is_empty()), home) {
        (Some(xdg), _) => PathBuf::from(xdg),
        (None, Some(home)) => PathBuf::from(home).join(".cache"),
        (None, None) => return None,
    };
    Some(root.join("trawler").join("launch-fast"))
}

/// Where a fast verdict lives: per machine, outside every checkout.
pub fn cache_file() -> Option<PathBuf> {
    cache_file_from(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

fn boot_key() -> String {
    let linux = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok();
    let bsd = || {
        Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    linux.or_else(bsd).map_or_else(|| "unknown-boot".to_string(), |key| key.trim().to_string())
}

fn cached_fast(file: &Path, boot: &str) -> bool {
    std::fs::read_to_string(file).is_ok_and(|stored| stored.trim() == boot)
}

fn store_fast(file: &Path, boot: &str) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(file, boot).map_err(|e| format!("write {}: {e}", file.display()))
}

fn probe_source(nonce: &str) -> String {
    format!("fn main() {{\n    std::hint::black_box(\"{nonce}\");\n}}\n")
}

fn nonce() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

fn rustc() -> OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"))
}

fn link_probe(dir: &Path) -> Result<PathBuf, String> {
    let source = dir.join("probe.rs");
    let program = dir.join("probe");
    std::fs::write(&source, probe_source(&nonce()))
        .map_err(|e| format!("write {}: {e}", source.display()))?;
    let out = Command::new(rustc())
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .output()
        .map_err(|e| format!("failed to spawn rustc for the launch probe: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "the launch probe failed to link: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(program)
}

// The assessment can block inside the spawn itself, so the deadline is kept on
// another thread: a slow launch is reported at the threshold, never awaited.
fn first_launch(mut program: Command, threshold: Duration) -> Result<Launch, String> {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let ran = program
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(drop)
            .map_err(|e| format!("failed to launch the probe: {e}"));
        let _ = done.send(ran);
    });
    match finished.recv_timeout(threshold) {
        Ok(ran) => ran.map(|()| Launch::Fast),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(Launch::Slow),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("the launch probe vanished".to_string()),
    }
}

fn probe() -> Result<Launch, String> {
    let dir = tempfile::Builder::new()
        .prefix("trawler-launch-probe")
        .tempdir()
        .map_err(|e| format!("create the launch probe dir: {e}"))?;
    let program = link_probe(dir.path())?;
    first_launch(Command::new(program), THRESHOLD)
}

/// How fast this machine first-launches a freshly linked binary: a fast verdict
/// is cached for the boot when the cache dir is writable, a slow one is measured
/// again on every call.
pub fn machine() -> Result<Launch, String> {
    let file = cache_file();
    let boot = boot_key();
    if file.as_deref().is_some_and(|file| cached_fast(file, &boot)) {
        return Ok(Launch::Fast);
    }
    let launch = probe()?;
    if let (Launch::Fast, Some(file)) = (&launch, file) {
        let _ = store_fast(&file, &boot);
    }
    Ok(launch)
}

#[cfg(test)]
mod tests {
    use super::{
        cache_file_from, cached_fast, first_launch, probe_source, store_fast, Launch, THRESHOLD,
    };
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn the_cache_follows_xdg_then_home() {
        let xdg = cache_file_from(Some(OsString::from("/x")), Some(OsString::from("/h")));
        assert_eq!(xdg, Some(PathBuf::from("/x/trawler/launch-fast")));
        let home = cache_file_from(None, Some(OsString::from("/h")));
        assert_eq!(home, Some(PathBuf::from("/h/.cache/trawler/launch-fast")));
        let empty_xdg = cache_file_from(Some(OsString::new()), Some(OsString::from("/h")));
        assert_eq!(empty_xdg, home);
        assert_eq!(cache_file_from(None, None), None);
    }

    #[test]
    fn a_stored_boot_is_a_hit_and_another_boot_a_miss() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("trawler").join("launch-fast");
        assert!(!cached_fast(&file, "boot-1"));
        store_fast(&file, "boot-1").unwrap();
        assert!(cached_fast(&file, "boot-1"));
        assert!(!cached_fast(&file, "boot-2"));
    }

    #[test]
    fn each_probe_source_is_a_distinct_program() {
        assert_ne!(probe_source("a"), probe_source("b"));
        assert!(probe_source("n0nce").contains("n0nce"));
    }

    #[test]
    fn a_program_that_exits_at_once_is_fast() {
        assert_eq!(first_launch(Command::new("true"), THRESHOLD), Ok(Launch::Fast));
    }

    #[test]
    fn a_launch_past_the_threshold_is_slow_without_waiting_for_it() {
        let mut sleeper = Command::new("sleep");
        sleeper.arg("5");
        let started = Instant::now();
        let launch = first_launch(sleeper, Duration::from_millis(200));
        assert_eq!(launch, Ok(Launch::Slow));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn a_program_that_cannot_start_is_an_error() {
        assert!(first_launch(Command::new("/nonexistent/probe"), THRESHOLD).is_err());
    }
}
