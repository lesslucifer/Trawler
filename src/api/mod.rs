mod decode;
pub mod routes;
pub mod view;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::store::Store;
use routes::AppState;

/// Build the inspect-only control API: filtered listing, full-flow detail, raw
/// or decoded bodies, a live SSE stream, and the CA download.
pub fn router(store: Arc<Store>, ca_cert_pem: Arc<String>) -> Router {
    let state = AppState { store, ca_cert_pem };
    Router::new()
        .route("/flows", get(routes::list_flows))
        .route("/flows/{id}", get(routes::get_flow))
        .route("/flows/{id}/request/body", get(routes::request_body))
        .route("/flows/{id}/response/body", get(routes::response_body))
        .route("/stream", get(routes::stream))
        .route("/ca.crt", get(routes::ca_cert))
        .with_state(state)
}
