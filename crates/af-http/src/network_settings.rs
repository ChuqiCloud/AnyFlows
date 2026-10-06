use std::sync::Arc;

use af_admin::{
    AdminNetworkSettings, AdminNetworkSettingsCommand, AdminNetworkSettingsError,
    AdminNetworkSettingsService, NetworkSettingsMode, SessionAuthenticator,
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
struct NetworkSettingsHttpState {
    service: Arc<dyn AdminNetworkSettingsService>,
}

/// 管理 API 使用的闭合出站网络模式。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminNetworkSettingsMode, rename_all = "snake_case")]
pub(crate) enum AdminNetworkSettingsModeDto {
    Inherit,
    Direct,
    Http,
    Https,
    Socks5,
    Socks5h,
}

impl From<NetworkSettingsMode> for AdminNetworkSettingsModeDto {
    fn from(value: NetworkSettingsMode) -> Self {
        match value {
            NetworkSettingsMode::Inherit => Self::Inherit,
            NetworkSettingsMode::Direct => Self::Direct,
            NetworkSettingsMode::Http => Self::Http,
            NetworkSettingsMode::Https => Self::Https,
            NetworkSettingsMode::Socks5 => Self::Socks5,
            NetworkSettingsMode::Socks5h => Self::Socks5h,
        }
    }
}

impl From<AdminNetworkSettingsModeDto> for NetworkSettingsMode {
    fn from(value: AdminNetworkSettingsModeDto) -> Self {
        match value {
            AdminNetworkSettingsModeDto::Inherit => Self::Inherit,
            AdminNetworkSettingsModeDto::Direct => Self::Direct,
            AdminNetworkSettingsModeDto::Http => Self::Http,
            AdminNetworkSettingsModeDto::Https => Self::Https,
            AdminNetworkSettingsModeDto::Socks5 => Self::Socks5,
            AdminNetworkSettingsModeDto::Socks5h => Self::Socks5h,
        }
    }
}

/// 管理员读取的网络设置，密码只返回是否已配置。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminNetworkSettings)]
pub(crate) struct AdminNetworkSettingsResponse {
    mode: AdminNetworkSettingsModeDto,
    #[schema(max_length = 255)]
    proxy_host: Option<String>,
    #[schema(minimum = 1, maximum = 65535)]
    proxy_port: Option<u16>,
    #[schema(max_length = 320)]
    username: Option<String>,
    password_configured: bool,
    trust_proxy_dns: bool,
    #[schema(minimum = 1)]
    version: i64,
}

impl From<AdminNetworkSettings> for AdminNetworkSettingsResponse {
    fn from(settings: AdminNetworkSettings) -> Self {
        Self {
            mode: settings.mode().into(),
            proxy_host: settings.proxy_host().map(str::to_owned),
            proxy_port: settings.proxy_port(),
            username: settings.username().map(str::to_owned),
            password_configured: settings.password_configured(),
            trust_proxy_dns: settings.trust_proxy_dns(),
            version: settings.version(),
        }
    }
}

/// 管理员完整覆盖网络设置；密码为空时保留已保存密文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminNetworkSettingsRequest)]
pub(crate) struct AdminNetworkSettingsRequest {
    mode: AdminNetworkSettingsModeDto,
    #[schema(max_length = 255)]
    proxy_host: Option<String>,
    #[schema(minimum = 1, maximum = 65535)]
    proxy_port: Option<u16>,
    #[schema(max_length = 320)]
    username: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    password: Option<String>,
    trust_proxy_dns: bool,
}

/// 构建仅管理员可访问的网络与代理设置路由。
pub(crate) fn build_network_settings_router(
    service: Arc<dyn AdminNetworkSettingsService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/network-settings",
            get(get_network_settings).put(update_network_settings),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(NetworkSettingsHttpState { service })
}

async fn get_network_settings(
    State(state): State<NetworkSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .settings(authentication.principal())
        .await
        .map_err(map_network_settings_error)?;
    Ok(no_store_json(AdminNetworkSettingsResponse::from(settings)))
}

async fn update_network_settings(
    State(state): State<NetworkSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminNetworkSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminNetworkSettingsCommand::new(
        request.mode.into(),
        request.proxy_host,
        request.proxy_port,
        request.username,
        request.password,
        request.trust_proxy_dns,
    )
    .map_err(map_network_settings_error)?;
    let settings = state
        .service
        .update(authentication.principal(), command)
        .await
        .map_err(map_network_settings_error)?;
    Ok(no_store_json(AdminNetworkSettingsResponse::from(settings)))
}

fn map_network_settings_error(error: AdminNetworkSettingsError) -> ManagementError {
    match error {
        AdminNetworkSettingsError::InvalidInput => ManagementError::InvalidRequest,
        AdminNetworkSettingsError::Forbidden => ManagementError::Forbidden,
        AdminNetworkSettingsError::Internal => ManagementError::Internal,
    }
}
