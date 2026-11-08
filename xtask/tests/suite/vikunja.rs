use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tempfile::NamedTempFile;
use xtask::vikunja;

const TOKEN: &str = "test-token";

type Call = (String, String, Option<Value>);

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    target: String,
    body: Option<Value>,
}

struct Reply {
    status: u16,
    body: String,
    pages: Option<u32>,
}

struct Fake {
    routes: BTreeMap<(String, String), Reply>,
    requests: Vec<Request>,
}

struct Server {
    api: String,
    key: NamedTempFile,
    state: Arc<Mutex<Fake>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = listener.local_addr().unwrap().to_string();
        let state = Arc::new(Mutex::new(Fake { routes: BTreeMap::new(), requests: Vec::new() }));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                serve(stream.unwrap(), &shared);
            }
        });
        let mut key = NamedTempFile::new().unwrap();
        writeln!(key, "{TOKEN}").unwrap();
        Self { api, key, state }
    }

    fn route(&self, method: &str, target: &str, status: u16, body: &Value) -> &Self {
        self.paged(method, target, status, body, None)
    }

    fn paged(
        &self,
        method: &str,
        target: &str,
        status: u16,
        body: &Value,
        pages: Option<u32>,
    ) -> &Self {
        let reply = Reply { status, body: body.to_string(), pages };
        self.state
            .lock()
            .unwrap()
            .routes
            .insert((method.into(), format!("/api/v1{target}")), reply);
        self
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let key = self.key.path().to_str().unwrap();
        let mut all = vec!["--api", self.api.as_str(), "--key-file", key];
        all.extend_from_slice(args);
        vikunja::run(&all).map(|(code, out)| {
            assert_eq!(code, 0);
            out
        })
    }

    fn requests(&self) -> Vec<Call> {
        let requests = self.state.lock().unwrap().requests.clone();
        requests.into_iter().map(|r| (r.method, r.target, r.body)).collect()
    }

    fn writes(&self) -> Vec<Call> {
        self.requests().into_iter().filter(|(method, ..)| method != "GET").collect()
    }
}

