//! The reach contract (E-ONBOARD.W1, DEC-55): Trawler answers only this machine
//! until something arms it, and what it reports once armed is an address that
//! actually works.
//!
//! The "second device" is a second listener on loopback. A real interface would
//! put this suite on the network it is forbidden to touch, and the property
//! under test — a bind beside the first one, reported and reachable — is the
//! same either way.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use serde_json::Value;
use tempfile::{tempdir, TempDir};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use trawler::{serve, Config, Reach, RunningServer};

const LOOPBACK: &str = "127.0.0.1";

/// Parks until the test releases it, holding an exchange open through whatever
/// proxy reached it.
async fn hold(State(release): State<Arc<Notify>>) -> &'static str {
    release.notified().await;
    "released"
}

struct Harness {
    origin: SocketAddr,
    api_addr: SocketAddr,
    client: reqwest::Client,
    /// Releases whatever is parked on the origin's `/hold` route, so a test can
    /// keep an exchange open through the proxy for exactly as long as it means
    /// to — a signal, never a sleep.
    release: Arc<Notify>,
    server: Option<RunningServer>,
    _ca_dir: TempDir,
    _origin: JoinHandle<()>,
}

impl Harness {
    async fn boot(reach: Reach) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = listener.local_addr().unwrap();
        let release = Arc::new(Notify::new());
        let app = Router::new()
            .route("/ok", get(|| async { "hello" }))
            .route("/hold", get(hold))
            .with_state(Arc::clone(&release));
        let origin_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let ca_dir = tempdir().unwrap();
        let config = Config {
            proxy_addr: "127.0.0.1:0".parse().unwrap(),
            api_addr: "127.0.0.1:0".parse().unwrap(),
            reach,
            ca_dir: ca_dir.path().to_path_buf(),
            ..Default::default()
        };
        let server = serve(config).await.unwrap();
        Self {
            origin,
            api_addr: server.api_addr,
            client: reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap(),
            release,
            server: Some(server),
            _ca_dir: ca_dir,
            _origin: origin_task,
        }
    }

    async fn local() -> Self {
        Self::boot(Reach::Local).await
    }

    fn server(&self) -> &RunningServer {
        self.server.as_ref().unwrap()
    }

    async fn reach_state(&self) -> Value {
        self.get_json(self.api_addr, "/reach").await
    }

    async fn arm(&self, address: &str) -> (reqwest::StatusCode, String) {
        let response = self
            .client
            .post(format!("http://{}/reach", self.api_addr))
            .header("content-type", "application/json")
            .body(serde_json::json!({ "address": address }).to_string())
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.text().await.unwrap())
    }

    async fn disarm(&self) -> Value {
        let body = self
            .client
            .delete(format!("http://{}/reach", self.api_addr))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        serde_json::from_str(&body).unwrap()
    }

    async fn get_json(&self, at: SocketAddr, path: &str) -> Value {
        let body = self
            .client
            .get(format!("http://{at}{path}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        serde_json::from_str(&body).unwrap()
    }

    /// A client that routes through the given proxy address — how a request
    /// "from another device" is made without a second device.
    fn through(proxy: SocketAddr) -> reqwest::Client {
        reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{proxy}")).unwrap())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }

    fn origin_url(&self, path: &str) -> String {
        format!("http://{}{}", self.origin, path)
    }

    /// Poll the capture until the flow for `path` is recorded — the request has
    /// then genuinely reached the proxy, which no sleep can promise.
    async fn wait_until_captured(&self, path: &str) {
        for attempt in 0..200 {
            let flows = self.get_json(self.api_addr, &format!("/flows?q={path}")).await;
            if !flows.as_array().unwrap().is_empty() {
                return;
            }
            assert!(attempt < 199, "the proxy never saw {path}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn shutdown(mut self) {
        self.release.notify_waiters();
        if let Some(server) = self.server.take() {
            server.shutdown().await;
        }
    }
}

fn socket(value: &Value) -> SocketAddr {
    value.as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn a_fresh_core_answers_only_this_machine() {
    let h = Harness::local().await;

    assert!(h.server().proxy_addr.ip().is_loopback());
    assert!(h.server().api_addr.ip().is_loopback());
    assert_eq!(h.server().reachable_at().await, None);

    let state = h.reach_state().await;
    assert_eq!(state["armed"], false);
    assert!(state["proxy"].is_null(), "an unarmed core has no address to give");
    assert!(state["api"].is_null());
    assert!(state["ca_urls"].is_null());
    assert!(state["wifi_page"].is_null());
    assert!(state["interfaces"].is_array(), "the choices are listed before anything is armed");

    h.shutdown().await;
}

#[tokio::test]
async fn arming_adds_a_proxy_and_an_api_that_answer_at_the_reported_address() {
    let h = Harness::local().await;

    let (status, body) = h.arm(LOOPBACK).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let armed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(armed["armed"], true);

    let reach_proxy = socket(&armed["proxy"]);
    let reach_api = socket(&armed["api"]);
    assert_ne!(
        reach_proxy,
        h.server().proxy_addr,
        "reach binds beside the local pair, not over it"
    );
    assert_ne!(reach_api, h.server().api_addr);
    assert_eq!(
        armed["ca_urls"],
        serde_json::json!({
            "pem": format!("http://{reach_api}/ca.pem"),
            "cer": format!("http://{reach_api}/ca.cer"),
            "mobileconfig": format!("http://{reach_api}/ca.mobileconfig"),
        }),
        "the certificate URLs name the API socket that actually answers"
    );

    let through_reach = Harness::through(reach_proxy);
    let fetched = through_reach.get(h.origin_url("/ok")).send().await.unwrap();
    assert_eq!(fetched.text().await.unwrap(), "hello");

    for format in ["pem", "cer", "mobileconfig"] {
        let certificate =
            h.client.get(armed["ca_urls"][format].as_str().unwrap()).send().await.unwrap();
        assert_eq!(certificate.status(), reqwest::StatusCode::OK, "{format}");
        assert!(!certificate.bytes().await.unwrap().is_empty(), "reachable but no {format}");
    }

    h.shutdown().await;
}

#[tokio::test]
async fn the_wifi_page_answers_on_the_reach_api_and_names_the_reach_proxy() {
    let h = Harness::local().await;
    let (_, body) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&body).unwrap();
    let reach_api = socket(&armed["api"]);
    let reach_proxy = socket(&armed["proxy"]);
    assert_eq!(armed["wifi_page"], format!("http://{reach_api}/android-wifi"));

    let page = h.client.get(armed["wifi_page"].as_str().unwrap()).send().await.unwrap();
    assert_eq!(page.status(), reqwest::StatusCode::OK);
    assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
    let html = page.text().await.unwrap();
    assert!(html.contains(&format!(
        "<dt>Proxy hostname</dt><dd>{}</dd><dt>Proxy port</dt><dd>{}</dd>",
        reach_proxy.ip(),
        reach_proxy.port()
    )));
    assert!(html.contains("intent:#Intent;action=android.settings.WIFI_SETTINGS;end"));

    h.shutdown().await;
}

#[tokio::test]
async fn traffic_from_the_reachable_proxy_lands_in_the_same_capture() {
    let h = Harness::local().await;
    let (_, body) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&body).unwrap();
    let reach_proxy = socket(&armed["proxy"]);

    Harness::through(reach_proxy).get(h.origin_url("/ok")).send().await.unwrap();

    for attempt in 0..100 {
        let flows = h.get_json(h.api_addr, "/flows?q=/ok").await;
        if !flows.as_array().unwrap().is_empty() {
            h.shutdown().await;
            return;
        }
        assert!(attempt < 99);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    unreachable!()
}

#[tokio::test]
async fn every_address_this_machine_holds_is_not_an_address_a_user_named() {
    let h = Harness::local().await;

    for unspecified in ["0.0.0.0", "::"] {
        let (status, body) = h.arm(unspecified).await;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{unspecified} was accepted");
        assert!(body.contains("one interface address"), "{body}");
    }
    assert_eq!(h.server().reachable_at().await, None, "a refusal must leave nothing armed");
    assert_eq!(h.reach_state().await["armed"], false);

    h.shutdown().await;
}

#[tokio::test]
async fn an_address_this_machine_does_not_hold_is_refused_by_the_bind() {
    let h = Harness::local().await;

    let (status, body) = h.arm("192.0.2.1").await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("could not listen on"), "{body}");
    assert_eq!(h.server().reachable_at().await, None);

    h.shutdown().await;
}

#[tokio::test]
async fn disarming_takes_the_reachable_pair_away_and_leaves_loopback_serving() {
    let h = Harness::local().await;
    let (_, body) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&body).unwrap();
    let reach_api = socket(&armed["api"]);

    let after = h.disarm().await;
    assert_eq!(after["armed"], false);
    assert!(after["proxy"].is_null());
    assert_eq!(h.server().reachable_at().await, None);

    let gone = h.client.get(format!("http://{reach_api}/stats")).send().await;
    assert!(gone.is_err(), "the reach API answered after it was disarmed");

    assert_eq!(h.reach_state().await["armed"], false, "loopback kept serving");

    h.shutdown().await;
}

#[tokio::test]
async fn arming_the_address_already_armed_leaves_that_session_untouched() {
    let h = Harness::local().await;
    let (_, first) = h.arm(LOOPBACK).await;
    let first: Value = serde_json::from_str(&first).unwrap();
    let first_api = socket(&first["api"]);

    let (status, second) = h.arm(LOOPBACK).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let second: Value = serde_json::from_str(&second).unwrap();

    assert_eq!(socket(&second["api"]), first_api, "the state asked for was already true");
    assert_eq!(socket(&second["proxy"]), socket(&first["proxy"]));
    assert_eq!(
        h.get_json(first_api, "/reach").await["armed"],
        true,
        "the device already routing through this address must not be dropped"
    );

    h.shutdown().await;
}

// The failure this guards is silent: a user confirmed an exposure, a phone is
// routing through it, and a second arm that never binds anything takes it away
// with nothing but a 400 to say so.
#[tokio::test]
async fn a_refused_re_arm_leaves_the_working_session_exactly_as_it_was() {
    let h = Harness::local().await;
    let (_, armed) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&armed).unwrap();
    let reach_api = socket(&armed["api"]);
    let reach_proxy = socket(&armed["proxy"]);

    let (status, body) = h.arm("192.0.2.1").await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");

    let still = h.reach_state().await;
    assert_eq!(still["armed"], true, "a refused re-arm disarmed a working session");
    assert_eq!(socket(&still["api"]), reach_api);
    assert_eq!(socket(&still["proxy"]), reach_proxy);
    assert_eq!(h.get_json(reach_api, "/reach").await["armed"], true, "the API stopped answering");
    let through = Harness::through(reach_proxy);
    assert_eq!(
        through.get(h.origin_url("/ok")).send().await.unwrap().text().await.unwrap(),
        "hello",
        "the proxy a device was routing through stopped answering"
    );

    h.shutdown().await;
}

#[tokio::test]
async fn the_cross_origin_grant_does_not_widen_with_the_bind() {
    let h = Harness::local().await;
    let (_, body) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&body).unwrap();
    let reach_api = socket(&armed["api"]);

    let headers_at = |at: SocketAddr| {
        let client = h.client.clone();
        async move {
            let response = client.get(format!("http://{at}/stats")).send().await.unwrap();
            [
                "access-control-allow-origin",
                "access-control-allow-methods",
                "access-control-allow-headers",
                "access-control-expose-headers",
            ]
            .map(|name| response.headers().get(name).map(|v| v.to_str().unwrap().to_owned()))
        }
    };

    assert_eq!(
        headers_at(reach_api).await,
        headers_at(h.api_addr).await,
        "arming may widen the bind, never the grant"
    );

    h.shutdown().await;
}

