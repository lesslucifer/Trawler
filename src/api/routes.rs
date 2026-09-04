use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use time::macros::format_description;
use time::OffsetDateTime;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use super::view::{self, FlowPageView, FlowSummary, FlowView};
use super::{decode, har};
use crate::flow::{header_value, BodyRec, FlowId, Header};
use crate::store::{FlowFilter, Store};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub ca_cert_pem: Arc<String>,
}

/// The filter half of a listing request. Every endpoint that selects flows
/// deserializes into this and converts, so the listing and the export cannot
/// come to select different sets. It is a field list rather than a nested
/// `#[serde(flatten)]` member because `serde_urlencoded`, which axum's `Query`
/// uses, cannot deserialize a flattened struct from a query string.
#[derive(Deserialize)]
pub struct FilterQuery {
    host: Option<String>,
    method: Option<String>,
    status: Option<u16>,
    content_type: Option<String>,
    q: Option<String>,
    source: Option<String>,
}

impl FilterQuery {
    /// Fails rather than ignoring an unrecognised `source`: an empty listing
    /// reads to a caller exactly like "nothing matched".
    fn into_filter(self) -> Result<FlowFilter, &'static str> {
        let source = match self.source.as_deref() {
            None => None,
            Some(value) => {
                Some(view::parse_source(value).ok_or("source must be captured or imported")?)
            }
        };
        Ok(FlowFilter {
            host: self.host,
            method: self.method,
            status: self.status,
            content_type: self.content_type,
            q: self.q,
            source,
        })
    }
}

/// The paging half, extracted alongside `FilterQuery` rather than nesting it:
/// `serde_urlencoded` ignores unknown keys, so two extractions over the same
/// query string each see their own parameters and the filter keeps one
/// definition. Only the listing accepts these — an export is never paged.
#[derive(Deserialize)]
pub struct PagingQuery {
    limit: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct BodyQuery {
    decode: Option<String>,
}

#[derive(Clone, Copy)]
enum Leg {
    Request,
    Response,
}

/// Two shapes, selected by whether paging was asked for: without `limit` the
/// whole filtered list as a bare array — the contract every existing client
/// already reads — and with it an envelope, because an array cannot carry the
/// cursor that continues the walk.
pub async fn list_flows(
    Query(filter): Query<FilterQuery>,
    Query(paging): Query<PagingQuery>,
    State(state): State<AppState>,
) -> Response {
    let filter = match filter.into_filter() {
        Ok(filter) => filter,
        Err(reason) => return bad_request(reason),
    };

    let cursor = match paging.cursor.as_deref().map(parse_cursor) {
        Some(Ok(id)) => Some(id),
        Some(Err(reason)) => return bad_request(reason),
        None => None,
    };

    let Some(limit) = paging.limit.as_deref() else {
        // A cursor without a limit is a client that believes it is paging, so it
        // is refused rather than silently served the whole unbounded listing.
        if cursor.is_some() {
            return bad_request("cursor requires limit");
        }
        let summaries: Vec<FlowSummary> =
            state.store.list(&filter).iter().map(FlowSummary::from).collect();
        return Json(summaries).into_response();
    };

    let limit = match limit.parse::<usize>() {
        Ok(0) | Err(_) => return bad_request("limit must be a positive integer"),
        Ok(limit) => limit,
    };

    let page = state.store.page(&filter, limit, cursor);
    Json(FlowPageView::from(&page)).into_response()
}

/// The whole filtered capture as one HAR 1.2 document — never a page, because a
/// HAR file split across pages is not a HAR file. Served as `application/json`
/// (a HAR file is JSON, and every reader accepts it) with the `.har` filename in
/// `Content-Disposition`, which is what tells a browser or the app what to save.
pub async fn export_har(
    Query(query): Query<FilterQuery>,
    State(state): State<AppState>,
) -> Response {
    let flows = match query.into_filter() {
        Ok(filter) => state.store.list(&filter),
        Err(reason) => return bad_request(reason),
    };
    let filename = format!("trawler-{}.har", export_timestamp());
    (
        [
            (header::CONTENT_TYPE, "application/json".to_owned()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
        ],
        Json(har::har_document(&flows)).into_response().into_body(),
    )
        .into_response()
}

/// The inverse door: a HAR 1.2 document becomes flows in the ring, marked as
/// imported. The whole document parses before the store is touched, so a
/// refused import leaves the capture exactly as it was.
pub async fn import_flows(State(state): State<AppState>, body: Bytes) -> Response {
    match har::flows_from_har(&body) {
        Ok(flows) => {
            let imported = state.store.import(flows);
            Json(json!({ "imported": imported })).into_response()
        }
        Err(reason) => (StatusCode::BAD_REQUEST, reason.to_string()).into_response(),
    }
}

/// A filename-safe UTC stamp, so successive exports do not overwrite each other
/// in a downloads folder.
fn export_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&format_description!("[year][month][day]-[hour][minute][second]"))
        .unwrap_or_else(|_| "export".to_owned())
}

