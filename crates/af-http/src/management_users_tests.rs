use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminUser, AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery, AdminUserPage,
    AdminUserReadError, AdminUserReader, AdminUserStatus, LoginCredentials, SessionAuthentication,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{AfError, GatewayPrincipal, GroupId, UserId};
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
    management_users::{get_admin_user, list_admin_users},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;

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

struct FakeAdminUserReader {
    list_calls: AtomicUsize,
    get_calls: AtomicUsize,
}

impl FakeAdminUserReader {
    fn new() -> Self {
        Self {
            list_calls: AtomicUsize::new(0),
            get_calls: AtomicUsize::new(0),
        }
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

fn router(reader: Arc<FakeAdminUserReader>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let admin_user_reader: Arc<dyn AdminUserReader> = reader;
    let list_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let detail_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/users",
            get(list_admin_users)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(list_authentication),
        )
        .route(
            "/api/admin/users/{id}",
            get(get_admin_user)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(detail_authentication),
        )
        .with_state(HttpState::new(
            Arc::new(UnusedChatService),
            session_authenticator,
            admin_user_reader,
        ))
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
}

#[tokio::test]
async fn non_admin_and_invalid_requests_do_not_reach_reader() {
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
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
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
                .method("POST")
                .uri("/api/admin/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(reader.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(reader.get_calls.load(Ordering::Relaxed), 0);
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
