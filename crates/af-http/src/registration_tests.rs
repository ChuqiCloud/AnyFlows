use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_admin::{
    IssuedSession, LoginCredentials, RegistrationCommand, RegistrationEmailVerificationCommand,
    RegistrationEmailVerificationFuture, RegistrationEmailVerificationResult, RegistrationError,
    RegistrationFuture, RegistrationPolicy, RegistrationPolicyCommand, RegistrationPolicyFuture,
    RegistrationPolicyUpdateFuture, RegistrationResult, RegistrationService, RegistrationStatus,
    RegistrationStatusFuture, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, SessionToken,
};
use af_config::ServerConfig;
use af_domain::{GroupId, TrustedClientIp, UserId};
use axum::{
    body::{Body, to_bytes},
    extract::ConnectInfo,
};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, RETRY_AFTER},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::registration::build_registration_router;

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";
const ISSUED_TOKEN: &str = "issued-session";

struct FakeRegistrationService {
    status: Mutex<RegistrationStatus>,
    register_outcome: Mutex<Result<RegistrationResult, RegistrationError>>,
    verification_outcome: Mutex<Result<RegistrationEmailVerificationResult, RegistrationError>>,
    register_calls: AtomicUsize,
    verification_calls: AtomicUsize,
    policy_calls: AtomicUsize,
    update_calls: AtomicUsize,
}

impl FakeRegistrationService {
    fn new() -> Self {
        Self {
            status: Mutex::new(RegistrationStatus::from_parts(true, true, true)),
            register_outcome: Mutex::new(Ok(RegistrationResult::from_user_id(
                UserId::new(9).unwrap(),
            ))),
            verification_outcome: Mutex::new(Ok(RegistrationEmailVerificationResult::from_parts(
                10_600, 10_060,
            ))),
            register_calls: AtomicUsize::new(0),
            verification_calls: AtomicUsize::new(0),
            policy_calls: AtomicUsize::new(0),
            update_calls: AtomicUsize::new(0),
        }
    }

    fn set_register_outcome(&self, outcome: Result<RegistrationResult, RegistrationError>) {
        *self.register_outcome.lock().unwrap() = outcome;
    }

    fn set_status(&self, status: RegistrationStatus) {
        *self.status.lock().unwrap() = status;
    }

    fn policy() -> RegistrationPolicy {
        RegistrationPolicy::from_parts(
            true,
            true,
            GroupId::new(3).unwrap(),
            500,
            25,
            true,
            5,
            3_600,
            2,
        )
    }
}

impl RegistrationService for FakeRegistrationService {
    fn status(&self) -> RegistrationStatusFuture<'_> {
        let status = *self.status.lock().unwrap();
        Box::pin(async move { Ok(status) })
    }

    fn register<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a RegistrationCommand,
    ) -> RegistrationFuture<'a> {
        self.register_calls.fetch_add(1, Ordering::Relaxed);
        let outcome = *self.register_outcome.lock().unwrap();
        Box::pin(async move { outcome })
    }

    fn send_email_verification<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a RegistrationEmailVerificationCommand,
    ) -> RegistrationEmailVerificationFuture<'a> {
        self.verification_calls.fetch_add(1, Ordering::Relaxed);
        let outcome = *self.verification_outcome.lock().unwrap();
        Box::pin(async move { outcome })
    }

    fn policy(&self, _principal: SessionPrincipal) -> RegistrationPolicyFuture<'_> {
        self.policy_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Self::policy()) })
    }

    fn update_policy(
        &self,
        _principal: SessionPrincipal,
        _command: RegistrationPolicyCommand,
    ) -> RegistrationPolicyUpdateFuture<'_> {
        self.update_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Self::policy()) })
    }
}