#[tokio::test]
async fn a_headless_core_can_boot_armed_and_gives_its_listeners_back_at_shutdown() {
    let h = Harness::boot(Reach::Interface(LOOPBACK.parse().unwrap())).await;

    let armed = h.server().reachable_at().await.expect("a configured reach is armed at boot");
    assert_eq!(h.reach_state().await["armed"], true);
    assert_eq!(h.get_json(armed.api, "/reach").await["armed"], true);

    h.shutdown().await;

    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();
    assert!(
        client.get(format!("http://{}/stats", armed.api)).send().await.is_err(),
        "an opt-in must not outlive the process it was confirmed for"
    );
}

// The lock that serialises arming must not span the old session's drain. A
// device's open exchange keeps the outgoing proxy task pending for as long as
// it likes — seconds, measurably — and while that drains nobody may be kept
// from reading reach's state, least of all the user revoking the exposure,
// which is the one thing DEC-55 promises stays available.
#[tokio::test]
async fn a_draining_disarm_never_blocks_a_reader_of_the_state() {
    let h = Harness::local().await;
    let (_, armed) = h.arm(LOOPBACK).await;
    let armed: Value = serde_json::from_str(&armed).unwrap();
    let reach_proxy = socket(&armed["proxy"]);

    let held = tokio::spawn({
        let through = Harness::through(reach_proxy);
        let url = h.origin_url("/hold");
        async move { through.get(url).send().await.unwrap().text().await.unwrap() }
    });
    h.wait_until_captured("/hold").await;

    let disarming = tokio::spawn({
        let client = h.client.clone();
        let url = format!("http://{}/reach", h.api_addr);
        async move { client.delete(url).send().await.unwrap().status() }
    });

    // Polled rather than read once: the disarm takes the lock a few
    // milliseconds after it is spawned, so a single read can finish before it
    // and prove nothing. Every read inside the drain window has to answer.
    let window = tokio::time::Instant::now() + Duration::from_secs(1);
    while tokio::time::Instant::now() < window {
        let read = tokio::time::timeout(Duration::from_millis(300), h.reach_state()).await;
        assert!(read.is_ok(), "reading /reach blocked behind a disarm draining an open exchange");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    h.release.notify_waiters();
    assert_eq!(held.await.unwrap(), "released", "the open exchange was cut off by the disarm");
    assert_eq!(disarming.await.unwrap(), reqwest::StatusCode::OK);
    assert_eq!(h.server().reachable_at().await, None);

    h.shutdown().await;
}
