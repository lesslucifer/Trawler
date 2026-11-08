pub mod http;
pub mod listen;

use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const DEFAULT_API: &str = "127.0.0.1:3456";
pub const ROUTER_PROJECT: u64 = 3;
pub const OWNER: &str = "vuluu";
pub const NEEDS_OWNER: &str = "needs-owner";
/// The router token holds no user-read right, so usernames resolve through the
/// ids vikunja-protocol.md fixes.
const USERS: [(&str, u64); 2] = [("vuluu", 1), ("trawler-router", 2)];
const RELATION_NAMES: [(&str, &str); 4] = [
    ("parenttask", "parent"),
    ("subtask", "subtasks"),
    ("blocked", "blocked-by"),
    ("blocking", "blocks"),
];

pub const USAGE: &str = "\
usage:
  cargo xtask vikunja init                  (the token file reaches the instance: GET project 3)
  cargo xtask vikunja show [--project <id>] [--needs-owner]
                                            (one line per open task: id, state, title, assignees, labels, relations;
                                             --needs-owner keeps tasks assigned to vuluu and labelled needs-owner; project defaults to 3)
  cargo xtask vikunja read <id>             (the task, its relations, description and every comment in order)
  cargo xtask vikunja put <id> [--comment <text|->] [--description <text|->] [--relation blocked-by|blocks:<id>]...
                               [--done|--undone] [--assignee <user>]... [--label <name>]... [--unlabel <name>]... [--unassign <user>]...
                                            (applied in this order whatever the flag order: comment, relations, task fields
                                             (done, description: one whole-task read-modify-write), assign, label, unlabel, unassign;
                                             an add already present or a removal already absent is skipped; a label that does not
                                             exist is created and reported; users: vuluu, trawler-router or a numeric id)
  cargo xtask vikunja create --title <text> [--project <id>] [--description <text|->] [--parent <id>] [--relation blocked-by|blocks:<id>]...
                                            (a new task; --parent makes it a subtask of that task)
  cargo xtask vikunja webhook-listen [--port <p>] [--state <file>] [--secret-file <file>] [--poll <secs>]
                                            (standing: binds 127.0.0.1:<p> (default 3457), registers itself as project 3's webhook
                                             with the secret file's key (default ~/.config/trawler/vikunja-webhook-secret, created 600),
                                             reconciles open tasks, then prints one JSON line per needs-owner, answered, closed,
                                             comment, unreachable or reachable event until killed; state defaults to
                                             ~/.config/trawler/vikunja-listener.json, the reconcile poll to 60 s; GET / answers ok)
  every command: [--api <ip:port>] (default 127.0.0.1:3456) [--key-file <path>] (default ~/.config/trawler/vikunja.key)
  only one flag per call may read stdin (-)";

pub struct Client {
    addr: SocketAddr,
    authorization: String,
}

impl Client {
    pub fn new(addr: SocketAddr, token: &str) -> Self {
        Self { addr, authorization: format!("Bearer {}", token.trim()) }
    }

    fn exchange(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<http::Response, String> {
        let payload = body.map(Value::to_string).unwrap_or_default();
        let mut headers = vec![("Authorization", self.authorization.as_str())];
        if body.is_some() {
            headers.push(("Content-Type", "application/json"));
        }
        http::request(self.addr, method, &format!("/api/v1{path}"), &headers, &payload)
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<http::Response, String> {
        let response = self.exchange(method, path, body)?;
        if (200..300).contains(&response.status) {
            Ok(response)
        } else {
            Err(refusal(method, path, response.status, &response.body))
        }
    }

    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let response = self.send(method, path, body)?;
        serde_json::from_str(&response.body)
            .map_err(|e| format!("{method} {path}: the response is not JSON: {e}"))
    }

    pub fn get_all(&self, path: &str) -> Result<Vec<Value>, String> {
        let separator = if path.contains('?') { '&' } else { '?' };
        let mut items = Vec::new();
        for page in 1.. {
            let paged = format!("{path}{separator}page={page}");
            let response = self.send("GET", &paged, None)?;
            let batch: Option<Vec<Value>> = serde_json::from_str(&response.body)
                .map_err(|e| format!("GET {paged}: the response is not a JSON list: {e}"))?;
            items.extend(batch.unwrap_or_default());
            if page >= response.total_pages.unwrap_or(1) {
                break;
            }
        }
        Ok(items)
    }
}

fn refusal(method: &str, path: &str, status: u16, body: &str) -> String {
    let hint = if status == 401 || status == 403 {
        " - check the router token and its permissions"
    } else {
        ""
    };
    format!("{method} {path}: HTTP {status}{hint}: {}", body.trim())
}

fn id_of(value: &Value) -> u64 {
    value["id"].as_u64().unwrap_or_default()
}

fn list<'a>(task: &'a Value, field: &str) -> impl Iterator<Item = &'a Value> {
    task[field].as_array().into_iter().flatten()
}