/// One-shot counterpart to the `connection` SSE event: "has any traffic reached
/// the proxy yet?" The onboarding reads it once on entry (like W4's `/flows`
/// read) so the proxy step reflects connections that arrived before the UI
/// subscribed; live updates then ride the stream.
pub async fn stats(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "connections": state.store.connection_count() }))
}

pub async fn get_flow(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    let Some(flow_id) = parse_flow_id(&id) else {
        return bad_id();
    };
    match state.store.get(flow_id) {
        Some(flow) => Json(FlowView::from(&flow)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn request_body(
    Path(id): Path<String>,
    Query(query): Query<BodyQuery>,
    State(state): State<AppState>,
) -> Response {
    body_response(&state, &id, Leg::Request, wants_decode(&query))
}

pub async fn response_body(
    Path(id): Path<String>,
    Query(query): Query<BodyQuery>,
    State(state): State<AppState>,
) -> Response {
    body_response(&state, &id, Leg::Response, wants_decode(&query))
}

pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = BroadcastStream::new(state.store.subscribe()).map(|result| {
        let (name, data) = match result {
            Ok(event) => view::sse_event(&event),
            Err(BroadcastStreamRecvError::Lagged(skipped)) => view::sse_lagged(skipped),
        };
        let payload = serde_json::to_string(&data).unwrap_or_default();
        Ok(Event::default().event(name).data(payload))
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

pub async fn ui_page() -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], include_str!("ui.html")).into_response()
}

pub async fn ca_cert(State(state): State<AppState>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/x-x509-ca-cert"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"trawler-ca.pem\""),
        ],
        state.ca_cert_pem.as_bytes().to_vec(),
    )
        .into_response()
}

fn body_response(state: &AppState, id: &str, leg: Leg, decode: bool) -> Response {
    let Some(flow_id) = parse_flow_id(id) else {
        return bad_id();
    };
    let Some(flow) = state.store.get(flow_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let (body, headers): (&BodyRec, &[Header]) = match leg {
        Leg::Request => (&flow.request.body, &flow.request.headers),
        Leg::Response => match &flow.response {
            Some(res) => (&res.body, &res.headers),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };

    let Some(bytes) = body.bytes() else {
        return StatusCode::NO_CONTENT.into_response();
    };

    let out = if decode {
        decode::decode_body(
            bytes,
            header_value(headers, "content-encoding"),
            header_value(headers, "content-type"),
        )
    } else {
        bytes.to_vec()
    };

    let content_type =
        header_value(headers, "content-type").unwrap_or("application/octet-stream").to_owned();
    ([(header::CONTENT_TYPE, content_type)], out).into_response()
}

fn parse_flow_id(id: &str) -> Option<FlowId> {
    Uuid::parse_str(id).ok().map(FlowId)
}

fn bad_id() -> Response {
    bad_request("invalid flow id")
}

fn bad_request(reason: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, reason).into_response()
}

fn parse_cursor(cursor: &str) -> Result<FlowId, &'static str> {
    parse_flow_id(cursor).ok_or("cursor must be a flow id")
}

