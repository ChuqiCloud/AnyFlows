use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use af_config::ServerConfig;
use af_telemetry::RequestId;
use axum::{
    Router,
    body::{Body, to_bytes},
};
use http::{
    Method, Request, Response, StatusCode,
    header::{
        ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ALLOW, CACHE_CONTROL,
        CONTENT_LENGTH, CONTENT_TYPE, ORIGIN,
    },
};
use tokio::sync::Notify;
use tower::ServiceExt;

use crate::{
    HEALTH_PATH, READINESS_PATH, REQUEST_ID_HEADER_NAME, ReadinessFuture, ReadinessHandle,
    ReadinessProbe, operations::operations_router, router::build_router_with_routes,
};

const HEALTH_BODY: &str = r#"{"status":"ok"}"#;
const READY_BODY: &str = r#"{"status":"ready"}"#;
const NOT_READY_BODY: &str = r#"{"status":"not_ready"}"#;

struct FakeProbe {
    ready: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

struct BlockingProbe {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl ReadinessProbe for FakeProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let ready = self.ready.load(Ordering::Acquire);
        Box::pin(async move { ready })
    }
}

impl ReadinessProbe for BlockingProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        let entered = Arc::clone(&self.entered);
        let release = Arc::clone(&self.release);
        Box::pin(async move {
            entered.notify_one();
            release.notified().await;
            true
        })
    }
}

fn test_router(body_limit: usize) -> (Router, ReadinessHandle, Arc<AtomicBool>, Arc<AtomicUsize>) {
    let ready = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(AtomicUsize::new(0));
    let readiness = ReadinessHandle::new(FakeProbe {
        ready: Arc::clone(&ready),
        calls: Arc::clone(&calls),
    });
    let router = build_router_with_routes(
        Router::new(),
        operations_router(readiness.clone()),
        &ServerConfig::default(),
        body_limit,
    );
    (router, readiness, ready, calls)
}

fn request(method: Method, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(ORIGIN, "https://probe-origin.example")
        .header(REQUEST_ID_HEADER_NAME, "client-selected-request-id")
        .body(Body::empty())
        .unwrap()
}

fn assert_request_id(response: &Response<Body>) {
    let values = response.headers().get_all(REQUEST_ID_HEADER_NAME);
    assert_eq!(values.iter().count(), 1);
    let value = values.iter().next().unwrap().to_str().unwrap();
    assert_ne!(value, "client-selected-request-id");
    RequestId::new(value.to_owned()).unwrap();
}

fn assert_operations_headers(response: &Response<Body>) {
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/json; charset=utf-8"
    );
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    assert!(
        response
            .headers()
            .get(ACCESS_CONTROL_EXPOSE_HEADERS)
            .is_none()
    );
    assert_request_id(response);
}

async fn assert_json_response(response: Response<Body>, status: StatusCode, expected: &str) {
    assert_eq!(response.status(), status);
    assert_operations_headers(&response);
    let body = to_bytes(response.into_body(), 256).await.unwrap();
    assert_eq!(body.as_ref(), expected.as_bytes());
}

#[tokio::test]
async fn lifecycle_and_dependency_state_drive_only_readiness() {
    let (router, readiness, ready, calls) = test_router(64);

    assert_json_response(
        router
            .clone()
            .oneshot(request(Method::GET, "/healthz?canary=health-secret"))
            .await
            .unwrap(),
        StatusCode::OK,
        HEALTH_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    assert_json_response(
        router
            .clone()
            .oneshot(request(Method::GET, READINESS_PATH))
            .await
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        NOT_READY_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    readiness.mark_ready();
    assert_json_response(
        router
            .clone()
            .oneshot(request(Method::GET, READINESS_PATH))
            .await
            .unwrap(),
        StatusCode::OK,
        READY_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    ready.store(false, Ordering::Release);
    assert_json_response(
        router
            .clone()
            .oneshot(request(Method::GET, READINESS_PATH))
            .await
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        NOT_READY_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    assert_json_response(
        router
            .clone()
            .oneshot(request(Method::GET, HEALTH_PATH))
            .await
            .unwrap(),
        StatusCode::OK,
        HEALTH_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    ready.store(true, Ordering::Release);
    let head = router
        .clone()
        .oneshot(request(Method::HEAD, READINESS_PATH))
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_operations_headers(&head);
    assert!(to_bytes(head.into_body(), 256).await.unwrap().is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 3);

    readiness.begin_draining();
    readiness.mark_ready();
    assert_json_response(
        router
            .oneshot(request(Method::GET, READINESS_PATH))
            .await
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        NOT_READY_BODY,
    )
    .await;
    assert_eq!(calls.load(Ordering::Relaxed), 3);
}

#[tokio::test]
async fn operations_routes_reject_methods_and_oversized_bodies_before_probing() {
    let (router, readiness, _, calls) = test_router(16);
    readiness.mark_ready();

    for method in [
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ] {
        for path in [HEALTH_PATH, READINESS_PATH] {
            let response = router
                .clone()
                .oneshot(request(method.clone(), path))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
            let allow = response.headers().get(ALLOW).unwrap().to_str().unwrap();
            assert!(allow.split(',').any(|value| value.trim() == "GET"));
            assert!(allow.split(',').any(|value| value.trim() == "HEAD"));
            assert!(
                response
                    .headers()
                    .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                    .is_none()
            );
            assert_request_id(&response);
        }
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let oversized = Request::builder()
        .method(Method::GET)
        .uri(READINESS_PATH)
        .header(CONTENT_LENGTH, 17)
        .body(Body::from(vec![b'x'; 17]))
        .unwrap();
    let response = router.clone().oneshot(oversized).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_request_id(&response);
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let response = router
        .oneshot(request(Method::GET, "/healthz/"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_request_id(&response);
}

#[tokio::test]
async fn draining_during_dependency_check_cannot_return_ready() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let readiness = ReadinessHandle::new(BlockingProbe {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    readiness.mark_ready();
    let router = build_router_with_routes(
        Router::new(),
        operations_router(readiness.clone()),
        &ServerConfig::default(),
        64,
    );

    let response = tokio::spawn(router.oneshot(request(Method::GET, READINESS_PATH)));
    entered.notified().await;
    readiness.begin_draining();
    release.notify_one();

    assert_json_response(
        response.await.unwrap().unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        NOT_READY_BODY,
    )
    .await;
}
