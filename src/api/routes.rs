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
use super::view::{self, FlowSummary, FlowView};
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

#[derive(Clone, Copy)]
enum BodyView {
    Stored,
    Source,
    Decoded,
}

pub async fn list_flows(
    Query(query): Query<ListQuery>,
    State(state): State<AppState>,
) -> Json<Vec<FlowSummary>> {
    let filter = FlowFilter {
        host: query.host,
        method: query.method,
        status: query.status,
        content_type: query.content_type,
        q: query.q,
    };
    let summaries = state.store.list(&filter).iter().map(FlowSummary::from).collect();
    Json(summaries)
}

/// One-shot counterpart to the `connection` SSE event: "has any traffic reached
/// the proxy yet?" The onboarding reads it once on entry (like W4's `/flows`
/// read) so a core that saw traffic before the UI subscribed reads as idle
/// rather than as never-used; live updates then ride the stream.
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

fn body_response(state: &AppState, id: &str, leg: Leg, decode: BodyView) -> Response {
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

    let out = match decode {
        BodyView::Decoded => decode::decode_body(
            bytes,
            header_value(headers, "content-encoding"),
            header_value(headers, "content-type"),
        ),
        BodyView::Source => {
            decode::decompress_body(bytes, header_value(headers, "content-encoding"))
        }
        BodyView::Stored => bytes.to_vec(),
    };

    let content_type =
        header_value(headers, "content-type").unwrap_or("application/octet-stream").to_owned();
    ([(header::CONTENT_TYPE, content_type)], out).into_response()
}

fn parse_flow_id(id: &str) -> Option<FlowId> {
    Uuid::parse_str(id).ok().map(FlowId)
}

fn bad_id() -> Response {
    (StatusCode::BAD_REQUEST, "invalid flow id").into_response()
}

/// `decode=1` renders the payload; `decode=source` only reverses the transport
/// encoding, so a compressed body is readable without being re-indented; absent
/// or anything else is the stored bytes.
fn wants_decode(query: &BodyQuery) -> BodyView {
    match query.decode.as_deref() {
        Some("1" | "true") => BodyView::Decoded,
        Some("source") => BodyView::Source,
        _ => BodyView::Stored,
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

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
