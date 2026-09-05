#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tokio_stream::StreamExt;
use tower::ServiceExt;
use trawler::flow::{BodyRec, NotCapturedReason};
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
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()))
    };
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
    for wired in ["fetch('/flows?'", "new EventSource('/stream')", "'cleared'"] {
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

// The WU contract at the router: paging is a re-slicing of the listing, and the
// unparameterised listing is untouched by its arrival.
#[tokio::test]
async fn the_unparameterised_listing_keeps_its_pre_paging_shape() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().method("GET").uri("http://a.example/one").status(200).build());
    store.insert(FlowBuilder::new().method("POST").uri("http://b.example/two").status(201).build());
    let r = router_with(store);

    let (status, v) = json(&r, "/flows").await;
    assert_eq!(status, StatusCode::OK);
    let rows = v.as_array().expect("a bare array, never an envelope");
    assert_eq!(rows.len(), 2);
    let mut fields: Vec<&str> = rows[0].as_object().unwrap().keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "duration_ms",
            "id",
            "method",
            "request_body_state",
            "response_body_state",
            "source",
            "started_at",
            "status",
            "uri",
        ],
        "no field joined or left the summary row"
    );
}

#[tokio::test]
async fn walking_by_cursor_yields_exactly_the_unpaged_listing() {
    let store = Arc::new(Store::new(1_000_000, 64));
    for i in 0..7 {
        let host = if i % 2 == 0 { "keep" } else { "drop" };
        store.insert(FlowBuilder::new().uri(&format!("http://{host}.example/{i}")).build());
    }
    let r = router_with(store);

    for filter in ["", "host=keep"] {
        let (_, listed) = json(&r, &format!("/flows?{filter}")).await;
        let expected: Vec<String> = listed
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect();

        let mut walked: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let query = match &cursor {
                Some(c) => format!("/flows?{filter}&limit=2&cursor={c}"),
                None => format!("/flows?{filter}&limit=2"),
            };
            let (status, page) = json(&r, &query).await;
            assert_eq!(status, StatusCode::OK);
            let rows = page["flows"].as_array().expect("flows");
            assert!(rows.len() <= 2, "a page never exceeds its limit");
            walked.extend(rows.iter().map(|row| row["id"].as_str().unwrap().to_owned()));
            match page["next_cursor"].as_str() {
                Some(next) => cursor = Some(next.to_owned()),
                None => break,
            }
        }

        assert_eq!(walked, expected, "paging must re-slice `{filter}`, not redefine it");
    }
}

#[tokio::test]
async fn the_router_refuses_malformed_paging_by_name() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://a.example/one").build());
    let r = router_with(store);

    for (query, named) in [
        ("/flows?limit=0", "limit"),
        ("/flows?limit=lots", "limit"),
        ("/flows?limit=2&cursor=nope", "cursor"),
        ("/flows?cursor=00000000-0000-0000-0000-000000000000", "cursor"),
    ] {
        let (status, _headers, body) = get(&r, query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        let reason = String::from_utf8(body).unwrap();
        assert!(reason.contains(named), "refusal for {query} must name {named}, got {reason}");
    }
}

