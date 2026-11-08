use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;

use super::{config_file, id_of, names, needs_owner, open_tasks, refusal, Client, USAGE};
use super::{NEEDS_OWNER, ROUTER_PROJECT};

pub const DEFAULT_PORT: u16 = 3457;
pub const DEFAULT_POLL_SECS: u64 = 60;
const ROUTER: &str = "trawler-router";
const EVENTS: [&str; 4] =
    ["task.updated", "task.assignee.created", "task.assignee.deleted", "task.comment.created"];
const MAX_REQUEST: u64 = 1 << 20;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
pub struct Options {
    pub port: u16,
    pub state: Option<PathBuf>,
    pub secret_file: Option<PathBuf>,
    pub poll_secs: u64,
}

pub fn parse(args: &[&str]) -> Result<Options, String> {
    let mut options = Options {
        port: DEFAULT_PORT,
        state: None,
        secret_file: None,
        poll_secs: DEFAULT_POLL_SECS,
    };
    let mut flags = args.iter();
    while let Some(flag) = flags.next() {
        let mut value = || flags.next().copied().ok_or_else(|| format!("{flag} needs a value"));
        match *flag {
            "--port" => {
                let port = value()?;
                options.port =
                    port.parse().map_err(|_| format!("--port {port}: expected a port"))?;
            }
            "--state" => options.state = Some(PathBuf::from(value()?)),
            "--secret-file" => options.secret_file = Some(PathBuf::from(value()?)),
            "--poll" => {
                let secs = value()?;
                options.poll_secs =
                    secs.parse().ok().filter(|s| *s > 0).ok_or_else(|| {
                        format!("--poll {secs}: expected whole seconds, at least 1")
                    })?;
            }
            _ => return Err(format!("webhook-listen: unexpected {flag}\n{USAGE}")),
        }
    }
    Ok(options)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub asking: bool,
    pub last_comment: u64,
}

pub fn asking(task: &Value) -> bool {
    task["done"].as_bool() != Some(true) && needs_owner(task)
}

/// The lines one task owes since `was`: each newer comment not written by the
/// router, then at most one lifecycle line. `remind` re-announces an ask that
/// was already pending, so a relaunch never leaves one unsaid.
pub fn changes(
    was: Seen,
    task: &Value,
    comments: &[Value],
    remind: bool,
    by: Option<&str>,
) -> (Vec<Value>, Seen) {
    let task_id = id_of(task);
    let title = task["title"].as_str().unwrap_or_default();
    let mut fresh: Vec<&Value> = comments.iter().filter(|c| id_of(c) > was.last_comment).collect();
    fresh.sort_by_key(|c| id_of(c));
    let mut lines = Vec::new();
    for comment in &fresh {
        let author = comment["author"]["username"].as_str().unwrap_or_default();
        if author != ROUTER {
            lines.push(json!({
                "event": "comment", "task_id": task_id, "title": title, "by": author,
                "text": comment["comment"].as_str().unwrap_or_default(),
            }));
        }
    }
    let now = asking(task);
    let kind = match (was.asking, now) {
        (false, true) => Some("needs-owner"),
        (true, true) if remind => Some("needs-owner"),
        (true, false) if names(task, "labels", "title").contains(&NEEDS_OWNER) => Some("closed"),
        (true, false) => Some("answered"),
        _ => None,
    };
    if let Some(kind) = kind {
        let mut line = json!({ "event": kind, "task_id": task_id, "title": title });
        if let Some(by) = by {
            line["by"] = json!(by);
        }
        lines.push(line);
    }
    let last_comment = fresh.last().map_or(was.last_comment, |c| id_of(c));
    (lines, Seen { asking: now, last_comment })
}

fn mac(secret: &str) -> Hmac<Sha256> {
    Hmac::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any length")
}

pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = mac(secret);
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

