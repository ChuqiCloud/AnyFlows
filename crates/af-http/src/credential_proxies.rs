use std::sync::Arc;

use af_admin::{
    AdminCredentialProxy, AdminCredentialProxyCreateCommand, AdminCredentialProxyError,
    AdminCredentialProxyListQuery, AdminCredentialProxyPage, AdminCredentialProxyService,
    AdminCredentialProxyUpdateCommand, CredentialProxyScheme, SessionAuthenticator,
};
use af_domain::ProxyId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
};

#[derive(Clone)]
struct CredentialProxyHttpState {
    service: Arc<dyn AdminCredentialProxyService>,
}

/// 管理 API 使用的闭合专属代理协议。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminCredentialProxyScheme, rename_all = "snake_case")]
pub(crate) enum AdminCredentialProxySchemeDto {
    Http,
    Https,
    Socks5,
    Socks5h,
}

impl From<CredentialProxyScheme> for AdminCredentialProxySchemeDto {
    fn from(value: CredentialProxyScheme) -> Self {
        match value {
            CredentialProxyScheme::Http => Self::Http,
            CredentialProxyScheme::Https => Self::Https,
            CredentialProxyScheme::Socks5 => Self::Socks5,
            CredentialProxyScheme::Socks5h => Self::Socks5h,
        }
    }
}

impl From<AdminCredentialProxySchemeDto> for CredentialProxyScheme {
    fn from(value: AdminCredentialProxySchemeDto) -> Self {
        match value {
            AdminCredentialProxySchemeDto::Http => Self::Http,
            AdminCredentialProxySchemeDto::Https => Self::Https,
            AdminCredentialProxySchemeDto::Socks5 => Self::Socks5,
            AdminCredentialProxySchemeDto::Socks5h => Self::Socks5h,
        }
    }
}

/// 管理端返回的专属代理快照，密码仅暴露是否已配置。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCredentialProxy)]
pub(crate) struct AdminCredentialProxyResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    scheme: AdminCredentialProxySchemeDto,
    #[schema(min_length = 1, max_length = 255)]
    host: String,
    #[schema(minimum = 1, maximum = 65535)]
    port: u16,
    #[schema(max_length = 320)]
    username: Option<String>,
    password_configured: bool,
    trust_proxy_dns: bool,
    enabled: bool,
    #[schema(minimum = 1)]
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl AdminCredentialProxyResponse {
    fn from_proxy(proxy: &AdminCredentialProxy) -> Self {
        Self {
            id: proxy.proxy_id().get(),
            name: proxy.name().to_owned(),
            scheme: proxy.scheme().into(),
            host: proxy.host().to_owned(),
            port: proxy.port(),
            username: proxy.username().map(str::to_owned),
            password_configured: proxy.password_configured(),
            trust_proxy_dns: proxy.trust_proxy_dns(),
            enabled: proxy.enabled(),
            version: proxy.version(),
            created_at: proxy.created_at(),
            updated_at: proxy.updated_at(),
        }
    }
}

/// 管理端专属代理目录分页响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCredentialProxyListResponse)]
pub(crate) struct AdminCredentialProxyListResponse {
    #[schema(max_items = 100)]
    proxies: Vec<AdminCredentialProxyResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

impl AdminCredentialProxyListResponse {
    fn from_page(page: AdminCredentialProxyPage) -> Self {
        Self {
            proxies: page
                .proxies()
                .iter()
                .map(AdminCredentialProxyResponse::from_proxy)
                .collect(),
            next_cursor: page.next_cursor().map(ProxyId::get),
        }
    }
}

/// 创建或完整更新专属代理的结构化正文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminCredentialProxyWriteRequest)]
pub(crate) struct AdminCredentialProxyWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    scheme: AdminCredentialProxySchemeDto,
    #[schema(min_length = 1, max_length = 255)]
    host: String,
    #[schema(minimum = 1, maximum = 65535)]
    port: u16,
    #[schema(max_length = 320)]
    username: Option<String>,
    /// 创建时用户名存在则必填；更新时为空表示保留现有密码。
    #[schema(write_only, min_length = 1, max_length = 4096)]
    password: Option<String>,
    trust_proxy_dns: bool,
    enabled: bool,
}

