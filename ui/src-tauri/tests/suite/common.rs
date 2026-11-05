use std::path::{Path, PathBuf};
use std::time::Duration;

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
