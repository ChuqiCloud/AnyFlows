use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupCreateFuture, AdminGroupDeleteFuture, AdminGroupGetFuture,
    AdminGroupListFuture, AdminGroupListQuery, AdminGroupReadError, AdminGroupReader,
    AdminGroupUpdateCommand, AdminGroupUpdateFuture, AdminGroupWriteError, AdminGroupWriter,
    AdminTokenCreateCommand, AdminTokenCreateFuture, AdminTokenDeleteFuture, AdminTokenGetFuture,
    AdminTokenListFuture, AdminTokenListQuery, AdminTokenReadError, AdminTokenReader,
    AdminTokenUpdateCommand, AdminTokenUpdateFuture, AdminTokenWriteError, AdminTokenWriter,
    AdminUser, AdminUserCreateCommand, AdminUserCreateFuture, AdminUserDeleteFuture,
    AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery, AdminUserPage, AdminUserReadError,
    AdminUserReader, AdminUserStatus, AdminUserUpdateCommand, AdminUserUpdateFuture,
    AdminUserWriteError, AdminUserWriter, LoginCredentials, PlatformAuditEntry, PlatformAuditError,
    PlatformAuditListFuture, PlatformAuditListQuery, PlatformAuditRecordFuture, PlatformAuditScope,
    PlatformAuditService, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
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
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    ChatService, ChatServiceFuture,
    chat_completions::HttpState,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_users::{
        create_admin_user, delete_admin_user, get_admin_user, list_admin_users, update_admin_user,
    },
    middleware::ensure_request_id,
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;
struct UnusedAdminGroupReader;
struct UnusedAdminTokenReader;

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

struct FakeAdminUserReader {
    list_calls: AtomicUsize,
    get_calls: AtomicUsize,
    create_calls: AtomicUsize,
    update_calls: AtomicUsize,
    delete_calls: AtomicUsize,
    audit_calls: AtomicUsize,
    audit_fails: AtomicBool,
}

impl FakeAdminUserReader {
    fn new() -> Self {
        Self {
            list_calls: AtomicUsize::new(0),
            get_calls: AtomicUsize::new(0),
            create_calls: AtomicUsize::new(0),
            update_calls: AtomicUsize::new(0),
            delete_calls: AtomicUsize::new(0),
            audit_calls: AtomicUsize::new(0),
            audit_fails: AtomicBool::new(false),
        }
    }
}

impl PlatformAuditService for FakeAdminUserReader {
    fn record<'a>(&'a self, _entry: PlatformAuditEntry) -> PlatformAuditRecordFuture<'a> {
        self.audit_calls.fetch_add(1, Ordering::Relaxed);
        let fails = self.audit_fails.load(Ordering::Relaxed);
        Box::pin(async move {
            if fails {
                Err(PlatformAuditError::Internal)
            } else {
                Ok(())
            }
        })
    }

    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _scope: PlatformAuditScope,
        _query: PlatformAuditListQuery,
    ) -> PlatformAuditListFuture<'a> {
        Box::pin(async { Err(PlatformAuditError::Internal) })
    }
}

impl AdminUserReader for FakeAdminUserReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminUserReadError::Forbidden);
            }
            if query.after().map(UserId::get) == Some(99) {
                return Err(AdminUserReadError::Internal);
            }
            Ok(AdminUserPage::from_parts(
                vec![
                    sample_user(
                        10,
                        "reader-admin",
                        SessionRole::Admin,
                        AdminUserStatus::Enabled,
                    ),
                    sample_user(
                        11,
                        "reader-user",
                        SessionRole::User,
                        AdminUserStatus::Disabled,
                    ),
                ],
                Some(UserId::new(11).unwrap()),
            ))
        })
    }

    fn get<'a>(&'a self, principal: SessionPrincipal, user_id: UserId) -> AdminUserGetFuture<'a> {
        self.get_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminUserReadError::Forbidden);
            }
            match user_id.get() {
                404 => Err(AdminUserReadError::NotFound),
                500 => Err(AdminUserReadError::Internal),
                id => Ok(sample_user(
                    id,
                    "detail-user",
                    SessionRole::User,
                    AdminUserStatus::Enabled,
                )),
            }
        })
    }
}

