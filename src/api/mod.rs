pub mod curl;
mod decode;
pub mod har;
pub mod routes;
pub mod view;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Router};

use crate::store::Store;
use routes::AppState;

/// Build the control API: the embedded UI shell, filtered listing, a HAR 1.2
/// export of it and the import that reads one back (or a pasted `curl`
/// command, under `?format=curl`), a connection tally
/// (`/stats`), full-flow detail, raw or decoded bodies, a live SSE stream, and
/// the CA download.
pub fn router(store: Arc<Store>, ca_cert_pem: Arc<String>) -> Router {
    let state = AppState { store, ca_cert_pem };
    Router::new()
        .route("/", get(routes::ui_page))
        .route("/flows", get(routes::list_flows).delete(routes::clear_flows))
        .route("/flows/export", get(routes::export_har))
        .route(
            "/flows/import",
            // axum's 2 MB default would reject documents far smaller than the
            // store's own capacity, a bound no part of the contract implies.
            post(routes::import_flows)
                .layer(DefaultBodyLimit::max(import_body_limit(state.store.max_bytes()))),
        )
        .route("/stats", get(routes::stats))
        .route("/flows/{id}", get(routes::get_flow).delete(routes::delete_flow))
        .route("/flows/{id}/request/body", get(routes::request_body))
        .route("/flows/{id}/response/body", get(routes::response_body))
        .route("/stream", get(routes::stream))
        .route("/ca.crt", get(routes::ca_cert))
        .layer(middleware::from_fn(allow_cross_origin))
        .with_state(state)
}

/// The ring's cap counts retained body bytes; the document that carries them
/// counts base64 — a third larger for every body that is not text — plus the JSON
/// that frames all of them. Bounding the import door by the cap alone therefore
/// refuses the store its own export of a body-heavy capture it was holding
/// legally, so the door admits the expansion.
///
/// What this guarantees is exactly the body-dominated case, which is the case the
/// cap is expressed in. It cannot guarantee more: `Ring::evict` fires on
/// `total_bytes`, and `Flow::retained_bytes` counts no header, URL or cookie, so a
/// capture of body-less exchanges holds unbounded rows and exports a document no
/// multiple of the cap can bound. No constant is available to fix that — only a
/// ring weighted by whole rows, or a streaming parse, and both are the roadmap's
/// deferred work rather than this door's to decide. The residual is a loud 413 on
/// a capture whose bytes are mostly headers, never a silent truncation.
fn import_body_limit(ring_cap: usize) -> usize {
    ring_cap.saturating_mul(DOCUMENT_EXPANSION)
}

/// Base64's 4/3, rounded up to 2 so the JSON framing rides the same factor rather
/// than a second guess.
const DOCUMENT_EXPANSION: usize = 2;

/// The native app is a cross-origin client (its webview origin is not the API's),
/// so every response carries an open CORS grant and preflights short-circuit.
/// `POST` joined the grant with HAR import and `DELETE` with clearing — the
/// methods that change capture state — and the export's response headers joined
/// it when the app started reading the filename off one. The reasoning behind
/// `*` is unchanged: the surface is loopback-only, and everything it now accepts
/// is equally reachable with `curl`, so the grant adds no exposure the bind
/// address does not already imply.
async fn allow_cross_origin(request: axum::extract::Request, next: middleware::Next) -> Response {
    let mut response = if request.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
    );
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
    // Without this a cross-origin reader sees only the CORS-safelisted response
    // headers, so the export's `Content-Disposition` — the filename the server
    // stamps so successive exports do not overwrite each other — would be
    // invisible to the app and it would have to invent one.
    headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, HeaderValue::from_static("*"));
    response
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
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

    // The body-dominated case the cap is expressed in — the one `import_body_limit`
    // can actually guarantee: a capture of binary bodies filling the ring, whose
    // document is base64 and so outweighs the ring it came from.
    #[tokio::test]
    async fn a_full_ring_of_binary_bodies_re_imports_its_own_export() {
        use crate::test_support::{captured, FlowBuilder};

        const RING_CAP: usize = 4096;
        let store = Arc::new(Store::new(RING_CAP, 8));
        store.insert(
            FlowBuilder::new()
                .uri("http://binary.example.com/blob")
                .status(200)
                .resp_body(captured(&vec![0xffu8; RING_CAP * 3 / 4]))
                .build(),
        );
        let router = super::router(Arc::clone(&store), Arc::new("PEM".into()));

        let export = Request::builder().uri("/flows/export").body(Body::empty()).expect("request");
        let exported = router.clone().oneshot(export).await.expect("response");
        assert_eq!(exported.status(), StatusCode::OK);
        let document = axum::body::to_bytes(exported.into_body(), usize::MAX).await.expect("bytes");
        assert!(
            document.len() > RING_CAP,
            "the premise: base64 makes the document outweigh the ring it came from ({} bytes)",
            document.len()
        );

        let reimport = Request::builder()
            .method("POST")
            .uri("/flows/import")
            .body(Body::from(document))
            .expect("request");
        let response = router.oneshot(reimport).await.expect("response");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the store must accept the document it just produced"
        );
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

    // `import` is a static segment sharing a prefix with `/flows/{id}`, and it is
    // the first POST on the surface — both would regress silently.
    #[tokio::test]
    async fn import_is_a_post_route_of_its_own() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        let get = Request::builder().uri("/flows/import").body(Body::empty()).expect("request");
        let response = router.clone().oneshot(get).await.expect("response");
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "the route exists and only answers POST"
        );

        let post = Request::builder()
            .method("POST")
            .uri("/flows/import")
            .body(Body::from(r#"{"log":{"version":"1.2","entries":[]}}"#))
            .expect("request");
        let response = router.oneshot(post).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    // `DELETE` joins two routes that already answered `GET`, and must not have
    // joined the export or the import — a method landing on the wrong path would
    // regress silently, since axum answers 405 either way.
    #[tokio::test]
    async fn delete_answers_only_where_removal_lives() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        for (path, expected) in [
            ("/flows", StatusCode::OK),
            ("/flows/00000000-0000-0000-0000-000000000000", StatusCode::NOT_FOUND),
            ("/flows/not-a-uuid", StatusCode::BAD_REQUEST),
            ("/flows/export", StatusCode::METHOD_NOT_ALLOWED),
            ("/flows/import", StatusCode::METHOD_NOT_ALLOWED),
        ] {
            let request =
                Request::builder().method("DELETE").uri(path).body(Body::empty()).expect("request");
            let response = router.clone().oneshot(request).await.expect("response");
            assert_eq!(response.status(), expected, "DELETE {path}");
        }
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
        assert_eq!(
            response.headers()["access-control-allow-methods"],
            "GET, POST, DELETE, OPTIONS",
            "the webview cannot POST an import or DELETE a capture without the methods in the grant"
        );
    }

    // `Content-Disposition` is not CORS-safelisted, so without the expose grant
    // the app reads null and has to invent a filename the server already chose.
    #[tokio::test]
    async fn the_export_filename_is_readable_by_a_cross_origin_client() {
        let router = super::router(Arc::new(Store::new(1024, 8)), Arc::new("PEM".into()));
        let request = Request::builder().uri("/flows/export").body(Body::empty()).expect("request");
        let response = router.oneshot(request).await.expect("response");

        assert_eq!(response.headers()["access-control-expose-headers"], "*");
        assert!(
            response.headers()[header::CONTENT_DISPOSITION]
                .to_str()
                .expect("ascii")
                .contains("filename=\"trawler-"),
            "the header the grant exposes has to be the one carrying the name"
        );
    }
}