pub fn verify(secret: &str, body: &[u8], signature: &str) -> bool {
    let Ok(expected) = hex::decode(signature.trim()) else { return false };
    let mut mac = mac(secret);
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

fn load_secret(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => {
            return Err(format!("the webhook secret at {} is empty", path.display()))
        }
        Ok(text) => return Ok(text.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    }
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(|e| format!("read /dev/urandom: {e}"))?;
    let secret = hex::encode(bytes);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .and_then(|mut file| file.write_all(secret.as_bytes()))
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(secret)
}

fn load_state(path: &Path) -> Result<BTreeMap<u64, Seen>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a file this command wrote: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

fn save_state(path: &Path, seen: &BTreeMap<u64, Seen>) -> Result<(), String> {
    let mut staging = path.as_os_str().to_owned();
    staging.push(".tmp");
    let staging = PathBuf::from(staging);
    let text = serde_json::to_string(seen).map_err(|e| e.to_string())?;
    std::fs::write(&staging, text).map_err(|e| format!("write {}: {e}", staging.display()))?;
    std::fs::rename(&staging, path).map_err(|e| format!("write {}: {e}", path.display()))
}

/// A relaunch on another port would otherwise leave this listener's old
/// webhook behind, so only a hook naming this exact target is replaced.
pub fn register(client: &Client, port: u16, secret: &str) -> Result<u64, String> {
    let target_url = format!("http://127.0.0.1:{port}/");
    let path = format!("/projects/{ROUTER_PROJECT}/webhooks");
    for hook in client.get_all(&path)? {
        if hook["target_url"].as_str() == Some(target_url.as_str()) {
            client.call("DELETE", &format!("{path}/{}", id_of(&hook)), None)?;
        }
    }
    let body = json!({ "target_url": target_url, "secret": secret, "events": EVENTS });
    let hook = client.call("PUT", &path, Some(&body))?;
    match id_of(&hook) {
        0 => Err(format!("PUT {path}: the response carried no webhook id")),
        id => Ok(id),
    }
}

fn log(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

/// A reader that went away would make every later line vanish, so the
/// process exits instead: the heartbeat relaunches it and the startup
/// reconcile re-announces what is pending.
fn emit(lines: &[Value]) {
    let mut out = std::io::stdout().lock();
    let written =
        lines.iter().try_for_each(|line| writeln!(out, "{line}")).and_then(|()| out.flush());
    if let Err(e) = written {
        log(&format!("stdout closed ({e}); exiting so the heartbeat relaunches the listener"));
        std::process::exit(2);
    }
}

struct Watch {
    client: Client,
    path: PathBuf,
    seen: BTreeMap<u64, Seen>,
}

impl Watch {
    fn observe(&mut self, task: &Value, remind: bool, by: Option<&str>) -> Result<(), String> {
        let id = id_of(task);
        let comments = self.client.get_all(&format!("/tasks/{id}/comments?order_by=asc"))?;
        let was = self.seen.get(&id).copied().unwrap_or_default();
        let (lines, now) = changes(was, task, &comments, remind, by);
        emit(&lines);
        if now != was {
            self.seen.insert(id, now);
            save_state(&self.path, &self.seen)?;
        }
        Ok(())
    }

    fn sync(&mut self, id: u64, by: Option<&str>) -> Result<(), String> {
        let path = format!("/tasks/{id}");
        let response = self.client.exchange("GET", &path, None)?;
        match response.status {
            200..=299 => {
                let task = serde_json::from_str(&response.body)
                    .map_err(|e| format!("GET {path}: the response is not JSON: {e}"))?;
                self.observe(&task, false, by)
            }
            404 => match self.seen.remove(&id) {
                Some(was) => {
                    if was.asking {
                        let mut deleted = json!({ "event": "closed", "task_id": id });
                        if let Some(by) = by {
                            deleted["by"] = json!(by);
                        }
                        emit(&[deleted]);
                    }
                    save_state(&self.path, &self.seen)
                }
                None => Ok(()),
            },
            status => Err(refusal("GET", &path, status, &response.body)),
        }
    }

    fn reconcile(&mut self, remind: bool) -> Result<(), String> {
        let open = open_tasks(&self.client, ROUTER_PROJECT)?;
        let listed: BTreeSet<u64> = open.iter().map(id_of).collect();
        for task in &open {
            self.observe(task, remind, None)?;
        }
        let left: Vec<u64> = self
            .seen
            .iter()
            .filter(|(id, seen)| seen.asking && !listed.contains(id))
            .map(|(id, _)| *id)
            .collect();
        left.into_iter().try_for_each(|id| self.sync(id, None))
    }
}

fn lock(watch: &Mutex<Watch>) -> std::sync::MutexGuard<'_, Watch> {
    watch.lock().unwrap_or_else(PoisonError::into_inner)
}

fn poll_forever(watch: &Mutex<Watch>, every: Duration) {
    let mut down = false;
    loop {
        let until = Instant::now() + every;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            std::thread::park_timeout(left);
        }
        match (lock(watch).reconcile(false), down) {
            (Err(error), false) => {
                emit(&[json!({ "event": "unreachable", "error": error })]);
                down = true;
            }
            (Ok(()), true) => {
                emit(&[json!({ "event": "reachable" })]);
                down = false;
            }
            _ => {}
        }
    }
}

struct Delivery {
    method: String,
    signature: String,
    body: Vec<u8>,
}

