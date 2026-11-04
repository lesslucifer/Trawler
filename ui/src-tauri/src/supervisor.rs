use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use trawler::cli::Ready;

pub const LOG_LINES: usize = 2000;
const POLL: Duration = Duration::from_millis(25);

#[derive(Clone, Debug)]
pub struct Launch {
    pub program: PathBuf,
    pub leading_args: Vec<OsString>,
    pub ca_dir: PathBuf,
    pub proxy_port: u16,
    pub api_port: u16,
    pub ready_timeout: Duration,
    pub grace: Duration,
}

impl Launch {
    pub fn args(&self, proxy_port: u16, api_port: u16) -> Vec<OsString> {
        let mut args = self.leading_args.clone();
        args.extend([
            "--lifeline".into(),
            "--ca-dir".into(),
            self.ca_dir.clone().into_os_string(),
            "--proxy-port".into(),
            proxy_port.to_string().into(),
            "--api-port".into(),
            api_port.to_string().into(),
        ]);
        args
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CoreState {
    Starting,
    Running { pid: u32, proxy: SocketAddr, api: SocketAddr, started_at_ms: u64 },
    Exited { code: Option<i32>, signal: Option<i32> },
    Attached { api: String },
}

impl CoreState {
    fn exited(status: Option<ExitStatus>) -> Self {
        Self::Exited { code: status.and_then(|s| s.code()), signal: status.and_then(signal_of) }
    }
}

#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

#[cfg(not(unix))]
fn signal_of(_: ExitStatus) -> Option<i32> {
    None
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogEntry {
    Start { pid: u32, at_ms: u64 },
    Line { text: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Logged {
    pub seq: u64,
    #[serde(flatten)]
    pub entry: LogEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreEvent {
    State(CoreState),
    Log(Logged),
}

pub type Observer = Arc<dyn Fn(CoreEvent) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    Conflict { port: u16 },
    Exited { code: Option<i32>, signal: Option<i32> },
    NoReady { waited: Duration },
    Spawn(String),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict { port } => write!(f, "port {port} is already in use"),
            Self::Exited { code: Some(code), .. } => {
                write!(f, "the capture service exited with code {code} before it was ready")
            }
            Self::Exited { signal, .. } => write!(
                f,
                "the capture service was stopped by signal {} before it was ready",
                signal.map_or_else(|| "?".to_string(), |s| s.to_string())
            ),
            Self::NoReady { waited } => {
                write!(f, "the capture service did not report ready within {}s", waited.as_secs())
            }
            Self::Spawn(problem) => write!(f, "the capture service could not start: {problem}"),
        }
    }
}

#[derive(Debug)]
pub struct LogRing {
    entries: VecDeque<Logged>,
    capacity: usize,
    next_seq: u64,
}

impl LogRing {
    pub fn new(capacity: usize) -> Self {
        Self { entries: VecDeque::with_capacity(capacity.min(LOG_LINES)), capacity, next_seq: 0 }
    }

    pub fn push(&mut self, entry: LogEntry) -> Logged {
        let logged = Logged { seq: self.next_seq, entry };
        self.next_seq += 1;
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        if self.capacity > 0 {
            self.entries.push_back(logged.clone());
        }
        logged
    }

    pub fn entries(&self) -> Vec<Logged> {
        self.entries.iter().cloned().collect()
    }
}

struct Owned {
    child: Child,
    stdin: Option<ChildStdin>,
}

struct Shared {
    state: CoreState,
    generation: u64,
    owned: Option<Owned>,
    ports: (u16, u16),
    log: LogRing,
    draining: Option<mpsc::Receiver<()>>,
}

struct Inner {
    shared: Mutex<Shared>,
    observer: Observer,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self, generation: u64, state: CoreState) -> bool {
        let mut shared = self.lock();
        if shared.generation != generation {
            return false;
        }
        shared.state = state.clone();
        drop(shared);
        (self.observer)(CoreEvent::State(state));
        true
    }

    fn record(&self, entry: LogEntry) {
        let logged = self.lock().log.push(entry);
        (self.observer)(CoreEvent::Log(logged));
    }
}

/// Owns one core child at a time: spawns it, learns its addresses from the
/// ready line, watches it exit, restarts it on the ports it first bound, and
/// stops it — stdin closed first so the core shuts down cleanly, killed after
/// the grace period. Every spawn and stop bumps a generation, so a child that
/// was replaced can never report its exit as the current core's.
pub struct Supervisor {
    launch: Launch,
    inner: Arc<Inner>,
    restarting: Mutex<()>,
}

impl Supervisor {
    pub fn start(launch: Launch, observer: Observer) -> Result<Self, StartError> {
        let ports = (launch.proxy_port, launch.api_port);
        let supervisor = Self {
            launch,
            inner: Arc::new(Inner {
                shared: Mutex::new(Shared {
                    state: CoreState::Starting,
                    generation: 0,
                    owned: None,
                    ports,
                    log: LogRing::new(LOG_LINES),
                    draining: None,
                }),
                observer,
            }),
            restarting: Mutex::new(()),
        };
        supervisor.spawn(ports)?;
        Ok(supervisor)
    }

    pub fn snapshot(&self) -> CoreState {
        self.inner.lock().state.clone()
    }

    pub fn log(&self) -> Vec<Logged> {
        self.inner.lock().log.entries()
    }

    // Each window can ask; without the lock a second restart would find no child
    // to stop and spawn onto ports the first is still releasing.
    pub fn restart(&self) -> Result<CoreState, StartError> {
        let _one_at_a_time = self.restarting.lock().unwrap_or_else(PoisonError::into_inner);
        self.stop();
        let ports = self.inner.lock().ports;
        self.spawn(ports)
    }

    pub fn stop(&self) {
        let (generation, owned) = {
            let mut shared = self.inner.lock();
            shared.generation += 1;
            (shared.generation, shared.owned.take())
        };
        let Some(Owned { mut child, stdin }) = owned else {
            return;
        };
        drop(stdin);
        let status = wait_until(&mut child, Instant::now() + self.launch.grace)
            .or_else(|| child.kill().ok().and_then(|()| child.wait().ok()));
        self.inner.publish(generation, CoreState::exited(status));
    }

    fn spawn(&self, (proxy_port, api_port): (u16, u16)) -> Result<CoreState, StartError> {
        let generation = {
            let mut shared = self.inner.lock();
            shared.generation += 1;
            shared.generation
        };
        self.inner.publish(generation, CoreState::Starting);

        let mut child = Command::new(&self.launch.program)
            .args(self.launch.args(proxy_port, api_port))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                self.inner.publish(generation, CoreState::exited(None));
                StartError::Spawn(err.to_string())
            })?;
        let stdin = child.stdin.take();
        self.drain_previous_log();
        self.inner.record(LogEntry::Start { pid: child.id(), at_ms: now_ms() });
        if let Some(stderr) = child.stderr.take() {
            let inner = Arc::clone(&self.inner);
            let (drained, draining) = mpsc::channel::<()>();
            inner.lock().draining = Some(draining);
            std::thread::spawn(move || {
                let _drained = drained;
                for text in BufReader::new(stderr).lines().map_while(Result::ok) {
                    inner.record(LogEntry::Line { text });
                }
            });
        }
        let ready = child.stdout.take().map(read_ready);

        match ready.map(|rx| rx.recv_timeout(self.launch.ready_timeout)) {
            Some(Ok(ready)) => {
                let running = CoreState::Running {
                    pid: child.id(),
                    proxy: ready.proxy,
                    api: ready.api,
                    started_at_ms: now_ms(),
                };
                {
                    let mut shared = self.inner.lock();
                    shared.owned = Some(Owned { child, stdin });
                    shared.ports = (ready.proxy.port(), ready.api.port());
                }
                self.inner.publish(generation, running.clone());
                self.watch(generation);
                Ok(running)
            }
            Some(Err(mpsc::RecvTimeoutError::Timeout)) => {
                let _ = child.kill();
                let status = child.wait().ok();
                self.inner.publish(generation, CoreState::exited(status));
                Err(StartError::NoReady { waited: self.launch.ready_timeout })
            }
            Some(Err(mpsc::RecvTimeoutError::Disconnected)) | None => {
                drop(stdin);
                let status = child.wait().ok();
                self.inner.publish(generation, CoreState::exited(status));
                Err(taken_port([proxy_port, api_port]).map_or_else(
                    || StartError::Exited {
                        code: status.and_then(|s| s.code()),
                        signal: status.and_then(signal_of),
                    },
                    |port| StartError::Conflict { port },
                ))
            }
        }
    }

