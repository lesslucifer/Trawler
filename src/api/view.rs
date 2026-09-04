use std::time::SystemTime;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::event::FlowEvent;
use crate::flow::{BodyRec, Flow, Header, NotCapturedReason};
use crate::store::FlowPage;

/// The timestamp format every projection puts on the wire, defined once so the
/// REST views and the HAR export cannot come to render an instant differently.
pub(super) fn rfc3339(t: SystemTime) -> String {
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
    pub request_http_version: String,
    pub response_http_version: Option<String>,
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
            request_http_version: flow.request.version.clone(),
            response_http_version: flow.response.as_ref().map(|r| r.version.clone()),
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

/// The envelope returned by `GET /flows` when paging was asked for. The
/// unparameterised listing stays a bare array — a shape that cannot carry a
/// cursor, and one every existing client already reads.
#[derive(Serialize)]
pub struct FlowPageView {
    pub flows: Vec<FlowSummary>,
    pub next_cursor: Option<String>,
}

impl From<&FlowPage> for FlowPageView {
    fn from(page: &FlowPage) -> Self {
        Self {
            flows: page.flows.iter().map(FlowSummary::from).collect(),
            next_cursor: page.next_cursor.map(|id| id.to_string()),
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
        FlowEvent::ConnectionSeen { authority } => {
            ("connection", json!({ "authority": authority }))
        }
    }
}

/// Emitted when a slow SSE consumer falls behind the broadcast buffer, so the
/// gap in lifecycle events is visible instead of silent.
pub fn sse_lagged(skipped: u64) -> (&'static str, Value) {
    ("lagged", json!({ "skipped": skipped }))
}

#[cfg(test)]
mod tests {
    use super::{sse_event, BodyView, FlowSummary, FlowView};
    use crate::event::FlowEvent;
    use crate::flow::{BodyRec, NotCapturedReason, ResponseRec};
    use crate::test_support::{captured, FlowBuilder};
    use serde_json::json;

    fn body_view(body: &BodyRec) -> serde_json::Value {
        serde_json::to_value(BodyView::from(body)).unwrap()
    }

    #[test]
    fn body_view_wire_shapes() {
        assert_eq!(
            body_view(&captured(b"hi")),
            json!({"state":"captured","size":2,"data_base64":"aGk="})
        );
        assert_eq!(
            body_view(&BodyRec::Streaming { content_type: "text/event-stream".into() }),
            json!({"state":"streaming","content_type":"text/event-stream"})
        );
        assert_eq!(
            body_view(&BodyRec::NotCaptured { reason: NotCapturedReason::TooLarge { actual: 42 } }),
            json!({"state":"not_captured","reason":"too_large","actual":42})
        );
        assert_eq!(
            body_view(&BodyRec::NotCaptured { reason: NotCapturedReason::ReadError }),
            json!({"state":"not_captured","reason":"read_error","actual":null})
        );
        assert_eq!(body_view(&BodyRec::Empty), json!({"state":"empty"}));
    }

    #[test]
    fn flow_view_full_shape() {
        let flow = FlowBuilder::new()
            .method("POST")
            .uri("http://x/y")
            .req_header("A", "b")
            .req_body(captured(b"req"))
            .status(201)
            .resp_header("C", "d")
            .resp_body(captured(b"res"))
            .build();
        let v = serde_json::to_value(FlowView::from(&flow)).unwrap();
        assert_eq!(v["method"], "POST");
        assert_eq!(v["uri"], "http://x/y");
        assert_eq!(v["status"], 201);
        assert_eq!(v["request_http_version"], "HTTP/1.1");
        assert_eq!(v["response_http_version"], "HTTP/1.1");
        assert_eq!(v["duration_ms"], 1);
        assert_eq!(v["request_headers"], json!([{"name":"A","value":"b"}]));
        assert_eq!(v["response_headers"], json!([{"name":"C","value":"d"}]));
        assert_eq!(v["request_body"]["state"], "captured");
        assert_eq!(v["response_body"]["state"], "captured");
        assert!(v["started_at"].as_str().unwrap().contains('T'));
    }

    #[test]
    fn flow_view_without_response() {
        let v = serde_json::to_value(FlowView::from(&FlowBuilder::new().build())).unwrap();
        assert_eq!(v["status"], serde_json::Value::Null);
        assert_eq!(v["response_http_version"], serde_json::Value::Null);
        assert_eq!(v["response_body"], serde_json::Value::Null);
        assert_eq!(v["response_headers"], json!([]));
    }

    #[test]
    fn flow_summary_is_lean() {
        let flow = FlowBuilder::new()
            .req_body(captured(b"req"))
            .status(200)
            .resp_body(captured(b"res"))
            .build();
        let v = serde_json::to_value(FlowSummary::from(&flow)).unwrap();
        assert_eq!(v["request_body_state"], "captured");
        assert_eq!(v["response_body_state"], "captured");
        assert!(v.get("request_headers").is_none());
        assert!(v.get("request_body").is_none());
    }

    #[test]
    fn sse_event_shapes() {
        let flow = FlowBuilder::new().method("GET").uri("http://x/").build();
        let (name, data) =
            sse_event(&FlowEvent::RequestCaptured { flow_id: flow.id, req: flow.request.clone() });
        assert_eq!(name, "request");
        assert_eq!(data["method"], "GET");
        assert_eq!(data["uri"], "http://x/");
        assert_eq!(data["request_body_state"], "empty");

        let res = ResponseRec {
            status: 200,
            version: "HTTP/1.1".into(),
            headers: vec![],
            body: BodyRec::Empty,
        };
        let (name, data) =
            sse_event(&FlowEvent::ResponseCaptured { flow_id: flow.id, res, duration_ms: 7 });
        assert_eq!(name, "response");
        assert_eq!(data["status"], 200);
        assert_eq!(data["duration_ms"], 7);
        assert_eq!(data["response_body_state"], "empty");

        let (name, data) = sse_event(&FlowEvent::FlowEvicted { flow_id: flow.id });
        assert_eq!(name, "evicted");
        assert_eq!(data["flow_id"], flow.id.to_string());

        let (name, data) =
            sse_event(&FlowEvent::ConnectionSeen { authority: "example.com:443".into() });
        assert_eq!(name, "connection");
        assert_eq!(data["authority"], "example.com:443");
    }

    #[test]
    fn sse_lagged_shape() {
        let (name, data) = super::sse_lagged(7);
        assert_eq!(name, "lagged");
        assert_eq!(data, json!({ "skipped": 7 }));
    }
}
