use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserTopupConfiguration, UserTopupError,
    UserTopupOrder, UserTopupOrderCreateCommand, UserTopupPaymentSession, UserTopupService,
};
use af_domain::TopupOrderStatus;
use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
};

/// 当前用户充值订单路由状态；未配置 Stripe 时保留显式不可用语义。
#[derive(Clone)]
struct UserTopupHttpState {
    service: Option<Arc<dyn UserTopupService>>,
}

/// 当前用户创建本地充值订单的结构化请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTopupOrderCreateRequest)]
pub(crate) struct UserTopupOrderCreateRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    idempotency_key: String,
    #[schema(minimum = 50, maximum = 99_999_999)]
    amount_minor: i64,
}

/// 当前登录用户可读取的 Stripe.js 充值配置。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTopupConfiguration)]
pub(crate) struct UserTopupConfigurationResponse {
    #[schema(value_type = String, example = "stripe")]
    provider: &'static str,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: &'static str,
    #[schema(minimum = 50)]
    min_amount_minor: i64,
    #[schema(maximum = 99_999_999)]
    max_amount_minor: i64,
    #[schema(min_length = 9, max_length = 512, pattern = "^pk_(test|live)_[!-~]+$")]
    publishable_key: String,
}

/// 当前用户可见的闭合充值订单状态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = UserTopupOrderStatus, rename_all = "snake_case")]
pub(crate) enum UserTopupOrderStatusResponse {
    Created,
    Pending,
    Paid,
    Failed,
    Canceled,
    Expired,
}

/// 当前用户可见的充值订单快照，不包含支付侧订单号或流水号。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTopupOrder)]
pub(crate) struct UserTopupOrderResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    order_id: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    status: UserTopupOrderStatusResponse,
    #[schema(minimum = 50, maximum = 99_999_999)]
    amount_minor: i64,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1)]
    created_at: i64,
    replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment: Option<UserTopupPaymentSessionResponse>,
}

/// 当前用户可交给 Stripe SDK 的短生命周期支付会话。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTopupPaymentSession)]
pub(crate) struct UserTopupPaymentSessionResponse {
    #[schema(min_length = 4, max_length = 128, pattern = "^pi_[A-Za-z0-9_]+$")]
    payment_intent_id: String,
    #[schema(min_length = 1, max_length = 4096, read_only = true)]
    client_secret: String,
}

/// 构建只允许有效登录会话创建本人充值订单的路由。
pub(crate) fn build_user_topup_router(
    service: Option<Arc<dyn UserTopupService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/account/wallet/topups/config",
            get(get_user_topup_configuration),
        )
        .route("/api/account/wallet/topups", post(create_user_topup_order))
        .route_layer(authentication)
        .with_state(UserTopupHttpState { service })
}

/// 返回只包含 Stripe.js 客户端材料和固定金额边界的充值配置。
async fn get_user_topup_configuration(
    State(state): State<UserTopupHttpState>,
) -> Result<Response, ManagementError> {
    let service = state
        .service
        .as_ref()
        .ok_or(ManagementError::TopupUnavailable)?;
    Ok(status_json(
        StatusCode::OK,
        UserTopupConfigurationResponse::from_configuration(service.configuration()),
    ))
}

/// 为当前会话用户幂等创建本地 Stripe 充值订单。
async fn create_user_topup_order(
    State(state): State<UserTopupHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserTopupOrderCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state
        .service
        .as_ref()
        .ok_or(ManagementError::TopupUnavailable)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserTopupOrderCreateCommand::new(&request.idempotency_key, request.amount_minor)
        .map_err(map_topup_error)?;
    let order = service
        .create(authentication.principal(), command)
        .await
        .map_err(map_topup_error)?;
    let status = if order.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(status_json(
        status,
        UserTopupOrderResponse::from_order(&order)?,
    ))
}

impl UserTopupOrderResponse {
    fn from_order(order: &UserTopupOrder) -> Result<Self, ManagementError> {
        Ok(Self {
            order_id: order.order_id().persistence_key(),
            provider: order.provider().to_owned(),
            status: match order.status() {
                TopupOrderStatus::Created => UserTopupOrderStatusResponse::Created,
                TopupOrderStatus::Pending => UserTopupOrderStatusResponse::Pending,
                TopupOrderStatus::Paid => UserTopupOrderStatusResponse::Paid,
                TopupOrderStatus::Failed => UserTopupOrderStatusResponse::Failed,
                TopupOrderStatus::Canceled => UserTopupOrderStatusResponse::Canceled,
                TopupOrderStatus::Expired => UserTopupOrderStatusResponse::Expired,
            },
            amount_minor: i64::try_from(order.amount_minor())
                .map_err(|_| ManagementError::Internal)?,
            currency: order.currency().to_owned(),
            quota_amount: order.quota_amount().units(),
            version: i64::try_from(order.version()).map_err(|_| ManagementError::Internal)?,
            created_at: i64::try_from(order.created_at()).map_err(|_| ManagementError::Internal)?,
            replayed: order.replayed(),
            payment: order
                .payment()
                .map(UserTopupPaymentSessionResponse::from_session)
                .transpose()?,
        })
    }
}

impl UserTopupConfigurationResponse {
    fn from_configuration(configuration: &UserTopupConfiguration) -> Self {
        Self {
            provider: configuration.provider(),
            currency: configuration.currency(),
            min_amount_minor: configuration.min_amount_minor(),
            max_amount_minor: configuration.max_amount_minor(),
            publishable_key: configuration.publishable_key().to_owned(),
        }
    }
}

impl UserTopupPaymentSessionResponse {
    fn from_session(session: &UserTopupPaymentSession) -> Result<Self, ManagementError> {
        Ok(Self {
            payment_intent_id: session.payment_intent_id().to_owned(),
            client_secret: session.client_secret().to_owned(),
        })
    }
}

fn map_topup_error(error: UserTopupError) -> ManagementError {
    match error {
        UserTopupError::InvalidInput => ManagementError::InvalidRequest,
        UserTopupError::InvalidSession => ManagementError::InvalidSession,
        UserTopupError::Conflict => ManagementError::TopupOrderConflict,
        UserTopupError::OutcomeUnknown => ManagementError::TopupOrderOutcomeUnknown,
        UserTopupError::Unavailable => ManagementError::TopupUnavailable,
        UserTopupError::ProviderRejected => ManagementError::TopupProviderRejected,
        UserTopupError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
