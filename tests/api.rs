#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tokio_stream::StreamExt;
use tower::ServiceExt;
use trawler::store::Store;

use common::{brotli, captured, gzip, FlowBuilder};

fn router_with(store: Arc<Store>) -> Router {
    trawler::api::router(store, Arc::new("--CA PEM--".to_string()))
}

async fn get(router: &Router, uri: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, headers, bytes)
}

async fn json(router: &Router, uri: &str) -> (StatusCode, Value) {
    let (status, _headers, body) = get(router, uri).await;
    let value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap() };
    (status, value)
}

#[tokio::test]
async fn list_flows_orders_and_filters() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(
        FlowBuilder::new()
            .method("POST")
            .uri("http://api.example.com/users")
            .req_header("Content-Type", "application/json")
            .status(201)
            .build(),
    );
    store.insert(
        FlowBuilder::new().method("GET").uri("http://cdn.other.com/img").status(200).build(),
    );
    let r = router_with(store);

    let (status, v) = json(&r, "/flows").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 2);
    assert_eq!(v[0]["method"], "GET"); // newest-first

    assert_eq!(json(&r, "/flows?method=post").await.1.as_array().unwrap().len(), 1);
    assert_eq!(json(&r, "/flows?status=200").await.1.as_array().unwrap().len(), 1);
    assert_eq!(json(&r, "/flows?host=example").await.1.as_array().unwrap().len(), 1);
    assert_eq!(json(&r, "/flows?q=users").await.1.as_array().unwrap().len(), 1);
    assert_eq!(json(&r, "/flows?content_type=json").await.1.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn status_filter_accepts_a_class_a_code_and_refuses_the_rest() {
    let store = Arc::new(Store::new(1_000_000, 64));
    for (uri, status) in [
        ("http://x/ok", 200),
        ("http://x/missing", 404),
        ("http://x/teapot", 418),
        ("http://x/boom", 500),
    ] {
        store.insert(FlowBuilder::new().uri(uri).status(status).build());
    }
    let r = router_with(store);

    let uris = |v: &Value| {
        let mut found: Vec<String> =
            v.as_array().unwrap().iter().map(|f| f["uri"].as_str().unwrap().to_string()).collect();
        found.sort();
        found
    };

    let (status, v) = json(&r, "/flows?status=4xx").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(uris(&v), vec!["http://x/missing", "http://x/teapot"]);

    let (status, v) = json(&r, "/flows?status=404").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(uris(&v), vec!["http://x/missing"]);

    for refused in ["9xx", "999", "abc", "40x"] {
        let (status, _, body) = get(&r, &format!("/flows?status={refused}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused} should be refused");
        let stated = String::from_utf8(body).unwrap();
        assert!(stated.contains("4xx"), "{refused} should be told the grammar, got {stated:?}");
    }
}

#[tokio::test]
async fn stats_reports_the_connection_tally() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let r = router_with(store.clone());

    assert_eq!(json(&r, "/stats").await.1["connections"], 0);
    store.note_connection("example.com:443".into());
    store.note_connection("other:443".into());
    assert_eq!(json(&r, "/stats").await.1["connections"], 2);
}

#[tokio::test]
async fn get_flow_ok_missing_and_malformed() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let flow = FlowBuilder::new().uri("http://x/y").status(200).build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (status, v) = json(&r, &format!("/flows/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["uri"], "http://x/y");
    assert_eq!(v["status"], 200);

    let (status, _, _) = get(&r, &format!("/flows/{}", uuid::Uuid::new_v4())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = get(&r, "/flows/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn body_endpoints_raw_decode_and_edges() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let gz = gzip(br#"{"a":1}"#);
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "application/json")
        .req_header("Content-Encoding", "gzip")
        .req_body(captured(&gz))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (status, headers, body) = get(&r, &format!("/flows/{id}/request/body")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body, gz);

    let (_s, _h, raw0) = get(&r, &format!("/flows/{id}/request/body?decode=0")).await;
    assert_eq!(raw0, gz);

    for flag in ["1", "true"] {
        let (_s, _h, decoded) = get(&r, &format!("/flows/{id}/request/body?decode={flag}")).await;
        let parsed: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(parsed["a"], 1);
    }

    let (status, _h, _b) = get(&r, &format!("/flows/{id}/response/body")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// The raw view's contract: a compressed body is readable without being
// re-rendered. `decode=source` reverses the transport encoding and stops —
// no pretty-print, so the bytes are what the server actually sent.
#[tokio::test]
async fn decode_source_decompresses_without_pretty_printing() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let minified = br#"{"a":1,"b":[2,3]}"#;
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "application/json")
        .req_header("Content-Encoding", "gzip")
        .req_body(captured(&gzip(minified)))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (status, _h, source) = get(&r, &format!("/flows/{id}/request/body?decode=source")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(source, minified, "source must be the served bytes, not a re-indented copy");

    let (_s, _h, decoded) = get(&r, &format!("/flows/{id}/request/body?decode=1")).await;
    assert!(decoded.len() > source.len(), "decoded pretty-prints, source does not");
}

// An uncompressed body has no transport encoding to reverse, so source and the
// stored bytes are the same thing — and neither is pretty-printed.
#[tokio::test]
async fn decode_source_leaves_an_uncompressed_body_alone() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let minified = br#"{"a":1}"#;
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "application/json")
        .req_body(captured(minified))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (_s, _h, source) = get(&r, &format!("/flows/{id}/request/body?decode=source")).await;
    assert_eq!(source, minified);
}

// Three views means an unrecognised value must still land somewhere defined —
// the stored bytes, the same as no parameter at all.
#[tokio::test]
async fn an_unknown_decode_value_serves_the_stored_bytes() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let gz = gzip(br#"{"a":1}"#);
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "application/json")
        .req_header("Content-Encoding", "gzip")
        .req_body(captured(&gz))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    for value in ["banana", "0", "false", ""] {
        let (_s, _h, body) = get(&r, &format!("/flows/{id}/request/body?decode={value}")).await;
        assert_eq!(body, gz, "decode={value} must serve the stored bytes");
    }
}

// A header that lies is the case that decides whether losing the stored-bytes
// view costs anything: decompression fails, falls back to its input, and raw
// shows exactly what it showed before this view existed.
#[tokio::test]
async fn decode_source_falls_back_to_the_stored_bytes_when_the_encoding_lies() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let not_really_gzip = b"plain text, no gzip here";
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "text/plain")
        .req_header("Content-Encoding", "gzip")
        .req_body(captured(not_really_gzip))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (_s, _h, source) = get(&r, &format!("/flows/{id}/request/body?decode=source")).await;
    assert_eq!(source, not_really_gzip, "a failed decompress serves its input, never an error");
}

