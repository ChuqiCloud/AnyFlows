use std::sync::Arc;

use af_admin::{
    AdminEmailSettings, AdminEmailSettingsCommand, AdminEmailSettingsError,
    AdminEmailSettingsService, AdminEmailTestCommand, AdminEmailTlsMode as ApplicationEmailTlsMode,
    SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_channels::no_store_empty,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct EmailSettingsHttpState {
    service: Arc<dyn AdminEmailSettingsService>,
}

/// 管理邮件设置 API 使用的闭合 TLS 模式。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminEmailTlsMode, rename_all = "snake_case")]
pub(crate) enum AdminEmailTlsModeDto {
    /// 明文连接建立后必须成功升级为 TLS。
    StartTls,
    /// 从首字节开始使用隐式 TLS。
    Tls,
}

impl From<ApplicationEmailTlsMode> for AdminEmailTlsModeDto {
    fn from(value: ApplicationEmailTlsMode) -> Self {
        match value {
            ApplicationEmailTlsMode::StartTls => Self::StartTls,
            ApplicationEmailTlsMode::Tls => Self::Tls,
        }
    }
}

impl From<AdminEmailTlsModeDto> for ApplicationEmailTlsMode {
    fn from(value: AdminEmailTlsModeDto) -> Self {
        match value {
            AdminEmailTlsModeDto::StartTls => Self::StartTls,
            AdminEmailTlsModeDto::Tls => Self::Tls,
        }
    }
}

/// 管理员读取的完整 SMTP 设置；密码只返回是否已配置。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminEmailSettings)]
pub(crate) struct AdminEmailSettingsResponse {
    enabled: bool,
    #[schema(max_length = 255)]
    host: String,
    #[schema(minimum = 1, maximum = 65535)]
    port: u16,
    tls_mode: AdminEmailTlsModeDto,
    #[schema(max_length = 320)]
    username: Option<String>,
    password_configured: bool,
    #[schema(max_length = 320, format = Email)]
    from_address: String,
    #[schema(max_length = 128)]
    from_name: Option<String>,
    #[schema(max_length = 320, format = Email)]
    reply_to: Option<String>,
    #[schema(minimum = 1, maximum = 60)]
    timeout_seconds: u16,
    #[schema(minimum = 1)]
    version: i64,
    delivery_ready: bool,
}

impl From<AdminEmailSettings> for AdminEmailSettingsResponse {
    fn from(settings: AdminEmailSettings) -> Self {
        Self {
            enabled: settings.enabled(),
            host: settings.host().to_owned(),
            port: settings.port(),
            tls_mode: settings.tls_mode().into(),
            username: settings.username().map(str::to_owned),
            password_configured: settings.password_configured(),
            from_address: settings.from_address().to_owned(),
            from_name: settings.from_name().map(str::to_owned),
            reply_to: settings.reply_to().map(str::to_owned),
            timeout_seconds: settings.timeout_seconds(),
            version: settings.version(),
            delivery_ready: settings.delivery_ready(),
        }
    }
}

/// 管理员完整覆盖 SMTP 设置的结构化请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminEmailSettingsRequest)]
pub(crate) struct AdminEmailSettingsRequest {
    enabled: bool,
    #[schema(max_length = 255)]
    host: String,
    #[schema(minimum = 1, maximum = 65535)]
    port: u16,
    tls_mode: AdminEmailTlsModeDto,
    #[schema(max_length = 320)]
    username: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    password: Option<String>,
    #[schema(max_length = 320, format = Email)]
    from_address: String,
    #[schema(max_length = 128)]
    from_name: Option<String>,
    #[schema(max_length = 320, format = Email)]
    reply_to: Option<String>,
    #[schema(minimum = 1, maximum = 60)]
    timeout_seconds: u16,
}

/// 测试邮件只允许指定收件人，不接受主题或正文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminEmailTestRequest)]
pub(crate) struct AdminEmailTestRequest {
    #[schema(min_length = 3, max_length = 320, format = Email)]
    recipient: String,
}

/// 构建管理员邮件设置路由，并在进入用例前统一完成会话与角色校验。
pub(crate) fn build_email_settings_router(
    service: Arc<dyn AdminEmailSettingsService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/email-settings",
            get(get_email_settings).put(update_email_settings),
        )
        .route("/api/admin/email-settings/test", post(send_test_email))
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(EmailSettingsHttpState { service })
}

/// 返回管理员可见的完整设置投影，密码始终保持脱敏。
async fn get_email_settings(
    State(state): State<EmailSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .settings(authentication.principal())
        .await
        .map_err(map_email_settings_error)?;
    Ok(no_store_json(AdminEmailSettingsResponse::from(settings)))
}

/// 原子覆盖 SMTP 设置；缺失密码表示保留现有密文。
async fn update_email_settings(
    State(state): State<EmailSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminEmailSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminEmailSettingsCommand::new(
        request.enabled,
        request.host,
        request.port,
        request.tls_mode.into(),
        request.username,
        request.password,
        request.from_address,
        request.from_name,
        request.reply_to,
        request.timeout_seconds,
    )
    .map_err(map_email_settings_error)?;
    let settings = state
        .service
        .update(authentication.principal(), command)
        .await
        .map_err(map_email_settings_error)?;
    Ok(no_store_json(AdminEmailSettingsResponse::from(settings)))
}

/// 使用当前数据库快照投递固定正文测试邮件，成功时不回显任何地址。
async fn send_test_email(
    State(state): State<EmailSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminEmailTestRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        AdminEmailTestCommand::new(request.recipient).map_err(map_email_settings_error)?;
    state
        .service
        .send_test(authentication.principal(), command)
        .await
        .map_err(map_email_settings_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

fn map_email_settings_error(error: AdminEmailSettingsError) -> ManagementError {
    match error {
        AdminEmailSettingsError::InvalidInput => ManagementError::InvalidRequest,
        AdminEmailSettingsError::Forbidden => ManagementError::Forbidden,
        AdminEmailSettingsError::NotConfigured => ManagementError::EmailNotConfigured,
        AdminEmailSettingsError::DeliveryFailed => ManagementError::EmailDeliveryFailed,
        AdminEmailSettingsError::Internal => ManagementError::Internal,
    }
}
