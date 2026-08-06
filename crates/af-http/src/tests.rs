use std::{
    collections::HashSet,
    env,
    io::{self, Write},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_config::{CorsOrigin, ServerConfig};
use af_domain::AfError;
use af_telemetry::RequestId;
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::Extension,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{
    HeaderMap, HeaderValue, Method, Request, StatusCode,
    header::{
        ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
        ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS,
        ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, AUTHORIZATION,
        CONTENT_ENCODING, CONTENT_LENGTH, COOKIE, ORIGIN, USER_AGENT, VARY,
    },
};
use tower::ServiceExt;
use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    fmt::MakeWriter,
    layer::{Layer as _, SubscriberExt as _},
};

use crate::{
    DEFAULT_REQUEST_BODY_LIMIT_BYTES, OpenAiHttpError, REQUEST_ID_HEADER_NAME,
    router::build_router_with_routes,
};

const ALLOWED_ORIGIN: &str = "https://console.example";
const LOGGING_CHILD: &str = "ANYFLOWS_HTTP_LOGGING_CHILD";

fn server_config(origins: &[&str]) -> ServerConfig {
    ServerConfig::default().with_cors_allowed_origins(
        origins
            .iter()
            .map(|origin| origin.parse::<CorsOrigin>().unwrap()),
    )
}

fn test_router(origins: &[&str], body_limit: usize) -> Router {
    let public_routes = Router::new()
        .route("/request-id", get(observe_request_id))
        .route("/body", post(consume_body))
        .route("/echo/{value}", post(consume_body))
        .route("/mapped-error/{value}", post(mapped_error))
        .route("/method-only", get(empty_response))
        .route("/large-response", get(large_response));
    let operations_routes = Router::new().route("/ops", get(empty_response));
    build_router_with_routes(
        public_routes,
        operations_routes,
        &server_config(origins),
        body_limit,
    )
}

async fn observe_request_id(
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
) -> Response {
    assert!(
        headers
            .get_all(REQUEST_ID_HEADER_NAME)
            .iter()
            .next()
            .is_none()
    );
    let mut response = request_id.as_str().to_owned().into_response();
    response.headers_mut().insert(
        REQUEST_ID_HEADER_NAME,
        HeaderValue::from_static("inner-selected-request-id"),
    );
    response
}

async fn consume_body(_: Bytes) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn empty_response() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn mapped_error() -> Result<StatusCode, OpenAiHttpError> {
    Err(AfError::InvalidRequest.into())
}

async fn large_response() -> Response {
    let mut response = Body::from(vec![b'x'; 65]).into_response();
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    response
}

fn request_id(response: &Response) -> String {
    let values = response.headers().get_all(REQUEST_ID_HEADER_NAME);
    assert_eq!(values.iter().count(), 1);
    let value = values.iter().next().unwrap().to_str().unwrap().to_owned();
    RequestId::new(value.clone()).unwrap();
    value
}

async fn response_text(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn request_id_is_server_generated_and_overwrites_both_directions() {
    let router = test_router(&[], 64);
    let mut request = Request::builder()
        .uri("/request-id")
        .body(Body::empty())
        .unwrap();
    request.headers_mut().append(
        REQUEST_ID_HEADER_NAME,
        HeaderValue::from_static("client-selected-request-id"),
    );
    request.headers_mut().append(
        REQUEST_ID_HEADER_NAME,
        HeaderValue::from_bytes(&[0xff]).unwrap(),
    );

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let canonical = request_id(&response);
    assert_ne!(canonical, "client-selected-request-id");
    assert_ne!(canonical, "inner-selected-request-id");
    assert_eq!(response_text(response).await, canonical);
}

#[tokio::test]
async fn fallback_method_rejection_and_parallel_requests_receive_unique_ids() {
    let router = test_router(&[], 64);
    for request in [
        Request::builder()
            .uri("/missing")
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .method(Method::POST)
            .uri("/method-only")
            .body(Body::empty())
            .unwrap(),
    ] {
        let response = router.clone().oneshot(request).await.unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ));
        request_id(&response);
    }

    let mut tasks = Vec::new();
    for _ in 0..32 {
        let router = router.clone();
        tasks.push(tokio::spawn(async move {
            let response = router
                .oneshot(
                    Request::builder()
                        .uri("/request-id")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            request_id(&response)
        }));
    }
    let mut ids = HashSet::new();
    for task in tasks {
        ids.insert(task.await.unwrap());
    }
    assert_eq!(ids.len(), 32);
}

