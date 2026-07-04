use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use tokio::sync::broadcast;

use crate::event::FlowEvent;
use crate::flow::{Flow, FlowId, ResponseRec};

/// Canonical owner of live flows. A bounded ring buffer weighted by retained
/// body bytes, fronted by a broadcast bus so every interface can live-tail
/// without the capture path knowing any interface exists.
pub struct Store {
    inner: Mutex<Ring>,
    tx: broadcast::Sender<FlowEvent>,
}

struct Ring {
    order: VecDeque<FlowId>,
    flows: HashMap<FlowId, Flow>,
    total_bytes: usize,
    max_bytes: usize,
}

impl Ring {
    fn evict(&mut self) -> Vec<FlowId> {
        let mut evicted = Vec::new();
        while self.total_bytes > self.max_bytes && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(flow) = self.flows.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(flow.retained_bytes());
                evicted.push(oldest);
            }
        }
        evicted
    }
}

#[derive(Default, Debug)]
pub struct FlowFilter {
    pub host: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub q: Option<String>,
}

impl FlowFilter {
    pub fn matches(&self, flow: &Flow) -> bool {
        if let Some(host) = &self.host {
            let flow_host = flow.host().unwrap_or_default();
            if !flow_host.to_ascii_lowercase().contains(&host.to_ascii_lowercase()) {
                return false;
            }
        }
        if let Some(method) = &self.method {
            if !flow.request.method.eq_ignore_ascii_case(method) {
                return false;
            }
        }
        if let Some(status) = self.status {
            if flow.response.as_ref().map(|r| r.status) != Some(status) {
                return false;
            }
        }
        if let Some(ct) = &self.content_type {
            let needle = ct.to_ascii_lowercase();
            let hit = flow
                .request_content_type()
                .map(|c| c.to_ascii_lowercase().contains(&needle))
                .unwrap_or(false)
                || flow
                    .response_content_type()
                    .map(|c| c.to_ascii_lowercase().contains(&needle))
                    .unwrap_or(false);
            if !hit {
                return false;
            }
        }
        if let Some(q) = &self.q {
            if !flow.request.uri.to_ascii_lowercase().contains(&q.to_ascii_lowercase()) {
                return false;
            }
        }
        true
    }
}

impl Store {
    pub fn new(max_bytes: usize, broadcast_capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(broadcast_capacity);
        Self {
            inner: Mutex::new(Ring {
                order: VecDeque::new(),
                flows: HashMap::new(),
                total_bytes: 0,
                max_bytes,
            }),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<FlowEvent> {
        self.tx.subscribe()
    }

    pub fn insert(&self, flow: Flow) {
        let id = flow.id;
        let req = flow.request.clone();
        let evicted = {
            let mut ring = self.inner.lock().expect("store lock");
            ring.total_bytes += flow.retained_bytes();
            ring.order.push_back(id);
            ring.flows.insert(id, flow);
            ring.evict()
        };
        let _ = self.tx.send(FlowEvent::RequestCaptured { flow_id: id, req });
        self.announce_evictions(evicted);
    }

    pub fn set_response(&self, id: FlowId, res: ResponseRec, duration_ms: u64) {
        let evicted = {
            let mut ring = self.inner.lock().expect("store lock");
            let Some(flow) = ring.flows.get_mut(&id) else {
                return;
            };
            let before = flow.retained_bytes();
            flow.response = Some(res.clone());
            flow.duration_ms = Some(duration_ms);
            let after = flow.retained_bytes();
            ring.total_bytes += after.saturating_sub(before);
            ring.evict()
        };
        let _ = self
            .tx
            .send(FlowEvent::ResponseCaptured { flow_id: id, res, duration_ms });
        self.announce_evictions(evicted);
    }

    pub fn get(&self, id: FlowId) -> Option<Flow> {
        self.inner.lock().expect("store lock").flows.get(&id).cloned()
    }

    pub fn list(&self, filter: &FlowFilter) -> Vec<Flow> {
        let ring = self.inner.lock().expect("store lock");
        ring.order
            .iter()
            .rev()
            .filter_map(|id| ring.flows.get(id))
            .filter(|flow| filter.matches(flow))
            .cloned()
            .collect()
    }

    fn announce_evictions(&self, evicted: Vec<FlowId>) {
        for flow_id in evicted {
            let _ = self.tx.send(FlowEvent::FlowEvicted { flow_id });
        }
    }
}