fn names<'a>(task: &'a Value, field: &str, key: &str) -> Vec<&'a str> {
    list(task, field).filter_map(|item| item[key].as_str()).collect()
}

fn find_id(task: &Value, field: &str, key: &str, name: &str) -> Option<u64> {
    list(task, field).find(|item| item[key].as_str() == Some(name)).map(id_of)
}

fn related_ids(task: &Value, kind: &str) -> Vec<u64> {
    list(&task["related_tasks"], kind).map(id_of).collect()
}

fn relations(task: &Value) -> Vec<(&str, Vec<&Value>)> {
    task["related_tasks"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(kind, others)| {
            let others: Vec<&Value> = others.as_array()?.iter().collect();
            (!others.is_empty()).then(|| (relation_name(kind), others))
        })
        .collect()
}

pub fn relation_name(kind: &str) -> &str {
    RELATION_NAMES.iter().find(|(k, _)| *k == kind).map_or(kind, |(_, name)| name)
}

pub fn relation_kind(flag: &str) -> Result<&'static str, String> {
    match flag {
        "blocked-by" => Ok("blocked"),
        "blocks" => Ok("blocking"),
        _ => Err(format!("relation {flag:?}: expected blocked-by or blocks")),
    }
}

pub fn parse_relation(spec: &str) -> Result<(&'static str, u64), String> {
    let (flag, other) = spec
        .split_once(':')
        .ok_or_else(|| format!("--relation {spec}: expected <blocked-by|blocks>:<id>"))?;
    Ok((relation_kind(flag)?, task_id(other)?))
}

pub fn task_id(text: &str) -> Result<u64, String> {
    text.strip_prefix('#')
        .unwrap_or(text)
        .parse()
        .map_err(|_| format!("{text}: expected a task id"))
}

pub fn user_id(name: &str) -> Result<u64, String> {
    if let Some((_, id)) = USERS.iter().find(|(user, _)| *user == name) {
        return Ok(*id);
    }
    name.parse().map_err(|_| {
        let known: Vec<String> = USERS.iter().map(|(user, id)| format!("{user} ({id})")).collect();
        format!(
            "unknown user {name:?}: the router token cannot look users up - use {} or a numeric id",
            known.join(", ")
        )
    })
}

/// The label's id and whether it had to be created.
pub fn label_id(
    name: &str,
    labels: &[Value],
    create: impl FnOnce(&str) -> Result<u64, String>,
) -> Result<(u64, bool), String> {
    match labels.iter().find(|label| label["title"].as_str() == Some(name)) {
        Some(label) => Ok((id_of(label), false)),
        None => create(name)
            .map(|id| (id, true))
            .map_err(|e| format!("no label named {name:?}, and creating it failed: {e}")),
    }
}

pub fn needs_owner(task: &Value) -> bool {
    names(task, "assignees", "username").contains(&OWNER)
        && names(task, "labels", "title").contains(&NEEDS_OWNER)
}

fn state(task: &Value) -> &'static str {
    if task["done"].as_bool() == Some(true) {
        "done"
    } else {
        "open"
    }
}

