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

use crate::common::{brotli, captured, gzip, FlowBuilder};

fn router_with(store: Arc<Store>) -> Router {
    trawler::test_support::api_router(store, "--CA PEM--")
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

fn a_ca() -> rcgen::Certificate {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    rcgen::CertificateParams::new(vec!["trawler-test".into()])
        .unwrap()
        .self_signed(&key_pair)
        .unwrap()
}

#[tokio::test]
async fn every_ca_download_suggests_the_file_its_url_names() {
    let ca = a_ca();
    let r = trawler::test_support::api_router(Arc::new(Store::new(1_000_000, 64)), &ca.pem());

    for (path, content_type, filename) in [
        ("/ca.crt", "application/x-x509-ca-cert", "trawler-ca.crt"),
        ("/ca.pem", "application/x-pem-file", "trawler-ca.pem"),
        ("/ca.cer", "application/x-x509-ca-cert", "trawler-ca.cer"),
        ("/ca.mobileconfig", "application/x-apple-aspen-config", "trawler-ca.mobileconfig"),
    ] {
        let (status, headers, _body) = get(&r, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(headers["content-type"], content_type, "{path}");
        assert_eq!(
            headers["content-disposition"],
            format!("attachment; filename=\"{filename}\"").as_str(),
            "{path}"
        );
    }
}

#[tokio::test]
async fn the_pem_downloads_carry_the_ca_byte_for_byte() {
    let ca = a_ca();
    let r = trawler::test_support::api_router(Arc::new(Store::new(1_000_000, 64)), &ca.pem());

    for path in ["/ca.crt", "/ca.pem"] {
        let (_status, _headers, body) = get(&r, path).await;
        assert_eq!(body, ca.pem().as_bytes(), "{path}");
    }
}

#[tokio::test]
async fn the_cer_download_is_the_der_of_the_ca() {
    let ca = a_ca();
    let r = trawler::test_support::api_router(Arc::new(Store::new(1_000_000, 64)), &ca.pem());

    let (_status, _headers, body) = get(&r, "/ca.cer").await;
    assert_eq!(body, ca.der().to_vec());
}

#[tokio::test]
async fn the_profile_download_carries_the_ca_as_its_root_payload() {
    use base64::Engine;
    let ca = a_ca();
    let r = trawler::test_support::api_router(Arc::new(Store::new(1_000_000, 64)), &ca.pem());

    let (_status, _headers, body) = get(&r, "/ca.mobileconfig").await;
    let profile = String::from_utf8(body).unwrap();
    assert!(profile.starts_with("<?xml"), "{profile}");
    assert!(profile.contains("<string>com.apple.security.root</string>"));
    let der = base64::engine::general_purpose::STANDARD.encode(ca.der());
    assert!(profile.contains(&format!("<data>{der}</data>")), "the payload is not this CA");
}

#[tokio::test]
async fn a_ca_that_is_not_a_certificate_fails_the_derived_downloads_rather_than_serving_nothing() {
    let r = trawler::test_support::api_router(Arc::new(Store::new(1_000_000, 64)), "PEMDATA");

    for path in ["/ca.cer", "/ca.mobileconfig"] {
        let (status, _headers, _body) = get(&r, path).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{path}");
    }
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
            "seed",
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
    assert_eq!(page["flows"].as_array().unwrap().len(), 0);
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

const CHROME_COMMAND: &str = "curl 'https://api.example.com/v1/orders?page=2' \\\n  \
  -H 'accept: application/json' \\\n  \
  -H 'content-type: application/json' \\\n  \
  -H 'cookie: sid=abc' \\\n  \
  --data-raw $'{\"note\":\"it\\'s here\"}' \\\n  \
  --compressed";

// The WU contract at the router: a browser's "Copy as cURL" lands as one
// request-only flow that every reading endpoint already knows how to serve.
#[tokio::test]
async fn a_pasted_curl_command_lands_as_a_request_only_flow_marked_imported() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://live/").status(200).build());
    let r = router_with(store.clone());
    let mut events = store.subscribe();

    let (status, result) =
        post(&r, "/flows/import?format=curl", CHROME_COMMAND.as_bytes().to_vec()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result, json!({ "imported": 1 }));

    let (_, listed) = json(&r, "/flows").await;
    let rows = listed.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let row = &rows[0];
    assert_eq!(row["method"], "POST");
    assert_eq!(row["uri"], "https://api.example.com/v1/orders?page=2");
    assert_eq!(row["source"], "imported");
    assert!(row["status"].is_null(), "nothing was sent, so nothing answered: {row}");
    assert!(row["duration_ms"].is_null());
    assert_eq!(row["request_body_state"], "captured");

    let id = row["id"].as_str().unwrap();
    let (status, detail) = json(&r, &format!("/flows/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let headers: Vec<(&str, &str)> = detail["request_headers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["name"].as_str().unwrap(), h["value"].as_str().unwrap()))
        .collect();
    assert_eq!(
        headers,
        [
            ("accept", "application/json"),
            ("content-type", "application/json"),
            ("cookie", "sid=abc"),
            ("Accept-Encoding", "deflate, gzip, br, zstd"),
        ]
    );
    assert!(detail["response_headers"].is_null() || detail["status"].is_null(), "{detail}");

    let (status, headers, body) = get(&r, &format!("/flows/{id}/request/body")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body, br#"{"note":"it's here"}"#);
    let (status, _, _) = get(&r, &format!("/flows/{id}/response/body")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "there is no response leg to read");

    let event = tokio::time::timeout(Duration::from_secs(2), events.recv()).await.unwrap().unwrap();
    let (name, payload) = trawler::api::view::sse_event(&event);
    assert_eq!(name, "request", "a pasted command lands like a request the proxy saw");
    assert_eq!(payload["flow_id"], id);
}

#[tokio::test]
async fn a_pasted_command_is_selected_by_provenance_beside_a_har_import() {
    let source = router_with(seeded_store());
    let document = export_bytes(&source).await;
    let r = router_with({
        let store = Arc::new(Store::new(1_000_000, 64));
        store.insert(FlowBuilder::new().uri("http://live/").status(200).build());
        store
    });
    post(&r, "/flows/import", document).await;
    post(&r, "/flows/import?format=curl", b"curl http://pasted/".to_vec()).await;

    let (_, imported) = json(&r, "/flows?source=imported").await;
    assert_eq!(imported.as_array().unwrap().len(), 5);
    assert_eq!(imported[0]["uri"], "http://pasted/");
    let (_, captured) = json(&r, "/flows?source=captured").await;
    assert_eq!(rows(&captured), [("http://live/".to_owned(), "captured".to_owned())]);

    let (status, result) = post(
        &r,
        "/flows/import?format=har",
        b"{\"log\":{\"version\":\"1.2\",\"entries\":[]}}".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result, json!({ "imported": 0 }), "the HAR door is unchanged under its own name");
}

#[tokio::test]
async fn a_refused_curl_command_names_its_fault_and_leaves_the_store_untouched() {
    let r = router_with(seeded_store());
    let (_, before) = json(&r, "/flows").await;

    let refusals: [(&str, &str); 9] = [
        ("", "empty command"),
        ("wget http://a/", "not a curl command: starts with \"wget\""),
        ("curl http://a/ -F 'file=@x'", "unsupported flag -F"),
        ("curl http://a/ -T upload.bin", "unsupported flag -T"),
        ("curl http://a/ --data-urlencode 'a=b'", "unsupported flag --data-urlencode"),
        ("curl http://a/ http://b/", "a second URL: \"http://b/\""),
        ("curl http://a/ -d @body.json", "-d @body.json reads a file"),
        ("curl -H 'A: 1'", "no URL"),
        ("curl 'http://a/ -H 'A: 1'", "unterminated quote"),
    ];
    for (command, expected) in refusals {
        let (status, reason) =
            post(&r, "/flows/import?format=curl", command.as_bytes().to_vec()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{command:?}: {reason}");
        let reason = reason.as_str().unwrap_or_default().to_owned();
        assert!(reason.contains(expected), "expected {expected:?} in {reason:?}");
        let (_, after) = json(&r, "/flows").await;
        assert_eq!(after, before, "a refused command changed the capture");
    }

    let (status, reason) = post(&r, "/flows/import?format=saz", b"curl http://a/".to_vec()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(reason, "format must be har or curl");
    assert_eq!(json(&r, "/flows").await.1, before);
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
    assert_eq!(value, "source must be captured, imported or sent");
    assert_eq!(json(&r, "/flows").await.1.as_array().unwrap().len(), 2);

    let (_, result) = delete(&r, "/flows?source=imported").await;
    assert_eq!(result, json!({ "removed": 1 }));

    let (_, listed) = json(&r, "/flows").await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["uri"], "http://live.example.com/", "the live capture is untouched");
}

// E-EXPORT.W6: the app reads the export's filename off the response rather than
// inventing one, and it reads it cross-origin — where only the safelisted
// headers are visible unless the grant says otherwise. Both the preflight and
// the real response have to carry it, because the browser checks each in turn.
#[tokio::test]
async fn a_cross_origin_client_can_read_the_filename_the_export_stamped() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://api.example.com/one").build());
    let r = router_with(store);

    let preflight =
        Request::builder().method("OPTIONS").uri("/flows/export").body(Body::empty()).unwrap();
    let res = r.clone().oneshot(preflight).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(res.headers()["access-control-expose-headers"], "*");

    let (status, headers, _) = get(&r, "/flows/export").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["access-control-expose-headers"], "*");
    let name = headers["content-disposition"].to_str().unwrap();
    assert!(name.starts_with("attachment; filename=\"trawler-"), "{name}");
    assert!(name.ends_with(".har\""), "{name}");
}

// The three doors as the app reaches them: the app never re-reads the listing
// after a mutation — it waits for the bus — so what each door answers has to be
// enough on its own, and a refusal has to arrive as text a screen can render.
#[tokio::test]
async fn every_door_the_app_calls_answers_what_it_did_and_refuses_in_a_sentence() {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(FlowBuilder::new().uri("http://api.example.com/one").status(200).build());
    store.insert(FlowBuilder::new().uri("http://other.test/two").status(200).build());
    let r = router_with(store);

    let (_, exported) = json(&r, "/flows/export?host=api.example.com").await;
    let entries = exported["log"]["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 1, "export is scoped by the same filter the listing is");

    let bytes = serde_json::to_vec(&exported).unwrap();
    let (status, body) = post(&r, "/flows/import", bytes).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["imported"], 1);

    let (status, body) =
        post(&r, "/flows/import?format=curl", b"curl 'http://api.example.com/three'".to_vec())
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["imported"], 1);

    let (status, body) =
        post(&r, "/flows/import?format=curl", b"curl -F a=b http://x/".to_vec()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let reason = body.as_str().unwrap();
    assert!(
        reason.contains("-F"),
        "the refusal names the flag, so the paste target can show it: {reason}"
    );

    let (status, body) = delete(&r, "/flows?host=other.test").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["removed"], 1,
        "clear reports what left rather than leaving the app to re-read"
    );

    let (_, listing) = json(&r, "/flows").await;
    assert_eq!(listing.as_array().unwrap().len(), 3);
    let (_, imported) = json(&r, "/flows?source=imported").await;
    assert_eq!(imported.as_array().unwrap().len(), 2, "both doors mark what they let in");
}

fn query_fixture_store() -> Arc<Store> {
    let store = Arc::new(Store::new(1_000_000, 64));
    store.insert(
        FlowBuilder::new().method("GET").uri("http://api.test/login?token=1").status(200).build(),
    );
    store.insert(
        FlowBuilder::new().method("OPTIONS").uri("https://auth.test/login").status(204).build(),
    );
    store.insert(
        FlowBuilder::new()
            .method("POST")
            .uri("http://localhost:8080/upload")
            .resp_header("Content-Type", "application/json")
            .status(503)
            .build(),
    );
    store.insert(FlowBuilder::new().method("GET").uri("https://api.test/pending").build());
    store.import(vec![FlowBuilder::new()
        .method("GET")
        .uri("http://file.test/a")
        .status(404)
        .build()
        .imported()]);
    store
}

fn listed_rows(value: &Value) -> Vec<(String, bool)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["uri"].as_str().unwrap().to_owned(), !row["status"].is_null()))
        .collect()
}

