use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::broadcast;

use crate::event::FlowEvent;
use crate::flow::{Flow, FlowId, ResponseRec};

/// Canonical owner of live flows. A bounded ring buffer weighted by retained
/// body bytes, fronted by a broadcast bus so every interface can live-tail
/// without the capture path knowing any interface exists. `connections` counts
/// every tunnel the proxy opened — kept outside the ring so eviction can never
/// touch it: it is a monotonic "traffic reached us" tally, not storage.
pub struct Store {
    inner: Mutex<Ring>,
    connections: AtomicU64,
    tx: broadcast::Sender<FlowEvent>,
}

struct Ring {
    order: VecDeque<FlowId>,
    flows: HashMap<FlowId, Flow>,
    total_bytes: usize,
    max_bytes: usize,
}

impl Ring {
    /// Newest-first is the listing order every read shares, so both `list` and
    /// `page` walk through here rather than each spelling out the reversal.
    fn newest_first(&self) -> impl Iterator<Item = &FlowId> {
        self.order.iter().rev()
    }

    /// Resolves ids to flows and applies the filter — the second half of the
    /// listing rule, kept beside the ordering so the two cannot drift apart.
    fn matching<'a>(
        &'a self,
        ids: impl Iterator<Item = &'a FlowId> + 'a,
        filter: &'a FlowFilter,
    ) -> impl Iterator<Item = &'a Flow> + 'a {
        ids.filter_map(|id| self.flows.get(id)).filter(|flow| filter.matches(flow))
    }

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

/// One page of a filtered listing. `next_cursor` is the id to resume after, and
/// its absence means the walk reached the oldest row the ring still holds.
#[derive(Default, Debug)]
pub struct FlowPage {
    pub flows: Vec<Flow>,
    pub next_cursor: Option<FlowId>,
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
            connections: AtomicU64::new(0),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<FlowEvent> {
        self.tx.subscribe()
    }

    /// The proxy opened a tunnel to `authority`. Records "traffic reached us"
    /// even when the exchange never decodes into a flow (untrusted-CA handshake
    /// failure), so the onboarding proxy step can go green without a decrypt.
    pub fn note_connection(&self, authority: String) {
        self.connections.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(FlowEvent::ConnectionSeen { authority });
    }

    pub fn connection_count(&self) -> u64 {
        self.connections.load(Ordering::Relaxed)
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
        ring.matching(ring.newest_first(), filter).cloned().collect()
    }