pub fn summary(task: &Value) -> String {
    let mut line = format!(
        "#{} [{}] {}",
        id_of(task),
        state(task),
        task["title"].as_str().unwrap_or_default()
    );
    for (field, key) in [("assignees", "username"), ("labels", "title")] {
        let values = names(task, field, key);
        if !values.is_empty() {
            let _ = write!(line, " · {field}: {}", values.join(", "));
        }
    }
    for (name, others) in relations(task) {
        let ids: Vec<String> = others.iter().map(|other| format!("#{}", id_of(other))).collect();
        let _ = write!(line, " · {name}: {}", ids.join(" "));
    }
    line
}

pub fn detail(task: &Value, comments: &[Value]) -> String {
    let mut out = summary(task);
    for (name, others) in relations(task) {
        for other in others {
            let _ = write!(out, "\n{name}: {}", summary(other));
        }
    }
    let description = task["description"].as_str().unwrap_or_default().trim();
    if description.is_empty() {
        out.push_str("\n\ndescription: (none)");
    } else {
        let _ = write!(out, "\n\ndescription:\n{description}");
    }
    let _ = write!(out, "\n\ncomments ({}):", comments.len());
    for comment in comments {
        let _ = write!(
            out,
            "\n--- #{} {} {}\n{}",
            id_of(comment),
            comment["created"].as_str().unwrap_or_default(),
            comment["author"]["username"].as_str().unwrap_or_default(),
            comment["comment"].as_str().unwrap_or_default().trim()
        );
    }
    out
}

pub fn init(client: &Client, key_file: &Path) -> Result<String, String> {
    let project = client.call("GET", &format!("/projects/{ROUTER_PROJECT}"), None)?;
    Ok(format!(
        "ok: the token at {} reaches http://{}/api/v1 - project {ROUTER_PROJECT} {:?}",
        key_file.display(),
        client.addr,
        project["title"].as_str().unwrap_or_default()
    ))
}

fn open_tasks(client: &Client, project: u64) -> Result<Vec<Value>, String> {
    let filter = http::percent_encode(&format!("done = false && project = {project}"));
    client.get_all(&format!("/tasks?filter={filter}&sort_by=id&order_by=asc"))
}

pub fn show(client: &Client, project: u64, needs_owner_only: bool) -> Result<String, String> {
    let tasks = open_tasks(client, project)?;
    let lines: Vec<String> =
        tasks.iter().filter(|task| !needs_owner_only || needs_owner(task)).map(summary).collect();
    Ok(match (lines.is_empty(), needs_owner_only) {
        (true, true) => format!("nothing needs {OWNER} in project {project}"),
        (true, false) => format!("no open tasks in project {project}"),
        (false, _) => lines.join("\n"),
    })
}

pub fn read(client: &Client, id: u64) -> Result<String, String> {
    let task = client.call("GET", &format!("/tasks/{id}"), None)?;
    let comments = client.get_all(&format!("/tasks/{id}/comments?order_by=asc"))?;
    Ok(detail(&task, &comments))
}

