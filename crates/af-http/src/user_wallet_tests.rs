use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, UserWalletEntry, UserWalletEntryType, UserWalletError, UserWalletListFuture,
    UserWalletListQuery, UserWalletPage, UserWalletService, UserWalletSummary,
    UserWalletSummaryFuture,
};
use af_domain::{GroupId, UserId};
use axum::{
    Router,
    body::{Body, to_bytes},
};
use http::{Request, StatusCode, header::CACHE_CONTROL};
use serde_json::Value;
use tower::ServiceExt;

use crate::user_wallet::build_user_wallet_router;

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

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

struct FakeUserWalletService {
    summary_calls: AtomicUsize,
    list_calls: AtomicUsize,
}

impl FakeUserWalletService {
    fn new() -> Self {
        Self {
            summary_calls: AtomicUsize::new(0),
            list_calls: AtomicUsize::new(0),
        }
    }
}

impl UserWalletService for FakeUserWalletService {
    fn summary<'a>(&'a self, principal: SessionPrincipal) -> UserWalletSummaryFuture<'a> {
        self.summary_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            match principal.user_id().get() {
                1 => Ok(UserWalletSummary::from_parts(100, 20, 5)),
                2 => Ok(UserWalletSummary::from_parts(200, 40, 10)),
                404 => Err(UserWalletError::InvalidSession),
                _ => Err(UserWalletError::Internal),
            }
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserWalletListQuery,
    ) -> UserWalletListFuture<'a> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if principal.user_id().get() != 2 {
                return Err(UserWalletError::Internal);
            }
            assert_eq!(query.before(), Some(99));
            assert_eq!(query.limit(), 25);
            Ok(UserWalletPage::from_parts(
                vec![UserWalletEntry::from_parts(
                    41,
                    UserWalletEntryType::AdminAdjustment,
                    25,
                    175,
                    200,
                    Some("活动额度补充".to_owned()),
                    1_722_470_400,
                )],
                Some(40),
            ))
        })
    }
}

#[tokio::test]
async fn summary_uses_only_authenticated_principal_for_admin_and_user() {
    let service = Arc::new(FakeUserWalletService::new());
    let app = router(Arc::clone(&service));

    let user = app
        .clone()
        .oneshot(authorized_request("/api/account/wallet", USER_TOKEN))
        .await
        .unwrap();
    assert_eq!(user.status(), StatusCode::OK);
    assert_eq!(user.headers()[CACHE_CONTROL], "no-store");
    let user_body = response_json(user).await;
    assert_eq!(user_body["balance"], 200);
    assert_eq!(user_body["used_quota"], 40);
    assert_eq!(user_body["frozen_quota"], 10);

    let admin = app
        .oneshot(authorized_request("/api/account/wallet", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(response_json(admin).await["balance"], 100);
    assert_eq!(service.summary_calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn entries_keep_stable_cursor_without_exposing_other_subjects() {
    let service = Arc::new(FakeUserWalletService::new());
    let app = router(Arc::clone(&service));
    let response = app
        .oneshot(authorized_request(
            "/api/account/wallet/entries?before=99&limit=25",
            USER_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    assert_eq!(body["next_cursor"], 40);
    assert_eq!(body["entries"][0]["entry_type"], "admin_adjustment");
    assert_eq!(body["entries"][0]["reason"], "活动额度补充");
    assert_eq!(body["entries"][0]["balance_after"], 200);
    assert!(body["entries"][0].get("user_id").is_none());
    assert!(body["entries"][0].get("actor_user_id").is_none());
    assert!(body["entries"][0].get("event_id").is_none());
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn invalid_session_and_pagination_fail_before_wallet_reads() {
    let service = Arc::new(FakeUserWalletService::new());
    let app = router(Arc::clone(&service));

    let unauthorized = app
        .clone()
        .oneshot(authorized_request("/api/account/wallet", "invalid"))
        .await
        .unwrap();
    assert_management_error(unauthorized, StatusCode::UNAUTHORIZED, "invalid_session").await;

    for uri in [
        "/api/account/wallet/entries?limit=0",
        "/api/account/wallet/entries?before=1&before=2",
        "/api/account/wallet/entries?unexpected=1",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request(uri, USER_TOKEN))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(service.summary_calls.load(Ordering::Relaxed), 0);
    assert_eq!(service.list_calls.load(Ordering::Relaxed), 0);
}

fn router(service: Arc<FakeUserWalletService>) -> Router {
    let wallet_service: Arc<dyn UserWalletService> = service;
    build_user_wallet_router(
        wallet_service,
        Arc::new(FakeSessionAuthenticator) as Arc<dyn SessionAuthenticator>,
    )
}

fn authorized_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn assert_management_error(
    response: axum::response::Response,
    status: StatusCode,
    code: &str,
) {
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
