use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture, AdminGroupGetFuture,
    AdminGroupListFuture, AdminGroupListQuery, AdminGroupReadError, AdminGroupReader,
    AdminGroupUpdateCommand, AdminGroupUpdateFuture, AdminGroupWriteError, AdminGroupWriter,
    AdminToken, AdminTokenGetFuture, AdminTokenListFuture, AdminTokenListQuery, AdminTokenPage,
    AdminTokenReadError, AdminTokenReader, AdminTokenStatus, AdminUserCreateCommand,
    AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserGetFuture, AdminUserListFuture,
    AdminUserListQuery, AdminUserReadError, AdminUserReader, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{AfError, GatewayPrincipal, GroupId, TokenId, UserId};
use af_protocol::CanonicalRequest;
use axum::{
    Router,
    body::{Body, to_bytes},
    middleware,
    routing::get,
};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    ChatService, ChatServiceFuture,
    chat_completions::HttpState,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_tokens::{get_admin_token, list_admin_tokens},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;
struct UnusedAdminGroupAccess;
struct UnusedAdminUserAccess;

impl ChatService for UnusedChatService {
    fn chat_completions<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _request: CanonicalRequest,
        _request_id: &'a str,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl AdminGroupReader for UnusedAdminGroupAccess {
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

impl AdminGroupWriter for UnusedAdminGroupAccess {
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

impl AdminUserReader for UnusedAdminUserAccess {
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

impl AdminUserWriter for UnusedAdminUserAccess {
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

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
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
                .map(|principal| SessionAuthentication::new(principal, current_timestamp() + 60))
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

struct FakeAdminTokenReader {
    list_calls: AtomicUsize,
    get_calls: AtomicUsize,
}

impl FakeAdminTokenReader {
    fn new() -> Self {
        Self {
            list_calls: AtomicUsize::new(0),
            get_calls: AtomicUsize::new(0),
        }
    }
}

impl AdminTokenReader for FakeAdminTokenReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminTokenReadError::Forbidden);
            }
            if query.after().map(TokenId::get) == Some(99) {
                return Err(AdminTokenReadError::Internal);
            }
            Ok(AdminTokenPage::from_parts(
                vec![sample_token(10, "primary"), sample_token(11, "backup")],
                Some(TokenId::new(11).unwrap()),
            ))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        self.get_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminTokenReadError::Forbidden);
            }
            match token_id.get() {
                404 => Err(AdminTokenReadError::NotFound),
                500 => Err(AdminTokenReadError::Internal),
                id => Ok(sample_token(id, "detail")),
            }
        })
    }
}

fn router(reader: Arc<FakeAdminTokenReader>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let admin_groups = Arc::new(UnusedAdminGroupAccess);
    let admin_token_reader: Arc<dyn AdminTokenReader> = reader;
    let admin_users = Arc::new(UnusedAdminUserAccess);
    let collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/tokens",
            get(list_admin_tokens)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        )
        .route(
            "/api/admin/tokens/{id}",
            get(get_admin_token)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication),
        )
        .with_state(HttpState::new(
            Arc::new(UnusedChatService),
            session_authenticator,
            admin_groups.clone(),
            admin_groups,
            admin_token_reader,
            admin_users.clone(),
            admin_users,
        ))
}

#[tokio::test]
async fn admin_can_read_token_list_and_detail_without_sensitive_fields() {
    let reader = Arc::new(FakeAdminTokenReader::new());
    let app = router(Arc::clone(&reader));

    let list_response = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/tokens?after=1&limit=2",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    assert_eq!(list_response.headers()[CACHE_CONTROL], "no-store");
    let list_body = response_json(list_response).await;
    assert_eq!(list_body["next_cursor"], 11);
    assert_eq!(list_body["tokens"][0]["id"], 10);
    assert_eq!(list_body["tokens"][0]["user_id"], 20);
    assert_eq!(list_body["tokens"][0]["key_prefix"], "sk-af-public000001");
    assert_eq!(list_body["tokens"][0]["status"], "enabled");
    assert_eq!(
        list_body["tokens"][0]["model_limits"],
        json!(["gpt-5.5", "gpt-5.5"])
    );
    assert_eq!(
        list_body["tokens"][0]["allow_ips"],
        json!(["127.0.0.1", "10.0.0.0/8"])
    );
    let rendered = list_body.to_string();
    assert!(!rendered.contains("key_hash"));
    assert!(!rendered.contains("full-token-secret"));

    let detail_response = app
        .oneshot(authorized_request("/api/admin/tokens/12", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_body = response_json(detail_response).await;
    assert_eq!(detail_body["id"], 12);
    assert_eq!(detail_body["name"], "detail");
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn non_admin_session_is_rejected_before_token_reader() {
    let reader = Arc::new(FakeAdminTokenReader::new());
    let app = router(Arc::clone(&reader));

    let list_response = app
        .clone()
        .oneshot(authorized_request("/api/admin/tokens", USER_TOKEN))
        .await
        .unwrap();
    assert_management_error(list_response, StatusCode::FORBIDDEN, "forbidden").await;

    let detail_response = app
        .oneshot(authorized_request("/api/admin/tokens/10", USER_TOKEN))
        .await
        .unwrap();
    assert_management_error(detail_response, StatusCode::FORBIDDEN, "forbidden").await;
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn invalid_token_read_inputs_are_rejected_before_reader() {
    let reader = Arc::new(FakeAdminTokenReader::new());
    let app = router(Arc::clone(&reader));

    for uri in [
        "/api/admin/tokens?limit=0",
        "/api/admin/tokens?limit=1&limit=2",
        "/api/admin/tokens?after=1&after=2",
        "/api/admin/tokens?unknown=1",
        "/api/admin/tokens?after=abc",
        "/api/admin/tokens?after=%FF",
        "/api/admin/tokens/-1",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request(uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn token_failures_map_to_stable_management_errors() {
    let reader = Arc::new(FakeAdminTokenReader::new());
    let app = router(Arc::clone(&reader));

    let not_found = app
        .clone()
        .oneshot(authorized_request("/api/admin/tokens/404", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_management_error(not_found, StatusCode::NOT_FOUND, "token_not_found").await;

    let detail_internal = app
        .clone()
        .oneshot(authorized_request("/api/admin/tokens/500", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_management_error(
        detail_internal,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;

    let list_internal = app
        .oneshot(authorized_request(
            "/api/admin/tokens?after=99",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(
        list_internal,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;
}

fn sample_token(id: i64, name: &str) -> AdminToken {
    AdminToken::from_parts(
        TokenId::new(id).unwrap(),
        UserId::new(20).unwrap(),
        "sk-af-public000001".to_owned(),
        name.to_owned(),
        AdminTokenStatus::Enabled,
        Some(GroupId::new(30).unwrap()),
        1_000,
        false,
        25,
        Some(1_800_000_000),
        Some(vec!["gpt-5.5".to_owned(), "gpt-5.5".to_owned()]),
        Some(vec!["127.0.0.1".to_owned(), "10.0.0.0/8".to_owned()]),
        true,
        Some(100),
        Some(200),
        Some(300),
        10,
        20,
        30,
        1_000,
        2_000,
        3_000,
        Some(1_000),
        4,
    )
}

fn authorized_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn assert_management_error(
    response: axum::response::Response,
    status: StatusCode,
    code: &str,
) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response_json(response).await["code"], code);
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