fn serve(mut stream: TcpStream, state: &Mutex<Fake>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        headers.insert(name.to_ascii_lowercase(), value.trim().to_string());
    }
    let length: usize = headers.get("content-length").map_or(0, |v| v.parse().unwrap());
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap().to_string();
    let target = parts.next().unwrap().to_string();
    let body = (!body.is_empty()).then(|| serde_json::from_slice(&body).unwrap());
    if body.is_some() {
        assert_eq!(headers.get("content-type").map(String::as_str), Some("application/json"));
    }

    let (status, payload, pages) = {
        let mut fake = state.lock().unwrap();
        fake.requests.push(Request { method: method.clone(), target: target.clone(), body });
        if headers.get("authorization").map(String::as_str) == Some(&format!("Bearer {TOKEN}")) {
            match fake.routes.get(&(method, target)) {
                Some(reply) => (reply.status, reply.body.clone(), reply.pages),
                None => (404, r#"{"code":4002,"message":"no route"}"#.to_string(), None),
            }
        } else {
            (401, r#"{"code":11,"message":"invalid token"}"#.to_string(), None)
        }
    };
    let pages_line =
        pages.map(|n| format!("X-Pagination-Total-Pages: {n}\r\n")).unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{pages_line}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn get(target: &str) -> Call {
    ("GET".into(), format!("/api/v1{target}"), None)
}

fn write(method: &str, target: &str, body: Option<Value>) -> Call {
    (method.into(), format!("/api/v1{target}"), body)
}

const OPEN_IN_3: &str =
    "/tasks?filter=done%20%3D%20false%20%26%26%20project%20%3D%203&sort_by=id&order_by=asc";

fn needs_owner_label() -> Value {
    json!({"id": 1, "title": "needs-owner"})
}

fn task(id: u64, assignees: &Value, labels: &Value) -> Value {
    json!({
        "id": id, "title": format!("E-X.W{id} work"), "description": "Branch: wu/x/w", "done": false,
        "project_id": 3, "priority": 2, "hex_color": "e8590c", "percent_done": 0.5,
        "assignees": assignees, "labels": labels,
        "related_tasks": {"parenttask": [{"id": 5, "title": "E-X epic", "done": false}]},
        "created_by": {"id": 2, "username": "trawler-router"}
    })
}

fn owner() -> Value {
    json!([{"id": 1, "username": "vuluu"}])
}

#[test]
fn init_reads_the_token_file_and_names_the_project() {
    let server = Server::start();
    server.route("GET", "/projects/3", 200, &json!({"id": 3, "title": "Trawler Router"}));
    let out = server.run(&["init"]).unwrap();
    assert!(out.starts_with("ok: the token at "), "{out}");
    assert!(
        out.ends_with(&format!(
            "reaches http://{}/api/v1 - project 3 \"Trawler Router\"",
            server.api
        )),
        "{out}"
    );
    assert_eq!(server.requests(), [get("/projects/3")]);
}

#[test]
fn init_without_a_token_file_or_with_a_refused_token_fails_saying_why() {
    let server = Server::start();
    let missing =
        vikunja::run(&["--api", &server.api, "--key-file", "/nonexistent/vikunja.key", "init"]);
    assert!(missing
        .unwrap_err()
        .starts_with("cannot read the router token at /nonexistent/vikunja.key"));
    assert_eq!(server.requests(), Vec::<Call>::new());

    let wrong = NamedTempFile::new().unwrap();
    std::fs::write(wrong.path(), "other-token").unwrap();
    let key = wrong.path().to_str().unwrap();
    let refused = vikunja::run(&["--api", &server.api, "--key-file", key, "init"]).unwrap_err();
    assert_eq!(
        refused,
        r#"GET /projects/3: HTTP 401 - check the router token and its permissions: {"code":11,"message":"invalid token"}"#
    );
}

#[test]
fn show_lists_the_open_tasks_of_every_page() {
    let server = Server::start();
    let first = json!([task(6, &owner(), &json!([needs_owner_label()]))]);
    let second = json!([task(7, &json!(null), &json!(null))]);
    server.paged("GET", &format!("{OPEN_IN_3}&page=1"), 200, &first, Some(2));
    server.paged("GET", &format!("{OPEN_IN_3}&page=2"), 200, &second, Some(2));
    assert_eq!(
        server.run(&["show"]).unwrap(),
        "#6 [open] E-X.W6 work · assignees: vuluu · labels: needs-owner · parent: #5\n#7 [open] E-X.W7 work · parent: #5"
    );
    assert_eq!(
        server.requests(),
        [get(&format!("{OPEN_IN_3}&page=1")), get(&format!("{OPEN_IN_3}&page=2"))]
    );
}

#[test]
fn show_needs_owner_keeps_only_tasks_assigned_to_the_owner_and_labelled() {
    let server = Server::start();
    let tasks = json!([
        task(6, &owner(), &json!([needs_owner_label()])),
        task(7, &owner(), &json!(null)),
        task(8, &json!(null), &json!([needs_owner_label()])),
    ]);
    server.route("GET", &format!("{OPEN_IN_3}&page=1"), 200, &tasks);
    let out = server.run(&["show", "--needs-owner"]).unwrap();
    assert_eq!(out, "#6 [open] E-X.W6 work · assignees: vuluu · labels: needs-owner · parent: #5");

    let other = "/tasks?filter=done%20%3D%20false%20%26%26%20project%20%3D%209&sort_by=id&order_by=asc&page=1";
    server.route("GET", other, 200, &json!([]));
    assert_eq!(
        server.run(&["show", "--project", "9", "--needs-owner"]).unwrap(),
        "nothing needs vuluu in project 9"
    );
    assert_eq!(server.run(&["show", "--project", "9"]).unwrap(), "no open tasks in project 9");
}

#[test]
fn read_prints_relations_description_and_every_comment_in_order() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &owner(), &json!([needs_owner_label()])));
    let comment = |id: u64, who: &str, text: &str| json!({"id": id, "comment": text, "author": {"username": who}, "created": format!("2026-11-08T10:0{id}:00Z")});
    server.paged(
        "GET",
        "/tasks/6/comments?order_by=asc&page=1",
        200,
        &json!([comment(1, "trawler-router", "the ask")]),
        Some(2),
    );
    server.paged(
        "GET",
        "/tasks/6/comments?order_by=asc&page=2",
        200,
        &json!([comment(2, "vuluu", "go")]),
        Some(2),
    );
    assert_eq!(
        server.run(&["read", "6"]).unwrap(),
        "#6 [open] E-X.W6 work · assignees: vuluu · labels: needs-owner · parent: #5\n\
         parent: #5 [open] E-X epic\n\n\
         description:\nBranch: wu/x/w\n\n\
         comments (2):\n\
         --- #1 2026-11-08T10:01:00Z trawler-router\nthe ask\n\
         --- #2 2026-11-08T10:02:00Z vuluu\ngo"
    );
}

