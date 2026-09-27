// The API front the walkthrough points the window at. Every route forwards to
// the real core, SSE streamed, except `/reach`: the core would report this
// machine's real interfaces and arming would bind a LAN address, so the front
// answers it from a script in the core's own shape. The front can also answer
// slowly or not at all, which is how a launch meets a core that is checking or
// down.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use trawler::reach::{Addresses, CaUrls};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Up,
    Slow(Duration),
    Down,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub address: IpAddr,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReachView {
    armed: bool,
    proxy: Option<String>,
    api: Option<String>,
    ca_urls: Option<CaUrls>,
    interfaces: Vec<Interface>,
}

#[derive(Default)]
struct Script {
    mode: Mode,
    interfaces: Vec<Interface>,
    armed_on: Option<IpAddr>,
}

pub struct Front {
    core: String,
    proxy_port: u16,
    api_port: u16,
    client: reqwest::Client,
    script: Mutex<Script>,
}

impl Front {
    pub fn new(core: SocketAddr, proxy_port: u16) -> Self {
        Self {
            core: format!("http://{core}"),
            proxy_port,
            api_port: core.port(),
            client: reqwest::Client::new(),
            script: Mutex::new(Script::default()),
        }
    }

    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.script.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_mode(&self, mode: Mode) {
        self.script().mode = mode;
    }

    pub fn set_interfaces(&self, interfaces: Vec<Interface>) {
        self.script().interfaces = interfaces;
    }

    fn mode(&self) -> Mode {
        self.script().mode
    }

    fn reach_view(&self) -> ReachView {
        let script = self.script();
        let at = |port: u16| script.armed_on.map(|ip| SocketAddr::new(ip, port).to_string());
        ReachView {
            armed: script.armed_on.is_some(),
            proxy: at(self.proxy_port),
            api: at(self.api_port),
            ca_urls: script.armed_on.map(|ip| {
                Addresses {
                    proxy: SocketAddr::new(ip, self.proxy_port),
                    api: SocketAddr::new(ip, self.api_port),
                }
                .ca_urls()
            }),
            interfaces: script.interfaces.clone(),
        }
    }

    fn arm(&self, address: IpAddr) -> Result<ReachView, String> {
        if !self.script().interfaces.iter().any(|i| i.address == address) {
            return Err(format!("{address} is not an address this machine holds"));
        }
        self.script().armed_on = Some(address);
        Ok(self.reach_view())
    }

    fn disarm(&self) -> ReachView {
        self.script().armed_on = None;
        self.reach_view()
    }
}

#[derive(Deserialize)]
struct ArmBody {
    address: String,
}

fn cross_origin(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
    );
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
    response
}

async fn reach(State(front): State<Arc<Front>>, request: Request) -> Response {
    let response = match *request.method() {
        Method::GET => Json(front.reach_view()).into_response(),
        Method::DELETE => Json(front.disarm()).into_response(),
        Method::POST => {
            let body =
                axum::body::to_bytes(request.into_body(), 64 * 1024).await.unwrap_or_default();
            match serde_json::from_slice::<ArmBody>(&body)
                .map_err(|e| e.to_string())
                .and_then(|b| b.address.parse::<IpAddr>().map_err(|e| e.to_string()))
                .and_then(|address| front.arm(address))
            {
                Ok(view) => Json(view).into_response(),
                Err(refusal) => (StatusCode::BAD_REQUEST, refusal).into_response(),
            }
        }
        _ => StatusCode::NO_CONTENT.into_response(),
    };
    cross_origin(response)
}

const HOP_BY_HOP: [header::HeaderName; 3] =
    [header::CONNECTION, header::TRANSFER_ENCODING, header::HOST];

