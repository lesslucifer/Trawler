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
            let flow_host = flow.host().unwrap_or_default().to_ascii_lowercase();
            if !flow_host.contains(&host.to_ascii_lowercase()) {
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
                .is_some_and(|c| c.to_ascii_lowercase().contains(&needle))
                || flow
                    .response_content_type()
                    .is_some_and(|c| c.to_ascii_lowercase().contains(&needle));
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
        let event_res = res.clone();
        let evicted = {
            let mut ring = self.inner.lock().expect("store lock");
            let Some(flow) = ring.flows.get_mut(&id) else {
                return;
            };
            let before = flow.retained_bytes();
            flow.response = Some(res);
            flow.duration_ms = Some(duration_ms);
            let after = flow.retained_bytes();
            ring.total_bytes += after.saturating_sub(before);
            ring.evict()
        };
        let _ =
            self.tx.send(FlowEvent::ResponseCaptured { flow_id: id, res: event_res, duration_ms });
        self.announce_evictions(evicted);
    }

    pub fn get(&self, id: FlowId) -> Option<Flow> {
        self.inner.lock().expect("store lock").flows.get(&id).cloned()
    }

    #[cfg(test)]
    fn total_bytes(&self) -> usize {
        self.inner.lock().expect("store lock").total_bytes
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().expect("store lock").flows.len()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::FlowEvent;
    use crate::flow::{BodyRec, Flow, FlowId, ResponseRec};
    use crate::test_support::{captured, FlowBuilder};

    fn store() -> Store {
        Store::new(1_000_000, 64)
    }

    fn resp(status: u16) -> ResponseRec {
        ResponseRec {
            status,
            version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body: BodyRec::Empty,
        }
    }

    fn uris(flows: &[Flow]) -> Vec<&str> {
        flows.iter().map(|f| f.request.uri.as_str()).collect()
    }

    #[test]
    fn insert_get_and_missing() {
        let s = store();
        let flow = FlowBuilder::new().build();
        let id = flow.id;
        s.insert(flow);
        assert!(s.get(id).is_some());
        assert!(s.get(FlowId::new()).is_none());
    }

    #[test]
    fn list_is_newest_first() {
        let s = store();
        for u in ["http://a/", "http://b/", "http://c/"] {
            s.insert(FlowBuilder::new().uri(u).build());
        }
        let list = s.list(&FlowFilter::default());
        assert_eq!(uris(&list), vec!["http://c/", "http://b/", "http://a/"]);
    }

    #[test]
    fn set_response_updates_and_ignores_unknown() {
        let s = store();
        let flow = FlowBuilder::new().build();
        let id = flow.id;
        s.insert(flow);
        s.set_response(id, resp(201), 42);
        let got = s.get(id).unwrap();
        assert_eq!(got.response.unwrap().status, 201);
        assert_eq!(got.duration_ms, Some(42));
        s.set_response(FlowId::new(), resp(200), 1);
    }

    #[test]
    fn eviction_is_byte_weighted_oldest_first() {
        let s = Store::new(10, 64);
        s.insert(FlowBuilder::new().uri("http://old/").req_body(captured(&[0u8; 8])).build());
        s.insert(FlowBuilder::new().uri("http://s1/").req_body(captured(&[0u8; 3])).build());
        s.insert(FlowBuilder::new().uri("http://s2/").req_body(captured(&[0u8; 3])).build());
        let list = s.list(&FlowFilter::default());
        assert_eq!(uris(&list), vec!["http://s2/", "http://s1/"]);
        assert_eq!(s.total_bytes(), 6);
    }

    #[test]
    fn keep_newest_even_if_over_cap() {
        let s = Store::new(10, 64);
        s.insert(FlowBuilder::new().req_body(captured(&[0u8; 100])).build());
        assert_eq!(s.len(), 1);
        assert!(s.total_bytes() > 10);
    }

    #[test]
    fn filter_fields_and_composition() {
        let s = store();
        s.insert(
            FlowBuilder::new()
                .method("POST")
                .uri("http://api.example.com/users")
                .req_header("Content-Type", "application/json")
                .status(201)
                .build(),
        );
        s.insert(
            FlowBuilder::new()
                .method("GET")
                .uri("http://cdn.other.com/img")
                .status(200)
                .resp_header("Content-Type", "image/png")
                .build(),
        );

        let one = |f: FlowFilter| s.list(&f).len();
        assert_eq!(one(FlowFilter { host: Some("EXAMPLE".into()), ..Default::default() }), 1);
        assert_eq!(one(FlowFilter { method: Some("post".into()), ..Default::default() }), 1);
        assert_eq!(one(FlowFilter { status: Some(200), ..Default::default() }), 1);
        assert_eq!(one(FlowFilter { content_type: Some("json".into()), ..Default::default() }), 1);
        assert_eq!(one(FlowFilter { content_type: Some("png".into()), ..Default::default() }), 1);
        assert_eq!(one(FlowFilter { q: Some("users".into()), ..Default::default() }), 1);
        assert_eq!(
            one(FlowFilter { method: Some("GET".into()), status: Some(201), ..Default::default() }),
            0
        );
        assert_eq!(one(FlowFilter::default()), 2);
    }

    #[test]
    fn status_filter_excludes_responseless() {
        let s = store();
        s.insert(FlowBuilder::new().build());
        assert_eq!(s.list(&FlowFilter { status: Some(200), ..Default::default() }).len(), 0);
    }

    #[test]
    fn events_on_insert_response_and_evict() {
        let s = Store::new(10, 64);
        let mut rx = s.subscribe();

        let first = FlowBuilder::new().req_body(captured(&[0u8; 8])).build();
        let first_id = first.id;
        s.insert(first);
        assert!(
            matches!(rx.try_recv(), Ok(FlowEvent::RequestCaptured { flow_id, .. }) if flow_id == first_id)
        );

        s.set_response(first_id, resp(200), 5);
        assert!(
            matches!(rx.try_recv(), Ok(FlowEvent::ResponseCaptured { flow_id, duration_ms, .. }) if flow_id == first_id && duration_ms == 5)
        );

        let second = FlowBuilder::new().req_body(captured(&[0u8; 8])).build();
        let second_id = second.id;
        s.insert(second);
        assert!(
            matches!(rx.try_recv(), Ok(FlowEvent::RequestCaptured { flow_id, .. }) if flow_id == second_id)
        );
        assert!(
            matches!(rx.try_recv(), Ok(FlowEvent::FlowEvicted { flow_id }) if flow_id == first_id)
        );
    }

    proptest::proptest! {
        #[test]
        fn ring_accounting(sizes in proptest::collection::vec(0usize..40, 0..30), max in 1usize..80) {
            let s = Store::new(max, 256);
            for sz in &sizes {
                let body = if *sz == 0 { BodyRec::Empty } else { captured(&vec![0u8; *sz]) };
                s.insert(FlowBuilder::new().req_body(body).build());
            }
            let held: usize = s.list(&FlowFilter::default()).iter().map(super::super::flow::Flow::retained_bytes).sum();
            proptest::prop_assert_eq!(held, s.total_bytes());
            proptest::prop_assert!(s.total_bytes() <= max || s.len() == 1);
        }

        #[test]
        fn listing_strictly_newest_first(n in 0usize..30) {
            let s = Store::new(1_000_000, 256);
            for i in 0..n {
                s.insert(FlowBuilder::new().uri(&format!("http://h/{i}")).build());
            }
            let list = s.list(&FlowFilter::default());
            let got: Vec<String> = list.iter().map(|f| f.request.uri.clone()).collect();
            let expected: Vec<String> = (0..n).rev().map(|i| format!("http://h/{i}")).collect();
            proptest::prop_assert_eq!(got, expected);
        }

        #[test]
        fn filter_only_shrinks(n in 1usize..20, needle in "[a-c]{1,3}") {
            let s = Store::new(1_000_000, 256);
            for i in 0..n {
                let pre = if i % 2 == 0 { "aa" } else { "bb" };
                s.insert(FlowBuilder::new().uri(&format!("http://{pre}{i}/")).build());
            }
            let all = s.list(&FlowFilter::default()).len();
            let filtered = s.list(&FlowFilter { host: Some(needle), ..Default::default() }).len();
            proptest::prop_assert!(filtered <= all);
        }
    }
}
