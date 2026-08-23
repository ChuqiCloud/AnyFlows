use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminGroup, AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture,
    AdminGroupGetFuture, AdminGroupListFuture, AdminGroupListQuery, AdminGroupPage, AdminGroupPeak,
    AdminGroupReadError, AdminGroupReader, AdminGroupUpdateCommand, AdminGroupUpdateFuture,
    AdminGroupWindow, AdminGroupWriteError, AdminGroupWriter, AdminTokenCreateCommand,
    AdminTokenCreateFuture, AdminTokenDeleteFuture, AdminTokenGetFuture, AdminTokenListFuture,
    AdminTokenListQuery, AdminTokenReadError, AdminTokenReader, AdminTokenUpdateCommand,
    AdminTokenUpdateFuture, AdminTokenWriteError, AdminTokenWriter, AdminUserCreateCommand,
    AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserGetFuture, AdminUserListFuture,
    AdminUserListQuery, AdminUserReadError, AdminUserReader, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{AfError, GatewayPrincipal, GroupId, TokenId, UserId};
use af_protocol::CanonicalRequestEnvelope;
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
    management_groups::{get_admin_group, list_admin_groups},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;

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
                .map(|principal| {
                    SessionAuthentication::new(
                        principal,
                        GroupId::new(1).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

struct FakeAdminGroupReader {
    list_calls: AtomicUsize,
    get_calls: AtomicUsize,
}

impl FakeAdminGroupReader {
    fn new() -> Self {
        Self {
            list_calls: AtomicUsize::new(0),
            get_calls: AtomicUsize::new(0),
        }
    }
}

impl AdminGroupReader for FakeAdminGroupReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminGroupReadError::Forbidden);
            }
            if query.after().map(GroupId::get) == Some(99) {
                return Err(AdminGroupReadError::Internal);
            }
            Ok(AdminGroupPage::from_parts(
                vec![sample_group(10, "standard"), sample_group(11, "premium")],
                Some(GroupId::new(11).unwrap()),
            ))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        self.get_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminGroupReadError::Forbidden);
            }
            match group_id.get() {
                404 => Err(AdminGroupReadError::NotFound),
                500 => Err(AdminGroupReadError::Internal),
                id => Ok(sample_group(id, "detail")),
            }
        })
    }
}

impl AdminGroupWriter for FakeAdminGroupReader {
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

struct UnusedAdminUserAccess;
struct UnusedAdminTokenAccess;

impl AdminTokenReader for UnusedAdminTokenAccess {
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

impl AdminTokenWriter for UnusedAdminTokenAccess {
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

fn router(reader: Arc<FakeAdminGroupReader>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let admin_group_reader: Arc<dyn AdminGroupReader> = reader.clone();
    let admin_group_writer: Arc<dyn AdminGroupWriter> = reader;
    let admin_tokens = Arc::new(UnusedAdminTokenAccess);
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
            "/api/admin/groups",
            get(list_admin_groups)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        )
        .route(
            "/api/admin/groups/{id}",
            get(get_admin_group)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication),
        )
        .with_state(HttpState::new(
            Arc::new(UnusedChatService),
            session_authenticator,
            admin_group_reader,
            admin_group_writer,
            admin_tokens.clone(),
            admin_tokens,
            admin_users.clone(),
            admin_users,
        ))
}

#[tokio::test]
async fn admin_can_read_group_list_and_detail_with_no_store_responses() {
    let reader = Arc::new(FakeAdminGroupReader::new());
    let app = router(Arc::clone(&reader));

    let list_response = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/groups?after=1&limit=2",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    assert_eq!(list_response.headers()[CACHE_CONTROL], "no-store");
    let list_body = response_json(list_response).await;
    assert_eq!(list_body["next_cursor"], 11);
    assert_eq!(list_body["groups"][0]["id"], 10);
    assert_eq!(list_body["groups"][0]["ratio_micros"], 1_250_000);
    assert_eq!(list_body["groups"][0]["peak_start"], "08:05:06");
    assert_eq!(list_body["groups"][0]["peak_end"], "22:30:00");

    let detail_response = app
        .oneshot(authorized_request("/api/admin/groups/10", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(detail_response.status(), StatusCode::OK);
    assert_eq!(detail_response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
        response_json(detail_response).await,
        json!({
            "id": 10,
            "name": "detail",
            "display_name": "detail display",
            "ratio_micros": 1250000,
            "peak_ratio_micros": 1500000,
            "peak_start": "08:05:06",
            "peak_end": "22:30:00",
            "is_exclusive": true,
            "daily_limit": 10000,
            "weekly_limit": null,
            "monthly_limit": 200000,
            "daily_window": {"usage": 100, "started_at": 1700000000, "resets_at": 1700086400},
            "weekly_window": {"usage": 200, "started_at": 1699833600, "resets_at": 1700438400},
            "monthly_window": {"usage": 300, "started_at": 1698796800, "resets_at": 1701388800},
            "rpm_limit": 60,
            "fallback_group_id": 20,
            "flags": {"allow_fallback": true}
        })
    );
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn non_admin_is_rejected_before_group_service() {
    let reader = Arc::new(FakeAdminGroupReader::new());
    let app = router(Arc::clone(&reader));

    let list_response = app
        .clone()
        .oneshot(authorized_request("/api/admin/groups", USER_TOKEN))
        .await
        .unwrap();
    assert_management_error(list_response, StatusCode::FORBIDDEN, "forbidden").await;

    let detail_response = app
        .oneshot(authorized_request("/api/admin/groups/10", USER_TOKEN))
        .await
        .unwrap();
    assert_management_error(detail_response, StatusCode::FORBIDDEN, "forbidden").await;
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn invalid_list_queries_are_rejected_before_group_service() {
    let reader = Arc::new(FakeAdminGroupReader::new());
    let app = router(Arc::clone(&reader));

    for uri in [
        "/api/admin/groups?limit=0",
        "/api/admin/groups?limit=1&limit=2",
        "/api/admin/groups?after=1&after=2",
        "/api/admin/groups?unknown=1",
        "/api/admin/groups?after=abc",
        "/api/admin/groups?after=%FF",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request(uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn group_failures_map_to_stable_management_errors() {
    let reader = Arc::new(FakeAdminGroupReader::new());
    let app = router(reader);

    let not_found = app
        .clone()
        .oneshot(authorized_request("/api/admin/groups/404", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_management_error(not_found, StatusCode::NOT_FOUND, "group_not_found").await;

    let detail_internal = app
        .clone()
        .oneshot(authorized_request("/api/admin/groups/500", ADMIN_TOKEN))
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
            "/api/admin/groups?after=99",
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

fn sample_group(id: i64, name: &str) -> AdminGroup {
    AdminGroup::from_parts(
        GroupId::new(id).unwrap(),
        name.to_owned(),
        format!("{name} display"),
        1_250_000,
        Some(AdminGroupPeak::from_parts(
            1_500_000,
            8 * 3_600 + 5 * 60 + 6,
            22 * 3_600 + 30 * 60,
        )),
        true,
        Some(10_000),
        None,
        Some(200_000),
        AdminGroupWindow::from_parts(100, 1_700_000_000, 1_700_086_400),
        AdminGroupWindow::from_parts(200, 1_699_833_600, 1_700_438_400),
        AdminGroupWindow::from_parts(300, 1_698_796_800, 1_701_388_800),
        Some(60),
        Some(GroupId::new(20).unwrap()),
        json!({"allow_fallback": true}),
    )
}

fn authorized_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
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