    // The previous core's reader can still hold lines it read before that core
    // exited; they belong above the next start mark (DEC-68 rule 13). Bounded by
    // the stop grace, in case something else keeps the old pipe open.
    fn drain_previous_log(&self) {
        let draining = self.inner.lock().draining.take();
        if let Some(draining) = draining {
            let _ = draining.recv_timeout(self.launch.grace);
        }
    }

    fn watch(&self, generation: u64) {
        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || loop {
            pause();
            let mut shared = inner.lock();
            if shared.generation != generation {
                return;
            }
            let status = match shared.owned.as_mut().map(|owned| owned.child.try_wait()) {
                Some(Ok(None)) => continue,
                Some(Ok(Some(status))) => Some(status),
                Some(Err(_)) | None => None,
            };
            shared.owned = None;
            drop(shared);
            inner.publish(generation, CoreState::exited(status));
            return;
        });
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn read_ready(stdout: std::process::ChildStdout) -> mpsc::Receiver<Ready> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut ready_tx = Some(ready_tx);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(ready) = Ready::parse(&line) {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(ready);
                }
            }
        }
    });
    ready_rx
}

fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => pause(),
            Ok(None) | Err(_) => return None,
        }
    }
}

// The core names a taken port only in a log line, which the supervisor never
// parses (DEC-68 rule 2). Once it has exited, its own sockets are released, so
// a port that still refuses a bind is held by someone else.
fn taken_port(ports: [u16; 2]) -> Option<u16> {
    ports
        .into_iter()
        .filter(|&port| port != 0)
        .find(|&port| TcpListener::bind(("127.0.0.1", port)).is_err())
}

