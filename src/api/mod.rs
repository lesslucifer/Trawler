mod decode;
pub mod har;
pub mod routes;
pub mod view;

use std::sync::Arc;

use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{middleware, Router};

use crate::store::Store;
use routes::AppState;

/// The timestamp format every projection puts on the wire, defined once so the
/// REST views and the HAR export cannot come to render an instant differently.
pub(crate) fn rfc3339(t: std::time::SystemTime) -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::from(t).format(&Rfc3339).unwrap_or_default()
}

/// Build the inspect-only control API: the embedded UI shell, filtered
/// listing, a HAR 1.2 export of it, a connection tally (`/stats`), full-flow
/// detail, raw or decoded bodies, a live SSE stream, and the CA download.
pub fn router(store: Arc<Store>, ca_cert_pem: Arc<String>) -> Router {
    let state = AppState { store, ca_cert_pem };
    Router::new()
        .route("/", get(routes::ui_page))
        .route("/flows", get(routes::list_flows))
        .route("/flows/export", get(routes::export_har))
        .route("/stats", get(routes::stats))
        .route("/flows/{id}", get(routes::get_flow))
        .route("/flows/{id}/request/body", get(routes::request_body))
        .route("/flows/{id}/response/body", get(routes::response_body))
        .route("/stream", get(routes::stream))
        .route("/ca.crt", get(routes::ca_cert))
        .layer(middleware::from_fn(allow_cross_origin))
        .with_state(state)
}

/// The native app is a cross-origin client (its webview origin is not the API's),
/// so every response carries an open CORS grant and preflights short-circuit. The
/// API is a read-only loopback inspection surface — everything it exposes is
/// already reachable with `curl` — so `*` adds no exposure the bind address
/// (127.0.0.1) does not already imply.
async fn allow_cross_origin(request: axum::extract::Request, next: middleware::Next) -> Response {
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, OPTIONS"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
    response
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
            ("/flows/export", StatusCode::OK),
            ("/stats", StatusCode::OK),
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

    // `export` is a static segment sharing a prefix with `/flows/{id}`; axum
    // prefers the static route, and that precedence is asserted rather than
    // assumed because it would regress silently.
    #[tokio::test]
    async fn export_is_a_route_of_its_own_not_a_flow_id() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        let request = Request::builder().uri("/flows/export").body(Body::empty()).expect("request");
        let response = router.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/json");
    }

    // The native app is a cross-origin client, so every response carries an open
    // CORS grant and a preflight OPTIONS short-circuits with the same headers.
    #[tokio::test]
    async fn every_response_carries_the_cross_origin_grant() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        let request = Request::builder().uri("/flows").body(Body::empty()).expect("request");
        let response = router.clone().oneshot(request).await.expect("response");
        assert_eq!(response.headers()["access-control-allow-origin"], "*");

        let preflight = Request::builder()
            .method("OPTIONS")
            .uri("/flows")
            .body(Body::empty())
            .expect("request");
        let response = router.oneshot(preflight).await.expect("response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers()["access-control-allow-origin"], "*");
    }
}
