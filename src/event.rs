use crate::flow::{FlowId, RequestRec, ResponseRec};

/// Everything the proxy observes, broadcast to every subscriber. Mostly flow
/// lifecycle (a single "flow updated" event could not express
/// request-before-response ordering or ring-buffer eviction without overloading
/// its payload), plus `ConnectionSeen` for the tunnels that never become a flow:
/// an HTTPS `CONNECT` whose client doesn't trust the CA fails its handshake and
/// leaves no decoded request, so this is the only trace that traffic reached the
/// proxy at all.
#[derive(Clone, Debug)]
pub enum FlowEvent {
    RequestCaptured { flow_id: FlowId, req: RequestRec },
    ResponseCaptured { flow_id: FlowId, res: ResponseRec, duration_ms: u64 },
    FlowEvicted { flow_id: FlowId },
    ConnectionSeen { authority: String },
}
