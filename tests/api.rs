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

use common::{captured, gzip, FlowBuilder};

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
