use crate::flow::{FlowId, RequestRec, ResponseRec};

/// Incremental lifecycle transitions broadcast to every subscriber. A single
/// "flow updated" event could not express request-before-response ordering or
/// ring-buffer eviction without overloading its payload.
#[derive(Clone, Debug)]
pub enum FlowEvent {
    RequestCaptured { flow_id: FlowId, req: RequestRec },
    ResponseCaptured { flow_id: FlowId, res: ResponseRec, duration_ms: u64 },
    FlowEvicted { flow_id: FlowId },
}
