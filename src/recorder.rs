use std::sync::Arc;
use std::time::Instant;

use http::{HeaderMap, Method};
use hudsucker::hyper::{Request, Response};
use hudsucker::{Body, HttpContext, HttpHandler, RequestOrResponse};

use crate::capture;
use crate::flow::{Flow, FlowId, Header, RequestRec, ResponseRec};
use crate::store::Store;

/// The proxy-side producer. Dumb by design: it turns each HTTP exchange into a
/// `Flow`, writes it to the store, and forwards the traffic — it knows nothing
/// about any interface. hudsucker clones one handler per exchange, so the
/// per-exchange scratch below correlates a response back to its request.
#[derive(Clone)]
pub struct Recorder {
    store: Arc<Store>,
    body_cap: usize,
    exchange: Option<Exchange>,
}

#[derive(Clone, Copy)]
struct Exchange {
    flow_id: FlowId,
    start: Instant,
}

impl Recorder {
    pub fn new(store: Arc<Store>, body_cap: usize) -> Self {
        Self { store, body_cap, exchange: None }
    }
}

fn headers_to_vec(map: &HeaderMap) -> Vec<Header> {
    map.iter()
        .map(|(name, value)| Header {
            name: name.as_str().to_owned(),
            value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
        })
        .collect()
}

impl HttpHandler for Recorder {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        if req.method() == Method::CONNECT {
            return req.into();
        }

        let start = Instant::now();
        let (parts, body) = req.into_parts();
        let (body_rec, forwarded) = capture::capture(&parts.headers, body, self.body_cap).await;

        let request = RequestRec {
            method: parts.method.as_str().to_owned(),
            uri: parts.uri.to_string(),
            version: format!("{:?}", parts.version),
            headers: headers_to_vec(&parts.headers),
            body: body_rec,
        };

        let flow = Flow::new(request);
        self.exchange = Some(Exchange { flow_id: flow.id, start });
        self.store.insert(flow);

        Request::from_parts(parts, forwarded).into()
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        let Some(exchange) = self.exchange.take() else {
            return res;
        };

        let (parts, body) = res.into_parts();
        let (body_rec, forwarded) = capture::capture(&parts.headers, body, self.body_cap).await;

        let response = ResponseRec {
            status: parts.status.as_u16(),
            version: format!("{:?}", parts.version),
            headers: headers_to_vec(&parts.headers),
            body: body_rec,
        };

        let duration_ms = exchange.start.elapsed().as_millis() as u64;
        self.store.set_response(exchange.flow_id, response, duration_ms);

        Response::from_parts(parts, forwarded)
    }
}