impl AdminUserWriter for FakeAdminUserReader {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        _command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        self.create_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminUserWriteError::Forbidden);
            }
            Ok(sample_user(
                21,
                "created-user",
                SessionRole::User,
                AdminUserStatus::Enabled,
            ))
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        _command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        self.update_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminUserWriteError::Forbidden);
            }
            match user_id.get() {
                404 => Err(AdminUserWriteError::NotFound),
                409 => Err(AdminUserWriteError::Conflict),
                500 => Err(AdminUserWriteError::Internal),
                id => Ok(sample_user(
                    id,
                    "updated-user",
                    SessionRole::Admin,
                    AdminUserStatus::Disabled,
                )),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        self.delete_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminUserWriteError::Forbidden);
            }
            match user_id.get() {
                404 => Err(AdminUserWriteError::NotFound),
                500 => Err(AdminUserWriteError::Internal),
                _ => Ok(()),
            }
        })
    }
}

fn router(reader: Arc<FakeAdminUserReader>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let admin_user_reader: Arc<dyn AdminUserReader> = reader.clone();
    let admin_user_writer: Arc<dyn AdminUserWriter> = reader.clone();
    let platform_audit_service: Arc<dyn PlatformAuditService> = reader;
    let admin_groups = Arc::new(UnusedAdminGroupReader);
    let admin_tokens = Arc::new(UnusedAdminTokenReader);
    let collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let collection_routes = get(list_admin_users)
        .route_layer(middleware::from_fn(ensure_request_id))
        .route_layer(collection_authentication.clone())
        .merge(
            axum::routing::post(create_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        );
    Router::new()
        .route("/api/admin/users", collection_routes)
        .route(
            "/api/admin/users/{id}",
            get(get_admin_user)
                .put(update_admin_user)
                .delete(delete_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication),
        )
        .with_state(
            HttpState::new(
                Arc::new(UnusedChatService),
                session_authenticator,
                admin_groups.clone(),
                admin_groups,
                admin_tokens.clone(),
                admin_tokens,
                admin_user_reader,
                admin_user_writer,
            )
            .with_platform_audit_service(Some(platform_audit_service)),
        )
}

#[tokio::test]
async fn admin_can_read_user_list_and_detail_with_no_store_responses() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(Arc::clone(&reader));

    let list_response = app
        .clone()
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users?after=1&limit=2",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    assert_eq!(list_response.headers()[CACHE_CONTROL], "no-store");
    let list_body = response_json(list_response).await;
    assert_eq!(list_body["next_cursor"], 11);
    assert_eq!(list_body["users"][0]["id"], 10);
    assert_eq!(list_body["users"][0]["role"], "admin");
    assert_eq!(list_body["users"][1]["status"], "disabled");

    let detail_response = app
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/10",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(detail_response.status(), StatusCode::OK);
    assert_eq!(detail_response.headers()[CACHE_CONTROL], "no-store");
    let detail_body = response_json(detail_response).await;
    assert_eq!(
        detail_body,
        json!({
            "id": 10,
            "username": "detail-user",
            "email": "detail-user@example.com",
            "role": "user",
            "status": "enabled",
            "default_group_id": 20,
            "quota": 1000,
            "used_quota": 25,
            "frozen_quota": 5,
            "request_count": 7,
            "rpm_limit": 60,
            "concurrency": 3
        })
    );
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.audit_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn admin_can_create_update_and_delete_users_with_no_store_responses() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(Arc::clone(&reader));
    let create_body = valid_create_body();

    let created = app
        .clone()
        .oneshot(authorized_json_request(
            "POST",
            "/api/admin/users",
            ADMIN_TOKEN,
            create_body,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[CACHE_CONTROL], "no-store");
    let created_body = response_json(created).await;
    assert_eq!(created_body["id"], 21);
    assert_eq!(created_body["username"], "created-user");
    assert!(!created_body.to_string().contains("secret-password"));

    let updated = app
        .clone()
        .oneshot(authorized_json_request(
            "PUT",
            "/api/admin/users/21",
            ADMIN_TOKEN,
            valid_update_body(),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()[CACHE_CONTROL], "no-store");
    let updated_body = response_json(updated).await;
    assert_eq!(updated_body["id"], 21);
    assert_eq!(updated_body["username"], "updated-user");
    assert_eq!(updated_body["role"], "admin");
    assert_eq!(updated_body["status"], "disabled");

    let deleted = app
        .oneshot(authorized_request(
            "DELETE",
            "/api/admin/users/21",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(deleted.headers()[CACHE_CONTROL], "no-store");

    assert_eq!(reader.create_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.update_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.delete_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn non_admin_and_invalid_requests_do_not_reach_user_service() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(Arc::clone(&reader));

    let forbidden = app
        .clone()
        .oneshot(authorized_request("GET", "/api/admin/users", USER_TOKEN))
        .await
        .unwrap();
    assert_management_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;

    for uri in ["/api/admin/users?limit=0", "/api/admin/users/abc"] {
        let response = app
            .clone()
            .oneshot(authorized_request("GET", uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    for request in [
        authorized_json_request(
            "POST",
            "/api/admin/users",
            ADMIN_TOKEN,
            json!({"username":"bad","role":"user","status":"enabled","default_group_id":0,"quota":0}),
        ),
        authorized_json_request(
            "PUT",
            "/api/admin/users/10",
            ADMIN_TOKEN,
            json!({"username":" bad ","role":"user","status":"enabled","default_group_id":20}),
        ),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.create_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.update_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.delete_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.audit_calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn audit_persistence_failure_closes_the_user_directory_response() {
    let reader = Arc::new(FakeAdminUserReader::new());
    reader.audit_fails.store(true, Ordering::Relaxed);
    let app = router(Arc::clone(&reader));

    let response = app
        .oneshot(authorized_request("GET", "/api/admin/users", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_management_error(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 1);
    assert_eq!(reader.audit_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn reader_failures_map_to_stable_management_errors() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(reader);

    let not_found = app
        .clone()
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/404",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(not_found, StatusCode::NOT_FOUND, "user_not_found").await;

    let internal = app
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users?after=99",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(
        internal,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;
}

#[tokio::test]
async fn writer_failures_map_to_stable_management_errors() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(reader);
    let body = valid_update_body();

    let missing = app
        .clone()
        .oneshot(authorized_json_request(
            "PUT",
            "/api/admin/users/404",
            ADMIN_TOKEN,
            body.clone(),
        ))
        .await
        .unwrap();
    assert_management_error(missing, StatusCode::NOT_FOUND, "user_not_found").await;

    let conflict = app
        .clone()
        .oneshot(authorized_json_request(
            "PUT",
            "/api/admin/users/409",
            ADMIN_TOKEN,
            body.clone(),
        ))
        .await
        .unwrap();
    assert_management_error(conflict, StatusCode::CONFLICT, "user_conflict").await;

    let internal = app
        .clone()
        .oneshot(authorized_json_request(
            "PUT",
            "/api/admin/users/500",
            ADMIN_TOKEN,
            body,
        ))
        .await
        .unwrap();
    assert_management_error(
        internal,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;

    let delete_missing = app
        .clone()
        .oneshot(authorized_request(
            "DELETE",
            "/api/admin/users/404",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(delete_missing, StatusCode::NOT_FOUND, "user_not_found").await;

    let delete_internal = app
        .oneshot(authorized_request(
            "DELETE",
            "/api/admin/users/500",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(
        delete_internal,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    )
    .await;
}

#[tokio::test]
async fn unknown_paths_and_methods_are_not_rewritten_by_authentication() {
    let reader = Arc::new(FakeAdminUserReader::new());
    let app = router(Arc::clone(&reader));

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/admin/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let method_not_allowed = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/admin/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.create_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.update_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.delete_calls.load(Ordering::Relaxed), 0);
}

fn sample_user(id: i64, username: &str, role: SessionRole, status: AdminUserStatus) -> AdminUser {
    AdminUser::from_parts(
        UserId::new(id).unwrap(),
        username.to_owned(),
        Some(format!("{username}@example.com")),
        role,
        status,
        GroupId::new(20).unwrap(),
        1_000,
        25,
        5,
        7,
        Some(60),
        Some(3),
    )
}

fn authorized_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn authorized_json_request(method: &str, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn valid_create_body() -> Value {
    json!({
        "username": "new-user",
        "email": "new-user@example.com",
        "password": "secret-password",
        "role": "user",
        "status": "enabled",
        "default_group_id": 20,
        "quota": 1000,
        "rpm_limit": 60,
        "concurrency": 3
    })
}

fn valid_update_body() -> Value {
    json!({
        "username": "updated-user",
        "email": "updated@example.com",
        "password": null,
        "role": "admin",
        "status": "disabled",
        "default_group_id": 20,
        "rpm_limit": 60,
        "concurrency": 3
    })
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
