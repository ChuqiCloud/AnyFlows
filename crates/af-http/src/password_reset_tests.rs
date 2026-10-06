use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_admin::{
    PasswordResetConfirmCommand, PasswordResetConfirmFuture, PasswordResetConfirmResult,
    PasswordResetError, PasswordResetRequestCommand, PasswordResetRequestFuture,
    PasswordResetRequestResult, PasswordResetService,
};
use af_config::ServerConfig;
use af_domain::TrustedClientIp;
use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
};
use http::{
    Request, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::password_reset::build_password_reset_router;

const VALID_TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct FakePasswordResetService {
    request_outcome: Mutex<Result<PasswordResetRequestResult, PasswordResetError>>,
    confirm_outcome: Mutex<Result<PasswordResetConfirmResult, PasswordResetError>>,
    request_calls: AtomicUsize,
}

impl FakePasswordResetService {
    fn new() -> Self {
        Self {
            request_outcome: Mutex::new(Ok(PasswordResetRequestResult::accepted())),
            confirm_outcome: Mutex::new(Ok(PasswordResetConfirmResult::from_user_id(
                af_domain::UserId::new(9).unwrap(),
            ))),
            request_calls: AtomicUsize::new(0),
        }
    }

    fn set_confirm_outcome(&self, outcome: Result<PasswordResetConfirmResult, PasswordResetError>) {
        *self.confirm_outcome.lock().unwrap() = outcome;
    }
}

impl PasswordResetService for FakePasswordResetService {
    fn request<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a PasswordResetRequestCommand,
    ) -> PasswordResetRequestFuture<'a> {
        self.request_calls.fetch_add(1, Ordering::Relaxed);
        let outcome = *self.request_outcome.lock().unwrap();
        Box::pin(async move { outcome })
    }

    fn confirm(&self, _command: PasswordResetConfirmCommand) -> PasswordResetConfirmFuture<'_> {
        let outcome = *self.confirm_outcome.lock().unwrap();
        Box::pin(async move { outcome })
    }
}

fn router(service: Arc<FakePasswordResetService>, with_peer: bool) -> axum::Router {
    let service: Arc<dyn PasswordResetService> = service;
    let router = build_password_reset_router(service, &ServerConfig::default());
    if with_peer {
        router.layer(axum::Extension(ConnectInfo(SocketAddr::new(
            "192.0.2.10".parse().unwrap(),
            43123,
        ))))
    } else {
        router
    }
}

#[tokio::test]
async fn request_masks_account_existence_and_disables_caching() {
    let service = Arc::new(FakePasswordResetService::new());
    let app = router(Arc::clone(&service), true);
    let known = app
        .clone()
        .oneshot(json_request(
            "/api/auth/password-reset/request",
            json!({"email": "known@example.com"}),
        ))
        .await
        .unwrap();
    let unknown = app
        .oneshot(json_request(
            "/api/auth/password-reset/request",
            json!({"email": "unknown@example.com"}),
        ))
        .await
        .unwrap();

    assert_eq!(known.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown.status(), StatusCode::ACCEPTED);
    assert_eq!(known.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(unknown.headers()[CACHE_CONTROL], "no-store");
    let known_body = response_json(known).await;
    let unknown_body = response_json(unknown).await;
    assert_eq!(known_body, unknown_body);
    assert_eq!(unknown_body, json!({"accepted": true}));
    assert_eq!(service.request_calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn confirm_returns_no_content_or_stable_rejection() {
    let service = Arc::new(FakePasswordResetService::new());
    let app = router(Arc::clone(&service), true);
    let success = app
        .clone()
        .oneshot(json_request(
            "/api/auth/password-reset/confirm",
            json!({"token": VALID_TOKEN, "password": "new secure password"}),
        ))
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::NO_CONTENT);
    assert_eq!(success.headers()[CACHE_CONTROL], "no-store");
    assert!(
        to_bytes(success.into_body(), 1_024)
            .await
            .unwrap()
            .is_empty()
    );

    service.set_confirm_outcome(Err(PasswordResetError::Rejected));
    let rejected = app
        .oneshot(json_request(
            "/api/auth/password-reset/confirm",
            json!({"token": VALID_TOKEN, "password": "new secure password"}),
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(rejected).await["code"],
        "password_reset_rejected"
    );
}

#[tokio::test]
async fn malformed_input_and_missing_peer_fail_closed() {
    let service = Arc::new(FakePasswordResetService::new());
    let malformed = router(Arc::clone(&service), true)
        .oneshot(json_request(
            "/api/auth/password-reset/confirm",
            json!({"token": VALID_TOKEN, "password": "short", "extra": true}),
        ))
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(malformed.headers()[CACHE_CONTROL], "no-store");

    let missing_peer = router(service, false)
        .oneshot(json_request(
            "/api/auth/password-reset/request",
            json!({"email": "known@example.com"}),
        ))
        .await
        .unwrap();
    assert_eq!(missing_peer.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(missing_peer.headers()[CACHE_CONTROL], "no-store");
}

fn json_request(path: &str, body: Value) -> Request<Body> {
    Request::post(path)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