#[tokio::test]
async fn password_login_obeys_the_persisted_capability_switch() {
    let service = Arc::new(FakeRegistrationService::new());
    let success = router(Arc::clone(&service))
        .oneshot(json_request(
            "POST",
            "/api/auth/login",
            json!({
                "username": "new-user",
                "password": "correct horse battery staple",
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::OK);

    service.set_status(RegistrationStatus::from_parts(false, false, false));
    let disabled = router(service)
        .oneshot(json_request(
            "POST",
            "/api/auth/login",
            json!({
                "username": "new-user",
                "password": "correct horse battery staple",
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(disabled).await["code"],
        "password_login_disabled"
    );
}

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        let result = if credentials.username() == "new-user"
            && credentials.password() == b"correct horse battery staple"
        {
            Ok(IssuedSession::from_parts(
                SessionToken::from_string(ISSUED_TOKEN.to_owned()),
                SessionPrincipal::new(UserId::new(9).unwrap(), SessionRole::User),
                current_timestamp() + 3_600,
            ))
        } else {
            Err(SessionAuthenticationError::InvalidCredentials)
        };
        Box::pin(async move { result })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            ADMIN_TOKEN => Some(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            )),
            USER_TOKEN => Some(SessionPrincipal::new(
                UserId::new(2).unwrap(),
                SessionRole::User,
            )),
            _ => None,
        };
        Box::pin(async move {
            principal
                .map(|principal| {
                    SessionAuthentication::new(
                        principal,
                        GroupId::new(3).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

fn router(service: Arc<FakeRegistrationService>) -> axum::Router {
    let service_port: Arc<dyn RegistrationService> = service;
    let authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    build_registration_router(service_port, authenticator, &ServerConfig::default(), None).layer(
        axum::Extension(ConnectInfo(SocketAddr::new(
            "192.0.2.10".parse().unwrap(),
            43123,
        ))),
    )
}

#[tokio::test]
async fn public_status_and_successful_registration_use_no_store_session_contract() {
    let service = Arc::new(FakeRegistrationService::new());
    let app = router(Arc::clone(&service));

    let status = app
        .clone()
        .oneshot(
            Request::get("/api/registration/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    assert_eq!(status.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
        response_json(status).await,
        json!({
            "password_login_enabled": true,
            "enabled": true,
            "email_required": true,
        })
    );

    let response = app
        .oneshot(json_request(
            "POST",
            "/api/registration",
            json!({
                "username": "new-user",
                "email": "new@example.com",
                "password": "correct horse battery staple",
                "verification_code": "042731",
                "invite_code": "af-AAAAAAAAAAAAAAAAAAAAAA",
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    assert_eq!(body["access_token"], ISSUED_TOKEN);
    assert_eq!(body["user"], json!({"id": 9, "role": "user"}));
    assert_eq!(service.register_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn email_verification_returns_only_server_time_boundaries() {
    let service = Arc::new(FakeRegistrationService::new());
    let response = router(Arc::clone(&service))
        .oneshot(json_request(
            "POST",
            "/api/registration/email-verification",
            json!({"email": "new@example.com"}),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
        response_json(response).await,
        json!({"expires_at": 10_600, "next_send_at": 10_060})
    );
    assert_eq!(service.verification_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn rate_limit_returns_stable_code_and_retry_after_header() {
    let service = Arc::new(FakeRegistrationService::new());
    service.set_register_outcome(Err(RegistrationError::RateLimited {
        retry_after_seconds: 37,
    }));
    let response = router(service)
        .oneshot(json_request(
            "POST",
            "/api/registration",
            json!({
                "username": "new-user",
                "password": "correct horse battery staple",
            }),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[RETRY_AFTER], "37");
    assert_eq!(
        response_json(response).await["code"],
        "registration_rate_limited"
    );
}

#[tokio::test]
async fn admin_policy_route_rejects_normal_users_before_service_access() {
    let service = Arc::new(FakeRegistrationService::new());
    let app = router(Arc::clone(&service));

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings",
            Value::Null,
            Some(USER_TOKEN),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(service.policy_calls.load(Ordering::Relaxed), 0);

    let policy = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/authentication-settings",
            Value::Null,
            Some(ADMIN_TOKEN),
        ))
        .await
        .unwrap();
    assert_eq!(policy.status(), StatusCode::OK);
    let policy = response_json(policy).await;
    assert_eq!(policy["default_group_id"], 3);
    assert_eq!(policy["invitation_rebate_quota"], 25);

    let updated = app
        .oneshot(json_request(
            "PUT",
            "/api/admin/authentication-settings",
            json!({
                "password_login_enabled": true,
                "registration_enabled": true,
                "default_group_id": 3,
                "initial_quota": 500,
                "invitation_rebate_quota": 25,
                "email_required": true,
                "rate_limit_attempts": 5,
                "rate_limit_window_seconds": 3600,
            }),
            Some(ADMIN_TOKEN),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);
}

fn json_request(method: &str, uri: &str, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if method != "GET" {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(serde_json::to_vec(&body).unwrap())
        })
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