// Only the supervisor's own threads and a blocking caller of `stop` poll here,
// never a tokio worker: `restart_core` reaches it through `spawn_blocking`.
#[allow(clippy::disallowed_methods)]
fn pause() {
    std::thread::sleep(POLL);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch() -> Launch {
        Launch {
            program: PathBuf::from("trawler-ui"),
            leading_args: vec!["--core".into()],
            ca_dir: PathBuf::from("/data/ca"),
            proxy_port: 7420,
            api_port: 7421,
            ready_timeout: Duration::from_secs(1),
            grace: Duration::from_secs(1),
        }
    }

    #[test]
    fn the_core_is_launched_with_a_lifeline_its_ca_dir_and_both_ports() {
        let args: Vec<String> =
            launch().args(7420, 7421).into_iter().map(|a| a.into_string().unwrap()).collect();
        assert_eq!(
            args,
            [
                "--core",
                "--lifeline",
                "--ca-dir",
                "/data/ca",
                "--proxy-port",
                "7420",
                "--api-port",
                "7421"
            ]
        );
    }

    fn line(text: &str) -> LogEntry {
        LogEntry::Line { text: text.into() }
    }

    #[test]
    fn the_log_ring_keeps_its_newest_entries_in_order() {
        let mut ring = LogRing::new(3);
        ring.push(LogEntry::Start { pid: 7, at_ms: 1 });
        for n in 1..=3 {
            ring.push(line(&format!("line {n}")));
        }
        let kept: Vec<(u64, LogEntry)> =
            ring.entries().into_iter().map(|logged| (logged.seq, logged.entry)).collect();
        assert_eq!(kept, [(1, line("line 1")), (2, line("line 2")), (3, line("line 3"))]);
    }

    #[test]
    fn every_entry_gets_the_next_sequence_number_even_unkept() {
        let mut ring = LogRing::new(0);
        assert_eq!(ring.push(line("a")).seq, 0);
        assert_eq!(ring.push(line("b")).seq, 1);
    }

    #[test]
    fn a_ring_with_no_room_keeps_nothing() {
        let mut ring = LogRing::new(0);
        ring.push(line("line"));
        assert!(ring.entries().is_empty());
    }

    #[test]
    fn each_log_entry_serializes_under_a_kind_tag() {
        let json = |entry: &LogEntry| serde_json::to_string(entry).unwrap();
        assert_eq!(
            json(&LogEntry::Start { pid: 42, at_ms: 1000 }),
            r#"{"kind":"start","pid":42,"at_ms":1000}"#
        );
        assert_eq!(json(&line("INFO up")), r#"{"kind":"line","text":"INFO up"}"#);
        assert_eq!(
            serde_json::to_string(&Logged { seq: 3, entry: line("INFO up") }).unwrap(),
            r#"{"seq":3,"kind":"line","text":"INFO up"}"#
        );
    }

    #[test]
    fn each_state_serializes_under_a_state_tag() {
        let running = CoreState::Running {
            pid: 42,
            proxy: "127.0.0.1:7420".parse().unwrap(),
            api: "127.0.0.1:7421".parse().unwrap(),
            started_at_ms: 1000,
        };
        let json = |state: &CoreState| serde_json::to_string(state).unwrap();
        assert_eq!(json(&CoreState::Starting), r#"{"state":"starting"}"#);
        assert_eq!(
            json(&running),
            r#"{"state":"running","pid":42,"proxy":"127.0.0.1:7420","api":"127.0.0.1:7421","started_at_ms":1000}"#
        );
        assert_eq!(
            json(&CoreState::Exited { code: None, signal: Some(9) }),
            r#"{"state":"exited","code":null,"signal":9}"#
        );
        assert_eq!(
            json(&CoreState::Attached { api: "http://127.0.0.1:9".into() }),
            r#"{"state":"attached","api":"http://127.0.0.1:9"}"#
        );
    }

    #[test]
    fn a_replaced_generation_cannot_publish_its_state() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let inner = Inner {
            shared: Mutex::new(Shared {
                state: CoreState::Starting,
                generation: 2,
                owned: None,
                ports: (0, 0),
                log: LogRing::new(1),
                draining: None,
            }),
            observer: Arc::new(move |event| sink.lock().unwrap().push(event)),
        };
        assert!(!inner.publish(1, CoreState::Exited { code: Some(1), signal: None }));
        assert_eq!(inner.lock().state, CoreState::Starting);
        assert!(seen.lock().unwrap().is_empty());
        assert!(inner.publish(2, CoreState::Exited { code: Some(0), signal: None }));
        assert_eq!(
            *seen.lock().unwrap(),
            [CoreEvent::State(CoreState::Exited { code: Some(0), signal: None })]
        );
    }

    #[test]
    fn a_held_port_is_the_one_named() {
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        assert_eq!(taken_port([0, port]), Some(port));
        drop(held);
        assert_eq!(taken_port([0, port]), None);
    }

    #[test]
    fn a_core_that_cannot_spawn_is_published_as_exited() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let supervisor = Supervisor {
            launch: Launch { program: PathBuf::from("/nonexistent/trawler-ui"), ..launch() },
            inner: Arc::new(Inner {
                shared: Mutex::new(Shared {
                    state: CoreState::Starting,
                    generation: 0,
                    owned: None,
                    ports: (0, 0),
                    log: LogRing::new(1),
                    draining: None,
                }),
                observer: Arc::new(move |event| sink.lock().unwrap().push(event)),
            }),
            restarting: Mutex::new(()),
        };
        assert!(matches!(supervisor.spawn((0, 0)), Err(StartError::Spawn(_))));
        let exited = CoreState::Exited { code: None, signal: None };
        assert_eq!(supervisor.snapshot(), exited);
        assert_eq!(seen.lock().unwrap().last(), Some(&CoreEvent::State(exited)));
    }

    #[cfg(unix)]
    #[test]
    fn a_cores_late_lines_stay_above_the_next_cores_start_mark() {
        let lingering = "(sleep 0.3; echo late >&2) >/dev/null & exit 0";
        let supervisor = Supervisor {
            launch: Launch {
                program: PathBuf::from("/bin/sh"),
                leading_args: vec!["-c".into(), lingering.into()],
                ..launch()
            },
            inner: Arc::new(Inner {
                shared: Mutex::new(Shared {
                    state: CoreState::Starting,
                    generation: 0,
                    owned: None,
                    ports: (0, 0),
                    log: LogRing::new(LOG_LINES),
                    draining: None,
                }),
                observer: Arc::new(|_| {}),
            }),
            restarting: Mutex::new(()),
        };
        assert!(supervisor.spawn((0, 0)).is_err());
        assert!(supervisor.spawn((0, 0)).is_err());
        let kinds: Vec<&str> = supervisor
            .log()
            .iter()
            .map(|logged| match &logged.entry {
                LogEntry::Start { .. } => "start",
                LogEntry::Line { .. } => "line",
            })
            .collect();
        assert_eq!(kinds[..3], ["start", "line", "start"]);
    }

    #[test]
    fn the_page_caps_its_log_at_the_rings_size() {
        let page = include_str!("../../src/coreService.ts");
        assert!(
            page.contains(&format!("export const LOG_LINES = {LOG_LINES};")),
            "ui/src/coreService.ts must declare the ring's LOG_LINES ({LOG_LINES})"
        );
    }

    #[test]
    fn a_conflict_names_its_port() {
        assert_eq!(StartError::Conflict { port: 7420 }.to_string(), "port 7420 is already in use");
    }
}
