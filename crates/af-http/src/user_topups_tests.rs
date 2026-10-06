use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, UserTopupConfiguration, UserTopupError, UserTopupMethod, UserTopupOrder,
    UserTopupOrderCreateCommand, UserTopupOrderCreateFuture, UserTopupPaymentSession,
    UserTopupService,
};
use af_domain::{GroupId, Quota, TopupOrderId, TopupOrderStatus, UserId};
use axum::{
    Router,
    body::{Body, to_bytes},
};
use http::{Request, StatusCode, header::CACHE_CONTROL};
use serde_json::Value;
use tower::ServiceExt;

use crate::user_topups::build_user_topup_router;

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";
const IDEMPOTENCY_KEY: &str = "1234567890abcdef1234567890abcdef";

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

struct FakeUserTopupService {
    calls: AtomicUsize,
    configuration: UserTopupConfiguration,
}

impl FakeUserTopupService {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            configuration: UserTopupConfiguration::new(vec![
                UserTopupMethod::stripe("pk_test_public".to_owned()).unwrap(),
                UserTopupMethod::epay("alipay").unwrap(),
            ])
            .unwrap(),
        }
    }
}

impl UserTopupService for FakeUserTopupService {
    fn configuration(&self) -> Result<UserTopupConfiguration, UserTopupError> {
        Ok(self.configuration.clone())
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(
                command.client_request_id().persistence_key(),
                IDEMPOTENCY_KEY
            );
            assert_eq!(command.amount_minor(), 500);
            assert_eq!(command.provider(), "stripe");
            assert_eq!(command.payment_method(), "card");
            let replayed = match principal.user_id().get() {
                1 => true,
                2 => false,
                _ => return Err(UserTopupError::InvalidSession),
            };
            let payment = UserTopupPaymentSession::from_parts(
                "pi_test_123".to_owned(),
                "pi_test_123_secret_client".to_owned(),
            )?;
            Ok(UserTopupOrder::from_parts(
                TopupOrderId::new([0x42; 16]).unwrap(),
                "stripe".to_owned(),
                "card".to_owned(),
                TopupOrderStatus::Pending,
                500,
                "USD".to_owned(),
                Quota::new(2_500_000).unwrap(),
                1,
                1_900_000_000,
                replayed,
                Some(payment),
            ))
        })
    }
}

#[tokio::test]
async fn configuration_requires_a_session_and_exposes_only_client_material() {
    let app = router(Some(Arc::new(FakeUserTopupService::new())));
    let unauthorized = app
        .clone()
        .oneshot(configuration_request("invalid"))
        .await
        .unwrap();
    assert_management_error(unauthorized, StatusCode::UNAUTHORIZED, "invalid_session").await;

    let response = app
        .oneshot(configuration_request(USER_TOKEN))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    let methods = body["methods"].as_array().unwrap();
    assert_eq!(methods.len(), 2);
    let epay = methods
        .iter()
        .find(|method| method["provider"] == "epay" && method["payment_method"] == "alipay")
        .expect("充值配置必须包含易支付支付宝方式");
    assert_eq!(epay["currency"], "CNY");
    let stripe = methods
        .iter()
        .find(|method| method["provider"] == "stripe" && method["payment_method"] == "card")
        .expect("充值配置必须包含 Stripe 银行卡方式");
    assert_eq!(stripe["publishable_key"], "pk_test_public");
    assert!(body.get("secret_key").is_none());
    assert!(body.get("webhook_secret").is_none());
}

#[tokio::test]
async fn create_uses_only_the_authenticated_user_and_preserves_replay_status() {
    let service = Arc::new(FakeUserTopupService::new());
    let app = router(Some(Arc::clone(&service)));

    let created = app
        .clone()
        .oneshot(topup_request(USER_TOKEN, valid_body()))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[CACHE_CONTROL], "no-store");
    let created = response_json(created).await;
    assert_eq!(created["provider"], "stripe");
    assert_eq!(created["payment_method"], "card");
    assert_eq!(created["status"], "pending");
    assert_eq!(created["amount_minor"], 500);
    assert_eq!(created["currency"], "USD");
    assert_eq!(created["quota_amount"], 2_500_000);
    assert_eq!(created["replayed"], false);
    assert_eq!(created["payment"]["payment_intent_id"], "pi_test_123");
    assert_eq!(created["payment"]["kind"], "stripe");
    assert_eq!(
        created["payment"]["client_secret"],
        "pi_test_123_secret_client"
    );
    assert!(created.get("user_id").is_none());
    assert!(created.get("idempotency_key").is_none());

    let replay = app
        .oneshot(topup_request(ADMIN_TOKEN, valid_body()))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(response_json(replay).await["replayed"], true);
    assert_eq!(service.calls.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn invalid_session_and_request_fail_before_the_topup_service() {
    let service = Arc::new(FakeUserTopupService::new());
    let app = router(Some(Arc::clone(&service)));

    let unauthorized = app
        .clone()
        .oneshot(topup_request("invalid", valid_body()))
        .await
        .unwrap();
    assert_management_error(unauthorized, StatusCode::UNAUTHORIZED, "invalid_session").await;

    for body in [
        r#"{"idempotency_key":"invalid","provider":"stripe","payment_method":"card","amount_minor":500}"#.to_owned(),
        format!(r#"{{"idempotency_key":"{IDEMPOTENCY_KEY}","provider":"Stripe","payment_method":"card","amount_minor":500}}"#),
        format!(r#"{{"idempotency_key":"{IDEMPOTENCY_KEY}","provider":"stripe","payment_method":"card","amount_minor":500,"user_id":9}}"#),
    ] {
        let response = app
            .clone()
            .oneshot(topup_request(USER_TOKEN, &body))
            .await
            .unwrap();
        assert_management_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(service.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn unconfigured_payment_returns_empty_configuration_but_rejects_orders() {
    let app = router(None);
    let configuration = app
        .clone()
        .oneshot(configuration_request(USER_TOKEN))
        .await
        .unwrap();
    assert_eq!(configuration.status(), StatusCode::OK);
    assert_eq!(configuration.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
        response_json(configuration).await["methods"],
        serde_json::json!([])
    );

    let response = app
        .oneshot(topup_request(USER_TOKEN, valid_body()))
        .await
        .unwrap();
    assert_management_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "topup_unavailable",
    )
    .await;
}

fn configuration_request(token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/account/wallet/topups/config")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn router(service: Option<Arc<FakeUserTopupService>>) -> Router {
    let service = service.map(|service| -> Arc<dyn UserTopupService> { service });
    build_user_topup_router(service, Arc::new(FakeSessionAuthenticator))
}

fn valid_body() -> &'static str {
    r#"{"idempotency_key":"1234567890abcdef1234567890abcdef","provider":"stripe","payment_method":"card","amount_minor":500}"#
}

fn topup_request(token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/account/wallet/topups")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
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