#[test]
fn a_refused_read_is_an_error_naming_the_status() {
    let server = Server::start();
    let err = server.run(&["read", "404"]).unwrap_err();
    assert_eq!(err, r#"GET /tasks/404: HTTP 404: {"code":4002,"message":"no route"}"#);
}

#[test]
fn put_raises_an_ask_comment_then_assignee_then_label_whatever_the_flag_order() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &json!(null), &json!(null)));
    server.route("GET", "/labels?page=1", 200, &json!([needs_owner_label()]));
    server.route("PUT", "/tasks/6/comments", 201, &json!({"id": 3}));
    server.route("PUT", "/tasks/6/assignees", 201, &json!({"user_id": 1}));
    server.route("PUT", "/tasks/6/labels", 201, &json!({"label_id": 1}));
    let out = server
        .run(&[
            "put",
            "6",
            "--label",
            "needs-owner",
            "--assignee",
            "vuluu",
            "--comment",
            "Ask: sign W2",
        ])
        .unwrap();
    assert_eq!(out, "#6: commented\n#6: assigned vuluu\n#6: labelled needs-owner");
    assert_eq!(
        server.requests(),
        [
            get("/tasks/6"),
            get("/labels?page=1"),
            write("PUT", "/tasks/6/comments", Some(json!({"comment": "Ask: sign W2"}))),
            write("PUT", "/tasks/6/assignees", Some(json!({"user_id": 1}))),
            write("PUT", "/tasks/6/labels", Some(json!({"label_id": 1}))),
        ]
    );
}

#[test]
fn put_closes_an_ask_label_off_before_the_assignee() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &owner(), &json!([needs_owner_label()])));
    server.route("DELETE", "/tasks/6/labels/1", 200, &json!({"message": "Successfully deleted."}));
    server.route(
        "DELETE",
        "/tasks/6/assignees/1",
        200,
        &json!({"message": "Successfully deleted."}),
    );
    let out = server.run(&["put", "6", "--unassign", "vuluu", "--unlabel", "needs-owner"]).unwrap();
    assert_eq!(out, "#6: unlabelled needs-owner\n#6: unassigned vuluu");
    assert_eq!(
        server.writes(),
        [write("DELETE", "/tasks/6/labels/1", None), write("DELETE", "/tasks/6/assignees/1", None)]
    );
}

#[test]
fn put_done_writes_back_the_whole_task_it_read() {
    let server = Server::start();
    let before = task(6, &owner(), &json!([needs_owner_label()]));
    server.route("GET", "/tasks/6", 200, &before);
    server.route("POST", "/tasks/6", 200, &json!({"id": 6}));
    let out = server.run(&["put", "6", "--done", "--description", "Branch: wu/x/y"]).unwrap();
    assert_eq!(out, "#6: marked done, description set (whole task written)");
    let mut expected = before;
    expected["done"] = json!(true);
    expected["description"] = json!("Branch: wu/x/y");
    assert_eq!(server.writes(), [write("POST", "/tasks/6", Some(expected))]);
}

#[test]
fn put_skips_what_already_holds_and_writes_nothing() {
    let server = Server::start();
    let mut done = task(6, &owner(), &json!([needs_owner_label()]));
    done["done"] = json!(true);
    done["related_tasks"]["blocking"] = json!([{"id": 9, "title": "later", "done": false}]);
    server.route("GET", "/tasks/6", 200, &done);
    server.route("GET", "/labels?page=1", 200, &json!([needs_owner_label()]));
    let out = server
        .run(&[
            "put",
            "6",
            "--done",
            "--assignee",
            "vuluu",
            "--label",
            "needs-owner",
            "--unlabel",
            "other",
            "--unassign",
            "trawler-router",
            "--relation",
            "blocks:9",
            "--description",
            "Branch: wu/x/w",
        ])
        .unwrap();
    assert_eq!(
        out,
        "#6: relation blocking #9 already there - skipped\n\
         #6: already done - skipped\n\
         #6: vuluu already assigned - skipped\n\
         #6: already labelled needs-owner - skipped\n\
         #6: not labelled other - skipped\n\
         #6: trawler-router not assigned - skipped"
    );
    assert_eq!(server.writes(), Vec::<Call>::new());
}

#[test]
fn put_relations_use_vikunja_s_kinds_and_skip_one_already_there() {
    let server = Server::start();
    let mut related = task(6, &json!(null), &json!(null));
    related["related_tasks"]["blocked"] = json!([{"id": 3, "title": "earlier", "done": true}]);
    server.route("GET", "/tasks/6", 200, &related);
    server.route("PUT", "/tasks/6/relations", 201, &json!({"task_id": 6}));
    let out = server
        .run(&[
            "put",
            "6",
            "--relation",
            "blocked-by:3",
            "--relation",
            "blocked-by:4",
            "--relation",
            "blocks:9",
        ])
        .unwrap();
    assert_eq!(
        out,
        "#6: relation blocked #3 already there - skipped\n#6: relation blocked #4\n#6: relation blocking #9"
    );
    assert_eq!(
        server.writes(),
        [
            write(
                "PUT",
                "/tasks/6/relations",
                Some(json!({"other_task_id": 4, "relation_kind": "blocked"}))
            ),
            write(
                "PUT",
                "/tasks/6/relations",
                Some(json!({"other_task_id": 9, "relation_kind": "blocking"}))
            ),
        ]
    );
}

