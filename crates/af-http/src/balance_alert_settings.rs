use std::sync::Arc;

use af_admin::{
    AdminBalanceAlertSettings, AdminBalanceAlertSettingsCommand, AdminBalanceAlertSettingsError,
    AdminBalanceAlertSettingsService, SessionAuthentication, SessionAuthenticator,
};
use af_domain::Quota;
use axum::{
    Json, Router,
    extract::{Extension, State, rejection::JsonRejection},
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
struct BalanceAlertSettingsHttpState {
    service: Arc<dyn AdminBalanceAlertSettingsService>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminBalanceAlertSettings)]
pub(crate) struct AdminBalanceAlertSettingsResponse {
    enabled: bool,
    #[schema(minimum = 1)]
    default_threshold: i64,
    #[schema(minimum = 3600, maximum = 604800)]
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    #[schema(minimum = 1, maximum = 99)]
    subscription_remaining_percent: i16,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminBalanceAlertSettingsRequest)]
pub(crate) struct AdminBalanceAlertSettingsRequest {
    enabled: bool,
    #[schema(minimum = 1)]
    default_threshold: i64,
    #[schema(minimum = 3600, maximum = 604800)]
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    #[schema(minimum = 1, maximum = 99)]
    subscription_remaining_percent: i16,
}

/// 构建仅管理员可访问的余额预警设置路由。
pub(crate) fn build_balance_alert_settings_router(
    service: Arc<dyn AdminBalanceAlertSettingsService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/balance-alert-settings",
            get(get_balance_alert_settings).put(update_balance_alert_settings),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(BalanceAlertSettingsHttpState { service })
}

async fn get_balance_alert_settings(
    State(state): State<BalanceAlertSettingsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .settings(authentication.principal())
        .await
        .map_err(map_balance_alert_settings_error)?;
    Ok(no_store_json(AdminBalanceAlertSettingsResponse::from(
        settings,
    )))
}

async fn update_balance_alert_settings(
    State(state): State<BalanceAlertSettingsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminBalanceAlertSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let threshold =
        Quota::new(request.default_threshold).map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminBalanceAlertSettingsCommand::new(
        request.enabled,
        threshold,
        request.reminder_interval_seconds,
        request.subscription_alert_enabled,
        request.subscription_remaining_percent,
    )
    .map_err(map_balance_alert_settings_error)?;
    let settings = state
        .service
        .update(authentication.principal(), command)
        .await
        .map_err(map_balance_alert_settings_error)?;
    Ok(no_store_json(AdminBalanceAlertSettingsResponse::from(
        settings,
    )))
}

impl From<AdminBalanceAlertSettings> for AdminBalanceAlertSettingsResponse {
    fn from(settings: AdminBalanceAlertSettings) -> Self {
        Self {
            enabled: settings.enabled(),
            default_threshold: settings.default_threshold().units(),
            reminder_interval_seconds: settings.reminder_interval_seconds(),
            subscription_alert_enabled: settings.subscription_alert_enabled(),
            subscription_remaining_percent: settings.subscription_remaining_percent(),
            version: settings.version(),
        }
    }
}

fn map_balance_alert_settings_error(error: AdminBalanceAlertSettingsError) -> ManagementError {
    match error {
        AdminBalanceAlertSettingsError::InvalidInput => ManagementError::InvalidRequest,
        AdminBalanceAlertSettingsError::Forbidden => ManagementError::Forbidden,
        AdminBalanceAlertSettingsError::Internal => ManagementError::Internal,
    }
}
