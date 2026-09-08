use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
    AdminWalletAdjustmentCommand, AdminWalletAdjustmentFuture, AdminWalletAdjustmentResult,
    AdminWalletEntry, AdminWalletEntryType, AdminWalletError, AdminWalletListFuture,
    AdminWalletListQuery, AdminWalletPage, AdminWalletService, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{AfError, GatewayPrincipal, GroupId, TokenId, UserId, WalletEventId};
use af_protocol::CanonicalRequestEnvelope;
use axum::{
    Router,
    body::{Body, to_bytes},
    middleware,
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
    management_authorization::authorize_management_admin,
    management_wallet::{adjust_admin_wallet, list_admin_wallet_entries},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";
const EVENT_ID: &str = "abababababababababababababababab";
const TOPUP_EVENT_ID: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
const REDEMPTION_EVENT_ID: &str = "efefefefefefefefefefefefefefefef";

struct UnusedChatService;
struct UnusedAdminServices;

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

impl AdminGroupReader for UnusedAdminServices {
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

impl AdminGroupWriter for UnusedAdminServices {
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

impl AdminTokenReader for UnusedAdminServices {
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

impl AdminTokenWriter for UnusedAdminServices {
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

impl AdminUserReader for UnusedAdminServices {
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

impl AdminUserWriter for UnusedAdminServices {
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

struct FakeAdminWalletService {
    list_calls: AtomicUsize,
    adjust_calls: AtomicUsize,
}

impl FakeAdminWalletService {
    fn new() -> Self {
        Self {
            list_calls: AtomicUsize::new(0),
            adjust_calls: AtomicUsize::new(0),
        }
    }
}

impl AdminWalletService for FakeAdminWalletService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        _query: AdminWalletListQuery,
    ) -> AdminWalletListFuture<'a> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminWalletError::Forbidden);
            }
            match user_id.get() {
                404 => Err(AdminWalletError::NotFound),
                500 => Err(AdminWalletError::Internal),
                _ => Ok(AdminWalletPage::from_parts(
                    vec![
                        sample_topup_entry(),
                        sample_redemption_entry(),
                        sample_adjustment_entry(),
                    ],
                    Some(40),
                )),
            }
        })
    }

    fn adjust<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        _command: AdminWalletAdjustmentCommand,
    ) -> AdminWalletAdjustmentFuture<'a> {
        let call = self.adjust_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.role() != SessionRole::Admin {
                return Err(AdminWalletError::Forbidden);
            }
            match user_id.get() {
                404 => Err(AdminWalletError::NotFound),
                409 => Err(AdminWalletError::Conflict),
                410 => Err(AdminWalletError::InsufficientQuota),
                411 => Err(AdminWalletError::Overflow),
                500 => Err(AdminWalletError::Internal),
                503 => Err(AdminWalletError::OutcomeUnknown),
                _ if call == 0 => Ok(AdminWalletAdjustmentResult::Applied(
                    sample_adjustment_entry(),
                )),
                _ => Ok(AdminWalletAdjustmentResult::Existing(
                    sample_adjustment_entry(),
                )),
            }
        })
    }
}

