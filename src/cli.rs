use std::ffi::OsString;
use std::future::Future;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::Config;

pub const USAGE: &str =
    "usage: trawler [--ca-dir <dir>] [--proxy-port <port>] [--api-port <port>] [--lifeline]";

/// What the command line asked of the core. Every field left out keeps
/// [`Config::default`], so a bare `trawler` is the core it has always been.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreArgs {
    pub ca_dir: Option<PathBuf>,
    pub proxy_port: Option<u16>,
    pub api_port: Option<u16>,
    pub lifeline: bool,
}

impl CoreArgs {
    pub fn parse<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut parsed = Self::default();
        let mut args = args.into_iter().map(Into::into);
        while let Some(flag) = args.next() {
            let flag = flag.to_string_lossy().into_owned();
            let mut value = || {
                args.next()
                    .map(|v| v.to_string_lossy().into_owned())
                    .ok_or_else(|| format!("{flag} needs a value"))
            };
            match flag.as_str() {
                "--ca-dir" => parsed.ca_dir = Some(PathBuf::from(value()?)),
                "--proxy-port" => parsed.proxy_port = Some(port(&flag, &value()?)?),
                "--api-port" => parsed.api_port = Some(port(&flag, &value()?)?),
                "--lifeline" => parsed.lifeline = true,
                _ => return Err(format!("unknown argument {flag}")),
            }
        }
        Ok(parsed)
    }

    pub fn config(&self) -> Config {
        let mut config = Config::default();
        if let Some(ca_dir) = &self.ca_dir {
            config.ca_dir.clone_from(ca_dir);
        }
        if let Some(port) = self.proxy_port {
            config.proxy_addr.set_port(port);
        }
        if let Some(port) = self.api_port {
            config.api_addr.set_port(port);
        }
        config
    }
}

fn port(flag: &str, value: &str) -> Result<u16, String> {
    value.parse().map_err(|_| format!("{flag} takes a port number 0-65535, got {value:?}"))
}

/// The single stdout line a core prints once both servers are bound. It
/// carries the resolved addresses, so a supervisor that asked for port 0
/// learns the ports from the same line a fixed-port launch prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    pub proxy: SocketAddr,
    pub api: SocketAddr,
}

impl Ready {
    pub fn line(&self) -> String {
        serde_json::to_string(self).expect("two socket addresses serialize")
    }

    pub fn parse(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

/// The core's process entry, shared by the `trawler` binary and the app's
/// `--core` mode: parse, boot, print [`Ready`], run until Ctrl-C or — with
/// `--lifeline` — until stdin closes. Returns the process exit code.
pub fn main<I, S>(args: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("trawler=info".parse().expect("valid tracing directive")),
        )
        .try_init();

    let args = match CoreArgs::parse(args) {
        Ok(args) => args,
        Err(problem) => {
            error!("{problem}\n{USAGE}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            error!("fatal: cannot start the async runtime: {err}");
            return 1;
        }
    };
    let stop = stop_signal(args.lifeline);
    match runtime.block_on(serve_until(args.config(), stop)) {
        Ok(()) => 0,
        Err(err) => {
            error!("fatal: {err}");
            1
        }
    }
}

async fn serve_until(
    config: Config,
    stop: impl Future<Output = ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = crate::serve(config).await?;
    crate::announce(&server).await;
    let ready = Ready { proxy: server.proxy_addr, api: server.api_addr };
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{}", ready.line())?;
    stdout.flush()?;
    drop(stdout);

    stop.await;
    info!("shutting down");
    server.shutdown().await;
    Ok(())
}

fn stop_signal(lifeline: bool) -> impl Future<Output = ()> {
    let stdin_closed = lifeline.then(stdin_eof);
    async move {
        let interrupted = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        match stdin_closed {
            Some(closed) => tokio::select! {
                () = interrupted => {}
                _ = closed => info!("stdin closed - the supervising app is gone"),
            },
            None => interrupted.await,
        }
    }
}

// A plain thread, not tokio's stdin: its blocking read would hold the
// runtime's shutdown open until the parent wrote another byte.
fn stdin_eof() -> oneshot::Receiver<()> {
    let (closed_tx, closed_rx) = oneshot::channel();
    std::thread::spawn(move || {
        let mut sink = [0u8; 256];
        let mut stdin = std::io::stdin();
        while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
        let _ = closed_tx.send(());
    });
    closed_rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flags_is_the_default_core() {
        let args = CoreArgs::parse(Vec::<String>::new()).unwrap();
        assert_eq!(args, CoreArgs::default());
        let config = args.config();
        let default = Config::default();
        assert_eq!(config.proxy_addr, default.proxy_addr);
        assert_eq!(config.api_addr, default.api_addr);
        assert_eq!(config.ca_dir, default.ca_dir);
    }

    #[test]
    fn every_flag_lands_in_the_config_it_names() {
        let args = CoreArgs::parse([
            "--ca-dir",
            "/tmp/trawler-ca",
            "--proxy-port",
            "0",
            "--api-port",
            "9000",
            "--lifeline",
        ])
        .unwrap();
        assert!(args.lifeline);
        let config = args.config();
        assert_eq!(config.ca_dir, PathBuf::from("/tmp/trawler-ca"));
        assert_eq!(config.proxy_addr.port(), 0);
        assert_eq!(config.api_addr.port(), 9000);
        assert!(config.proxy_addr.ip().is_loopback());
        assert!(config.api_addr.ip().is_loopback());
    }

    #[test]
    fn a_port_out_of_range_is_named() {
        let problem = CoreArgs::parse(["--proxy-port", "70000"]).unwrap_err();
        assert_eq!(problem, "--proxy-port takes a port number 0-65535, got \"70000\"");
    }

    #[test]
    fn a_flag_missing_its_value_is_named() {
        assert_eq!(CoreArgs::parse(["--api-port"]).unwrap_err(), "--api-port needs a value");
    }

    #[test]
    fn an_unknown_flag_is_refused() {
        assert_eq!(CoreArgs::parse(["--core"]).unwrap_err(), "unknown argument --core");
    }

    #[test]
    fn the_ready_line_is_one_json_object_with_both_addresses() {
        let ready = Ready {
            proxy: "127.0.0.1:7420".parse().unwrap(),
            api: "127.0.0.1:7421".parse().unwrap(),
        };
        assert_eq!(ready.line(), r#"{"proxy":"127.0.0.1:7420","api":"127.0.0.1:7421"}"#);
        assert!(!ready.line().contains('\n'));
    }

    #[test]
    fn the_ready_line_parses_back_with_its_newline() {
        let ready = Ready {
            proxy: "127.0.0.1:50001".parse().unwrap(),
            api: "127.0.0.1:50002".parse().unwrap(),
        };
        assert_eq!(Ready::parse(&format!("{}\n", ready.line())), Some(ready));
    }

    #[test]
    fn a_log_line_is_not_a_ready_line() {
        assert_eq!(Ready::parse("INFO trawler: proxy listening on http://127.0.0.1:7420"), None);
    }
}
