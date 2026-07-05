use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use uuid::Uuid;

use super::decode;
use super::view::{self, FlowSummary, FlowView};
use crate::flow::{BodyRec, FlowId, Header};
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
    (StatusCode::BAD_REQUEST, "invalid flow id").into_response()
}

fn wants_decode(query: &BodyQuery) -> bool {
    matches!(query.decode.as_deref(), Some("1" | "true"))
}

fn header_value<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}