#[tokio::test]
async fn a_cursor_the_ring_evicted_ends_the_walk_rather_than_restarting_it() {
    let store = Arc::new(Store::new(10, 64));
    store.insert(
        FlowBuilder::new().uri("http://a.example/one").req_body(captured(&[0u8; 8])).build(),
    );
    let evicted = store.list(&trawler::store::FlowFilter::default())[0].id.to_string();
    store.insert(
        FlowBuilder::new().uri("http://b.example/two").req_body(captured(&[0u8; 8])).build(),
    );
    let r = router_with(store);

    let (status, page) = json(&r, &format!("/flows?limit=2&cursor={evicted}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page["flows"].as_array().unwrap().is_empty());
    assert!(page["next_cursor"].is_null());
}

// A cursor row can now vanish two ways, and paging has to answer the same for
// both - the walk ends rather than restarting at the newest row.
#[tokio::test]
async fn a_cursor_a_delete_removed_ends_the_walk_like_an_evicted_one() {
    let store = Arc::new(Store::new(1_000_000, 64));
    for uri in ["http://a.example/one", "http://b.example/two", "http://c.example/three"] {
        store.insert(FlowBuilder::new().uri(uri).status(200).build());
    }
    let r = router_with(store);

    let (_, first) = json(&r, "/flows?limit=1").await;
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();

    assert_eq!(delete(&r, &format!("/flows/{cursor}")).await.0, StatusCode::OK);

    let (status, page) = json(&r, &format!("/flows?limit=2&cursor={cursor}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page["flows"].as_array().unwrap().is_empty(), "the walk ends, it does not restart");
    assert!(page["next_cursor"].is_null());
}

fn har_fixture_store() -> Arc<Store> {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(
        FlowBuilder::new()
            .method("GET")
            .uri("http://api.example.com/users?page=2&sort=name")
            .req_header("Accept", "application/json")
            .status(200)
            .resp_header("Content-Type", "application/json")
            .resp_header("Set-Cookie", "sid=xyz; Path=/")
            .resp_body(captured(br#"{"users":[]}"#))
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .method("POST")
            .uri("http://cdn.other.com/upload")
            .req_header("Content-Type", "image/png")
            .req_body(captured(&[0x89, 0x50, 0x4e, 0x47]))
            .status(201)
            .resp_header("Location", "http://cdn.other.com/upload/1")
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .method("GET")
            .uri("http://events.example.com/stream")
            .status(200)
            .resp_header("Content-Type", "text/event-stream")
            .resp_body(BodyRec::Streaming { content_type: "text/event-stream".into() })
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .method("GET")
            .uri("http://big.example.com/blob")
            .status(200)
            .resp_header("Content-Type", "application/octet-stream")
            .resp_body(BodyRec::NotCaptured {
                reason: NotCapturedReason::TooLarge { actual: 9_000_000 },
            })
            .build(),
    );
    store
}

#[tokio::test]
async fn export_serves_a_har_1_2_document_of_the_capture() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(
        FlowBuilder::new()
            .method("POST")
            .uri("http://api.example.com/users?page=2")
            .req_header("Content-Type", "application/json")
            .req_header("Cookie", "sid=abc")
            .req_body(captured(br#"{"name":"ada"}"#))
            .status(201)
            .resp_header("Content-Type", "application/json")
            .resp_body(captured(br#"{"id":1}"#))
            .build(),
    );
    let r = router_with(store);

    let (status, headers, body) = get(&r, "/flows/export").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert!(headers["content-disposition"].to_str().unwrap().contains(".har"));

    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["log"]["version"], "1.2");
    assert_eq!(doc["log"]["creator"]["name"], "Trawler");

    let entry = &doc["log"]["entries"][0];
    assert!(entry["startedDateTime"].as_str().unwrap().contains('T'));
    assert!(entry["time"].as_i64().unwrap() >= 0);
    assert_eq!(entry["timings"]["wait"], entry["time"]);
    assert_eq!(entry["request"]["method"], "POST");
    assert_eq!(entry["request"]["url"], "http://api.example.com/users?page=2");
    assert_eq!(entry["request"]["httpVersion"], "HTTP/1.1");
    assert_eq!(entry["request"]["queryString"], json!([{"name": "page", "value": "2"}]));
    assert_eq!(entry["request"]["cookies"], json!([{"name": "sid", "value": "abc"}]));
    assert_eq!(entry["request"]["postData"]["text"], r#"{"name":"ada"}"#);
    assert_eq!(entry["response"]["status"], 201);
    assert_eq!(entry["response"]["statusText"], "Created");
    assert_eq!(entry["response"]["httpVersion"], "HTTP/1.1");
    assert_eq!(entry["response"]["content"]["text"], r#"{"id":1}"#);
    assert_eq!(entry["response"]["content"]["size"], 8);
    assert_eq!(entry["cache"], json!({}));
}

// Expected result 3: the export and the listing must select the same set, which
// holds by construction (one shared filter) and is asserted endpoint-to-endpoint.
#[tokio::test]
async fn export_and_listing_select_the_same_flows() {
    let r = router_with(har_fixture_store());

    for query in ["", "host=example", "method=get", "status=201", "content_type=json", "q=upload"] {
        let (_, listed) = json(&r, &format!("/flows?{query}")).await;
        let listed_urls: Vec<&str> =
            listed.as_array().unwrap().iter().map(|row| row["uri"].as_str().unwrap()).collect();

        let (_, exported) = json(&r, &format!("/flows/export?{query}")).await;
        let exported_urls: Vec<&str> = exported["log"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["request"]["url"].as_str().unwrap())
            .collect();

        assert_eq!(exported_urls, listed_urls, "export must select what `{query}` lists");
    }
}

#[tokio::test]
async fn export_of_an_empty_capture_is_a_valid_empty_har() {
    let r = router_with(Arc::new(Store::new(1_000_000, 64)));
    let (status, doc) = json(&r, "/flows/export").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["log"]["version"], "1.2");
    assert_eq!(doc["log"]["entries"].as_array().unwrap().len(), 0);

    let (_, filtered) = json(&r, "/flows/export?host=nothing").await;
    assert_eq!(filtered["log"]["entries"].as_array().unwrap().len(), 0);
}

// The contract a HAR reader depends on: a body Trawler never captured is marked
// as such, never rendered as an empty one.
#[tokio::test]
async fn export_marks_uncaptured_bodies_rather_than_reporting_them_empty() {
    let r = router_with(har_fixture_store());
    let (_, doc) = json(&r, "/flows/export").await;
    let content_for = |url: &str| {
        doc["log"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["request"]["url"] == url)
            .unwrap_or_else(|| panic!("no entry for {url}"))["response"]["content"]
            .clone()
    };

    let streaming = content_for("http://events.example.com/stream");
    assert_eq!(streaming["size"], -1);
    assert!(streaming.get("text").is_none());
    assert!(streaming["comment"].as_str().unwrap().contains("streaming"));

    let too_large = content_for("http://big.example.com/blob");
    assert_eq!(too_large["size"], 9_000_000);
    assert!(too_large.get("text").is_none());
    assert!(too_large["comment"].as_str().unwrap().contains("too_large"));

    let empty = content_for("http://cdn.other.com/upload");
    assert_eq!(empty["size"], 0);
    assert!(empty.get("text").is_none());
    assert!(empty.get("comment").is_none(), "an empty body has no reason to give");
}

// The WU obligation that a new wire contract is snapshot-locked: a later change
// to the projection surfaces as a reviewable diff rather than passing silently.
// The two fields that cannot be deterministic — the capture timestamp and the
// crate version in `creator` — are normalised first, per the testing contract.
#[tokio::test]
async fn the_har_document_shape_is_locked() {
    let r = router_with(har_fixture_store());
    let (_, mut doc) = json(&r, "/flows/export").await;

    doc["log"]["creator"]["version"] = Value::String("<version>".into());
    for entry in doc["log"]["entries"].as_array_mut().unwrap() {
        entry["startedDateTime"] = Value::String("<started>".into());
    }

    insta::assert_json_snapshot!(doc);
}

async fn post(router: &Router, uri: &str, body: Vec<u8>) -> (StatusCode, Value) {
    let req = Request::builder().method("POST").uri(uri).body(Body::from(body)).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, value)
}

async fn export_bytes(router: &Router) -> Vec<u8> {
    let (status, _, bytes) = get(router, "/flows/export").await;
    assert_eq!(status, StatusCode::OK);
    bytes
}

// A capture with every shape the format has to carry: a text body, a binary
// request body, a streamed response, a too-large one, and a flow with headers
// worth checking the order of.
fn seeded_store() -> Arc<Store> {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(
        FlowBuilder::new()
            .method("POST")
            .uri("http://api.example.com/users?page=2")
            .req_header("Content-Type", "application/json")
            .req_header("Accept", "application/json")
            .req_header("Accept", "text/plain")
            .req_body(captured(br#"{"name":"ada"}"#))
            .status(201)
            .resp_header("Content-Type", "application/json")
            .resp_body(captured(br#"{"id":7}"#))
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .method("PUT")
            .uri("http://cdn.other.com/blob")
            .req_header("Content-Type", "application/octet-stream")
            .req_body(captured(&[0u8, 1, 2, 250, 255]))
            .status(204)
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .uri("http://events.example.com/sse")
            .status(200)
            .resp_header("Content-Type", "text/event-stream")
            .resp_body(BodyRec::Streaming { content_type: "text/event-stream".into() })
            .build(),
    );
    store.insert(
        FlowBuilder::new()
            .uri("http://big.example.com/dump")
            .status(200)
            .resp_header("Content-Type", "application/zip")
            .resp_body(BodyRec::NotCaptured {
                reason: NotCapturedReason::TooLarge { actual: 9_000_000 },
            })
            .build(),
    );
    store
}

fn rows(value: &Value) -> Vec<(String, String)> {
    value
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| {
            (
                r["uri"].as_str().expect("uri").to_owned(),
                r["source"].as_str().expect("source").to_owned(),
            )
        })
        .collect()
}

// The WU contract at the router: what the export writes, the import reads back
// as the same flows in the same order, marked as having come from a file.
#[tokio::test]
async fn an_exported_capture_reimports_as_the_same_flows_marked_imported() {
    let source = router_with(seeded_store());
    let document = export_bytes(&source).await;
    let (_, exported_rows) = json(&source, "/flows").await;

    let target = router_with(Arc::new(Store::new(1_000_000, 64)));
    let (status, result) = post(&target, "/flows/import", document).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({ "imported": 4 }));

    let (status, imported_rows) = json(&target, "/flows").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        rows(&imported_rows).iter().map(|(uri, _)| uri.clone()).collect::<Vec<String>>(),
        rows(&exported_rows).iter().map(|(uri, _)| uri.clone()).collect::<Vec<String>>(),
        "the listing order survives the round trip"
    );
    assert!(
        rows(&imported_rows).iter().all(|(_, source)| source == "imported"),
        "every re-imported row carries the mark"
    );
    assert!(
        rows(&exported_rows).iter().all(|(_, source)| source == "captured"),
        "the capture it came from did not"
    );

    // Field-for-field on the exchange with the most to lose.
    let detail_uri = |rows: &Value| {
        let id = rows.as_array().unwrap().iter().find(|r| r["method"] == "POST").expect("the POST")
            ["id"]
            .as_str()
            .expect("id")
            .to_owned();
        format!("/flows/{id}")
    };
    let (_, before) = json(&source, &detail_uri(&exported_rows)).await;
    let (_, after) = json(&target, &detail_uri(&imported_rows)).await;
    for field in [
        "method",
        "uri",
        "status",
        "request_http_version",
        "response_http_version",
        "started_at",
        "duration_ms",
        "request_headers",
        "response_headers",
        "request_body",
        "response_body",
    ] {
        assert_eq!(after[field], before[field], "{field} did not survive the round trip");
    }
    assert_eq!(after["source"], "imported");
    assert_ne!(after["id"], before["id"], "an id is this store's handle, not the file's");
}

#[tokio::test]
async fn the_source_filter_selects_each_kind_across_listing_and_export() {
    let store = seeded_store();
    let r = router_with(store);
    let document = export_bytes(&r).await;
    let (_, result) = post(&r, "/flows/import", document).await;
    assert_eq!(result, json!({ "imported": 4 }));

    let (_, all) = json(&r, "/flows").await;
    assert_eq!(all.as_array().unwrap().len(), 8, "import added rather than replaced");

    let (status, imported) = json(&r, "/flows?source=imported").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(imported.as_array().unwrap().len(), 4);
    assert!(rows(&imported).iter().all(|(_, s)| s == "imported"));

    let (_, captured_rows) = json(&r, "/flows?source=captured").await;
    assert_eq!(captured_rows.as_array().unwrap().len(), 4);
    assert!(rows(&captured_rows).iter().all(|(_, s)| s == "captured"));

    // The export inherits the filter, so it selects what the listing lists.
    let ids = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["uri"].as_str().expect("uri").to_owned())
            .collect()
    };
    let (_, _, exported) = get(&r, "/flows/export?source=imported").await;
    let document: Value = serde_json::from_slice(&exported).unwrap();
    let urls: Vec<String> = document["log"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["request"]["url"].as_str().expect("url").to_owned())
        .collect();
    assert_eq!(urls, ids(&imported));

    // Composes with the rest of the filter, rather than replacing it.
    let (_, composed) = json(&r, "/flows?source=imported&method=post").await;
    assert_eq!(composed.as_array().unwrap().len(), 1);
    assert_eq!(composed[0]["method"], "POST");

    // A silently empty listing reads exactly like "nothing matched".
    let (status, reason) = json(&r, "/flows?source=proxied").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(reason.as_str().unwrap_or_default().contains("source"), "{reason}");
}

// UC-6's claim that an imported flow is otherwise an ordinary flow: the same
// detail endpoint, the same body endpoints, the same decoding surface.
#[tokio::test]
async fn an_imported_flow_is_an_ordinary_flow() {
    let source = router_with({
        let store = Arc::new(Store::new(1_000_000, 64));
        store.insert(
            FlowBuilder::new()
                .method("POST")
                .uri("http://api.example.com/echo")
                .req_header("Content-Type", "application/json")
                .req_body(captured(br#"{"in":1}"#))
                .status(200)
                .resp_header("Content-Type", "application/json")
                .resp_body(captured(br#"{"out":  2}"#))
                .build(),
        );
        store
    });
    let document = export_bytes(&source).await;

    let r = router_with(Arc::new(Store::new(1_000_000, 64)));
    post(&r, "/flows/import", document).await;
    let (_, listed) = json(&r, "/flows").await;
    let id = listed[0]["id"].as_str().expect("id").to_owned();

    let (status, detail) = json(&r, &format!("/flows/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["source"], "imported");
    assert_eq!(detail["uri"], "http://api.example.com/echo");

    let (status, _, raw) = get(&r, &format!("/flows/{id}/request/body")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, br#"{"in":1}"#);

    let (status, _, decoded) = get(&r, &format!("/flows/{id}/response/body?decode=1")).await;
    assert_eq!(status, StatusCode::OK);
    let decoded = String::from_utf8(decoded).unwrap();
    assert!(decoded.contains("\"out\""), "the decode surface works on an imported body: {decoded}");
    assert!(decoded.contains('\n'), "JSON was pretty-printed: {decoded}");
}

#[tokio::test]
async fn an_imported_flow_appears_on_the_stream_as_it_lands() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let r = router_with(store);

    let req = Request::builder().uri("/stream").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let mut stream = res.into_body().into_data_stream();

    let document = r#"{"log":{"version":"1.2","entries":[
             {"startedDateTime":"2026-09-04T10:00:00Z","time":9,
               "request":{"method":"GET","url":"http://from-a-file/","httpVersion":"HTTP/1.1",
                           "headers":[],"bodySize":0},
               "response":{"status":200,"httpVersion":"HTTP/1.1","headers":[],
                            "content":{"size":0,"mimeType":"text/plain"}}}]}}"#;
    let (status, _) = post(&r, "/flows/import", document.as_bytes().to_vec()).await;
    assert_eq!(status, StatusCode::OK);

    let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no SSE frame before timeout")
        .expect("stream ended")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame);
    assert!(text.contains("event: request"), "got: {text}");
    assert!(text.contains("http://from-a-file/"), "got: {text}");
}

// The honest-body contract, inverted: a body Trawler never captured comes back
// in the state the file stated, never as an empty one.
#[tokio::test]
async fn an_uncaptured_body_survives_the_round_trip_as_a_client_sees_it() {
    let source = router_with(seeded_store());
    let document = export_bytes(&source).await;

    let r = router_with(Arc::new(Store::new(1_000_000, 64)));
    post(&r, "/flows/import", document).await;
    let (_, listed) = json(&r, "/flows").await;

    let state_of = |uri: &str| -> String {
        listed
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["uri"] == uri)
            .unwrap_or_else(|| panic!("no row for {uri}"))["response_body_state"]
            .as_str()
            .expect("state")
            .to_owned()
    };
    assert_eq!(state_of("http://events.example.com/sse"), "streaming");
    assert_eq!(state_of("http://big.example.com/dump"), "not_captured");
    assert_eq!(state_of("http://cdn.other.com/blob"), "empty", "a real 204 stays empty");

    // The too-large body still reports the size that got it dropped.
    let id = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["uri"] == "http://big.example.com/dump")
        .expect("the too-large flow")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let (_, detail) = json(&r, &format!("/flows/{id}")).await;
    assert_eq!(detail["response_body"]["state"], "not_captured");
    assert_eq!(detail["response_body"]["reason"], "too_large");
    assert_eq!(detail["response_body"]["actual"], 9_000_000);
}

#[tokio::test]
async fn a_refused_import_leaves_the_store_untouched() {
    let r = router_with(seeded_store());
    let (_, before) = json(&r, "/flows").await;

    let good = r#"{"startedDateTime":"2026-09-04T10:00:00Z","time":1,
             "request":{"method":"GET","url":"http://ok/","httpVersion":"HTTP/1.1",
                         "headers":[],"bodySize":0},
             "response":{"status":200,"httpVersion":"HTTP/1.1","headers":[],
                          "content":{"size":0,"mimeType":"text/plain"}}}"#;
    let malformations: Vec<(Vec<u8>, &str)> = vec![
        (b"<html>not json</html>".to_vec(), "not a JSON document"),
        (br#"{"version":"1.2"}"#.to_vec(), "missing log"),
        (br#"{"log":{"version":"1.3","entries":[]}}"#.to_vec(), "1.3"),
        (br#"{"log":{"version":"1.2"}}"#.to_vec(), "missing log.entries"),
        (
            format!(r#"{{"log":{{"version":"1.2","entries":[{good},{{"time":1}}]}}}}"#).into_bytes(),
            "log.entries[1]",
        ),
        (
            format!(
                r#"{{"log":{{"version":"1.2","entries":[{}]}}}}"#,
                good.replace(r#""startedDateTime":"2026-09-04T10:00:00Z""#, r#""startedDateTime":"tuesday""#)
            )
            .into_bytes(),
            "RFC3339",
        ),
        (
            format!(
                r#"{{"log":{{"version":"1.2","entries":[{}]}}}}"#,
                good.replace(r#""status":200"#, r#""status":9000"#)
            )
            .into_bytes(),
            "not an HTTP status code",
        ),
        (
            format!(
                r#"{{"log":{{"version":"1.2","entries":[{}]}}}}"#,
                good.replace(
                    r#""content":{"size":0,"mimeType":"text/plain"}"#,
                    r#""content":{"size":4,"mimeType":"image/png","text":"@@@","encoding":"base64"}"#
                )
            )
            .into_bytes(),
            "not valid base64",
        ),
    ];

    for (document, expected) in malformations {
        let (status, reason) = post(&r, "/flows/import", document).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "refused: {reason}");
        let reason = reason.as_str().unwrap_or_default().to_owned();
        assert!(reason.contains(expected), "expected {expected:?} in {reason:?}");

        let (_, after) = json(&r, "/flows").await;
        assert_eq!(after, before, "a refused import changed the capture");
    }

    // A valid but empty document is not a malformation — it says nothing was
    // captured, which is the same answer an empty export gives.
    let (status, result) =
        post(&r, "/flows/import", br#"{"log":{"version":"1.2","entries":[]}}"#.to_vec()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({ "imported": 0 }));
    assert_eq!(json(&r, "/flows").await.1, before);
}

// Questions 3 and 4 of the epic plan, as assertions: import adds, and an
// imported flow keeps its instant while taking a fresh handle.
#[tokio::test]
async fn the_same_file_imported_twice_yields_two_independent_sets() {
    let source = router_with({
        let store = Arc::new(Store::new(1_000_000, 64));
        store.insert(FlowBuilder::new().uri("http://twice/").status(200).build());
        store
    });
    let document = export_bytes(&source).await;

    let r = router_with({
        let store = Arc::new(Store::new(1_000_000, 64));
        store.insert(FlowBuilder::new().uri("http://already-here/").status(200).build());
        store
    });
    for expected_total in [2, 3] {
        let (status, result) = post(&r, "/flows/import", document.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result, json!({ "imported": 1 }));
        let (_, listed) = json(&r, "/flows").await;
        assert_eq!(listed.as_array().unwrap().len(), expected_total);
    }

    let (_, listed) = json(&r, "/flows?source=imported").await;
    let imported = listed.as_array().unwrap();
    assert_eq!(imported.len(), 2);
    assert_ne!(imported[0]["id"], imported[1]["id"], "each import mints its own handle");
    assert_eq!(
        imported[0]["started_at"], imported[1]["started_at"],
        "both keep the instant the file recorded"
    );
    assert!(
        listed.as_array().unwrap().iter().all(|r| r["uri"] == "http://twice/"),
        "the pre-existing capture is untouched and unmarked"
    );
}

async fn delete(router: &Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

#[tokio::test]
async fn clearing_the_capture_empties_the_listing_and_tells_the_stream() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://a.example.com/").status(200).build());
    store.insert(FlowBuilder::new().uri("http://b.example.com/").status(200).build());
    store.note_connection("a.example.com:443".into());
    let r = router_with(store.clone());

    let req = Request::builder().uri("/stream").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(req).await.unwrap();
    let mut stream = res.into_body().into_data_stream();

    let (status, result) = delete(&r, "/flows").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({ "removed": 2 }));
    assert_eq!(json(&r, "/flows").await.1.as_array().unwrap().len(), 0);

    let frame = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no SSE frame before timeout")
        .expect("stream ended")
        .expect("frame error");
    let text = String::from_utf8_lossy(&frame);
    assert!(text.contains("event: cleared"), "a subscriber must not have to refetch: {text}");
    assert_eq!(text.matches("event:").count(), 1, "one event for the batch: {text}");

    assert_eq!(
        json(&r, "/stats").await.1["connections"],
        1,
        "a cleared store is empty, not restarted"
    );
}

#[tokio::test]
async fn a_filtered_clear_removes_only_its_matches_and_leaves_the_rest_whole() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://drop.example.com/x").status(200).build());
    let survivor = FlowBuilder::new()
        .uri("http://keep.example.com/y")
        .req_body(captured(b"request payload"))
        .resp_body(captured(b"response payload"))
        .status(200)
        .build();
    let survivor_id = survivor.id.to_string();
    store.insert(survivor);
    let r = router_with(store);

    let (status, result) = delete(&r, "/flows?host=drop.example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({ "removed": 1 }));

    let (_, listed) = json(&r, "/flows").await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["uri"], "http://keep.example.com/y");

    assert_eq!(json(&r, &format!("/flows/{survivor_id}")).await.0, StatusCode::OK);
    let (status, _, body) = get(&r, &format!("/flows/{survivor_id}/request/body")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"request payload");
    let (status, _, body) = get(&r, &format!("/flows/{survivor_id}/response/body")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"response payload");
}

#[tokio::test]
async fn deleting_one_flow_and_the_three_ways_a_delete_is_refused() {
    let store = Arc::new(Store::new(1_000_000, 64));
    let flow = FlowBuilder::new().uri("http://one.example.com/").status(200).build();
    let id = flow.id.to_string();
    store.insert(flow);
    let r = router_with(store);

    assert_eq!(delete(&r, "/flows/not-a-uuid").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        delete(&r, "/flows/00000000-0000-0000-0000-000000000000").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&r, "/flows?host=nothing.example.com").await,
        (StatusCode::OK, json!({ "removed": 0 })),
        "the end state it asked for is already true"
    );
    assert_eq!(json(&r, "/flows").await.1.as_array().unwrap().len(), 1, "nothing left yet");

    assert_eq!(
        delete(&r, &format!("/flows/{id}")).await,
        (StatusCode::OK, json!({ "removed": 1 }))
    );
    assert_eq!(json(&r, "/flows").await.1.as_array().unwrap().len(), 0);
    assert_eq!(delete(&r, &format!("/flows/{id}")).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_clear_selects_by_provenance_like_every_other_endpoint() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://live.example.com/").status(200).build());
    let r = router_with(store);

    let document = {
        let source = Arc::new(Store::new(1_000_000, 64));
        source.insert(FlowBuilder::new().uri("http://from-file.example.com/").status(200).build());
        export_bytes(&router_with(source)).await
    };
    assert_eq!(post(&r, "/flows/import", document).await.0, StatusCode::OK);

    let (status, value) = delete(&r, "/flows?source=nonsense").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a clear must never guess what it removes");
    assert_eq!(value, "source must be captured or imported");
    assert_eq!(json(&r, "/flows").await.1.as_array().unwrap().len(), 2);

    let (_, result) = delete(&r, "/flows?source=imported").await;
    assert_eq!(result, json!({ "removed": 1 }));

    let (_, listed) = json(&r, "/flows").await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["uri"], "http://live.example.com/", "the live capture is untouched");
}
