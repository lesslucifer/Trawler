use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::Duration;

use trawler_ui_lib::supervisor::Launch;

pub const PATIENCE: Duration = Duration::from_secs(20);

const PROGRAM: &str = env!("CARGO_BIN_EXE_trawler-ui");

static SCANNED: Once = Once::new();

// macOS scans a freshly linked binary before its first exec, which takes as
// long as PATIENCE for this one, and `cargo test` relinks it before every run.
// Paying that once with an argument the core refuses at once keeps PATIENCE
// measuring the core's own start.
fn scan_once() {
    SCANNED.call_once(|| {
        let refused = Command::new(PROGRAM)
            .args([trawler_ui_lib::CORE_FLAG, "--scan-before-the-clock"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("the app binary runs");
        assert_eq!(refused.code(), Some(2), "the core refuses an unknown flag");
    });
}

pub fn launch(ca_dir: &Path, proxy_port: u16, api_port: u16) -> Launch {
    scan_once();
    Launch {
        program: PathBuf::from(PROGRAM),
        leading_args: vec![trawler_ui_lib::CORE_FLAG.into()],
        ca_dir: ca_dir.to_path_buf(),
        proxy_port,
        api_port,
        ready_timeout: PATIENCE,
        grace: Duration::from_secs(5),
    }
}
