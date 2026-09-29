use std::sync::Arc;

use af_admin::{
    AdminPaymentSettings, AdminPaymentSettingsCommand, AdminPaymentSettingsError,
    AdminPaymentSettingsService, SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct PaymentSettingsHttpState {
    service: Arc<dyn AdminPaymentSettingsService>,
}

/// 管理员读取的在线支付设置，所有服务端密钥保持脱敏。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminPaymentSettings)]
pub(crate) struct AdminPaymentSettingsResponse {
    stripe_enabled: bool,
    #[schema(max_length = 512)]
    stripe_publishable_key: Option<String>,
    stripe_secret_key_configured: bool,
    stripe_webhook_secret_configured: bool,
    #[schema(minimum = 30, maximum = 900)]
    stripe_signature_tolerance_seconds: u16,
    epay_enabled: bool,
    #[schema(max_length = 2048)]
    epay_gateway_url: Option<String>,
    #[schema(max_length = 128)]
    epay_merchant_id: Option<String>,
    epay_merchant_key_configured: bool,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_qr_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
    #[schema(minimum = 1)]
    epay_quota_per_cny: i64,
    #[schema(minimum = 1)]
    version: i64,
}

impl From<AdminPaymentSettings> for AdminPaymentSettingsResponse {
    fn from(settings: AdminPaymentSettings) -> Self {
        Self {
            stripe_enabled: settings.stripe_enabled(),
            stripe_publishable_key: settings.stripe_publishable_key().map(str::to_owned),
            stripe_secret_key_configured: settings.stripe_secret_key_configured(),
            stripe_webhook_secret_configured: settings.stripe_webhook_secret_configured(),
            stripe_signature_tolerance_seconds: settings.stripe_signature_tolerance_seconds(),
            epay_enabled: settings.epay_enabled(),
            epay_gateway_url: settings.epay_gateway_url().map(str::to_owned),
            epay_merchant_id: settings.epay_merchant_id().map(str::to_owned),
            epay_merchant_key_configured: settings.epay_merchant_key_configured(),
            epay_alipay_enabled: settings.epay_alipay_enabled(),
            epay_wxpay_enabled: settings.epay_wxpay_enabled(),
            epay_qr_enabled: settings.epay_qr_enabled(),
            epay_refund_enabled: settings.epay_refund_enabled(),
            refund_auto_submit_enabled: settings.refund_auto_submit_enabled(),
            epay_quota_per_cny: settings.epay_quota_per_cny(),
            version: settings.version(),
        }
    }
}

/// 管理员完整保存在线支付设置；空密钥保留旧值，清除操作必须显式声明。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminPaymentSettingsRequest)]
pub(crate) struct AdminPaymentSettingsRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    stripe_enabled: bool,
    #[schema(max_length = 512)]
    stripe_publishable_key: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    stripe_secret_key: Option<String>,
    clear_stripe_secret_key: bool,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    stripe_webhook_secret: Option<String>,
    clear_stripe_webhook_secret: bool,
    #[schema(minimum = 30, maximum = 900)]
    stripe_signature_tolerance_seconds: u16,
    epay_enabled: bool,
    #[schema(max_length = 2048)]
    epay_gateway_url: Option<String>,
    #[schema(max_length = 128)]
    epay_merchant_id: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    epay_merchant_key: Option<String>,
    clear_epay_merchant_key: bool,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_qr_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
    #[schema(minimum = 1)]
    epay_quota_per_cny: i64,
}

/// 构建仅管理员可访问的在线支付设置路由。
pub(crate) fn build_payment_settings_router(
    service: Arc<dyn AdminPaymentSettingsService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/payment-settings",
            get(get_payment_settings).put(update_payment_settings),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(PaymentSettingsHttpState { service })
}

async fn get_payment_settings(
    State(state): State<PaymentSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .settings(authentication.principal())
        .await
        .map_err(map_payment_settings_error)?;
    Ok(no_store_json(AdminPaymentSettingsResponse::from(settings)))
}

async fn update_payment_settings(
    State(state): State<PaymentSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminPaymentSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminPaymentSettingsCommand::new(
        request.expected_version,
        request.stripe_enabled,
        request.stripe_publishable_key,
        request.stripe_secret_key,
        request.clear_stripe_secret_key,
        request.stripe_webhook_secret,
        request.clear_stripe_webhook_secret,
        request.stripe_signature_tolerance_seconds,
        request.epay_enabled,
        request.epay_gateway_url,
        request.epay_merchant_id,
        request.epay_merchant_key,
        request.clear_epay_merchant_key,
        request.epay_alipay_enabled,
        request.epay_wxpay_enabled,
        request.epay_qr_enabled,
        request.epay_refund_enabled,
        request.refund_auto_submit_enabled,
        request.epay_quota_per_cny,
    )
    .map_err(map_payment_settings_error)?;
    let settings = state
        .service
        .update(authentication.principal(), command)
        .await
        .map_err(map_payment_settings_error)?;
    Ok(no_store_json(AdminPaymentSettingsResponse::from(settings)))
}

fn map_payment_settings_error(error: AdminPaymentSettingsError) -> ManagementError {
    match error {
        AdminPaymentSettingsError::InvalidInput => ManagementError::InvalidRequest,
        AdminPaymentSettingsError::Forbidden => ManagementError::Forbidden,
        AdminPaymentSettingsError::Internal => ManagementError::Internal,
    }
}