#[derive(Debug, Default, PartialEq)]
pub struct Put {
    pub comment: Option<String>,
    pub description: Option<String>,
    pub relations: Vec<(&'static str, u64)>,
    pub done: Option<bool>,
    pub assign: Vec<String>,
    pub label: Vec<String>,
    pub unlabel: Vec<String>,
    pub unassign: Vec<String>,
}

pub fn put(client: &Client, id: u64, change: &Put) -> Result<String, String> {
    if *change == Put::default() {
        return Err(format!("put #{id}: nothing to do\n{USAGE}"));
    }
    let assign = change
        .assign
        .iter()
        .map(|user| Ok((user.as_str(), user_id(user)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let mut task = client.call("GET", &format!("/tasks/{id}"), None)?;
    let labels = if change.label.is_empty() { Vec::new() } else { client.get_all("/labels")? };
    let mut said = Vec::new();
    apply(client, id, &mut task, change, &assign, &labels, &mut said)
        .map_err(|e| stopped(&said, &e))?;
    Ok(said.join("\n"))
}

fn stopped(said: &[String], error: &str) -> String {
    if said.is_empty() {
        error.to_string()
    } else {
        format!("{}\nthen stopped: {error}", said.join("\n"))
    }
}

fn apply(
    client: &Client,
    id: u64,
    task: &mut Value,
    change: &Put,
    assign: &[(&str, u64)],
    labels: &[Value],
    said: &mut Vec<String>,
) -> Result<(), String> {
    let path = format!("/tasks/{id}");
    if let Some(text) = &change.comment {
        client.call("PUT", &format!("{path}/comments"), Some(&json!({ "comment": text })))?;
        said.push(format!("#{id}: commented"));
    }
    for &(kind, other) in &change.relations {
        if related_ids(task, kind).contains(&other) {
            said.push(format!("#{id}: relation {kind} #{other} already there - skipped"));
        } else {
            relate(client, id, kind, other, said)?;
        }
    }
    write_fields(client, id, task, change, said)?;
    let mut touched = false;
    for &(user, user_id) in assign {
        if list(task, "assignees").any(|a| id_of(a) == user_id) {
            said.push(format!("#{id}: {user} already assigned - skipped"));
        } else {
            client.call(
                "PUT",
                &format!("{path}/assignees"),
                Some(&json!({ "user_id": user_id })),
            )?;
            said.push(format!("#{id}: assigned {user}"));
            touched = true;
        }
    }
    for name in &change.label {
        let (label, created) = label_id(name, labels, |title| {
            client.call("PUT", "/labels", Some(&json!({ "title": title }))).map(|l| id_of(&l))
        })?;
        if created {
            said.push(format!(
                "CREATED LABEL {name:?} (id {label}): no label had that name - check the spelling; vikunja-protocol.md defines {NEEDS_OWNER}"
            ));
        }
        if list(task, "labels").any(|l| id_of(l) == label) {
            said.push(format!("#{id}: already labelled {name} - skipped"));
        } else {
            client.call("PUT", &format!("{path}/labels"), Some(&json!({ "label_id": label })))?;
            said.push(format!("#{id}: labelled {name}"));
            touched = true;
        }
    }
    for name in &change.unlabel {
        match find_id(task, "labels", "title", name) {
            Some(label) => {
                client.call("DELETE", &format!("{path}/labels/{label}"), None)?;
                said.push(format!("#{id}: unlabelled {name}"));
                touched = true;
            }
            None => said.push(format!("#{id}: not labelled {name} - skipped")),
        }
    }
    for user in &change.unassign {
        let named = |a: &&Value| {
            a["username"].as_str() == Some(user) || user.parse().ok() == Some(id_of(a))
        };
        match list(task, "assignees").find(named).map(id_of) {
            Some(user_id) => {
                client.call("DELETE", &format!("{path}/assignees/{user_id}"), None)?;
                said.push(format!("#{id}: unassigned {user}"));
                touched = true;
            }
            None => said.push(format!("#{id}: {user} not assigned - skipped")),
        }
    }
    if touched {
        said.extend(hidden_while_done(id, task, change));
    }
    Ok(())
}

fn hidden_while_done(id: u64, task: &Value, change: &Put) -> Option<String> {
    (task["done"].as_bool() == Some(true) && change.done != Some(true)).then(|| {
        format!("#{id}: note - this task is done, so show (and show --needs-owner) will not list it until it is marked open (--undone)")
    })
}

/// `POST /tasks/<id>` replaces the whole task - a partial body clears the
/// assignees - so the task read at the start is written back whole, with only
/// the changed fields swapped in.
fn write_fields(
    client: &Client,
    id: u64,
    task: &mut Value,
    change: &Put,
    said: &mut Vec<String>,
) -> Result<(), String> {
    let mut changed = Vec::new();
    if let Some(done) = change.done {
        if task["done"].as_bool().unwrap_or_default() == done {
            said.push(format!("#{id}: already {} - skipped", state(task)));
        } else {
            task["done"] = json!(done);
            changed.push(if done { "marked done" } else { "marked open" });
        }
    }
    if let Some(description) = &change.description {
        if task["description"].as_str() != Some(description) {
            task["description"] = json!(description);
            changed.push("description set");
        }
    }
    if !changed.is_empty() {
        client.call("POST", &format!("/tasks/{id}"), Some(task))?;
        said.push(format!("#{id}: {} (whole task written)", changed.join(", ")));
    }
    Ok(())
}

fn relate(
    client: &Client,
    id: u64,
    kind: &str,
    other: u64,
    said: &mut Vec<String>,
) -> Result<(), String> {
    let body = json!({ "other_task_id": other, "relation_kind": kind });
    client.call("PUT", &format!("/tasks/{id}/relations"), Some(&body))?;
    said.push(format!("#{id}: relation {kind} #{other}"));
    Ok(())
}

#[derive(Debug, PartialEq)]
pub struct Create {
    pub project: u64,
    pub title: String,
    pub description: Option<String>,
    pub parent: Option<u64>,
    pub relations: Vec<(&'static str, u64)>,
}

pub fn create(client: &Client, new: &Create) -> Result<String, String> {
    let mut body = json!({ "title": new.title });
    if let Some(description) = &new.description {
        body["description"] = json!(description);
    }
    let task = client.call("PUT", &format!("/projects/{}/tasks", new.project), Some(&body))?;
    let id = id_of(&task);
    if id == 0 {
        return Err(format!(
            "PUT /projects/{}/tasks: the response carried no task id",
            new.project
        ));
    }
    let mut said = vec![format!("created #{id} {:?} in project {}", new.title, new.project)];
    link(client, id, new, &mut said).map_err(|e| stopped(&said, &e))?;
    Ok(said.join("\n"))
}

fn link(client: &Client, id: u64, new: &Create, said: &mut Vec<String>) -> Result<(), String> {
    if let Some(parent) = new.parent {
        relate(client, parent, "subtask", id, said)?;
    }
    for &(kind, other) in &new.relations {
        relate(client, id, kind, other, said)?;
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Init,
    Show { project: u64, needs_owner: bool },
    Read(u64),
    Put(u64, Put),
    Create(Create),
    Listen(listen::Options),
}

fn take_flag(args: &mut Vec<&str>, flag: &str) -> Result<Option<String>, String> {
    let Some(i) = args.iter().position(|a| *a == flag) else { return Ok(None) };
    let value = args.get(i + 1).ok_or_else(|| format!("{flag} needs a value"))?.to_string();
    args.drain(i..=i + 1);
    Ok(Some(value))
}

fn text(source: &str, stdin_taken: &mut bool) -> Result<String, String> {
    if source != "-" {
        return Ok(source.to_string());
    }
    if std::mem::replace(stdin_taken, true) {
        return Err("only one flag per call can read stdin (-)".to_string());
    }
    std::io::read_to_string(std::io::stdin()).map_err(|e| format!("read stdin: {e}"))
}

fn push_once<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

pub fn parse(args: &[&str]) -> Result<Command, String> {
    let (command, rest) = args.split_first().ok_or_else(|| USAGE.to_string())?;
    if *command == "webhook-listen" {
        return listen::parse(rest).map(Command::Listen);
    }
    let mut stdin_taken = false;
    let mut flags = rest.iter();
    let mut show = (ROUTER_PROJECT, false);
    let mut change = Put::default();
    let mut new = Create {
        project: ROUTER_PROJECT,
        title: String::new(),
        description: None,
        parent: None,
        relations: Vec::new(),
    };
    let id = match *command {
        "read" | "put" => {
            task_id(flags.next().ok_or_else(|| format!("{command} needs a task id\n{USAGE}"))?)?
        }
        "init" | "show" | "create" => 0,
        _ => return Err(USAGE.to_string()),
    };
    while let Some(flag) = flags.next() {
        let mut value = || flags.next().copied().ok_or_else(|| format!("{flag} needs a value"));
        match (*command, *flag) {
            ("show" | "create", "--project") => {
                let project = value()?;
                let project =
                    project.parse().map_err(|_| format!("--project {project}: expected an id"))?;
                show.0 = project;
                new.project = project;
            }
            ("show", "--needs-owner") => show.1 = true,
            ("put", "--comment") => change.comment = Some(text(value()?, &mut stdin_taken)?),
            ("put", "--description") => {
                change.description = Some(text(value()?, &mut stdin_taken)?);
            }
            ("put", "--relation") => push_once(&mut change.relations, parse_relation(value()?)?),
            ("put", "--done" | "--undone") => {
                let done = *flag == "--done";
                if change.done.is_some_and(|d| d != done) {
                    return Err("--done and --undone contradict each other".to_string());
                }
                change.done = Some(done);
            }
            ("put", "--assignee") => push_once(&mut change.assign, value()?.to_string()),
            ("put", "--unassign") => push_once(&mut change.unassign, value()?.to_string()),
            ("put", "--label") => push_once(&mut change.label, value()?.to_string()),
            ("put", "--unlabel") => push_once(&mut change.unlabel, value()?.to_string()),
            ("create", "--title") => new.title = value()?.to_string(),
            ("create", "--description") => {
                new.description = Some(text(value()?, &mut stdin_taken)?);
            }
            ("create", "--parent") => new.parent = Some(task_id(value()?)?),
            ("create", "--relation") => push_once(&mut new.relations, parse_relation(value()?)?),
            _ => return Err(format!("{command}: unexpected {flag}\n{USAGE}")),
        }
    }
    Ok(match *command {
        "init" => Command::Init,
        "show" => Command::Show { project: show.0, needs_owner: show.1 },
        "read" => Command::Read(id),
        "put" => {
            let both = |adds: &[String], removes: &[String]| {
                adds.iter().find(|a| removes.contains(a)).cloned()
            };
            if let Some(name) = both(&change.assign, &change.unassign)
                .or_else(|| both(&change.label, &change.unlabel))
            {
                return Err(format!("put: {name} is both added and removed"));
            }
            Command::Put(id, change)
        }
        _ if new.title.trim().is_empty() => return Err(format!("create needs --title\n{USAGE}")),
        _ => Command::Create(new),
    })
}

fn config_file(name: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    home.join(".config/trawler").join(name)
}

pub fn run(args: &[&str]) -> Result<(i32, String), String> {
    let mut args = args.to_vec();
    let api = take_flag(&mut args, "--api")?.unwrap_or_else(|| DEFAULT_API.to_string());
    let addr: SocketAddr =
        api.parse().map_err(|_| format!("--api {api}: expected <ip:port>, e.g. {DEFAULT_API}"))?;
    let key_file = take_flag(&mut args, "--key-file")?
        .map_or_else(|| config_file("vikunja.key"), PathBuf::from);
    let command = parse(&args)?;
    let token = std::fs::read_to_string(&key_file)
        .map_err(|e| format!("cannot read the router token at {}: {e}", key_file.display()))?;
    if token.trim().is_empty() {
        return Err(format!("the router token at {} is empty", key_file.display()));
    }
    let client = Client::new(addr, &token);
    let out = match command {
        Command::Init => init(&client, &key_file)?,
        Command::Show { project, needs_owner } => show(&client, project, needs_owner)?,
        Command::Read(id) => read(&client, id)?,
        Command::Put(id, change) => put(&client, id, &change)?,
        Command::Create(new) => create(&client, &new)?,
        Command::Listen(options) => return listen::run(client, &options),
    };
    Ok((0, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Value {
        json!({
            "id": 7, "title": "E-X.W1 build", "done": false,
            "assignees": [{"id": 1, "username": "vuluu"}],
            "labels": [{"id": 1, "title": "needs-owner"}],
            "related_tasks": {
                "parenttask": [{"id": 5, "title": "E-X epic", "done": false}],
                "blocked": [{"id": 6, "title": "E-X.W0 first", "done": true}],
                "blocking": []
            }
        })
    }

    #[test]
    fn the_cli_spelling_maps_to_vikunja_s_relation_enum() {
        assert_eq!(parse_relation("blocked-by:3").unwrap(), ("blocked", 3));
        assert_eq!(parse_relation("blocks:#4").unwrap(), ("blocking", 4));
        assert!(parse_relation("blocked:3").unwrap_err().contains("blocked-by or blocks"));
        assert!(parse_relation("blocks").unwrap_err().contains("<blocked-by|blocks>:<id>"));
        assert!(parse_relation("blocks:x").unwrap_err().contains("expected a task id"));
        assert_eq!(relation_name("blocked"), "blocked-by");
        assert_eq!(relation_name("parenttask"), "parent");
        assert_eq!(relation_name("related"), "related");
    }

    #[test]
    fn a_known_label_resolves_without_creating_one() {
        let labels = [json!({"id": 1, "title": "needs-owner"}), json!({"id": 4, "title": "x"})];
        let resolved = label_id("x", &labels, |_| panic!("must not create"));
        assert_eq!(resolved.unwrap(), (4, false));
    }

    #[test]
    fn a_missing_label_is_created_and_flagged() {
        let labels = [json!({"id": 1, "title": "needs-owner"})];
        let mut asked = None;
        let resolved = label_id("blocked-on-ci", &labels, |title| {
            asked = Some(title.to_string());
            Ok(9)
        });
        assert_eq!(resolved.unwrap(), (9, true));
        assert_eq!(asked.as_deref(), Some("blocked-on-ci"));
        let refused = label_id("y", &labels, |_| Err("PUT /labels: HTTP 401".into()));
        assert_eq!(
            refused.unwrap_err(),
            "no label named \"y\", and creating it failed: PUT /labels: HTTP 401"
        );
    }

    #[test]
    fn users_resolve_through_the_protocol_ids_or_a_number() {
        assert_eq!(user_id("vuluu").unwrap(), 1);
        assert_eq!(user_id("trawler-router").unwrap(), 2);
        assert_eq!(user_id("17").unwrap(), 17);
        let unknown = user_id("someone").unwrap_err();
        assert!(unknown.contains("vuluu (1), trawler-router (2)"), "{unknown}");
    }

    #[test]
    fn the_needs_owner_queue_wants_both_the_owner_and_the_label() {
        assert!(needs_owner(&task()));
        let mut unassigned = task();
        unassigned["assignees"] = json!(null);
        assert!(!needs_owner(&unassigned));
        let mut unlabelled = task();
        unlabelled["labels"] = json!([{"id": 2, "title": "other"}]);
        assert!(!needs_owner(&unlabelled));
    }

    #[test]
    fn a_summary_line_carries_state_people_labels_and_relations() {
        assert_eq!(
            summary(&task()),
            "#7 [open] E-X.W1 build · assignees: vuluu · labels: needs-owner · blocked-by: #6 · parent: #5"
        );
        assert_eq!(
            summary(&json!({"id": 2, "title": "t", "done": true, "labels": null})),
            "#2 [done] t"
        );
    }

    #[test]
    fn a_detail_lists_each_relation_the_description_and_comments_in_order() {
        let comments = [
            json!({"id": 1, "created": "c1", "author": {"username": "trawler-router"}, "comment": "ask\n"}),
            json!({"id": 2, "created": "c2", "author": {"username": "vuluu"}, "comment": "ok"}),
        ];
        let mut with_text = task();
        with_text["description"] = json!("Branch: wu/x/build");
        let out = detail(&with_text, &comments);
        assert!(
            out.contains("\nblocked-by: #6 [done] E-X.W0 first\nparent: #5 [open] E-X epic\n"),
            "{out}"
        );
        assert!(out.contains("\n\ndescription:\nBranch: wu/x/build\n"), "{out}");
        assert!(
            out.ends_with("comments (2):\n--- #1 c1 trawler-router\nask\n--- #2 c2 vuluu\nok"),
            "{out}"
        );
        assert!(detail(&task(), &[]).ends_with("description: (none)\n\ncomments (0):"));
    }

    #[test]
    fn put_flags_parse_in_any_order_and_contradictions_are_refused() {
        let Command::Put(id, change) = parse(&[
            "put",
            "#7",
            "--label",
            "needs-owner",
            "--assignee",
            "vuluu",
            "--comment",
            "ask",
            "--relation",
            "blocked-by:6",
            "--done",
            "--unlabel",
            "a",
            "--unassign",
            "b",
        ])
        .unwrap() else {
            panic!("not a put")
        };
        assert_eq!(id, 7);
        assert_eq!(
            change,
            Put {
                comment: Some("ask".into()),
                description: None,
                relations: vec![("blocked", 6)],
                done: Some(true),
                assign: vec!["vuluu".into()],
                label: vec!["needs-owner".into()],
                unlabel: vec!["a".into()],
                unassign: vec!["b".into()],
            }
        );
        assert!(parse(&["put", "7", "--done", "--undone"]).unwrap_err().contains("contradict"));
        let both = parse(&["put", "7", "--label", "x", "--unlabel", "x"]).unwrap_err();
        assert_eq!(both, "put: x is both added and removed");
        assert!(parse(&["put", "7", "--assignee", "vuluu", "--unassign", "vuluu"]).is_err());
        let Command::Put(_, twice) = parse(&["put", "7", "--label", "x", "--label", "x"]).unwrap()
        else {
            panic!("not a put")
        };
        assert_eq!(twice.label, ["x"]);
        assert!(parse(&["put", "7", "--comment"]).unwrap_err().contains("--comment needs a value"));
        assert!(parse(&["put", "7", "--title", "t"])
            .unwrap_err()
            .starts_with("put: unexpected --title"));
        assert!(parse(&["put"]).unwrap_err().starts_with("put needs a task id"));
        assert!(parse(&["put", "x"]).unwrap_err().contains("expected a task id"));
    }

    #[test]
    fn a_done_task_gets_the_hidden_note_unless_this_call_marks_it_done() {
        let mut done = task();
        done["done"] = json!(true);
        let note = hidden_while_done(7, &done, &Put::default()).unwrap();
        assert!(note.starts_with("#7: note - this task is done"), "{note}");
        let marking_done = Put { done: Some(true), ..Put::default() };
        assert_eq!(hidden_while_done(7, &done, &marking_done), None);
        assert_eq!(hidden_while_done(7, &task(), &Put::default()), None);
    }

    #[test]
    fn show_and_create_flags_parse() {
        assert_eq!(parse(&["show"]).unwrap(), Command::Show { project: 3, needs_owner: false });
        assert_eq!(
            parse(&["show", "--needs-owner", "--project", "8"]).unwrap(),
            Command::Show { project: 8, needs_owner: true }
        );
        assert!(parse(&["show", "--needs-owner", "x"]).is_err());
        assert_eq!(
            parse(&[
                "create",
                "--title",
                "E-X.W2 t",
                "--parent",
                "5",
                "--relation",
                "blocked-by:6"
            ])
            .unwrap(),
            Command::Create(Create {
                project: 3,
                title: "E-X.W2 t".into(),
                description: None,
                parent: Some(5),
                relations: vec![("blocked", 6)],
            })
        );
        assert!(parse(&["create", "--parent", "5"])
            .unwrap_err()
            .starts_with("create needs --title"));
        assert_eq!(parse(&["read", "4"]).unwrap(), Command::Read(4));
        assert_eq!(parse(&["init"]).unwrap(), Command::Init);
        assert!(parse(&["init", "--project", "3"]).is_err());
    }

    #[test]
    fn a_second_stdin_flag_is_refused_before_reading_it() {
        let mut taken = true;
        assert!(text("-", &mut taken).unwrap_err().contains("only one flag"));
        assert_eq!(text("plain", &mut taken).unwrap(), "plain");
    }

    #[test]
    fn an_unknown_command_prints_usage_before_reading_any_token() {
        assert_eq!(run(&["--key-file", "/nonexistent/key"]).unwrap_err(), USAGE);
        assert_eq!(run(&["bogus", "--key-file", "/nonexistent/key"]).unwrap_err(), USAGE);
        assert!(run(&["--api", "nowhere", "init"]).unwrap_err().contains("expected <ip:port>"));
    }

    #[test]
    fn an_auth_refusal_points_at_the_token() {
        assert_eq!(
            refusal("GET", "/x", 401, " {\"code\":11}\n"),
            "GET /x: HTTP 401 - check the router token and its permissions: {\"code\":11}"
        );
        assert_eq!(refusal("PUT", "/y", 500, "boom"), "PUT /y: HTTP 500: boom");
    }
}
