mod decode;
pub mod routes;
pub mod view;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::store::Store;
use routes::AppState;

/// Build the inspect-only control API: the embedded UI shell, filtered
/// listing, full-flow detail, raw or decoded bodies, a live SSE stream, and
/// the CA download.
pub fn router(store: Arc<Store>, ca_cert_pem: Arc<String>) -> Router {
    let state = AppState { store, ca_cert_pem };
    Router::new()
        .route("/", get(routes::ui_page))
        .route("/flows", get(routes::list_flows))
        .route("/flows/{id}", get(routes::get_flow))
        .route("/flows/{id}/request/body", get(routes::request_body))
        .route("/flows/{id}/response/body", get(routes::response_body))
        .route("/stream", get(routes::stream))
        .route("/ca.crt", get(routes::ca_cert))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::store::Store;

    #[tokio::test]
    async fn router_serves_the_ui_shell_and_every_preexisting_route() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        for (path, expected) in [
            ("/", StatusCode::OK),
            ("/flows", StatusCode::OK),
            ("/flows/not-a-uuid", StatusCode::BAD_REQUEST),
            ("/flows/not-a-uuid/request/body", StatusCode::BAD_REQUEST),
            ("/flows/not-a-uuid/response/body", StatusCode::BAD_REQUEST),
            ("/stream", StatusCode::OK),
            ("/ca.crt", StatusCode::OK),
        ] {
            let request = Request::builder().uri(path).body(Body::empty()).expect("request");
            let response = router.clone().oneshot(request).await.expect("response");
            assert_eq!(response.status(), expected, "route {path}");
        }
    }
}
