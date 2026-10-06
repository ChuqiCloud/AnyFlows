use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use af_admin::{
    IssuedSession, LoginCredentials, PasskeyAuthenticationCommand, PasskeyAuthenticationError,
    PasskeyAuthenticationFuture, PasskeyAuthenticationOptionsFuture, PasskeyAuthenticationService,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, SessionPrincipal, SessionRole, SessionToken,
};
use af_config::ServerConfig;
use af_domain::{TrustedClientIp, UserId};
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

use crate::passkey_auth::build_passkey_authentication_router;

const SESSION_TOKEN: &str = "passkey.session.token";

struct FakePasskeyAuthenticationService {
    start_error: PasskeyAuthenticationError,
    finish_result: Result<UserId, PasskeyAuthenticationError>,
}

impl PasskeyAuthenticationService for FakePasskeyAuthenticationService {
    fn start<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _username: &'a str,
    ) -> PasskeyAuthenticationOptionsFuture<'a> {
        let error = self.start_error;
        Box::pin(async move { Err(error) })
    }

    fn finish<'a>(
        &'a self,
        _command: PasskeyAuthenticationCommand,
    ) -> PasskeyAuthenticationFuture<'a> {
        let result = self.finish_result;
        Box::pin(async move { result })
    }
}

struct FakeSessionAuthenticator {
    issued_users: Mutex<Vec<UserId>>,
}

impl FakeSessionAuthenticator {
    fn new() -> Self {
        Self {
            issued_users: Mutex::new(Vec::new()),
        }
    }
}

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn issue_for_user(&self, user_id: UserId) -> SessionLoginFuture<'_> {
        self.issued_users.lock().unwrap().push(user_id);
        Box::pin(async move {
            Ok(IssuedSession::from_parts(
                SessionToken::from_string(SESSION_TOKEN.to_owned()),
                SessionPrincipal::new(user_id, SessionRole::User),
                current_timestamp() + 3_600,
            ))
        })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
    }
}

#[tokio::test]
async fn verified_passkey_reuses_the_existing_session_response() {
    let user_id = UserId::new(42).unwrap();
    let sessions = Arc::new(FakeSessionAuthenticator::new());
    let app = router(
        Some(FakePasskeyAuthenticationService {
            start_error: PasskeyAuthenticationError::Rejected,
            finish_result: Ok(user_id),
        }),
        Arc::clone(&sessions),
    );

    let response = app
        .oneshot(json_request(
            "/api/auth/passkey/verify",
            json!({"credential": {}}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    assert_eq!(body["access_token"], SESSION_TOKEN);
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["user"], json!({"id":42,"role":"user"}));
    assert_eq!(sessions.issued_users.lock().unwrap().as_slice(), [user_id]);
}

#[tokio::test]
async fn passkey_failures_converge_without_leaking_account_or_credential_state() {
    let sessions = Arc::new(FakeSessionAuthenticator::new());
    let rejected = router(
        Some(FakePasskeyAuthenticationService {
            start_error: PasskeyAuthenticationError::Rejected,
            finish_result: Err(PasskeyAuthenticationError::Rejected),
        }),
        Arc::clone(&sessions),
    );
    let disabled = router(
        Some(FakePasskeyAuthenticationService {
            start_error: PasskeyAuthenticationError::LoginDisabled,
            finish_result: Err(PasskeyAuthenticationError::LoginDisabled),
        }),
        Arc::clone(&sessions),
    );
    let unavailable = build_passkey_authentication_router(None, sessions, &ServerConfig::default());

    for (app, request, expected_status, expected_code) in [
        (
            rejected.clone(),
            json_request(
                "/api/auth/passkey/options",
                json!({"username":"unknown-user"}),
            ),
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
        ),
        (
            rejected.clone(),
            json_request("/api/auth/passkey/verify", json!({"credential": {}})),
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
        ),
        (
            rejected,
            json_request("/api/auth/passkey/verify", json!({"credential": []})),
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
        ),
        (
            disabled,
            json_request(
                "/api/auth/passkey/options",
                json!({"username":"disabled-login"}),
            ),
            StatusCode::FORBIDDEN,
            "password_login_disabled",
        ),
        (
            router(
                Some(FakePasskeyAuthenticationService {
                    start_error: PasskeyAuthenticationError::LoginDisabled,
                    finish_result: Err(PasskeyAuthenticationError::LoginDisabled),
                }),
                Arc::new(FakeSessionAuthenticator::new()),
            ),
            json_request("/api/auth/passkey/verify", json!({"credential": {}})),
            StatusCode::FORBIDDEN,
            "password_login_disabled",
        ),
        (
            unavailable,
            json_request(
                "/api/auth/passkey/options",
                json!({"username":"configured-user"}),
            ),
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
        ),
    ] {
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected_status);
        let body = response_json(response).await;
        assert_eq!(body["code"], expected_code);
        assert_eq!(body.as_object().unwrap().len(), 2);
    }
}

fn router(
    service: Option<FakePasskeyAuthenticationService>,
    sessions: Arc<FakeSessionAuthenticator>,
) -> axum::Router {
    let service = service.map(|service| Arc::new(service) as Arc<dyn PasskeyAuthenticationService>);
    build_passkey_authentication_router(service, sessions, &ServerConfig::default())
}

fn json_request(uri: &str, body: Value) -> Request<Body> {
    let mut request = Request::post(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        "192.0.2.44".parse().unwrap(),
        43_123,
    )));
    request
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 8 * 1024).await.unwrap()).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