async fn forward(State(front): State<Arc<Front>>, request: Request) -> Response {
    if let Mode::Slow(delay) = front.mode() {
        if request.method() == Method::GET && request.uri().path() == "/flows" {
            tokio::time::sleep(delay).await;
        }
    }
    let (parts, body) = request.into_parts();
    let url = format!("{}{}", front.core, parts.uri.path_and_query().map_or("/", |p| p.as_str()));
    let Ok(body) = axum::body::to_bytes(body, 256 * 1024 * 1024).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let mut outgoing = front.client.request(parts.method, url).body(body);
    for (name, value) in &parts.headers {
        if !HOP_BY_HOP.contains(name) {
            outgoing = outgoing.header(name, value);
        }
    }
    let Ok(answer) = outgoing.send().await else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    let mut response = Response::builder().status(answer.status());
    for (name, value) in answer.headers() {
        if !HOP_BY_HOP.contains(name) {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::from_stream(answer.bytes_stream()))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn router(front: Arc<Front>) -> Router {
    Router::new().route("/reach", any(reach)).fallback(forward).with_state(front)
}

// A down core is a connection nobody answers, not an HTTP error: the app tells
// a refusal from a silence, and a 502 would be a refusal.
struct Gated {
    inner: TcpListener,
    front: Arc<Front>,
}

impl axum::serve::Listener for Gated {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((stream, addr)) = self.inner.accept().await else { continue };
            if self.front.mode() == Mode::Down {
                drop(stream);
                continue;
            }
            return (stream, addr);
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

pub async fn serve(front: Arc<Front>) -> SocketAddr {
    let inner = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = inner.local_addr().unwrap();
    let listener = Gated { inner, front: Arc::clone(&front) };
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(front)).await;
    });
    addr
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn front() -> Front {
        Front::new("127.0.0.1:7421".parse().unwrap(), 7420)
    }

    fn ethernet() -> Interface {
        Interface { name: "en0".into(), address: "192.168.1.23".parse().unwrap() }
    }

    #[test]
    fn an_unarmed_front_reports_the_scripted_interfaces_and_no_address() {
        let front = front();
        front.set_interfaces(vec![ethernet()]);
        assert_eq!(
            front.reach_view(),
            ReachView {
                armed: false,
                proxy: None,
                api: None,
                ca_urls: None,
                interfaces: vec![ethernet()]
            }
        );
    }

    #[test]
    fn arming_a_scripted_address_answers_in_the_cores_shape_on_the_cores_ports() {
        let front = front();
        front.set_interfaces(vec![ethernet()]);
        let view = front.arm("192.168.1.23".parse().unwrap()).unwrap();
        assert_eq!(view.proxy.as_deref(), Some("192.168.1.23:7420"));
        assert_eq!(view.api.as_deref(), Some("192.168.1.23:7421"));
        assert_eq!(
            view.ca_urls.map(|urls| urls.mobileconfig).as_deref(),
            Some("http://192.168.1.23:7421/ca.mobileconfig")
        );
        assert!(view.armed);
        assert!(!front.disarm().armed);
    }

    #[test]
    fn arming_an_address_the_script_does_not_hold_is_refused() {
        let front = front();
        assert!(front.arm("10.9.9.9".parse().unwrap()).unwrap_err().contains("10.9.9.9"));
        assert!(!front.reach_view().armed);
    }

    #[test]
    fn the_view_serialises_to_the_fields_the_app_reads() {
        let json = serde_json::to_value(front().reach_view()).unwrap();
        for field in ["armed", "proxy", "api", "ca_urls", "interfaces"] {
            assert!(json.get(field).is_some(), "missing {field}");
        }
    }

    #[tokio::test]
    async fn a_down_front_drops_the_connection_rather_than_answering() {
        let front = Arc::new(front());
        front.set_mode(Mode::Down);
        let addr = serve(Arc::clone(&front)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let _ = stream.write_all(b"GET /flows HTTP/1.1\r\nhost: x\r\n\r\n").await;
        let mut answer = Vec::new();
        let read = stream.read_to_end(&mut answer).await;
        assert!(read.is_err() || answer.is_empty(), "a down front answered: {answer:?}");
    }

    #[tokio::test]
    async fn an_up_front_answers_reach_itself() {
        let front = Arc::new(front());
        front.set_interfaces(vec![ethernet()]);
        let addr = serve(front).await;
        let view: serde_json::Value =
            reqwest::get(format!("http://{addr}/reach")).await.unwrap().json().await.unwrap();
        assert_eq!(view["interfaces"][0]["address"], "192.168.1.23");
        assert_eq!(view["armed"], false);
    }
}