#[tokio::test]
async fn cors_allows_exact_business_origin_and_exposes_request_id() {
    let router = test_router(&[ALLOWED_ORIGIN], 64);
    let preflight = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/body")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    ACCESS_CONTROL_REQUEST_HEADERS,
                    "authorization, content-type",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::OK);
    assert_eq!(
        preflight.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static(ALLOWED_ORIGIN))
    );
    assert!(
        preflight.headers()[ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .contains("POST")
    );
    let allowed_headers = preflight.headers()[ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(allowed_headers.contains("authorization"));
    assert!(allowed_headers.contains("content-type"));
    assert!(
        preflight
            .headers()
            .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .is_none()
    );
    assert!(preflight.headers().get_all(VARY).iter().any(|value| {
        value
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("origin")
    }));
    request_id(&preflight);

    let response = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static(ALLOWED_ORIGIN))
    );
    assert!(
        response.headers()[ACCESS_CONTROL_EXPOSE_HEADERS]
            .to_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains(REQUEST_ID_HEADER_NAME)
    );
    request_id(&response);
}

#[tokio::test]
async fn cors_rejects_untrusted_origin_before_handler_and_stays_off_operations_routes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::clone(&calls);
    let public_routes = Router::new().route(
        "/mutate",
        post(move || {
            let calls = Arc::clone(&handler_calls);
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                StatusCode::NO_CONTENT
            }
        }),
    );
    let operations_routes = Router::new().route("/ops", get(empty_response));
    let router = build_router_with_routes(
        public_routes,
        operations_routes,
        &server_config(&[ALLOWED_ORIGIN]),
        64,
    );

    for origin in ["https://evil.example", "https://console.example.evil"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/mutate")
                    .header(ORIGIN, origin)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
        request_id(&response);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let operations = router
        .oneshot(
            Request::builder()
                .uri("/ops")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(operations.status(), StatusCode::NO_CONTENT);
    assert!(
        operations
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
    assert!(
        operations
            .headers()
            .get(ACCESS_CONTROL_EXPOSE_HEADERS)
            .is_none()
    );
    request_id(&operations);
}

#[tokio::test]
async fn empty_cors_allowlist_allows_same_origin_and_api_clients_but_rejects_cross_origin() {
    let router = test_router(&[], 64);
    let same_origin = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .header(http::header::HOST, "console.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(same_origin.status(), StatusCode::NO_CONTENT);
    request_id(&same_origin);

    let blocked = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .header(http::header::HOST, "gateway.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    request_id(&blocked);

    let api_client = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api_client.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn same_origin_requires_exact_host_and_effective_port() {
    let router = test_router(&[], 64);
    for (origin, host, expected) in [
        (
            "https://console.example",
            "CONSOLE.EXAMPLE",
            StatusCode::NO_CONTENT,
        ),
        (
            "http://127.0.0.1:8085",
            "127.0.0.1:8085",
            StatusCode::NO_CONTENT,
        ),
        (
            "http://127.0.0.1:8085",
            "127.0.0.1:8086",
            StatusCode::FORBIDDEN,
        ),
        (
            "https://console.example",
            "console.example.evil",
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/body")
                    .header(ORIGIN, origin)
                    .header(http::header::HOST, host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        request_id(&response);
    }
}

#[tokio::test]
async fn body_limit_covers_declared_and_actual_lengths_with_cors_and_request_id() {
    const LIMIT: usize = 16;
    let router = test_router(&[ALLOWED_ORIGIN], LIMIT);

    let exact = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .body(Body::from(vec![b'x'; LIMIT]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exact.status(), StatusCode::NO_CONTENT);

    let actual_overflow = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .body(Body::from("request-body-canary"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(actual_overflow.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        actual_overflow.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static(ALLOWED_ORIGIN))
    );
    request_id(&actual_overflow);
    assert!(
        !response_text(actual_overflow)
            .await
            .contains("request-body-canary")
    );

    let declared_overflow = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/method-only")
                .header(CONTENT_LENGTH, (LIMIT + 1).to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(declared_overflow.status(), StatusCode::PAYLOAD_TOO_LARGE);
    request_id(&declared_overflow);

    let understated = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .header(CONTENT_LENGTH, LIMIT / 2)
                .body(Body::from(vec![b'z'; LIMIT + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(understated.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn project_limit_replaces_axum_two_mib_default_without_limiting_responses() {
    let accepted = test_router(&[], DEFAULT_REQUEST_BODY_LIMIT_BYTES)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/body")
                .body(Body::from(vec![b'x'; 3 * 1_024 * 1_024]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::NO_CONTENT);

    let response = test_router(&[], 64)
        .oneshot(
            Request::builder()
                .uri("/large-response")
                .header(http::header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(CONTENT_ENCODING).is_none());
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .len(),
        65
    );
}

#[derive(Clone, Default)]
struct CapturedOutput(Arc<Mutex<Vec<u8>>>);

impl CapturedOutput {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for CapturedOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .flush()
    }
}

impl<'writer> MakeWriter<'writer> for CapturedOutput {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn logs_only_canonical_request_metadata_and_keeps_request_span() {
    if env::var_os(LOGGING_CHILD).is_none() {
        let output = Command::new(env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::logs_only_canonical_request_metadata_and_keeps_request_span")
            .arg("--nocapture")
            .env_clear()
            .env(LOGGING_CHILD, "1")
            .output()
            .expect("必须能启动隔离 HTTP 日志测试进程");
        let rendered = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success(), "隔离测试失败: {rendered}");
        return;
    }

    let output = CapturedOutput::default();
    let filter = Targets::new()
        .with_default(LevelFilter::OFF)
        .with_target("af_http", LevelFilter::TRACE)
        .with_target("af_request", LevelFilter::TRACE);
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .without_time()
            .with_ansi(false)
            .with_target(true)
            .with_writer(output.clone())
            .with_filter(filter),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(async {
            let response = test_router(&[], 64)
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/echo/path-secret?query=query-secret")
                        .header(AUTHORIZATION, "Bearer authorization-secret")
                        .header(COOKIE, "session=cookie-secret")
                        .header(USER_AGENT, "user-agent-secret")
                        .header("x-api-key", "api-key-secret")
                        .header(REQUEST_ID_HEADER_NAME, "client-request-secret")
                        .body(Body::from("body-secret"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            let canonical = request_id(&response);
            to_bytes(response.into_body(), usize::MAX).await.unwrap();

            let rejected = test_router(&[], 64)
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/body")
                        .header(CONTENT_LENGTH, 65)
                        .body(Body::from("overflow-body-secret"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(rejected.status(), StatusCode::PAYLOAD_TOO_LARGE);
            request_id(&rejected);
            to_bytes(rejected.into_body(), usize::MAX).await.unwrap();

            let mapped = test_router(&[], 64)
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/mapped-error/error-path-secret?query=error-query-secret")
                        .header(AUTHORIZATION, "Bearer error-authorization-secret")
                        .header(COOKIE, "session=error-cookie-secret")
                        .header(USER_AGENT, "error-user-agent-secret")
                        .header("x-api-key", "error-api-key-secret")
                        .header(REQUEST_ID_HEADER_NAME, "error-client-request-secret")
                        .body(Body::from("error-body-secret"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(mapped.status(), StatusCode::BAD_REQUEST);
            let mapped_request_id = request_id(&mapped);
            to_bytes(mapped.into_body(), usize::MAX).await.unwrap();

            let logs = output.text();
            assert!(logs.contains("af_http::middleware"));
            assert!(logs.contains("af_http::error_response"));
            assert!(logs.contains("HTTP 请求完成"));
            assert!(logs.contains("/echo/{value}"), "安全日志内容: {logs}");
            assert!(logs.contains("/mapped-error/{value}"));
            assert!(logs.contains("status_code=204"));
            assert!(logs.contains("status_code=400"));
            assert!(logs.contains("status_code=413"));
            assert!(logs.contains("response_latency_ms"));
            assert!(logs.contains(&canonical));
            assert!(logs.contains(&mapped_request_id));
            assert!(logs.contains("error_code=\"invalid_request\""));
            assert!(logs.contains("error_kind=\"invalid_request\""));
            for secret in [
                "path-secret",
                "query-secret",
                "authorization-secret",
                "cookie-secret",
                "user-agent-secret",
                "api-key-secret",
                "client-request-secret",
                "body-secret",
                "overflow-body-secret",
                "error-path-secret",
                "error-query-secret",
                "error-authorization-secret",
                "error-cookie-secret",
                "error-user-agent-secret",
                "error-api-key-secret",
                "error-client-request-secret",
                "error-body-secret",
            ] {
                assert!(!logs.contains(secret), "日志泄露了测试敏感值: {secret}");
            }
        });
    });
}
