use std::sync::Arc;

use af_admin::{
    AdminCustomOAuth2Provider, AdminCustomOAuth2ProviderCommand,
    AdminCustomOAuth2ProviderCommandInput, AdminCustomOAuth2ProviderError,
    AdminCustomOAuth2ProviderService, SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
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
struct CustomOAuth2HttpState {
    service: Arc<dyn AdminCustomOAuth2ProviderService>,
}

/// 管理端自定义 OAuth2 Provider 的脱敏配置投影。
#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCustomOAuth2Provider)]
pub(crate) struct AdminCustomOAuth2ProviderResponse {
    #[schema(max_length = 32, pattern = "^custom_[a-z0-9_-]+$")]
    provider_key: String,
    #[schema(max_length = 128)]
    display_name: String,
    #[schema(max_length = 255)]
    client_id: String,
    authorization_endpoint_configured: bool,
    token_endpoint_configured: bool,
    userinfo_endpoint_configured: bool,
    scope_configured: bool,
    subject_field_configured: bool,
    enabled: bool,
    secret_configured: bool,
    #[schema(minimum = 1)]
    version: i64,
}

impl From<AdminCustomOAuth2Provider> for AdminCustomOAuth2ProviderResponse {
    fn from(provider: AdminCustomOAuth2Provider) -> Self {
        Self {
            provider_key: provider.provider_key().to_owned(),
            display_name: provider.display_name().to_owned(),
            client_id: provider.client_id().to_owned(),
            authorization_endpoint_configured: provider.authorization_endpoint_configured(),
            token_endpoint_configured: provider.token_endpoint_configured(),
            userinfo_endpoint_configured: provider.userinfo_endpoint_configured(),
            scope_configured: provider.scope_configured(),
            subject_field_configured: provider.subject_field_configured(),
            enabled: provider.enabled(),
            secret_configured: provider.secret_configured(),
            version: provider.version(),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCustomOAuth2ProviderList)]
pub(crate) struct AdminCustomOAuth2ProviderListResponse {
    providers: Vec<AdminCustomOAuth2ProviderResponse>,
}

/// 自定义 Provider 写入请求；端点原文只允许进入写入命令，不会出现在响应中。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCustomOAuth2ProviderRequest)]
pub(crate) struct AdminCustomOAuth2ProviderRequest {
    #[schema(minimum = 0)]
    expected_version: i64,
    #[schema(max_length = 128)]
    display_name: String,
    #[schema(max_length = 255)]
    client_id: String,
    #[schema(max_length = 2048, format = "uri")]
    authorization_endpoint: String,
    #[schema(max_length = 2048, format = "uri")]
    token_endpoint: String,
    #[schema(max_length = 2048, format = "uri")]
    userinfo_endpoint: String,
    #[schema(max_length = 2048)]
    scope: String,
    #[schema(max_length = 64)]
    subject_field: String,
    enabled: bool,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    client_secret: Option<String>,
    clear_client_secret: bool,
}

/// 构建管理员自定义 OAuth2 Provider 配置路由；公开登录入口暂不在此注册。
pub(crate) fn build_custom_oauth2_router(
    service: Arc<dyn AdminCustomOAuth2ProviderService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/authentication-settings/oauth/custom",
            get(list_custom_oauth2_providers),
        )
        .route(
            "/api/admin/authentication-settings/oauth/custom/{provider_key}",
            get(get_custom_oauth2_provider).put(update_custom_oauth2_provider),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(CustomOAuth2HttpState { service })
}

async fn list_custom_oauth2_providers(
    State(state): State<CustomOAuth2HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let providers = state
        .service
        .list(authentication.principal())
        .await
        .map_err(map_custom_oauth2_error)?;
    Ok(no_store_json(AdminCustomOAuth2ProviderListResponse {
        providers: providers
            .into_iter()
            .map(AdminCustomOAuth2ProviderResponse::from)
            .collect(),
    }))
}

async fn get_custom_oauth2_provider(
    State(state): State<CustomOAuth2HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Path(provider_key): Path<String>,
) -> Result<Response, ManagementError> {
    let provider = state
        .service
        .get(authentication.principal(), provider_key)
        .await
        .map_err(map_custom_oauth2_error)?;
    Ok(no_store_json(AdminCustomOAuth2ProviderResponse::from(
        provider,
    )))
}

async fn update_custom_oauth2_provider(
    State(state): State<CustomOAuth2HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Path(provider_key): Path<String>,
    request: Result<Json<AdminCustomOAuth2ProviderRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminCustomOAuth2ProviderCommand::new(AdminCustomOAuth2ProviderCommandInput {
        expected_version: request.expected_version,
        display_name: request.display_name,
        client_id: request.client_id,
        authorization_endpoint: request.authorization_endpoint,
        token_endpoint: request.token_endpoint,
        userinfo_endpoint: request.userinfo_endpoint,
        scope: request.scope,
        subject_field: request.subject_field,
        enabled: request.enabled,
        client_secret: request.client_secret,
        clear_client_secret: request.clear_client_secret,
    })
    .map_err(map_custom_oauth2_error)?;
    let provider = state
        .service
        .save(authentication.principal(), provider_key, command)
        .await
        .map_err(map_custom_oauth2_error)?;
    Ok(no_store_json(AdminCustomOAuth2ProviderResponse::from(
        provider,
    )))
}

fn map_custom_oauth2_error(error: AdminCustomOAuth2ProviderError) -> ManagementError {
    match error {
        AdminCustomOAuth2ProviderError::InvalidInput => ManagementError::InvalidRequest,
        AdminCustomOAuth2ProviderError::Forbidden => ManagementError::Forbidden,
        AdminCustomOAuth2ProviderError::NotFound => ManagementError::CustomOAuth2ProviderNotFound,
        AdminCustomOAuth2ProviderError::Conflict => ManagementError::CustomOAuth2ProviderConflict,
        AdminCustomOAuth2ProviderError::Internal => ManagementError::Internal,
    }
}
