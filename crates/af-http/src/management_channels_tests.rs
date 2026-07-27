use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminChannel, AdminChannelGetFuture, AdminChannelListFuture, AdminChannelListQuery,
    AdminChannelPage, AdminChannelReadError, AdminChannelReader, AdminCredential,
    AdminCredentialGetFuture, AdminCredentialListFuture, AdminCredentialListQuery,
    AdminCredentialMultiKeyMode, AdminCredentialPage, AdminCredentialQuotaDimension,
    AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture, AdminGroupGetFuture,
    AdminGroupListFuture, AdminGroupListQuery, AdminGroupReadError, AdminGroupReader,
    AdminGroupUpdateCommand, AdminGroupUpdateFuture, AdminGroupWriteError, AdminGroupWriter,
    AdminRoutingStatus, AdminTokenCreateCommand, AdminTokenCreateFuture, AdminTokenDeleteFuture,
    AdminTokenGetFuture, AdminTokenListFuture, AdminTokenListQuery, AdminTokenReadError,
    AdminTokenReader, AdminTokenUpdateCommand, AdminTokenUpdateFuture, AdminTokenWriteError,
    AdminTokenWriter, AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture,
    AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery, AdminUserReadError,
    AdminUserReader, AdminUserUpdateCommand, AdminUserUpdateFuture, AdminUserWriteError,
    AdminUserWriter, LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_domain::{
    AfError, ChannelId, ChannelType, CredentialId, CredentialKind, GatewayPrincipal, GroupId,
    Protocol, TokenId, UserId,
};
use af_protocol::CanonicalRequest;
use axum::{
    Router,
    body::{Body, to_bytes},
    middleware,
    routing::get,
};
use http::{Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    ChatService, ChatServiceFuture,
    chat_completions::HttpState,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_channels::{get_admin_channel, list_admin_channels},
    management_credentials::{get_admin_credential, list_admin_credentials},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;
struct UnusedAdminAccess;

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

impl AdminGroupReader for UnusedAdminAccess {
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

impl AdminGroupWriter for UnusedAdminAccess {
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

impl AdminTokenReader for UnusedAdminAccess {
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

impl AdminTokenWriter for UnusedAdminAccess {
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

impl AdminUserReader for UnusedAdminAccess {
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

impl AdminUserWriter for UnusedAdminAccess {
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

struct FakeAdminChannelReader {
    calls: AtomicUsize,
}

impl FakeAdminChannelReader {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl AdminChannelReader for FakeAdminChannelReader {
    fn list_channels<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if query.after().map(ChannelId::get) == Some(500) {
                return Err(AdminChannelReadError::Internal);
            }
            Ok(AdminChannelPage::from_parts(
                vec![sample_channel(10, "primary"), sample_channel(11, "backup")],
                Some(ChannelId::new(11).unwrap()),
            ))
        })
    }

    fn get_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            match channel_id.get() {
                404 => Err(AdminChannelReadError::ChannelNotFound),
                500 => Err(AdminChannelReadError::Internal),
                id => Ok(sample_channel(id, "detail")),
            }
        })
    }

    fn list_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        _query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                return Err(AdminChannelReadError::ChannelNotFound);
            }
            Ok(AdminCredentialPage::from_parts(
                vec![sample_credential(channel_id, 20)],
                Some(CredentialId::new(20).unwrap()),
            ))
        })
    }

    fn get_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                return Err(AdminChannelReadError::ChannelNotFound);
            }
            if credential_id.get() == 404 {
                return Err(AdminChannelReadError::CredentialNotFound);
            }
            Ok(sample_credential(channel_id, credential_id.get()))
        })
    }
}

fn router(reader: Arc<FakeAdminChannelReader>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let unused = Arc::new(UnusedAdminAccess);
    let collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let credential_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let credential_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let channel_reader: Arc<dyn AdminChannelReader> = reader;
    Router::new()
        .route(
            "/api/admin/channels",
            get(list_admin_channels)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        )
        .route(
            "/api/admin/channels/{id}",
            get(get_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials",
            get(list_admin_credentials)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}",
            get(get_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_item_authentication),
        )
        .with_state(
            HttpState::new(
                Arc::new(UnusedChatService),
                session_authenticator,
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused,
            )
            .with_admin_channel_reader(channel_reader),
        )
}

#[tokio::test]
async fn admin_can_read_channel_and_credential_snapshots() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let app = router(Arc::clone(&reader));

    let list = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/channels?after=1&limit=2",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list = response_json(list).await;
    assert_eq!(list["channels"][0]["id"], 10);
    assert_eq!(list["channels"][0]["type"], "openai");
    assert_eq!(list["channels"][0]["status"], "enabled");
    assert_eq!(list["next_cursor"], 11);

    let detail = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels/12", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(response_json(detail).await["name"], "detail");

    let credentials = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/channels/10/credentials?limit=1",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    let credentials = response_json(credentials).await;
    assert_eq!(credentials["credentials"][0]["kind"], "api_key");
    assert_eq!(credentials["credentials"][0]["multi_key_mode"], "random");
    assert!(!credentials.to_string().contains("secret"));

    let credential = app
        .oneshot(authorized_request(
            "/api/admin/channels/10/credentials/21",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(response_json(credential).await["id"], 21);
    assert_eq!(reader.calls.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn authorization_inputs_and_not_found_errors_are_closed() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let app = router(Arc::clone(&reader));

    let forbidden = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels", USER_TOKEN))
        .await
        .unwrap();
    assert_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);

    for uri in [
        "/api/admin/channels?limit=0",
        "/api/admin/channels?after=1&after=2",
        "/api/admin/channels/-1",
        "/api/admin/channels/1/credentials?after=%FF",
        "/api/admin/channels/1/credentials/abc",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request(uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);

    let channel_missing = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels/404", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_error(channel_missing, StatusCode::NOT_FOUND, "channel_not_found").await;
    let credential_missing = app
        .oneshot(authorized_request(
            "/api/admin/channels/1/credentials/404",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_error(
        credential_missing,
        StatusCode::NOT_FOUND,
        "credential_not_found",
    )
    .await;
}

fn sample_channel(id: i64, name: &str) -> AdminChannel {
    AdminChannel::from_parts(
        ChannelId::new(id).unwrap(),
        name.to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some("https://api.example.com/v1".to_owned()),
        AdminRoutingStatus::Enabled,
        10,
        20,
        true,
        json!({"gpt-public": "gpt-upstream"}),
        json!({"temperature": 0}),
        Some(1_000),
        25,
        Some("primary".to_owned()),
        1_000,
        2_000,
    )
}

fn sample_credential(channel_id: ChannelId, id: i64) -> AdminCredential {
    AdminCredential::from_parts(
        CredentialId::new(id).unwrap(),
        channel_id,
        CredentialKind::ApiKey,
        AdminRoutingStatus::Enabled,
        Some(AdminCredentialMultiKeyMode::Random),
        30,
        40,
        Some(2),
        Some(1_100_000),
        Some(900_000),
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        AdminCredentialQuotaDimension::Global,
        None,
        Some("example-oauth".to_owned()),
        Some("account-public-id".to_owned()),
        Some("project-public-id".to_owned()),
        None,
        1_000,
        2_000,
    )
}

fn authorized_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn assert_error(response: axum::response::Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
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