// The listing, the export and the clear share one filter, so for every list,
// negation and `is` query they select the same flows.
#[tokio::test]
async fn listing_export_and_clear_select_the_same_flows_for_lists_negation_and_is() {
    let expectations: [(&str, &[&str]); 9] = [
        (
            "host=api&host=auth",
            &[
                "https://api.test/pending",
                "https://auth.test/login",
                "http://api.test/login?token=1",
            ],
        ),
        ("q=login&q=token", &["http://api.test/login?token=1"]),
        ("content_type=json&not_method=OPTIONS", &["http://localhost:8080/upload"]),
        (
            "not_status=5xx&not_source=imported",
            &[
                "https://api.test/pending",
                "https://auth.test/login",
                "http://api.test/login?token=1",
            ],
        ),
        ("is=pending", &["https://api.test/pending"]),
        ("is=https&not_is=pending", &["https://auth.test/login"]),
        ("host=localhost:8080", &["http://localhost:8080/upload"]),
        (
            "method=get&method=post&status=4xx&status=5xx",
            &["http://file.test/a", "http://localhost:8080/upload"],
        ),
        ("not_q=login&not_q=upload", &["http://file.test/a", "https://api.test/pending"]),
    ];
    for (query, expected) in expectations {
        let r = router_with(query_fixture_store());
        let (status, listed) = json(&r, &format!("/flows?{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        let rows = listed_rows(&listed);
        let uris: Vec<&str> = rows.iter().map(|(uri, _)| uri.as_str()).collect();
        assert_eq!(uris, expected, "`{query}` lists");

        let (_, exported) = json(&r, &format!("/flows/export?{query}")).await;
        let exported_urls: Vec<&str> = exported["log"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["request"]["url"].as_str().unwrap())
            .collect();
        let answered: Vec<&str> = rows
            .iter()
            .filter(|(_, has_status)| *has_status)
            .map(|(uri, _)| uri.as_str())
            .collect();
        assert_eq!(exported_urls, answered, "`{query}` exports what it lists");

        let (_, before) = json(&r, "/flows").await;
        let (status, removed) = delete(&r, &format!("/flows?{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert_eq!(removed, json!({ "removed": expected.len() }), "`{query}` clears what it lists");
        let (_, after) = json(&r, "/flows").await;
        let survivors: Vec<String> = listed_rows(&before)
            .into_iter()
            .map(|(uri, _)| uri)
            .filter(|uri| !expected.contains(&uri.as_str()))
            .collect();
        let after_uris: Vec<String> = listed_rows(&after).into_iter().map(|(uri, _)| uri).collect();
        assert_eq!(after_uris, survivors, "`{query}` leaves everything it did not list");
    }
}

#[tokio::test]
async fn a_single_parameter_selects_what_it_did_before_lists_arrived() {
    let r = router_with(query_fixture_store());
    for (query, expected) in [
        ("host=api.test", 2),
        ("host=localhost", 1),
        ("method=GET", 3),
        ("status=204", 1),
        ("status=4xx", 1),
        ("content_type=json", 1),
        ("q=login", 2),
        ("source=imported", 1),
        ("host=", 5),
        ("", 5),
    ] {
        let (status, listed) = json(&r, &format!("/flows?{query}")).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert_eq!(listed.as_array().unwrap().len(), expected, "`{query}`");
    }
}

#[tokio::test]
async fn every_door_refuses_an_unreadable_value_naming_the_parameter_as_sent() {
    let r = router_with(query_fixture_store());
    for (query, leads) in [
        ("not_status=9xx", "not_status must be"),
        ("host=a&is=done", "is must be pending or https"),
        ("not_source=proxied", "not_source must be"),
    ] {
        for (door, (status, body)) in [
            ("listing", get_text(&r, &format!("/flows?{query}")).await),
            ("export", get_text(&r, &format!("/flows/export?{query}")).await),
            ("clear", delete_text(&r, &format!("/flows?{query}")).await),
        ] {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{door} {query}");
            assert!(body.starts_with(leads), "{door} {query}: {body:?}");
        }
    }
    assert_eq!(
        json(&r, "/flows").await.1.as_array().unwrap().len(),
        5,
        "a refused clear removes nothing"
    );
}

async fn delete_text(router: &Router, uri: &str) -> (StatusCode, String) {
    let req = Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, String::from_utf8(bytes).unwrap())
}

async fn get_text(router: &Router, uri: &str) -> (StatusCode, String) {
    let (status, _, body) = get(router, uri).await;
    (status, String::from_utf8(body).unwrap())
}
