use std::path::{Path, PathBuf};
use std::time::Duration;

use trawler_ui_lib::supervisor::Launch;

pub const PATIENCE: Duration = Duration::from_secs(20);

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