#[tokio::test]
async fn decode_brotli_charset_and_xml_over_router() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let br = brotli(br#"{"a":1}"#);
    let latin1 = *b"caf\xe9";
    let flow = FlowBuilder::new()
        .req_header("Content-Type", "application/json")
        .req_header("Content-Encoding", "br")
        .req_body(captured(&br))
        .status(200)
        .resp_header("Content-Type", "application/xml; charset=ISO-8859-1")
        .resp_body(captured(&latin1))
        .build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (_s, _h, decoded) = get(&r, &format!("/flows/{id}/request/body?decode=1")).await;
    let parsed: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(parsed["a"], 1);

    let (_s, _h, resp) = get(&r, &format!("/flows/{id}/response/body?decode=1")).await;
    assert_eq!(String::from_utf8(resp).unwrap(), "café");
}

#[tokio::test]
async fn body_204_when_nothing_retained() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let flow = FlowBuilder::new().build();
    let id = flow.id;
    store.insert(flow);
    let r = router_with(store);

    let (status, _h, _b) = get(&r, &format!("/flows/{id}/request/body")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn ca_cert_download() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let r = trawler::api::router(store, Arc::new("PEMDATA".to_string()));

    let (status, headers, body) = get(&r, "/ca.crt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/x-x509-ca-cert");
    assert_eq!(headers["content-disposition"], "attachment; filename=\"trawler-ca.pem\"");
    assert_eq!(body, b"PEMDATA");
}

#[tokio::test]
async fn stream_signals_lag_instead_of_silent_gap() {
    let store = Arc::new(Store::new(1_000_000, 1));
    let r = router_with(store.clone());

    let req = Request::builder().uri("/stream").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(req).await.unwrap();
    let mut stream = res.into_body().into_data_stream();

    for i in 0..3 {
        store.insert(FlowBuilder::new().uri(&format!("http://flood/{i}")).build());
    }

    let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no SSE frame before timeout")
        .expect("stream ended")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame);
    assert!(text.contains("event: lagged"), "got: {text}");
    assert!(text.contains("\"skipped\":2"), "got: {text}");
}

#[tokio::test]
async fn ui_shell_page_and_the_request_sequence_it_performs() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let r = router_with(store.clone());

    let (status, headers, body) = get(&r, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    let html = String::from_utf8(body).unwrap();
    for wired in ["fetch('/flows?'", "new EventSource('/stream')"] {
        assert!(html.contains(wired), "page must wire {wired}");
    }

    let req = Request::builder().uri("/stream").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(req).await.unwrap();
    let mut stream = res.into_body().into_data_stream();

    store.insert(FlowBuilder::new().uri("http://shop.example.com/cart").status(200).build());

    assert_eq!(json(&r, "/flows?host=shop.example.com").await.1.as_array().unwrap().len(), 1);
    assert_eq!(json(&r, "/flows?host=elsewhere").await.1.as_array().unwrap().len(), 0);

    let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no SSE frame before timeout")
        .expect("stream ended")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame);
    assert!(text.contains("event: request"), "the page's live-refetch trigger, got: {text}");
}

#[tokio::test]
async fn stream_delivers_live_event() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let r = router_with(store.clone());

    let req = Request::builder().uri("/stream").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let mut stream = res.into_body().into_data_stream();

    store.insert(FlowBuilder::new().method("GET").uri("http://live/").build());

    let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no SSE frame before timeout")
        .expect("stream ended")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame);
    assert!(text.contains("event: request"), "got: {text}");
    assert!(text.contains("http://live/"), "got: {text}");
}

// The native app is a cross-origin client of the API, so listings, the stream,
// and a preflight all carry the open CORS grant — without it the webview's
// fetch/EventSource are blocked and UC-4 never populates. Headers only for
// /stream: its body never ends, so draining it would hang.
#[tokio::test]
async fn cross_origin_grant_rides_every_response() {
    let r = router_with(Arc::new(Store::new(1_000_000, 64)));

    for uri in ["/flows", "/stream"] {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let res = r.clone().oneshot(req).await.unwrap();
        assert_eq!(res.headers()["access-control-allow-origin"], "*", "no grant on {uri}");
    }

    let preflight = Request::builder().method("OPTIONS").uri("/flows").body(Body::empty()).unwrap();
    let res = r.oneshot(preflight).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(res.headers()["access-control-allow-origin"], "*");
}