fn read_delivery(stream: &TcpStream) -> Result<Delivery, String> {
    let mut reader = BufReader::new(stream.take(MAX_REQUEST));
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let method = line.split_whitespace().next().ok_or("empty request")?.to_string();
    let (mut length, mut signature) = (0_u64, String::new());
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        let Some((name, value)) = header.trim_end().split_once(':') else { break };
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().map_err(|_| "bad Content-Length")?;
        } else if name.eq_ignore_ascii_case("x-vikunja-signature") {
            signature = value.trim().to_string();
        }
    }
    if length > MAX_REQUEST {
        return Err(format!("body over {MAX_REQUEST} bytes"));
    }
    let mut body = vec![0; usize::try_from(length).map_err(|_| "bad Content-Length")?];
    reader.read_exact(&mut body).map_err(|e| format!("body: {e}"))?;
    Ok(Delivery { method, signature, body })
}

fn respond(mut stream: &TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn task_of(payload: &[u8], peer: &str) -> Option<(u64, Option<String>)> {
    let Ok(payload) = serde_json::from_slice::<Value>(payload) else {
        log(&format!("signed delivery from {peer} is not JSON - ignored"));
        return None;
    };
    let id = payload["data"]["task"]["id"].as_u64()?;
    Some((id, payload["data"]["doer"]["username"].as_str().map(String::from)))
}

/// A verified delivery is answered only after its task is synced: Vikunja
/// sends from its own goroutine and serves the reads meanwhile, and a
/// sender that waits for the answer sees the lines already printed.
fn handle(stream: &TcpStream, secret: &str, watch: &Mutex<Watch>) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let peer = stream.peer_addr().map_or_else(|_| "?".to_string(), |a| a.to_string());
    let delivery = match read_delivery(stream) {
        Ok(delivery) => delivery,
        Err(e) => {
            log(&format!("rejected request from {peer}: {e}"));
            return respond(stream, "400 Bad Request", "bad request\n");
        }
    };
    match delivery.method.as_str() {
        "GET" => respond(stream, "200 OK", "ok\n"),
        "POST" if !verify(secret, &delivery.body, &delivery.signature) => {
            log(&format!(
                "rejected delivery from {peer}: X-Vikunja-Signature does not verify ({} bytes ignored)",
                delivery.body.len()
            ));
            respond(stream, "401 Unauthorized", "bad signature\n");
        }
        "POST" => {
            if let Some((id, doer)) = task_of(&delivery.body, &peer) {
                if let Err(e) = lock(watch).sync(id, doer.as_deref()) {
                    log(&format!("task #{id}: {e} - the next reconcile retries it"));
                }
            }
            respond(stream, "200 OK", "ok\n");
        }
        _ => respond(stream, "405 Method Not Allowed", "GET or POST\n"),
    }
}