    /// One bounded page of `list`, resumed at `after`. The whole walk happens
    /// under one lock, so a page is a consistent slice of the ring rather than
    /// a stitch of two reads.
    ///
    /// `after` names a row, not an offset, which is what makes the walk safe
    /// against a live capture: a concurrent insert lands newer than the cursor
    /// and so sits behind the walk, and an eviction only removes rows the walk
    /// would otherwise have reached. When the cursor row is itself evicted the
    /// ring has no position to resume from, so the page is empty and final —
    /// restarting from the newest row would re-serve everything the caller
    /// already holds.
    pub fn page(&self, filter: &FlowFilter, limit: usize, after: Option<FlowId>) -> FlowPage {
        let ring = self.inner.lock().expect("store lock");
        let mut walk = ring.newest_first();

        // The cursor is positioned over the unfiltered order, so it stays valid
        // when a client narrows the filter between pages.
        if let Some(cursor) = after {
            if !walk.any(|id| *id == cursor) {
                return FlowPage::default();
            }
        }

        let mut matching = ring.matching(walk, filter);
        let flows: Vec<Flow> = matching.by_ref().take(limit).cloned().collect();
        // Peeking one past the page is what lets a full final page report no
        // cursor, so a caller stops on `next_cursor` rather than on an empty page.
        let next_cursor = matching.next().and(flows.last()).map(|flow| flow.id);
        FlowPage { flows, next_cursor }
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
    fn page_walks_newest_first_and_ends_without_a_cursor() {
        let s = store();
        for u in ["http://a/", "http://b/", "http://c/", "http://d/", "http://e/"] {
            s.insert(FlowBuilder::new().uri(u).build());
        }

        let first = s.page(&FlowFilter::default(), 2, None);
        assert_eq!(uris(&first.flows), vec!["http://e/", "http://d/"]);

        let second = s.page(&FlowFilter::default(), 2, first.next_cursor);
        assert_eq!(uris(&second.flows), vec!["http://c/", "http://b/"]);

        let last = s.page(&FlowFilter::default(), 2, second.next_cursor);
        assert_eq!(uris(&last.flows), vec!["http://a/"]);
        assert_eq!(last.next_cursor, None);
    }

    #[test]
    fn a_page_that_exactly_empties_the_ring_reports_no_cursor() {
        let s = store();
        s.insert(FlowBuilder::new().uri("http://a/").build());
        s.insert(FlowBuilder::new().uri("http://b/").build());

        let page = s.page(&FlowFilter::default(), 2, None);
        assert_eq!(uris(&page.flows), vec!["http://b/", "http://a/"]);
        assert_eq!(page.next_cursor, None, "a full final page must not hand out a dead cursor");
    }

    #[test]
    fn page_limit_beyond_the_ring_returns_everything_once() {
        let s = store();
        for u in ["http://a/", "http://b/"] {
            s.insert(FlowBuilder::new().uri(u).build());
        }
        let page = s.page(&FlowFilter::default(), 99, None);
        assert_eq!(uris(&page.flows), vec!["http://b/", "http://a/"]);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn page_of_an_empty_ring_is_empty_and_final() {
        let page = store().page(&FlowFilter::default(), 5, None);
        assert!(page.flows.is_empty());
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn page_walks_the_filtered_set_not_the_whole_ring() {
        let s = store();
        for i in 0..6 {
            let host = if i % 2 == 0 { "keep" } else { "drop" };
            s.insert(FlowBuilder::new().uri(&format!("http://{host}/{i}")).build());
        }
        let filter = || FlowFilter { host: Some("keep".into()), ..Default::default() };

        let first = s.page(&filter(), 2, None);
        assert_eq!(uris(&first.flows), vec!["http://keep/4", "http://keep/2"]);
        let second = s.page(&filter(), 2, first.next_cursor);
        assert_eq!(uris(&second.flows), vec!["http://keep/0"]);
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn a_cursor_the_ring_no_longer_holds_ends_the_walk() {
        let s = store();
        s.insert(FlowBuilder::new().uri("http://a/").build());
        let page = s.page(&FlowFilter::default(), 2, Some(FlowId::new()));
        assert!(
            page.flows.is_empty() && page.next_cursor.is_none(),
            "an evicted cursor must not silently restart the walk at the newest row"
        );
    }

    #[test]
    fn a_cursor_the_filter_excludes_still_positions_the_walk() {
        let s = store();
        s.insert(FlowBuilder::new().uri("http://keep/old").build());
        let pivot = FlowBuilder::new().uri("http://drop/pivot").build();
        let pivot_id = pivot.id;
        s.insert(pivot);
        s.insert(FlowBuilder::new().uri("http://keep/new").build());

        let filter = FlowFilter { host: Some("keep".into()), ..Default::default() };
        let page = s.page(&filter, 5, Some(pivot_id));
        assert_eq!(uris(&page.flows), vec!["http://keep/old"]);
    }

    #[test]
    fn an_insert_between_pages_stays_behind_the_walk() {
        let s = store();
        for u in ["http://a/", "http://b/", "http://c/"] {
            s.insert(FlowBuilder::new().uri(u).build());
        }
        let first = s.page(&FlowFilter::default(), 2, None);
        s.insert(FlowBuilder::new().uri("http://newcomer/").build());

        let second = s.page(&FlowFilter::default(), 2, first.next_cursor);
        assert_eq!(uris(&second.flows), vec!["http://a/"], "a newer row is not served twice");
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

    #[test]
    fn note_connection_counts_and_broadcasts_independently_of_flows() {
        let s = Store::new(10, 64);
        let mut rx = s.subscribe();
        assert_eq!(s.connection_count(), 0);

        s.note_connection("example.com:443".into());
        assert_eq!(s.connection_count(), 1);
        assert!(
            matches!(rx.try_recv(), Ok(FlowEvent::ConnectionSeen { authority }) if authority == "example.com:443")
        );

        // A flow insert that evicts an older flow leaves the connection tally alone.
        s.insert(FlowBuilder::new().req_body(captured(&[0u8; 8])).build());
        s.insert(FlowBuilder::new().req_body(captured(&[0u8; 8])).build());
        s.note_connection("other:443".into());
        assert_eq!(s.connection_count(), 2, "eviction and inserts never touch the count");
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

        /// The WU's central claim: walking a live ring by cursor serves every row
        /// that survives the walk exactly once. Inserts and evictions are driven
        /// between pages, so the walk is never over a frozen store.
        #[test]
        fn paging_a_live_ring_never_duplicates_or_skips(
            seed in proptest::collection::vec(0usize..12, 1..25),
            churn in proptest::collection::vec(0usize..12, 0..12),
            limit in 1usize..5,
            max in 20usize..200,
        ) {
            let s = Store::new(max, 512);
            let body = |sz: usize| if sz == 0 { BodyRec::Empty } else { captured(&vec![0u8; sz]) };
            for sz in &seed {
                s.insert(FlowBuilder::new().req_body(body(*sz)).build());
            }

            let before: Vec<FlowId> =
                s.list(&FlowFilter::default()).iter().map(|f| f.id).collect();

            let mut served = Vec::new();
            let mut cursor = None;
            let budget = limit * (1 + seed.len() + churn.len());
            let mut churn = churn.iter();
            loop {
                let page = s.page(&FlowFilter::default(), limit, cursor);
                served.extend(page.flows.iter().map(|f| f.id));
                let Some(next) = page.next_cursor else { break };
                cursor = Some(next);
                if let Some(sz) = churn.next() {
                    s.insert(FlowBuilder::new().req_body(body(*sz)).build());
                }
            }

            let mut seen = std::collections::HashSet::new();
            for id in &served {
                proptest::prop_assert!(seen.insert(*id), "row {id} served twice");
            }
            proptest::prop_assert!(served.len() <= budget, "the walk must terminate, not cycle");

            // Every row that was present when the walk started and is still held
            // now was reachable by the walk — nothing was skipped over.
            let after: std::collections::HashSet<FlowId> =
                s.list(&FlowFilter::default()).iter().map(|f| f.id).collect();
            for id in before.iter().filter(|id| after.contains(id)) {
                proptest::prop_assert!(seen.contains(id), "row {id} survived but was skipped");
            }
        }

        #[test]
        fn a_page_walk_matches_the_unpaged_listing_on_a_still_ring(
            n in 0usize..25,
            limit in 1usize..6,
        ) {
            let s = Store::new(1_000_000, 512);
            for i in 0..n {
                s.insert(FlowBuilder::new().uri(&format!("http://h/{i}")).build());
            }

            let mut walked = Vec::new();
            let mut cursor = None;
            loop {
                let page = s.page(&FlowFilter::default(), limit, cursor);
                walked.extend(page.flows.iter().map(|f| f.id));
                let Some(next) = page.next_cursor else { break };
                cursor = Some(next);
            }

            let listed: Vec<FlowId> =
                s.list(&FlowFilter::default()).iter().map(|f| f.id).collect();
            proptest::prop_assert_eq!(walked, listed, "paging must re-slice the listing, not redefine it");
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
