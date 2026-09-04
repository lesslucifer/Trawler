use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use super::decode;
use super::view::{self, FlowPageView, FlowSummary, FlowView};
use crate::flow::{header_value, BodyRec, FlowId, Header};
use crate::store::{FlowFilter, Store};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub ca_cert_pem: Arc<String>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    host: Option<String>,
    method: Option<String>,
    status: Option<u16>,
    content_type: Option<String>,
    q: Option<String>,
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
pub async fn list_flows(Query(query): Query<ListQuery>, State(state): State<AppState>) -> Response {
    let filter = FlowFilter {
        host: query.host,
        method: query.method,
        status: query.status,
        content_type: query.content_type,
        q: query.q,
    };

    let cursor = match query.cursor.as_deref().map(parse_cursor) {
        Some(Ok(id)) => Some(id),
        Some(Err(reason)) => return bad_request(reason),
        None => None,
    };

    let Some(limit) = query.limit.as_deref() else {
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

    use axum::body::to_bytes;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use serde_json::Value;

    use super::AppState;
    use crate::store::Store;
    use crate::test_support::FlowBuilder;

    fn state_with(uris: &[&str]) -> AppState {
        let store = Arc::new(Store::new(1_000_000, 64));
        for uri in uris {
            store.insert(FlowBuilder::new().uri(uri).build());
        }
        AppState { store, ca_cert_pem: Arc::new("PEM".into()) }
    }

    async fn list(state: &AppState, query: &str) -> (StatusCode, Value) {
        let uri = format!("http://x/flows?{query}").parse::<axum::http::Uri>().expect("uri");
        let query = Query::try_from_uri(&uri).expect("query");
        let response = super::list_flows(query, State(state.clone())).await;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let value = serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, value)
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