/// 构建仅管理员可访问的专属代理目录路由。
pub(crate) fn build_credential_proxy_router(
    service: Arc<dyn AdminCredentialProxyService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/proxies",
            get(list_admin_credential_proxies).post(create_admin_credential_proxy),
        )
        .route(
            "/api/admin/proxies/{id}",
            get(get_admin_credential_proxy)
                .put(update_admin_credential_proxy)
                .delete(delete_admin_credential_proxy),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(CredentialProxyHttpState { service })
}

async fn list_admin_credential_proxies(
    State(state): State<CredentialProxyHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminCredentialProxyListResponse::from_page(
        page,
    )))
}

async fn get_admin_credential_proxy(
    State(state): State<CredentialProxyHttpState>,
    Path(proxy_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let proxy = state
        .service
        .get(authentication.principal(), parse_proxy_id(&proxy_id)?)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminCredentialProxyResponse::from_proxy(
        &proxy,
    )))
}

async fn create_admin_credential_proxy(
    State(state): State<CredentialProxyHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialProxyWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let proxy = state
        .service
        .create(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_error)?;
    let mut response = no_store_json(AdminCredentialProxyResponse::from_proxy(&proxy));
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

async fn update_admin_credential_proxy(
    State(state): State<CredentialProxyHttpState>,
    Path(proxy_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialProxyWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let proxy_id = parse_proxy_id(&proxy_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let proxy = state
        .service
        .update(
            authentication.principal(),
            proxy_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminCredentialProxyResponse::from_proxy(
        &proxy,
    )))
}

async fn delete_admin_credential_proxy(
    State(state): State<CredentialProxyHttpState>,
    Path(proxy_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    state
        .service
        .delete(authentication.principal(), parse_proxy_id(&proxy_id)?)
        .await
        .map_err(map_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

impl AdminCredentialProxyWriteRequest {
    fn into_create_command(self) -> Result<AdminCredentialProxyCreateCommand, ManagementError> {
        AdminCredentialProxyCreateCommand::new(
            self.name,
            self.scheme.into(),
            self.host,
            self.port,
            self.username,
            self.password,
            self.trust_proxy_dns,
            self.enabled,
        )
        .map_err(map_error)
    }

    fn into_update_command(self) -> Result<AdminCredentialProxyUpdateCommand, ManagementError> {
        AdminCredentialProxyUpdateCommand::new(
            self.name,
            self.scheme.into(),
            self.host,
            self.port,
            self.username,
            self.password,
            self.trust_proxy_dns,
            self.enabled,
        )
        .map_err(map_error)
    }
}

fn parse_proxy_id(value: &str) -> Result<ProxyId, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| ProxyId::new(value).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_list_query(
    raw_query: Option<&str>,
) -> Result<AdminCredentialProxyListQuery, ManagementError> {
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "after" if after.is_none() => after = Some(parse_proxy_id(&value)?),
            "limit" if limit.is_none() => {
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| ManagementError::InvalidRequest)?,
                );
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminCredentialProxyListQuery::new(after, limit.unwrap_or(100)).map_err(map_error)
}

fn map_error(error: AdminCredentialProxyError) -> ManagementError {
    match error {
        AdminCredentialProxyError::InvalidInput => ManagementError::InvalidRequest,
        AdminCredentialProxyError::Forbidden => ManagementError::Forbidden,
        AdminCredentialProxyError::NotFound => ManagementError::CredentialProxyNotFound,
        AdminCredentialProxyError::Conflict => ManagementError::CredentialProxyConflict,
        AdminCredentialProxyError::Referenced => ManagementError::CredentialProxyReferenced,
        AdminCredentialProxyError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
