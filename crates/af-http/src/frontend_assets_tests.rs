use std::sync::Arc;

use af_config::ServerConfig;
use axum::{
    Router,
    body::{Body, to_bytes},
    routing::get,
};
use http::{
    Method, Request, StatusCode,
    header::{
        CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, X_CONTENT_TYPE_OPTIONS,
    },
};
use tower::ServiceExt;

use crate::{
    FrontendAsset, FrontendAssetSource, REQUEST_ID_HEADER_NAME,
    router::build_router_with_routes_and_frontend,
};

const INDEX: &[u8] = b"<!doctype html><title>AnyFlows</title>";
const SCRIPT: &[u8] = b"console.log('AnyFlows')";
const ICON: &[u8] = b"<svg></svg>";

struct StaticAssets;

impl FrontendAssetSource for StaticAssets {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        match path {
            "index.html" => Some(FrontendAsset::from_static(INDEX)),
            "assets/app.abc123.js" => Some(FrontendAsset::from_static(SCRIPT)),
            "favicon.svg" => Some(FrontendAsset::from_static(ICON)),
            _ => None,
        }
    }
}

fn router() -> Router {
    let public_routes = Router::new().route("/api/known", get(|| async { StatusCode::NO_CONTENT }));
    let operations_routes =
        Router::new().route("/healthz", get(|| async { StatusCode::NO_CONTENT }));
    build_router_with_routes_and_frontend(
        public_routes,
        operations_routes,
        &ServerConfig::default(),
        1024,
        Some(Arc::new(StaticAssets)),
    )
}

async fn get_response(path: &str) -> axum::response::Response {
    router()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

#[tokio::test]
async fn root_and_frontend_deep_links_return_index() {
    for path in ["/", "/login", "/admin/channels", "/admin/channels/"] {
        let response = get_response(path).await;
        assert_eq!(response.status(), StatusCode::OK, "请求路径: {path}");
        assert_eq!(response.headers()[CONTENT_TYPE], "text/html");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-cache");
        assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(response.headers().contains_key(ETAG));
        assert!(response.headers().contains_key(REQUEST_ID_HEADER_NAME));
        assert_eq!(body(response).await, INDEX);
    }
}

#[tokio::test]
async fn static_assets_use_mime_cache_and_conditional_requests() {
    let response = get_response("/assets/app.abc123.js").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .contains("javascript")
    );
    assert_eq!(
        response.headers()[CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    let etag = response.headers()[ETAG].clone();
    assert_eq!(body(response).await, SCRIPT);

    let not_modified = router()
        .oneshot(
            Request::builder()
                .uri("/assets/app.abc123.js")
                .header(IF_NONE_MATCH, format!("W/{}", etag.to_str().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(not_modified.headers()[ETAG], etag);
    assert!(body(not_modified).await.is_empty());

    let icon = get_response("/favicon.svg").await;
    assert_eq!(icon.status(), StatusCode::OK);
    assert_eq!(icon.headers()[CONTENT_TYPE], "image/svg+xml");
    assert_eq!(icon.headers()[CACHE_CONTROL], "no-cache");
}

#[tokio::test]
async fn api_operations_and_missing_files_never_fall_back_to_html() {
    for path in ["/api", "/api/"] {
        let explorer = get_response(path).await;
        assert_eq!(explorer.status(), StatusCode::OK);
        assert_eq!(body(explorer).await, INDEX);
    }

    for path in [
        "/api/missing",
        "/v1/missing",
        "/healthz/missing",
        "/readyz",
        "/metrics/missing",
        "/assets/missing.js",
        "/.env",
    ] {
        let response = get_response(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "请求路径: {path}");
        assert_ne!(body(response).await, INDEX);
    }

    assert_eq!(
        get_response("/api/known").await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get_response("/healthz").await.status(),
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn head_has_metadata_without_body_and_other_methods_are_rejected() {
    let head = router()
        .oneshot(
            Request::builder()
                .method(Method::HEAD)
                .uri("/assets/app.abc123.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(
        head.headers()[CONTENT_LENGTH].to_str().unwrap(),
        SCRIPT.len().to_string()
    );
    assert!(body(head).await.is_empty());

    let post = router()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/admin/channels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::NOT_FOUND);
    assert_ne!(body(post).await, INDEX);
}
