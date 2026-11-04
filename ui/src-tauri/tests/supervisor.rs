use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use trawler_ui_lib::supervisor::{CoreEvent, CoreState, LogEntry, Logged, StartError, Supervisor};

mod common;
use common::{launch, PATIENCE};

const ROLE_VAR: &str = "TRAWLER_SUPERVISOR_ROLE_CA";
// libtest prints the test's name with no newline before the role speaks, so the
// report is found inside a line, never at its start.
const CORE_REPORT: &str = "supervised core: ";

fn watched() -> (Arc<dyn Fn(CoreEvent) + Send + Sync>, mpsc::Receiver<CoreEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    (Arc::new(move |event| drop(tx.lock().expect("an observer lock").send(event))), rx)
}

fn running(state: &CoreState) -> (u32, SocketAddr, SocketAddr) {
    match state {
        CoreState::Running { pid, proxy, api, .. } => (*pid, *proxy, *api),
        other => panic!("expected running, got {other:?}"),
    }
}

fn flows_status(api: SocketAddr) -> String {
    let mut stream = TcpStream::connect(api).expect("the API accepts");
    write!(stream, "GET /flows HTTP/1.1\r\nHost: {api}\r\nConnection: close\r\n\r\n")
        .expect("a request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("a response");
    response.lines().next().unwrap_or_default().to_string()
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

fn bindable(addr: SocketAddr) -> bool {
    TcpListener::bind(addr).is_ok()
}

#[test]
fn a_supervised_core_survives_a_kill_restarts_on_its_ports_and_leaves_on_stop() {
    let ca = tempfile::tempdir().unwrap();
    let (observer, events) = watched();
    let supervisor = Supervisor::start(launch(ca.path(), 0, 0), observer).unwrap();

    let (pid, proxy, api) = running(&supervisor.snapshot());
    assert!(proxy.port() != 0 && api.port() != 0);
    assert!(flows_status(api).contains("200"), "the API answers at the ready line's address");
    assert!(ca.path().join("trawler-ca.pem").is_file(), "the core minted its CA in --ca-dir");

    assert!(Command::new("kill").args(["-9", &pid.to_string()]).status().unwrap().success());
    eventually("the kill is observed", || {
        supervisor.snapshot() == CoreState::Exited { code: None, signal: Some(9) }
    });

    let (restarted_pid, restarted_proxy, restarted_api) = running(&supervisor.restart().unwrap());
    assert_ne!(restarted_pid, pid);
    assert_eq!((restarted_proxy, restarted_api), (proxy, api), "restart keeps the ports");
    assert!(flows_status(api).contains("200"));

    supervisor.stop();
    assert!(matches!(supervisor.snapshot(), CoreState::Exited { code: Some(0), .. }));
    assert!(!alive(restarted_pid));
    assert!(bindable(proxy) && bindable(api), "both ports are free after stop");

    let seen: Vec<CoreEvent> = events.try_iter().collect();
    let states: Vec<&str> = seen
        .iter()
        .filter_map(|event| match event {
            CoreEvent::State(CoreState::Starting) => Some("starting"),
            CoreEvent::State(CoreState::Running { .. }) => Some("running"),
            CoreEvent::State(CoreState::Exited { .. }) => Some("exited"),
            _ => None,
        })
        .collect();
    assert_eq!(states, ["starting", "running", "exited", "starting", "running", "exited"]);
    assert!(seen.iter().any(
        |e| matches!(e, CoreEvent::Log(Logged { entry: LogEntry::Line { text }, .. }) if text.contains("listening"))
    ));
    assert!(supervisor.log().iter().any(
        |logged| matches!(&logged.entry, LogEntry::Line { text } if text.contains("listening"))
    ));
    let starts: Vec<u32> = supervisor
        .log()
        .iter()
        .filter_map(|logged| match logged.entry {
            LogEntry::Start { pid, .. } => Some(pid),
            LogEntry::Line { .. } => None,
        })
        .collect();
    assert_eq!(starts, [pid, restarted_pid], "each core's lines begin under its own start mark");
}

#[test]
fn restarts_asked_for_at_once_run_one_after_the_other() {
    let ca = tempfile::tempdir().unwrap();
    let (observer, _events) = watched();
    let supervisor = Arc::new(Supervisor::start(launch(ca.path(), 0, 0), observer).unwrap());
    let (_, proxy, api) = running(&supervisor.snapshot());

    let restarts: Vec<_> = (0..2)
        .map(|_| {
            let supervisor = Arc::clone(&supervisor);
            std::thread::spawn(move || supervisor.restart())
        })
        .collect();
    let pids: Vec<u32> = restarts
        .into_iter()
        .map(|restart| {
            let state = restart.join().unwrap().expect("each restart succeeds");
            let (pid, restarted_proxy, restarted_api) = running(&state);
            assert_eq!((restarted_proxy, restarted_api), (proxy, api));
            pid
        })
        .collect();

    let (current, _, _) = running(&supervisor.snapshot());
    assert!(pids.contains(&current));
    let replaced = pids.into_iter().find(|&pid| pid != current).expect("two cores were started");
    eventually("the replaced core is gone", || !alive(replaced));
    supervisor.stop();
}

#[test]
fn a_taken_port_is_named_before_any_window() {
    let ca = tempfile::tempdir().unwrap();
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let (observer, _events) = watched();
    let refused = Supervisor::start(launch(ca.path(), port, 0), observer).err();
    assert_eq!(refused, Some(StartError::Conflict { port }));
}

#[test]
fn a_killed_supervisor_leaves_no_core_behind() {
    let ca = tempfile::tempdir().unwrap();
    let mut role = Command::new(std::env::current_exe().unwrap())
        .args(["supervising_role", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ROLE_VAR, ca.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(role.stdout.take().unwrap()).lines();
    let report = lines
        .find_map(|line| {
            line.unwrap().split_once(CORE_REPORT).map(|(_, report)| report.to_string())
        })
        .expect("the role reports its core");
    let mut fields = report.split(' ');
    let pid: u32 = fields.next().unwrap().parse().unwrap();
    let api: SocketAddr = fields.next().unwrap().parse().unwrap();
    assert!(alive(pid));

    role.kill().unwrap();
    role.wait().unwrap();
    eventually("the orphaned core exits", || !alive(pid));
    assert!(bindable(api));
}

#[test]
#[ignore = "the supervising process a_killed_supervisor_leaves_no_core_behind re-executes"]
fn supervising_role() {
    let Some(ca) = std::env::var_os(ROLE_VAR) else { return };
    let (observer, _events) = watched();
    let supervisor = Supervisor::start(launch(Path::new(&ca), 0, 0), observer).unwrap();
    let (pid, _, api) = running(&supervisor.snapshot());
    println!("{CORE_REPORT}{pid} {api}");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
