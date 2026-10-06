use std::sync::{Arc, Mutex};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture, AdminGroupGetFuture,
    AdminGroupListFuture, AdminGroupListQuery, AdminGroupReadError, AdminGroupReader,
    AdminGroupUpdateCommand, AdminGroupUpdateFuture, AdminGroupWriteError, AdminGroupWriter,
    AdminTokenCreateCommand, AdminTokenCreateFuture, AdminTokenDeleteFuture, AdminTokenGetFuture,
    AdminTokenListFuture, AdminTokenListQuery, AdminTokenReadError, AdminTokenReader,
    AdminTokenUpdateCommand, AdminTokenUpdateFuture, AdminTokenWriteError, AdminTokenWriter,
    AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserGetFuture,
    AdminUserListFuture, AdminUserListQuery, AdminUserReadError, AdminUserReader,
    AdminUserUpdateCommand, AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter,
    IssuedSession, LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, SessionToken,
};
use af_domain::{AfError, GatewayPrincipal, GroupId, TokenId, UserId};
use af_protocol::CanonicalRequestEnvelope;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    ChatService, ChatServiceFuture,
    chat_completions::HttpState,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::{LoginRequest, current_session, login_with_credentials},
};

const SESSION_TOKEN: &str = "signed.jwt.value";

struct UnusedChatService;
struct UnusedAdminGroupReader;
struct UnusedAdminTokenReader;
struct UnusedAdminUserReader;

impl ChatService for UnusedChatService {
    fn chat_completions<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<af_domain::ConcurrencyLimit>,
        _request: CanonicalRequestEnvelope,
        _response_protocol: af_domain::Protocol,
        _request_id: &'a str,
        _diagnostic: af_relay::RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl AdminGroupReader for UnusedAdminGroupReader {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }
}

impl AdminGroupWriter for UnusedAdminGroupReader {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
        _command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }
}

impl AdminTokenReader for UnusedAdminTokenReader {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }
}

impl AdminTokenWriter for UnusedAdminTokenReader {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
        _command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }
}

impl AdminUserReader for UnusedAdminUserReader {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }

    fn get<'a>(&'a self, _principal: SessionPrincipal, _user_id: UserId) -> AdminUserGetFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }
}

impl AdminUserWriter for UnusedAdminUserReader {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
        _command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }
}

struct FakeSessionAuthenticator {
    authenticated_tokens: Mutex<Vec<String>>,
}

impl FakeSessionAuthenticator {
    fn new() -> Self {
        Self {
            authenticated_tokens: Mutex::new(Vec::new()),
        }
    }
}

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        let outcome =
            if credentials.username() == "admin" && credentials.password() == b"correct-password" {
                Ok(IssuedSession::from_parts(
                    SessionToken::from_string(SESSION_TOKEN.to_owned()),
                    test_principal(),
                    current_timestamp() + 3_600,
                ))
            } else if credentials.username() == "internal" {
                Err(SessionAuthenticationError::Internal)
            } else {
                Err(SessionAuthenticationError::InvalidCredentials)
            };
        Box::pin(async move { outcome })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        self.authenticated_tokens
            .lock()
            .unwrap()
            .push(token.to_owned());
        let outcome = if token == SESSION_TOKEN {
            Ok(SessionAuthentication::new(
                test_principal(),
                GroupId::new(3).unwrap(),
                current_timestamp() + 3_600,
            ))
        } else {
            Err(SessionAuthenticationError::InvalidSession)
        };
        Box::pin(async move { outcome })
    }
}

fn test_principal() -> SessionPrincipal {
    SessionPrincipal::new(UserId::new(42).unwrap(), SessionRole::Admin)
}

fn router(authenticator: Arc<FakeSessionAuthenticator>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = authenticator;
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/auth/login", post(login))
        .route(
            "/api/auth/session",
            get(current_session).route_layer(authentication),
        )
        .with_state({
            let admin_users = Arc::new(UnusedAdminUserReader);
            let admin_groups = Arc::new(UnusedAdminGroupReader);
            HttpState::new(
                Arc::new(UnusedChatService),
                session_authenticator,
                admin_groups.clone(),
                admin_groups,
                Arc::new(UnusedAdminTokenReader),
                Arc::new(UnusedAdminTokenReader),
                admin_users.clone(),
                admin_users,
            )
        })
}

/// 复用生产登录响应构造，能力开关本身由注册路由回归覆盖。
async fn login(
    State(state): State<HttpState>,
    request: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    login_with_credentials(&state, request.username, request.password).await
}

#[tokio::test]
async fn login_and_session_routes_use_stable_management_contract() {
    let authenticator = Arc::new(FakeSessionAuthenticator::new());
    let app = router(Arc::clone(&authenticator));

    let login_response = app
        .clone()
        .oneshot(json_request(
            "/api/auth/login",
            json!({"username":"admin","password":"correct-password"}),
        ))
        .await
        .unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
    assert_eq!(login_response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(login_response).await;
    assert_eq!(body["access_token"], SESSION_TOKEN);
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["user"], json!({"id":42,"role":"admin"}));

    let request = Request::builder()
        .uri("/api/auth/session")
        .header(AUTHORIZATION, format!("Bearer {SESSION_TOKEN}"))
        .body(Body::empty())
        .unwrap();
    let session_response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(session_response.status(), StatusCode::OK);
    assert_eq!(session_response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(session_response).await;
    assert_eq!(body["user"], json!({"id":42,"role":"admin"}));
    assert_eq!(
        authenticator.authenticated_tokens.lock().unwrap().clone(),
        vec![SESSION_TOKEN.to_owned()]
    );

    let method_not_allowed = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn management_failures_do_not_leak_credentials_or_change_method_errors() {
    let app = router(Arc::new(FakeSessionAuthenticator::new()));

    for (request, expected_status, expected_code) in [
        (
            json_request(
                "/api/auth/login",
                json!({"username":"admin","password":"wrong-password"}),
            ),
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
        ),
        (
            json_request(
                "/api/auth/login",
                json!({"username":"internal","password":"secret"}),
            ),
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        ),
        (
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from("{invalid-json"))
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            Request::builder()
                .uri("/api/auth/session")
                .body(Body::empty())
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "invalid_session",
        ),
        (
            Request::builder()
                .uri("/api/auth/session")
                .header(AUTHORIZATION, "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "invalid_session",
        ),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected_status);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let rendered = response_json(response).await.to_string();
        assert!(rendered.contains(expected_code));
        assert!(!rendered.contains("wrong-password"));
        assert!(!rendered.contains("wrong-token"));
    }
}

fn json_request(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
