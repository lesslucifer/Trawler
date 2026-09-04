use crate::flow::{FlowId, RequestRec, ResponseRec};

/// Everything the proxy observes, broadcast to every subscriber. Mostly flow
/// lifecycle (a single "flow updated" event could not express
/// request-before-response ordering or ring-buffer eviction without overloading
/// its payload), plus `ConnectionSeen` for the tunnels that never become a flow:
/// an HTTPS `CONNECT` whose client doesn't trust the CA fails its handshake and
/// leaves no decoded request, so this is the only trace that traffic reached the
/// proxy at all.
///
/// `FlowsCleared` is distinct from `FlowEvicted` because a clear is a user
/// acting and an eviction is the ring protecting itself: an interface that
/// cannot tell them apart would announce "capture cleared" every time memory
/// pressure dropped a row. It carries the whole batch rather than one event per
/// flow so clearing a full ring cannot lag every subscriber past the broadcast
/// capacity.
#[derive(Clone, Debug)]
pub enum FlowEvent {
    RequestCaptured { flow_id: FlowId, req: RequestRec },
    ResponseCaptured { flow_id: FlowId, res: ResponseRec, duration_ms: u64 },
    FlowEvicted { flow_id: FlowId },
    FlowsCleared { flow_ids: Vec<FlowId> },
    ConnectionSeen { authority: String },
}