fn wants_decode(query: &BodyQuery) -> bool {
    matches!(query.decode.as_deref(), Some("1" | "true"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{to_bytes, Bytes};
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use serde_json::Value;

    use super::AppState;
    use crate::store::Store;
    use crate::test_support::FlowBuilder;

    fn state_with(uris: &[&str]) -> AppState {
        let store = Arc::new(Store::new(1_000_000, 64));
        for uri in uris {
            store.insert(FlowBuilder::new().uri(uri).status(200).build());
        }
        AppState { store, ca_cert_pem: Arc::new("PEM".into()) }
    }

    async fn export(state: &AppState, query: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
        let uri = format!("http://x/flows/export?{query}").parse::<axum::http::Uri>().expect("uri");
        let query = Query::try_from_uri(&uri).expect("query");
        let response = super::export_har(query, State(state.clone())).await;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let value = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, headers, value)
    }

    async fn list(state: &AppState, query: &str) -> (StatusCode, Value) {
        let uri = format!("http://x/flows?{query}").parse::<axum::http::Uri>().expect("uri");
        let filter = Query::try_from_uri(&uri).expect("filter");
        let paging = Query::try_from_uri(&uri).expect("paging");
        let response = super::list_flows(filter, paging, State(state.clone())).await;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let value = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }

    async fn import(state: &AppState, document: &[u8]) -> (StatusCode, Value) {
        let response =
            super::import_flows(State(state.clone()), Bytes::copy_from_slice(document)).await;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let value = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
    }

    fn document(entries: &str) -> Vec<u8> {
        format!(r#"{{"log":{{"version":"1.2","entries":[{entries}]}}}}"#).into_bytes()
    }

    fn entry(url: &str) -> String {
        format!(
            r#"{{"startedDateTime":"2026-09-04T10:00:00Z","time":5,
                 "request":{{"method":"GET","url":"{url}","httpVersion":"HTTP/1.1",
                             "headers":[],"bodySize":0}},
                 "response":{{"status":200,"httpVersion":"HTTP/1.1","headers":[],
                              "content":{{"size":0,"mimeType":"text/plain"}}}}}}"#
        )
    }

    #[tokio::test]
    async fn import_reports_the_count_it_took() {
        let state = state_with(&[]);
        let (status, value) =
            import(&state, &document(&format!("{},{}", entry("http://a/"), entry("http://b/"))))
                .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value, serde_json::json!({ "imported": 2 }));

        let (_, listed) = list(&state, "source=imported").await;
        assert_eq!(listed.as_array().expect("rows").len(), 2);
    }

    #[tokio::test]
    async fn a_refused_import_says_why_and_stores_nothing() {
        let state = state_with(&["http://already/"]);
        for (document, expected) in [
            (b"not json".to_vec(), "not a JSON document"),
            (br#"{"log":{"version":"1.3","entries":[]}}"#.to_vec(), "1.3"),
            (document(r#"{"startedDateTime":"2026-09-04T10:00:00Z"}"#), "missing request"),
        ] {
            let (status, value) = import(&state, &document).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let reason = value.as_str().expect("a plain-text reason");
            assert!(reason.contains(expected), "expected {expected:?} in {reason:?}");
        }
        let (_, listed) = list(&state, "").await;
        assert_eq!(listed.as_array().expect("rows").len(), 1, "the capture is unchanged");
    }

    #[tokio::test]
    async fn the_source_filter_reaches_the_store_on_both_reading_endpoints() {
        let state = state_with(&["http://live/"]);
        import(&state, &document(&entry("http://file/"))).await;

        let (_, imported) = list(&state, "source=imported").await;
        assert_eq!(imported[0]["uri"], "http://file/");
        assert_eq!(imported.as_array().expect("rows").len(), 1);

        let (_, captured) = list(&state, "source=captured").await;
        assert_eq!(captured[0]["uri"], "http://live/");
        assert_eq!(captured.as_array().expect("rows").len(), 1);

        let (_, _, exported) = export(&state, "source=imported").await;
        let urls: Vec<&str> = exported["log"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e["request"]["url"].as_str().expect("url"))
            .collect();
        assert_eq!(urls, ["http://file/"], "the export inherits the filter unchanged");
    }

    // A silently empty listing reads to a caller exactly like "nothing matched".
    #[tokio::test]
    async fn an_unrecognised_source_is_refused_by_name_on_both_endpoints() {
        let state = state_with(&["http://a/"]);
        let (status, value) = list(&state, "source=proxied").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(value.as_str().expect("reason").contains("source"), "{value}");

        let (status, _, _) = export(&state, "source=proxied").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn without_a_limit_the_listing_is_the_bare_array_it_has_always_been() {
        let state = state_with(&["http://a/", "http://b/"]);
        let (status, value) = list(&state, "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value.as_array().expect("an array, not an envelope").len(), 2);
    }

    #[tokio::test]
    async fn a_limit_switches_the_response_to_the_paging_envelope() {
        let state = state_with(&["http://a/", "http://b/", "http://c/"]);
        let (status, value) = list(&state, "limit=2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["flows"].as_array().expect("flows").len(), 2);
        assert!(value["next_cursor"].is_string());

        let cursor = value["next_cursor"].as_str().expect("cursor");
        let (_, next) = list(&state, &format!("limit=2&cursor={cursor}")).await;
        assert_eq!(next["flows"].as_array().expect("flows").len(), 1);
        assert!(next["next_cursor"].is_null());
    }

    #[tokio::test]
    async fn paging_composes_with_the_filters() {
        let state = state_with(&["http://keep/1", "http://drop/1", "http://keep/2"]);
        let (_, value) = list(&state, "host=keep&limit=1").await;
        assert_eq!(value["flows"][0]["uri"], "http://keep/2");
        assert!(value["next_cursor"].is_string());
    }

    #[tokio::test]
    async fn malformed_paging_parameters_are_refused_by_name() {
        let state = state_with(&["http://a/"]);
        for (query, named) in [
            ("limit=0", "limit"),
            ("limit=none", "limit"),
            ("limit=-1", "limit"),
            ("cursor=not-a-uuid&limit=2", "cursor"),
            ("cursor=00000000-0000-0000-0000-000000000000", "cursor"),
        ] {
            let (status, value) = list(&state, query).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "query {query}");
            assert!(
                value.as_str().is_some_and(|reason| reason.contains(named)),
                "refusal for {query} must name {named}, got {value}"
            );
        }
    }

    #[tokio::test]
    async fn the_export_is_served_as_a_har_file_attachment() {
        let state = state_with(&["http://a/"]);
        let (status, headers, value) = export(&state, "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["content-type"], "application/json");
        let disposition = headers["content-disposition"].to_str().expect("ascii");
        assert!(disposition.starts_with("attachment; filename=\"trawler-"), "{disposition}");
        assert!(disposition.ends_with(".har\""), "{disposition}");
        assert_eq!(value["log"]["version"], "1.2");
    }

    #[tokio::test]
    async fn an_empty_capture_exports_a_valid_har_with_no_entries() {
        let (_, _, value) = export(&state_with(&[]), "").await;
        assert_eq!(value["log"]["version"], "1.2");
        assert_eq!(value["log"]["entries"].as_array().expect("entries").len(), 0);
    }

    #[tokio::test]
    async fn the_export_filter_reaches_the_store() {
        let state = state_with(&["http://keep/1", "http://drop/1"]);
        let (_, _, value) = export(&state, "host=keep").await;
        let entries = value["log"]["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["request"]["url"], "http://keep/1");
    }

    #[tokio::test]
    async fn ui_page_serves_the_embedded_shell_as_html() {
        let response = super::ui_page().await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/html; charset=utf-8");
        let body = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let html = std::str::from_utf8(&body).expect("utf8");
        assert_eq!(html, include_str!("ui.html"));
        for wired in ["/flows", "/stream", "EventSource"] {
            assert!(html.contains(wired), "shell page must wire {wired}");
        }
    }
}
