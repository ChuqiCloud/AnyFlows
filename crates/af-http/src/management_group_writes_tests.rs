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
    routing::{post, put},
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
    management_authorization::authorize_management_admin,
    management_group_writes::{create_admin_group, delete_admin_group, update_admin_group},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;
struct UnusedAdminTokenAccess;
struct UnusedAdminUserAccess;

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

struct FakeAdminGroupAccess {
    create_calls: AtomicUsize,
    update_calls: AtomicUsize,
    delete_calls: AtomicUsize,
}

impl FakeAdminGroupAccess {
    fn new() -> Self {
        Self {
            create_calls: AtomicUsize::new(0),
            update_calls: AtomicUsize::new(0),
            delete_calls: AtomicUsize::new(0),
        }
    }
}

impl AdminGroupReader for FakeAdminGroupAccess {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async { Ok(AdminGroupPage::from_parts(Vec::new(), None)) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::NotFound) })
    }
}

impl AdminGroupWriter for FakeAdminGroupAccess {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        _command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        self.create_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminGroupWriteError::Forbidden);
            }
            Ok(sample_group(21, "created"))
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        _command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        self.update_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminGroupWriteError::Forbidden);
            }
            match group_id.get() {
                404 => Err(AdminGroupWriteError::NotFound),
                409 => Err(AdminGroupWriteError::Conflict),
                500 => Err(AdminGroupWriteError::Internal),
                id => Ok(sample_group(id, "updated")),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        self.delete_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminGroupWriteError::Forbidden);
            }
            match group_id.get() {
                404 => Err(AdminGroupWriteError::NotFound),
                409 => Err(AdminGroupWriteError::InUse),
                500 => Err(AdminGroupWriteError::Internal),
                _ => Ok(()),
            }
        })
    }
}

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

fn router(access: Arc<FakeAdminGroupAccess>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let admin_group_reader: Arc<dyn AdminGroupReader> = access.clone();
    let admin_group_writer: Arc<dyn AdminGroupWriter> = access;
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
            post(create_admin_group)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        )
        .route(
            "/api/admin/groups/{id}",
            put(update_admin_group)
                .delete(delete_admin_group)
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
async fn create_and_update_use_canonical_group_contract() {
    let access = Arc::new(FakeAdminGroupAccess::new());
    let app = router(Arc::clone(&access));
    let created = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/groups",
            ADMIN_TOKEN,
            valid_body(),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(created).await;
    assert_eq!(body["id"], 21);
    assert_eq!(body["name"], "created");
    assert_eq!(body["ratio_micros"], 1_250_000);
    assert_eq!(body["peak_start"], "08:00:00");
    assert_eq!(body["peak_end"], "20:30:00");

    let updated = app
        .clone()
        .oneshot(write_request(
            "PUT",
            "/api/admin/groups/31",
            ADMIN_TOKEN,
            valid_body(),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(response_json(updated).await["name"], "updated");
    assert_eq!(access.create_calls.load(Ordering::Relaxed), 1);
    assert_eq!(access.update_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn invalid_bodies_and_non_admins_do_not_reach_writer() {
    let access = Arc::new(FakeAdminGroupAccess::new());
    let app = router(Arc::clone(&access));
    for body in [
        json!({"name":"missing-fields"}),
        json!({
            "name":"bad-peak","display_name":"Bad Peak","ratio_micros":1000000,
            "peak_ratio_micros":1200000,"peak_start":"08:00:00","peak_end":Value::Null,
            "is_exclusive":false,"flags":{}
        }),
        json!({
            "name":"bad-time","display_name":"Bad Time","ratio_micros":1000000,
            "peak_ratio_micros":1200000,"peak_start":"8:00:00","peak_end":"20:00:00",
            "is_exclusive":false,"flags":{}
        }),
    ] {
        let response = app
            .clone()
            .oneshot(write_request(
                "POST",
                "/api/admin/groups",
                ADMIN_TOKEN,
                body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["code"], "invalid_request");
    }
    let forbidden = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/groups",
            USER_TOKEN,
            valid_body(),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(access.create_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn write_errors_have_stable_codes_and_delete_is_no_store() {
    let app = router(Arc::new(FakeAdminGroupAccess::new()));
    for (id, status, code) in [
        (404, StatusCode::NOT_FOUND, "group_not_found"),
        (409, StatusCode::CONFLICT, "group_conflict"),
        (500, StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    ] {
        let response = app
            .clone()
            .oneshot(write_request(
                "PUT",
                &format!("/api/admin/groups/{id}"),
                ADMIN_TOKEN,
                valid_body(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response_json(response).await["code"], code);
    }

    let in_use = app
        .clone()
        .oneshot(empty_request(
            "DELETE",
            "/api/admin/groups/409",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(in_use.status(), StatusCode::CONFLICT);
    assert_eq!(response_json(in_use).await["code"], "group_in_use");
    let deleted = app
        .oneshot(empty_request("DELETE", "/api/admin/groups/42", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(deleted.headers()[CACHE_CONTROL], "no-store");
}

fn sample_group(id: i64, name: &str) -> AdminGroup {
    AdminGroup::from_parts(
        GroupId::new(id).unwrap(),
        name.to_owned(),
        format!("{name} display"),
        1_250_000,
        Some(AdminGroupPeak::from_parts(
            1_500_000,
            8 * 3_600,
            20 * 3_600 + 30 * 60,
        )),
        true,
        Some(10_000),
        Some(60_000),
        Some(200_000),
        AdminGroupWindow::from_parts(100, 1_700_000_000, 1_700_086_400),
        AdminGroupWindow::from_parts(200, 1_699_833_600, 1_700_438_400),
        AdminGroupWindow::from_parts(300, 1_698_796_800, 1_701_388_800),
        Some(120),
        Some(GroupId::new(1).unwrap()),
        json!({"claude_code_only": true}),
    )
}

fn valid_body() -> Value {
    json!({
        "name": "vip",
        "display_name": "VIP",
        "ratio_micros": 1_250_000,
        "peak_ratio_micros": 1_500_000,
        "peak_start": "08:00:00",
        "peak_end": "20:30:00",
        "is_exclusive": true,
        "daily_limit": 10_000,
        "weekly_limit": 60_000,
        "monthly_limit": 200_000,
        "rpm_limit": 120,
        "fallback_group_id": 1,
        "flags": {"claude_code_only": true}
    })
}

fn write_request(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn empty_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