#[test]
fn put_creates_a_missing_label_and_says_so_loudly() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &json!(null), &json!(null)));
    server.route("GET", "/labels?page=1", 200, &json!([needs_owner_label()]));
    server.route("PUT", "/labels", 201, &json!({"id": 9, "title": "waiting-ci"}));
    server.route("PUT", "/tasks/6/labels", 201, &json!({"label_id": 9}));
    let out = server.run(&["put", "6", "--label", "waiting-ci"]).unwrap();
    assert!(
        out.starts_with("CREATED LABEL \"waiting-ci\" (id 9): no label had that name"),
        "{out}"
    );
    assert!(out.ends_with("\n#6: labelled waiting-ci"), "{out}");
    assert_eq!(
        server.writes(),
        [
            write("PUT", "/labels", Some(json!({"title": "waiting-ci"}))),
            write("PUT", "/tasks/6/labels", Some(json!({"label_id": 9}))),
        ]
    );
}

#[test]
fn a_put_refused_midway_reports_what_it_already_did() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &json!(null), &json!(null)));
    server.route("PUT", "/tasks/6/comments", 201, &json!({"id": 3}));
    server.route("PUT", "/tasks/6/assignees", 500, &json!({"message": "boom"}));
    let err = server.run(&["put", "6", "--comment", "c", "--assignee", "vuluu"]).unwrap_err();
    assert_eq!(
        err,
        "#6: commented\nthen stopped: PUT /tasks/6/assignees: HTTP 500: {\"message\":\"boom\"}"
    );
}

#[test]
fn put_refuses_an_unknown_user_and_an_empty_change_before_any_call() {
    let server = Server::start();
    let err = server.run(&["put", "6", "--comment", "c", "--assignee", "nobody"]).unwrap_err();
    assert!(err.starts_with("unknown user \"nobody\""), "{err}");
    assert!(server.run(&["put", "6"]).unwrap_err().starts_with("put #6: nothing to do"));
    assert_eq!(server.requests(), Vec::<Call>::new());
}

#[test]
fn create_makes_the_task_then_its_parent_and_blocking_relations() {
    let server = Server::start();
    server.route("PUT", "/projects/3/tasks", 201, &json!({"id": 12, "title": "E-X.W2 next"}));
    server.route("PUT", "/tasks/5/relations", 201, &json!({"task_id": 5}));
    server.route("PUT", "/tasks/12/relations", 201, &json!({"task_id": 12}));
    let out = server
        .run(&[
            "create",
            "--title",
            "E-X.W2 next",
            "--description",
            "Branch: wu/x/next",
            "--parent",
            "5",
            "--relation",
            "blocked-by:11",
        ])
        .unwrap();
    assert_eq!(out, "created #12 \"E-X.W2 next\" in project 3\n#5: relation subtask #12\n#12: relation blocked #11");
    assert_eq!(
        server.writes(),
        [
            write(
                "PUT",
                "/projects/3/tasks",
                Some(json!({"title": "E-X.W2 next", "description": "Branch: wu/x/next"}))
            ),
            write(
                "PUT",
                "/tasks/5/relations",
                Some(json!({"other_task_id": 12, "relation_kind": "subtask"}))
            ),
            write(
                "PUT",
                "/tasks/12/relations",
                Some(json!({"other_task_id": 11, "relation_kind": "blocked"}))
            ),
        ]
    );
}

#[test]
fn a_create_whose_relation_fails_still_names_the_task_it_made() {
    let server = Server::start();
    server.route("PUT", "/projects/8/tasks", 201, &json!({"id": 12}));
    let err =
        server.run(&["create", "--project", "8", "--title", "t", "--parent", "5"]).unwrap_err();
    assert!(
        err.starts_with(
            "created #12 \"t\" in project 8\nthen stopped: PUT /tasks/5/relations: HTTP 404"
        ),
        "{err}"
    );
}

#[test]
fn put_unassigns_a_user_named_by_numeric_id() {
    let server = Server::start();
    server.route("GET", "/tasks/6", 200, &task(6, &owner(), &json!(null)));
    server.route(
        "DELETE",
        "/tasks/6/assignees/1",
        200,
        &json!({"message": "Successfully deleted."}),
    );
    assert_eq!(server.run(&["put", "6", "--unassign", "1"]).unwrap(), "#6: unassigned 1");
    assert_eq!(server.writes(), [write("DELETE", "/tasks/6/assignees/1", None)]);
}