pub fn run(client: Client, options: &Options) -> Result<(i32, String), String> {
    let secret_file =
        options.secret_file.clone().unwrap_or_else(|| config_file("vikunja-webhook-secret"));
    let state = options.state.clone().unwrap_or_else(|| config_file("vikunja-listener.json"));
    let secret = load_secret(&secret_file)?;
    let seen = load_state(&state)?;
    let listener = TcpListener::bind(("127.0.0.1", options.port))
        .map_err(|e| format!("cannot bind 127.0.0.1:{}: {e}", options.port))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let webhook_id = register(&client, port, &secret)?;
    let mut watch = Watch { client, path: state, seen };
    watch.reconcile(true)?;
    emit(&[json!({ "event": "listening", "port": port, "webhook_id": webhook_id })]);
    let watch = Arc::new(Mutex::new(watch));
    let polled = Arc::clone(&watch);
    let every = Duration::from_secs(options.poll_secs);
    std::thread::spawn(move || poll_forever(&polled, every));
    let secret: Arc<str> = secret.into();
    for stream in listener.incoming().flatten() {
        let (secret, watch) = (Arc::clone(&secret), Arc::clone(&watch));
        std::thread::spawn(move || handle(&stream, &secret, &watch));
    }
    Ok((0, String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(done: bool, labels: &Value) -> Value {
        json!({
            "id": 9, "title": "E-X.W1 build", "done": done,
            "assignees": [{"id": 1, "username": "vuluu"}], "labels": labels,
        })
    }

    fn labelled() -> Value {
        json!([{"id": 1, "title": "needs-owner"}])
    }

    fn comment(id: u64, who: &str, text: &str) -> Value {
        json!({"id": id, "comment": text, "author": {"username": who}})
    }

    const ASKING: Seen = Seen { asking: true, last_comment: 0 };

    #[test]
    fn an_ask_raised_is_one_needs_owner_line_carrying_the_doer() {
        let (lines, seen) =
            changes(Seen::default(), &task(false, &labelled()), &[], false, Some("trawler-router"));
        assert_eq!(
            lines,
            [
                json!({"event": "needs-owner", "task_id": 9, "title": "E-X.W1 build", "by": "trawler-router"})
            ]
        );
        assert_eq!(seen, ASKING);
    }

    #[test]
    fn a_pending_ask_is_silent_unless_reminded() {
        let open = task(false, &labelled());
        assert_eq!(changes(ASKING, &open, &[], false, None), (vec![], ASKING));
        let (reminded, _) = changes(ASKING, &open, &[], true, None);
        assert_eq!(
            reminded,
            [json!({"event": "needs-owner", "task_id": 9, "title": "E-X.W1 build"})]
        );
        let (plain, _) = changes(Seen::default(), &task(false, &json!([])), &[], true, None);
        assert_eq!(plain, Vec::<Value>::new());
    }

    #[test]
    fn removing_the_label_answers_and_leaving_the_queue_otherwise_closes() {
        let (answered, seen) =
            changes(ASKING, &task(true, &json!(null)), &[], false, Some("vuluu"));
        assert_eq!(answered[0]["event"], "answered");
        assert!(!seen.asking);
        let (done, _) = changes(ASKING, &task(true, &labelled()), &[], false, None);
        assert_eq!(done, [json!({"event": "closed", "task_id": 9, "title": "E-X.W1 build"})]);
        let mut unassigned = task(false, &labelled());
        unassigned["assignees"] = json!([]);
        assert_eq!(changes(ASKING, &unassigned, &[], false, None).0[0]["event"], "closed");
    }

    #[test]
    fn only_newer_comments_not_by_the_router_are_lines_and_they_precede_the_transition() {
        let comments = [
            comment(7, "vuluu", "go ahead"),
            comment(3, "vuluu", "seen before"),
            comment(5, "trawler-router", "the ask"),
            comment(6, "vuluu", "one question"),
        ];
        let was = Seen { asking: true, last_comment: 4 };
        let (lines, seen) = changes(was, &task(false, &json!([])), &comments, false, Some("vuluu"));
        let said: Vec<(&str, &str)> = lines
            .iter()
            .map(|l| {
                (l["event"].as_str().unwrap_or_default(), l["text"].as_str().unwrap_or_default())
            })
            .collect();
        assert_eq!(said, [("comment", "one question"), ("comment", "go ahead"), ("answered", "")]);
        assert_eq!(lines[0]["by"], "vuluu");
        assert_eq!(seen, Seen { asking: false, last_comment: 7 });
        let (router_only, after) = changes(
            Seen::default(),
            &task(false, &json!([])),
            &[comment(8, ROUTER, "x")],
            false,
            None,
        );
        assert_eq!((router_only, after.last_comment), (vec![], 8));
    }

    #[test]
    fn a_signature_verifies_only_for_its_key_and_body() {
        let rfc4231_case2 = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        assert_eq!(sign("Jefe", b"what do ya want for nothing?"), rfc4231_case2);
        assert!(verify("Jefe", b"what do ya want for nothing?", rfc4231_case2));
        assert!(verify("Jefe", b"what do ya want for nothing?", &rfc4231_case2.to_uppercase()));
        assert!(!verify("Jefe", b"what do ya want for nothing!", rfc4231_case2));
        assert!(!verify("other", b"what do ya want for nothing?", rfc4231_case2));
        assert!(!verify("Jefe", b"what do ya want for nothing?", ""));
        assert!(!verify("Jefe", b"what do ya want for nothing?", "not hex"));
    }

    #[test]
    fn listen_flags_parse_and_bad_values_are_refused() {
        assert_eq!(
            parse(&[]).unwrap(),
            Options { port: DEFAULT_PORT, state: None, secret_file: None, poll_secs: 60 }
        );
        assert_eq!(
            parse(&["--poll", "5", "--port", "0", "--state", "s.json", "--secret-file", "k"])
                .unwrap(),
            Options {
                port: 0,
                state: Some("s.json".into()),
                secret_file: Some("k".into()),
                poll_secs: 5
            }
        );
        assert!(parse(&["--poll", "0"]).unwrap_err().contains("at least 1"));
        assert!(parse(&["--port", "x"]).unwrap_err().contains("expected a port"));
        assert!(parse(&["--port"]).unwrap_err().contains("--port needs a value"));
        assert!(parse(&["--bogus"]).unwrap_err().starts_with("webhook-listen: unexpected --bogus"));
    }

    #[test]
    fn a_missing_secret_is_created_private_and_then_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/secret");
        let first = load_secret(&path).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(load_secret(&path).unwrap(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "\n").unwrap();
        assert!(load_secret(&path).unwrap_err().ends_with("is empty"));
    }
}
