use std::net::TcpListener;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use trawler::cli::Ready;

const DEADLINE: Duration = Duration::from_secs(10);

fn spawn_core(args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_trawler"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the trawler binary spawns")
}

#[tokio::test]
async fn the_core_reports_where_it_listens_and_leaves_when_its_stdin_closes() {
    let ca_dir = tempfile::tempdir().expect("ca tempdir");
    let ca_dir_arg = ca_dir.path().to_str().expect("utf-8 tempdir");
    let mut core =
        spawn_core(&["--ca-dir", ca_dir_arg, "--proxy-port", "0", "--api-port", "0", "--lifeline"]);

    let mut stdout = BufReader::new(core.stdout.take().expect("piped stdout")).lines();
    let first_line = tokio::time::timeout(DEADLINE, stdout.next_line())
        .await
        .expect("a ready line before the deadline")
        .expect("stdout readable")
        .expect("stdout open until ready");
    let ready = Ready::parse(&first_line)
        .unwrap_or_else(|| panic!("stdout's first line is not a ready line: {first_line:?}"));
    assert_ne!(ready.proxy.port(), 0);
    assert_ne!(ready.api.port(), 0);
    assert!(ca_dir.path().join("trawler-ca.pem").exists(), "the CA lands in --ca-dir");

    let flows = reqwest::get(format!("http://{}/flows", ready.api)).await.expect("GET /flows");
    assert!(flows.status().is_success(), "GET /flows answered {}", flows.status());

    drop(core.stdin.take());
    let status = tokio::time::timeout(DEADLINE, core.wait())
        .await
        .expect("the core exits once its stdin closes")
        .expect("core exit status");
    assert!(status.success(), "the core exited {status}");
    TcpListener::bind(ready.proxy).expect("the proxy port is free again");
    TcpListener::bind(ready.api).expect("the API port is free again");
}

#[tokio::test]
async fn an_unknown_flag_exits_with_usage_before_binding_anything() {
    let core = spawn_core(&["--no-such-flag"]);
    let out = tokio::time::timeout(DEADLINE, core.wait_with_output())
        .await
        .expect("the core refuses before the deadline")
        .expect("core output");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown argument --no-such-flag"), "stderr: {stderr}");
    assert!(stderr.contains("usage: trawler"), "stderr: {stderr}");
    assert!(out.stdout.is_empty(), "nothing on stdout without a bound core");
}
