use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    LoginCredentials, PlaygroundShareCreateCommand, PlaygroundShareCreateFuture,
    PlaygroundShareError, PlaygroundShareReadFuture, PlaygroundShareRevokeFuture,
    PlaygroundShareService, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_domain::{GroupId, UserId};
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt;

use crate::playground_shares::build_playground_share_router;

struct AcceptSession;

impl SessionAuthenticator for AcceptSession {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(SessionAuthentication::new(
                SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User),
                GroupId::new(3).unwrap(),
                u64::MAX,
            ))
        })
    }
}

#[derive(Default)]
struct RejectShareService {
    create_calls: AtomicUsize,
    read_calls: AtomicUsize,
    revoke_calls: AtomicUsize,
}

impl PlaygroundShareService for RejectShareService {
    fn create(
        &self,
        _principal: SessionPrincipal,
        _command: PlaygroundShareCreateCommand,
    ) -> PlaygroundShareCreateFuture<'_> {
        self.create_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundShareError::LimitReached) })
    }

    fn read(&self, _token: String) -> PlaygroundShareReadFuture<'_> {
        self.read_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundShareError::NotFound) })
    }

    fn revoke(
        &self,
        _principal: SessionPrincipal,
        _token: String,
    ) -> PlaygroundShareRevokeFuture<'_> {
        self.revoke_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Err(PlaygroundShareError::NotFound) })
    }
}

#[tokio::test]
async fn public_read_needs_no_session_and_hardens_not_found_response() {
    let service = Arc::new(RejectShareService::default());
    let router = build_playground_share_router(service.clone(), Arc::new(AcceptSession));
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/playground/shares/{}", share_token()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["x-robots-tag"], "noindex");
    assert_eq!(service.read_calls.load(Ordering::Relaxed), 1);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["code"], "playground_share_not_found");
}

#[tokio::test]
async fn create_and_revoke_require_session_and_keep_uniform_errors() {
    let service = Arc::new(RejectShareService::default());
    let router = build_playground_share_router(service.clone(), Arc::new(AcceptSession));
    let unauthenticated = router
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/playground/shares",
            valid_body(),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(service.create_calls.load(Ordering::Relaxed), 0);

    let limited = router
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/playground/shares",
            valid_body(),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::CONFLICT);
    assert_eq!(service.create_calls.load(Ordering::Relaxed), 1);

    let invalid = router
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/playground/shares",
            valid_body().replace("\"ttl_days\":1", "\"ttl_days\":2"),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(service.create_calls.load(Ordering::Relaxed), 1);

    let revoked = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/playground/shares/{}", share_token()))
                .header(AUTHORIZATION, "Bearer session-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NOT_FOUND);
    assert_eq!(service.revoke_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn create_request_has_a_route_specific_body_limit() {
    let service = Arc::new(RejectShareService::default());
    let router = build_playground_share_router(service.clone(), Arc::new(AcceptSession));
    let response = router
        .oneshot(json_request(
            "POST",
            "/api/playground/shares",
            format!("{{\"padding\":\"{}\"}}", "x".repeat(700 * 1024)),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(service.create_calls.load(Ordering::Relaxed), 0);
}

fn json_request(method: &str, uri: &str, body: String, authenticated: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if authenticated {
        builder = builder.header(AUTHORIZATION, "Bearer session-token");
    }
    builder.body(Body::from(body)).unwrap()
}

fn valid_body() -> String {
    serde_json::json!({
        "ttl_days": 1,
        "sessions": [{
            "model": "gpt-5",
            "messages": [
                {"role": "user", "content": "question"},
                {"role": "assistant", "content": "answer"}
            ]
        }]
    })
    .to_string()
}

fn share_token() -> String {
    format!("sh-af-{}", "A".repeat(43))
}