#[tokio::test]
async fn admin_reads_wallet_ledger_with_stable_cursor_and_no_store() {
    let service = Arc::new(FakeAdminWalletService::new());
    let app = router(Arc::clone(&service));

    let response = app
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/10/wallet/entries?before=99&limit=25",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    assert_eq!(body["next_cursor"], 40);
    assert_eq!(body["entries"][0]["event_id"], TOPUP_EVENT_ID);
    assert_eq!(body["entries"][0]["entry_type"], "topup");
    assert_eq!(body["entries"][0]["actor_user_id"], Value::Null);
    assert_eq!(body["entries"][0]["reason"], Value::Null);
    assert_eq!(body["entries"][0]["quota_delta"], 50);
    assert_eq!(body["entries"][0]["balance_after"], 175);
    assert_eq!(body["entries"][1]["entry_type"], "redemption");
    assert_eq!(body["entries"][1]["actor_user_id"], Value::Null);
    assert_eq!(body["entries"][2]["entry_type"], "admin_adjustment");
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn new_adjustment_returns_created_and_idempotent_replay_returns_ok() {
    let service = Arc::new(FakeAdminWalletService::new());
    let app = router(Arc::clone(&service));
    let request_body = valid_adjustment_body();

    let created = app
        .clone()
        .oneshot(authorized_json_request(
            "/api/admin/users/10/wallet/adjustments",
            ADMIN_TOKEN,
            request_body.clone(),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[CACHE_CONTROL], "no-store");
    let created_body = response_json(created).await;

    let replayed = app
        .oneshot(authorized_json_request(
            "/api/admin/users/10/wallet/adjustments",
            ADMIN_TOKEN,
            request_body,
        ))
        .await
        .unwrap();
    assert_eq!(replayed.status(), StatusCode::OK);
    assert_eq!(replayed.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response_json(replayed).await, created_body);
    assert_eq!(service.adjust_calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn non_admin_and_invalid_wallet_requests_never_reach_service() {
    let service = Arc::new(FakeAdminWalletService::new());
    let app = router(Arc::clone(&service));

    let forbidden = app
        .clone()
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/10/wallet/entries",
            USER_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;

    for uri in [
        "/api/admin/users/abc/wallet/entries",
        "/api/admin/users/10/wallet/entries?limit=0",
        "/api/admin/users/10/wallet/entries?before=1&before=2",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request("GET", uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }

    for body in [
        json!({"event_id":"00000000000000000000000000000000","quota_delta":1,"reason":"调账"}),
        json!({"event_id":"00000000000000010000000000000001","quota_delta":1,"reason":"调账"}),
        json!({"event_id":EVENT_ID,"quota_delta":0,"reason":"调账"}),
        json!({"event_id":EVENT_ID,"quota_delta":1,"reason":" 调账 "}),
        json!({"event_id":EVENT_ID,"quota_delta":1,"reason":"调账","extra":true}),
    ] {
        let response = app
            .clone()
            .oneshot(authorized_json_request(
                "/api/admin/users/10/wallet/adjustments",
                ADMIN_TOKEN,
                body,
            ))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 0);
    assert_eq!(service.adjust_calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn wallet_service_failures_map_to_stable_status_and_codes() {
    let app = router(Arc::new(FakeAdminWalletService::new()));
    for (user_id, status, code) in [
        (404, StatusCode::NOT_FOUND, "user_not_found"),
        (409, StatusCode::CONFLICT, "wallet_event_conflict"),
        (410, StatusCode::CONFLICT, "wallet_insufficient_quota"),
        (411, StatusCode::CONFLICT, "wallet_overflow"),
        (
            503,
            StatusCode::SERVICE_UNAVAILABLE,
            "wallet_outcome_unknown",
        ),
        (500, StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    ] {
        let response = app
            .clone()
            .oneshot(authorized_json_request(
                &format!("/api/admin/users/{user_id}/wallet/adjustments"),
                ADMIN_TOKEN,
                valid_adjustment_body(),
            ))
            .await
            .unwrap();
        assert_management_error(response, status, code).await;
    }

    let missing = app
        .clone()
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/404/wallet/entries",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_management_error(missing, StatusCode::NOT_FOUND, "user_not_found").await;

    let internal = app
        .oneshot(authorized_request(
            "GET",
            "/api/admin/users/500/wallet/entries",
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

fn router(service: Arc<FakeAdminWalletService>) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let unused = Arc::new(UnusedAdminServices);
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let wallet_service: Arc<dyn AdminWalletService> = service;
    Router::new()
        .route(
            "/api/admin/users/{id}/wallet/entries",
            get(list_admin_wallet_entries)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication.clone()),
        )
        .route(
            "/api/admin/users/{id}/wallet/adjustments",
            post(adjust_admin_wallet)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication),
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
            .with_admin_wallet_service(Some(wallet_service)),
        )
}

fn sample_adjustment_entry() -> AdminWalletEntry {
    AdminWalletEntry::from_parts(
        41,
        WalletEventId::from_persistence_key(EVENT_ID).unwrap(),
        UserId::new(10).unwrap(),
        Some(UserId::new(1).unwrap()),
        AdminWalletEntryType::AdminAdjustment,
        25,
        100,
        125,
        Some("人工调账".to_owned()),
        1_722_470_400,
    )
}

fn sample_topup_entry() -> AdminWalletEntry {
    AdminWalletEntry::from_parts(
        42,
        WalletEventId::from_persistence_key(TOPUP_EVENT_ID).unwrap(),
        UserId::new(10).unwrap(),
        None,
        AdminWalletEntryType::Topup,
        50,
        125,
        175,
        None,
        1_722_470_500,
    )
}

fn sample_redemption_entry() -> AdminWalletEntry {
    AdminWalletEntry::from_parts(
        43,
        WalletEventId::from_persistence_key(REDEMPTION_EVENT_ID).unwrap(),
        UserId::new(10).unwrap(),
        None,
        AdminWalletEntryType::Redemption,
        30,
        175,
        205,
        None,
        1_722_470_600,
    )
}

fn valid_adjustment_body() -> Value {
    json!({"event_id":EVENT_ID,"quota_delta":25,"reason":"人工调账"})
}

fn authorized_request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn authorized_json_request(uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
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
