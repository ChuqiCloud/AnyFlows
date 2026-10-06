use std::sync::Arc;

use af_admin::{
    PasskeyRegistrationCommand, PasskeyRenameCommand, PasskeyRevokeCommand, SessionAuthentication,
    SessionAuthenticator, UserEmailBindingConfirmCommand, UserEmailBindingStartCommand,
    UserNotificationPreferences, UserNotificationPreferencesCommand, UserPasskey,
    UserPasswordChangeCommand, UserProfile, UserProfileError, UserProfileService,
    UserProfileUpdateCommand, UserTwoFactorDisableCommand, UserTwoFactorEnableCommand,
};
use af_domain::Quota;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
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
        .route(
            "/api/account/profile/email-verification",
            post(send_email_binding_verification),
        )
        .route("/api/account/profile/email", put(confirm_email_binding))
        .route("/api/account/password", put(change_password))
        .route(
            "/api/account/two-factor",
            get(get_two_factor)
                .post(enable_two_factor)
                .delete(disable_two_factor),
        )
        .route(
            "/api/account/passkeys/registration/options",
            post(start_passkey_registration),
        )
        .route(
            "/api/account/passkeys/registration/verify",
            post(finish_passkey_registration),
        )
        .route("/api/account/passkeys", get(list_passkeys))
        .route(
            "/api/account/passkeys/{id}",
            patch(rename_passkey).delete(revoke_passkey),
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
#[schema(as = UserEmailBindingVerificationRequest)]
pub(crate) struct UserEmailBindingVerificationRequest {
    #[schema(min_length = 3, max_length = 320, format = Email)]
    email: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserEmailBindingConfirmRequest)]
pub(crate) struct UserEmailBindingConfirmRequest {
    #[schema(min_length = 3, max_length = 320, format = Email)]
    email: String,
    #[schema(min_length = 6, max_length = 6, pattern = "^[0-9]{6}$", write_only)]
    verification_code: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserEmailBindingVerificationResponse)]
pub(crate) struct UserEmailBindingVerificationResponse {
    expires_at: u64,
    next_send_at: u64,
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

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyResponse)]
pub(crate) struct UserPasskeyResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    created_at: i64,
    #[schema(required = true)]
    last_used_at: Option<i64>,
    #[schema(required = true)]
    revoked_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyListResponse)]
pub(crate) struct UserPasskeyListResponse {
    items: Vec<UserPasskeyResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyRegistrationOptionsResponse)]
pub(crate) struct UserPasskeyRegistrationOptionsResponse {
    options: serde_json::Value,
    #[schema(min_length = 64, max_length = 64)]
    challenge_digest: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyRegistrationVerifyRequest)]
pub(crate) struct UserPasskeyRegistrationVerifyRequest {
    credential: serde_json::Value,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyRenameRequest)]
pub(crate) struct UserPasskeyRenameRequest {
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserPasskeyRevokeRequest)]
pub(crate) struct UserPasskeyRevokeRequest {
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    current_password: String,
    #[schema(max_length = 128, write_only)]
    totp_code: Option<String>,
}

/// 返回当前用户的脱敏 Passkey 目录。
pub(crate) async fn list_passkeys(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let items = state
        .service
        .list_passkeys(authentication.principal())
        .await
        .map_err(map_profile_error)?
        .into_iter()
        .map(|passkey| UserPasskeyResponse::from_passkey(&passkey))
        .collect();
    Ok(no_store_json(UserPasskeyListResponse { items }))
}

/// 创建浏览器注册 Passkey 所需的 WebAuthn 选项。
pub(crate) async fn start_passkey_registration(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let options = state
        .service
        .start_passkey_registration(authentication.principal())
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserPasskeyRegistrationOptionsResponse {
        options: options.options().clone(),
        challenge_digest: options.challenge_digest().to_owned(),
    }))
}

/// 验证浏览器注册响应并保存新的 Passkey。
pub(crate) async fn finish_passkey_registration(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserPasskeyRegistrationVerifyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = PasskeyRegistrationCommand::new(request.credential, request.display_name)
        .map_err(map_profile_error)?;
    let passkey = state
        .service
        .finish_passkey_registration(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserPasskeyResponse::from_passkey(&passkey)))
}

/// 修改当前用户 Passkey 的展示名称。
pub(crate) async fn rename_passkey(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    request: Result<Json<UserPasskeyRenameRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = PasskeyRenameCommand::new(request.display_name).map_err(map_profile_error)?;
    let passkey = state
        .service
        .rename_passkey(authentication.principal(), id, command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserPasskeyResponse::from_passkey(&passkey)))
}

/// 通过密码和必要的二次验证撤销当前用户 Passkey。
pub(crate) async fn revoke_passkey(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    request: Result<Json<UserPasskeyRevokeRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = PasskeyRevokeCommand::new(request.current_password, request.totp_code)
        .map_err(map_profile_error)?;
    state
        .service
        .revoke_passkey(authentication.principal(), id, command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
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

pub(crate) async fn send_email_binding_verification(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserEmailBindingVerificationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserEmailBindingStartCommand::new(request.email).map_err(map_profile_error)?;
    let issued = state
        .service
        .send_email_binding_verification(authentication.principal(), command)
        .await
        .map_err(map_profile_error)?;
    Ok(no_store_json(UserEmailBindingVerificationResponse {
        expires_at: issued.expires_at(),
        next_send_at: issued.next_send_at(),
    }))
}

pub(crate) async fn confirm_email_binding(
    State(state): State<UserProfileHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<UserEmailBindingConfirmRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserEmailBindingConfirmCommand::new(request.email, request.verification_code)
        .map_err(map_profile_error)?;
    let profile = state
        .service
        .confirm_email_binding(authentication.principal(), command)
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

impl UserPasskeyResponse {
    fn from_passkey(passkey: &UserPasskey) -> Self {
        Self {
            id: passkey.id(),
            display_name: passkey.display_name().to_owned(),
            created_at: passkey.created_at(),
            last_used_at: passkey.last_used_at(),
            revoked_at: passkey.revoked_at(),
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
        UserProfileError::TwoFactorRequired => ManagementError::TwoFactorRequired,
        UserProfileError::TwoFactorInvalid => ManagementError::TwoFactorInvalid,
        UserProfileError::PasskeyRegistrationRejected => ManagementError::InvalidRequest,
        UserProfileError::PasskeyNotFound => ManagementError::InvalidRequest,
        UserProfileError::Conflict => ManagementError::UserConflict,
        UserProfileError::Internal => ManagementError::Internal,
        UserProfileError::EmailVerificationRejected => ManagementError::RegistrationRejected,
        UserProfileError::EmailNotConfigured => ManagementError::EmailNotConfigured,
        UserProfileError::EmailDeliveryFailed => ManagementError::EmailDeliveryFailed,
        UserProfileError::EmailVerificationRateLimited {
            retry_after_seconds,
        } => ManagementError::RegistrationRateLimited {
            retry_after_seconds,
        },
    }
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
