use std::time::SystemTime;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::event::FlowEvent;
use crate::flow::{BodyRec, Flow, Header, NotCapturedReason};

fn rfc3339(t: SystemTime) -> String {
    OffsetDateTime::from(t).format(&Rfc3339).unwrap_or_default()
}

#[derive(Serialize)]
pub struct HeaderView {
    pub name: String,
    pub value: String,
}

impl From<&Header> for HeaderView {
    fn from(header: &Header) -> Self {
        Self { name: header.name.clone(), value: header.value.clone() }
    }
}

/// The serialized body shape returned by REST/SSE. Mirrors `BodyRec`, but drops
/// the raw `Bytes` in favor of base64 so it can cross the JSON boundary.
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BodyView {
    Captured { size: usize, data_base64: String },
    Truncated { captured: usize, total_hint: Option<usize>, data_base64: String },
    Streaming { content_type: String },
    NotCaptured { reason: &'static str, actual: Option<usize> },
    Empty,
}

impl From<&BodyRec> for BodyView {
    fn from(body: &BodyRec) -> Self {
        match body {
            BodyRec::Captured { bytes, size } => {
                BodyView::Captured { size: *size, data_base64: STANDARD.encode(bytes) }
            }
            BodyRec::Truncated { prefix, captured, total_hint } => BodyView::Truncated {
                captured: *captured,
                total_hint: *total_hint,
                data_base64: STANDARD.encode(prefix),
            },
            BodyRec::Streaming { content_type } => {
                BodyView::Streaming { content_type: content_type.clone() }
            }
            BodyRec::NotCaptured { reason } => match reason {
                NotCapturedReason::TooLarge { actual } => {
                    BodyView::NotCaptured { reason: "too_large", actual: Some(*actual) }
                }
                NotCapturedReason::ReadError => {
                    BodyView::NotCaptured { reason: "read_error", actual: None }
                }
            },
            BodyRec::Empty => BodyView::Empty,
        }
    }
}

/// A flattened, UI-ready projection of a `Flow` — the JSON returned by
/// `GET /flows/{id}`. Decoupled from the internal `Flow` so storage can evolve
/// without breaking the API contract.
#[derive(Serialize)]
pub struct FlowView {
    pub id: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub method: String,
    pub uri: String,
    pub status: Option<u16>,
    pub request_headers: Vec<HeaderView>,
    pub response_headers: Vec<HeaderView>,
    pub request_body: BodyView,
    pub response_body: Option<BodyView>,
}

impl From<&Flow> for FlowView {
    fn from(flow: &Flow) -> Self {
        Self {
            id: flow.id.to_string(),
            started_at: rfc3339(flow.started_at),
            duration_ms: flow.duration_ms,
            method: flow.request.method.clone(),
            uri: flow.request.uri.clone(),
            status: flow.response.as_ref().map(|r| r.status),
            request_headers: flow.request.headers.iter().map(HeaderView::from).collect(),
            response_headers: flow
                .response
                .as_ref()
                .map(|r| r.headers.iter().map(HeaderView::from).collect())
                .unwrap_or_default(),
            request_body: BodyView::from(&flow.request.body),
            response_body: flow.response.as_ref().map(|r| BodyView::from(&r.body)),
        }
    }
}

/// The lean row returned by `GET /flows` — no header lists, no base64 bodies, so
/// a listing stays cheap even with large captured payloads.
#[derive(Serialize)]
pub struct FlowSummary {
    pub id: String,
    pub started_at: String,
    pub duration_ms: Option<u64>,
    pub method: String,
    pub uri: String,
    pub status: Option<u16>,
    pub request_body_state: &'static str,
    pub response_body_state: Option<&'static str>,
}

impl From<&Flow> for FlowSummary {
    fn from(flow: &Flow) -> Self {
        Self {
            id: flow.id.to_string(),
            started_at: rfc3339(flow.started_at),
            duration_ms: flow.duration_ms,
            method: flow.request.method.clone(),
            uri: flow.request.uri.clone(),
            status: flow.response.as_ref().map(|r| r.status),
            request_body_state: flow.request.body.state(),
            response_body_state: flow.response.as_ref().map(|r| r.body.state()),
        }
    }
}

/// Compact live payload for the SSE stream: a lifecycle notification, not a full
/// body dump. Clients fetch detail via `/flows/{id}` when they want it.
pub fn sse_event(event: &FlowEvent) -> (&'static str, Value) {
    match event {
        FlowEvent::RequestCaptured { flow_id, req } => (
            "request",
            json!({
                "flow_id": flow_id.to_string(),
                "method": req.method,
                "uri": req.uri,
                "request_body_state": req.body.state(),
            }),
        ),
        FlowEvent::ResponseCaptured { flow_id, res, duration_ms } => (
            "response",
            json!({
                "flow_id": flow_id.to_string(),
                "status": res.status,
                "duration_ms": duration_ms,
                "response_body_state": res.body.state(),
            }),
        ),
        FlowEvent::FlowEvicted { flow_id } => {
            ("evicted", json!({ "flow_id": flow_id.to_string() }))
        }
    }
}
