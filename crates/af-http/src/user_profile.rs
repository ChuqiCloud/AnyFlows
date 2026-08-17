use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserNotificationPreferences,
    UserNotificationPreferencesCommand, UserPasswordChangeCommand, UserProfile, UserProfileError,
    UserProfileService, UserProfileUpdateCommand, UserTwoFactorDisableCommand,
    UserTwoFactorEnableCommand,
};
use af_domain::Quota;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

/// 个人资料路由独立持有的应用服务状态。
#[derive(Clone)]
pub(crate) struct UserProfileHttpState {
    service: Arc<dyn UserProfileService>,
}

impl UserProfileHttpState {
    /// 绑定启动期装配的用户资料应用服务。
    pub(crate) fn new(service: Arc<dyn UserProfileService>) -> Self {
        Self { service }
    }
}

/// 构建只要求有效登录会话的个人资料与安全设置路由。
pub(crate) fn build_user_profile_router(
    service: Arc<dyn UserProfileService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/account/profile", get(get_profile).put(update_profile))
        .route("/api/account/password", put(change_password))
        .route(
            "/api/account/two-factor",
            get(get_two_factor)
                .post(enable_two_factor)
                .delete(disable_two_factor),
        )
        .route("/api/account/notifications", put(update_notifications))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .layer(authentication)
        .with_state(UserProfileHttpState::new(service))
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserProfileResponse)]
pub(crate) struct UserProfileResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(max_length = 320, format = Email, required = true)]
    email: Option<String>,
    #[schema(value_type = crate::openapi::schema::AdminSessionRoleSchema, inline)]
    role: af_admin::SessionRole,
    notifications: UserNotificationPreferencesResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotificationPreferencesResponse)]
pub(crate) struct UserNotificationPreferencesResponse {
    email_product_updates: bool,
    email_usage_alerts: bool,
    balance_alert_enabled: bool,
    #[schema(minimum = 1, required = true)]
    balance_alert_threshold: Option<i64>,
    #[schema(minimum = 1)]
    effective_balance_alert_threshold: i64,
    subscription_alert_enabled: bool,
    #[schema(minimum = 1, maximum = 99)]
    subscription_remaining_percent: i16,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserProfileUpdateRequest)]
pub(crate) struct UserProfileUpdateRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasswordChangeRequest)]
pub(crate) struct UserPasswordChangeRequest {
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    current_password: String,
    #[schema(
        min_length = 12,
        max_length = 128,
        format = Password,
        write_only
    )]
    new_password: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTwoFactorStatusResponse)]
pub(crate) struct UserTwoFactorStatusResponse {
    enabled: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTwoFactorPasswordRequest)]
pub(crate) struct UserTwoFactorPasswordRequest {
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    current_password: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTwoFactorEnrollmentResponse)]
pub(crate) struct UserTwoFactorEnrollmentResponse {
    enabled: bool,
    #[schema(min_length = 32, max_length = 64)]
    secret: String,
    #[schema(format = Uri)]
    otpauth_uri: String,
    #[schema(min_items = 1, max_items = 10)]
    backup_codes: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserNotificationPreferencesRequest)]
pub(crate) struct UserNotificationPreferencesRequest {
    email_product_updates: bool,
    email_usage_alerts: bool,
    #[schema(minimum = 1, required = true)]
    balance_alert_threshold: Option<i64>,
}

/// 返回当前会话用户资料；不会读取或返回其他用户记录。
pub(crate) async fn get_profile(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let profile = state
        .service
        .get(authentication.principal())
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserProfileResponse::from_profile(&profile)))
}

/// 更新当前会话用户登录用户名；邮箱变更保留给后续验证切片。
pub(crate) async fn update_profile(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserProfileUpdateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserProfileUpdateCommand::new(request.username).map_err(map_profile_error)?;
    let profile = state
        .service
        .update_profile(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserProfileResponse::from_profile(&profile)))
}

/// 校验当前密码并更新新密码；成功后客户端必须重新登录。
pub(crate) async fn change_password(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserPasswordChangeRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserPasswordChangeCommand::new(request.current_password, request.new_password)
        .map_err(map_profile_error)?;
    state
        .service
        .change_password(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

/// 返回当前账户是否启用了 TOTP，不读取 secret 或备份码。
pub(crate) async fn get_two_factor(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let status = state
        .service
        .get_two_factor(authentication.principal())
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserTwoFactorStatusResponse {
        enabled: status.enabled(),
    }))
}

/// 校验当前密码并启用 TOTP；secret 和备份码只在本次响应返回。
pub(crate) async fn enable_two_factor(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserTwoFactorPasswordRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        UserTwoFactorEnableCommand::new(request.current_password).map_err(map_profile_error)?;
    let enrollment = state
        .service
        .enable_two_factor(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserTwoFactorEnrollmentResponse {
        enabled: true,
        secret: enrollment.secret().to_owned(),
        otpauth_uri: enrollment.otpauth_uri().to_owned(),
        backup_codes: enrollment.backup_codes().to_vec(),
    }))
}

/// 校验当前密码并停用 TOTP，同时让所有既有会话失效。
pub(crate) async fn disable_two_factor(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserTwoFactorPasswordRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        UserTwoFactorDisableCommand::new(request.current_password).map_err(map_profile_error)?;
    state
        .service
        .disable_two_factor(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

/// 更新当前会话用户的邮件通知偏好。
pub(crate) async fn update_notifications(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserNotificationPreferencesRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let balance_alert_threshold = request
        .balance_alert_threshold
        .map(Quota::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserNotificationPreferencesCommand::new(
        request.email_product_updates,
        request.email_usage_alerts,
        balance_alert_threshold,
    )
    .map_err(map_profile_error)?;
    let profile = state
        .service
        .update_notifications(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserProfileResponse::from_profile(&profile)))
}

impl UserProfileResponse {
    fn from_profile(profile: &UserProfile) -> Self {
        Self {
            id: profile.user_id().get(),
            username: profile.username().to_owned(),
            email: profile.email().map(str::to_owned),
            role: profile.role(),
            notifications: UserNotificationPreferencesResponse::from_preferences(
                profile.notifications(),
            ),
        }
    }
}

impl UserNotificationPreferencesResponse {
    fn from_preferences(preferences: UserNotificationPreferences) -> Self {
        Self {
            email_product_updates: preferences.email_product_updates(),
            email_usage_alerts: preferences.email_usage_alerts(),
            balance_alert_enabled: preferences.balance_alert_enabled(),
            balance_alert_threshold: preferences.balance_alert_threshold().map(Quota::units),
            effective_balance_alert_threshold: preferences
                .effective_balance_alert_threshold()
                .units(),
            subscription_alert_enabled: preferences.subscription_alert_enabled(),
            subscription_remaining_percent: preferences.subscription_remaining_percent(),
        }
    }
}

fn map_profile_error(error: UserProfileError) -> ManagementError {
    match error {
        UserProfileError::InvalidInput => ManagementError::InvalidRequest,
        UserProfileError::InvalidSession => ManagementError::InvalidSession,
        UserProfileError::CurrentPasswordInvalid => ManagementError::PasswordChangeRejected,
        UserProfileError::TwoFactorAlreadyEnabled => ManagementError::TwoFactorAlreadyEnabled,
        UserProfileError::TwoFactorNotEnabled => ManagementError::TwoFactorNotEnabled,
        UserProfileError::Conflict => ManagementError::UserConflict,
        UserProfileError::Internal => ManagementError::Internal,
    }
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
