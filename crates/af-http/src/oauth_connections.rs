use std::{
    fmt,
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use af_admin::{SessionAuthenticator, SessionPrincipal};
use af_domain::{ChannelId, CredentialId};
use axum::{
    Extension, Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, Path, RawQuery, Request, State, rejection::JsonRejection,
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
};
use http::{
    HeaderMap, HeaderName, HeaderValue, Method, StatusCode,
    header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, HOST, TRANSFER_ENCODING},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tower_http::limit::RequestBodyLimitLayer;
use url::Url;
use utoipa::ToSchema;

use crate::{
    HttpRouter,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_channels::status_json,
    management_error::ManagementError,
    management_session::no_store_json,
};

/// 管理端手动回调正文上限；完整 callback URL 自身仍受更严格的领域校验。
pub const MAX_ADMIN_OAUTH_CALLBACK_BODY_BYTES: usize = 20 * 1_024;
/// 专用 loopback listener 接受的原始查询串上限。
pub const MAX_OAUTH_LOOPBACK_QUERY_BYTES: usize = 16 * 1_024;
/// 回调 token 交换和持久化的外层硬截止。
pub const OAUTH_CALLBACK_COMPLETION_TIMEOUT: Duration = Duration::from_secs(120);

/// 管理 API 支持的上游 OAuth provider。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminOAuthProvider, rename_all = "snake_case")]
pub enum AdminOAuthProvider {
    /// Anthropic Claude Code 官方账号。
    ClaudeCode,
    /// OpenAI Codex 官方账号。
    Codex,
    /// Google Gemini CLI 官方账号。
    Gemini,
    /// Google Antigravity 官方账号。
    Antigravity,
}

impl AdminOAuthProvider {
    /// 返回稳定的配置与响应标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Antigravity => "antigravity",
        }
    }
}

/// 已配置 provider 的安全管理快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminOAuthProviderStatus {
    provider: AdminOAuthProvider,
    redirect_uri: String,
    callback_port: u16,
    callback_path: String,
    loopback_listener_ready: bool,
}

impl AdminOAuthProviderStatus {
    /// 组合不可变回调合约与当前服务端 listener 状态。
    #[must_use]
    pub fn new(
        provider: AdminOAuthProvider,
        redirect_uri: String,
        callback_port: u16,
        callback_path: String,
        loopback_listener_ready: bool,
    ) -> Self {
        Self {
            provider,
            redirect_uri,
            callback_port,
            callback_path,
            loopback_listener_ready,
        }
    }

    /// 返回 provider 标识。
    #[must_use]
    pub const fn provider(&self) -> AdminOAuthProvider {
        self.provider
    }

    /// 返回注册在上游客户端的精确回调 URI。
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// 返回远程隧道或本机 listener 使用的固定端口。
    #[must_use]
    pub const fn callback_port(&self) -> u16 {
        self.callback_port
    }

    /// 返回 listener 唯一允许的回调路径。
    #[must_use]
    pub fn callback_path(&self) -> &str {
        &self.callback_path
    }

    /// 返回 AnyFlows 服务所在主机的专用 listener 是否已成功绑定。
    #[must_use]
    pub const fn loopback_listener_ready(&self) -> bool {
        self.loopback_listener_ready
    }
}

/// 专用回调 Router 使用的固定监听与 redirect URI 配对。
#[derive(Clone, Eq, PartialEq)]
pub struct OAuthLoopbackBinding {
    provider: AdminOAuthProvider,
    bind_address: SocketAddr,
    redirect_uri: Url,
    expected_authority: String,
}

impl OAuthLoopbackBinding {
    /// 防御性校验 account profile 传入的固定 loopback 合约。
    pub fn new(
        provider: AdminOAuthProvider,
        bind_address: SocketAddr,
        redirect_uri: String,
    ) -> Result<Self, OAuthLoopbackBindingError> {
        let redirect_uri =
            Url::parse(&redirect_uri).map_err(|_| OAuthLoopbackBindingError::InvalidContract)?;
        if bind_address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
            || bind_address.port() == 0
            || redirect_uri.scheme() != "http"
            || redirect_uri.host_str() != Some("localhost")
            || redirect_uri.port() != Some(bind_address.port())
            || redirect_uri.username() != ""
            || redirect_uri.password().is_some()
            || redirect_uri.path().is_empty()
            || !redirect_uri.path().starts_with('/')
            || redirect_uri.query().is_some()
            || redirect_uri.fragment().is_some()
        {
            return Err(OAuthLoopbackBindingError::InvalidContract);
        }
        let expected_authority = format!("localhost:{}", bind_address.port());
        Ok(Self {
            provider,
            bind_address,
            redirect_uri,
            expected_authority,
        })
    }

    /// 返回 provider 标识。
    #[must_use]
    pub const fn provider(&self) -> AdminOAuthProvider {
        self.provider
    }

    /// 返回固定 IPv4 loopback 监听地址。
    #[must_use]
    pub const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    /// 返回注册在上游客户端的固定 redirect URI。
    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// 返回唯一允许的回调路径。
    #[must_use]
    pub fn callback_path(&self) -> &str {
        self.redirect_uri.path()
    }

    fn expected_authority(&self) -> &str {
        &self.expected_authority
    }
}

impl fmt::Debug for OAuthLoopbackBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthLoopbackBinding")
            .field("provider", &self.provider)
            .field("bind_address", &self.bind_address)
            .field("callback_path", &self.callback_path())
            .finish()
    }
}

/// 固定回调监听合约违反安全边界。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthLoopbackBindingError {
    /// 地址必须是 `127.0.0.1`，redirect URI 必须是同端口的精确 `localhost` URL。
    #[error("OAuth loopback 回调合约无效")]
    InvalidContract,
}

/// 创建授权会话后返回给管理 HTTP 层的公开材料。
pub struct AdminOAuthAuthorizationStart {
    provider: AdminOAuthProvider,
    authorization_url: String,
    redirect_uri: String,
    expires_in_seconds: u64,
    loopback_listener_ready: bool,
}

impl AdminOAuthAuthorizationStart {
    /// 组合授权 URL、固定回调和剩余有效期；授权 URL 不得进入日志。
    #[must_use]
    pub fn new(
        provider: AdminOAuthProvider,
        authorization_url: String,
        redirect_uri: String,
        expires_in_seconds: u64,
        loopback_listener_ready: bool,
    ) -> Self {
        Self {
            provider,
            authorization_url,
            redirect_uri,
            expires_in_seconds,
            loopback_listener_ready,
        }
    }

    fn into_parts(self) -> (AdminOAuthProvider, String, String, u64, bool) {
        (
            self.provider,
            self.authorization_url,
            self.redirect_uri,
            self.expires_in_seconds,
            self.loopback_listener_ready,
        )
    }
}

impl fmt::Debug for AdminOAuthAuthorizationStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminOAuthAuthorizationStart")
            .field("provider", &self.provider)
            .field("authorization_url", &"<已脱敏>")
            .field("redirect_uri", &"<已脱敏>")
            .field("expires_in_seconds", &self.expires_in_seconds)
            .field("loopback_listener_ready", &self.loopback_listener_ready)
            .finish()
    }
}

/// OAuth 完成后的稳定业务结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminOAuthCompletionOutcome {
    /// token 已写入发起授权时绑定的凭据。
    Connected,
    /// 授权期间目标被删除、转移或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经绑定其他 provider。
    CredentialProviderMismatch,
}

/// 管理 OAuth 端口的稳定错误；不携带 state、code、token、URL 或上游正文。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminOAuthConnectionError {
    #[error("OAuth 请求无效")]
    InvalidInput,
    #[error("OAuth 操作权限不足")]
    Forbidden,
    #[error("OAuth provider 未配置")]
    ProviderNotConfigured,
    #[error("OAuth 目标凭据不存在")]
    TargetNotFound,
    #[error("OAuth 目标凭据类型不匹配")]
    TargetKindMismatch,
    #[error("OAuth 目标凭据 provider 不匹配")]
    CredentialProviderMismatch,
    #[error("OAuth 待授权会话容量已满")]
    AuthorizationCapacityExceeded,
    #[error("OAuth 授权会话不存在")]
    AuthorizationNotFound,
    #[error("OAuth 授权会话已过期")]
    AuthorizationExpired,
    #[error("OAuth provider 拒绝授权")]
    AuthorizationDenied,
    #[error("OAuth 上游请求超时")]
    UpstreamTimeout,
    #[error("OAuth 上游拒绝请求")]
    UpstreamRejected,
    #[error("OAuth 上游响应无效")]
    UpstreamInvalidResponse,
    #[error("OAuth 服务暂不可用")]
    Unavailable,
    #[error("OAuth 内部状态损坏")]
    Internal,
}

/// 管理端发起授权的对象安全 Future。
pub type AdminOAuthBeginFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminOAuthAuthorizationStart, AdminOAuthConnectionError>>
            + Send
            + 'a,
    >,
>;
/// 管理端或 loopback listener 完成授权的对象安全 Future。
pub type AdminOAuthCompleteFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminOAuthCompletionOutcome, AdminOAuthConnectionError>>
            + Send
            + 'a,
    >,
>;

/// HTTP 层使用的上游账号 OAuth 应用端口。
pub trait AdminOAuthConnectionService: Send + Sync {
    /// 返回当前已配置 provider 与服务端 listener 状态。
    fn provider_statuses(
        &self,
        principal: SessionPrincipal,
    ) -> Result<Vec<AdminOAuthProviderStatus>, AdminOAuthConnectionError>;

    /// 校验现有 OAuth 凭据并创建绑定当前管理员的授权会话。
    fn begin<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        provider: AdminOAuthProvider,
    ) -> AdminOAuthBeginFuture<'a>;

    /// 使用完整 callback URL 完成当前管理员发起的授权。
    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a>;

    /// 使用专用 loopback listener 收到的完整 callback URL 完成授权。
    fn complete_loopback<'a>(
        &'a self,
        provider: AdminOAuthProvider,
        callback_url: String,
    ) -> AdminOAuthCompleteFuture<'a>;
}

#[derive(Clone)]
struct AdminOAuthHttpState {
    service: Arc<dyn AdminOAuthConnectionService>,
}

/// 管理员查询已配置 OAuth provider 的响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthProviderListResponse)]
pub(crate) struct AdminOAuthProviderListResponse {
    providers: Vec<AdminOAuthProviderResponse>,
}

/// 单个已配置 OAuth provider 的回调能力。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthProviderStatus)]
pub(crate) struct AdminOAuthProviderResponse {
    provider: AdminOAuthProvider,
    #[schema(format = "uri", max_length = 2048)]
    redirect_uri: String,
    callback_port: u16,
    #[schema(max_length = 255)]
    callback_path: String,
    loopback_listener_ready: bool,
    manual_callback_supported: bool,
}

impl From<AdminOAuthProviderStatus> for AdminOAuthProviderResponse {
    fn from(status: AdminOAuthProviderStatus) -> Self {
        Self {
            provider: status.provider(),
            redirect_uri: status.redirect_uri().to_owned(),
            callback_port: status.callback_port(),
            callback_path: status.callback_path().to_owned(),
            loopback_listener_ready: status.loopback_listener_ready(),
            manual_callback_supported: true,
        }
    }
}

/// 管理端发起一次已有凭据 OAuth 连接。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthAuthorizationRequest)]
pub(crate) struct AdminOAuthAuthorizationRequest {
    provider: AdminOAuthProvider,
}

/// 发起 OAuth 后返回的浏览器地址与固定回调说明。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthAuthorizationResponse)]
pub(crate) struct AdminOAuthAuthorizationResponse {
    provider: AdminOAuthProvider,
    #[schema(format = "uri", max_length = 8192)]
    authorization_url: String,
    #[schema(format = "uri", max_length = 2048)]
    redirect_uri: String,
    #[schema(minimum = 1, maximum = 900)]
    expires_in_seconds: u64,
    loopback_listener_ready: bool,
    manual_callback_supported: bool,
}

impl From<AdminOAuthAuthorizationStart> for AdminOAuthAuthorizationResponse {
    fn from(start: AdminOAuthAuthorizationStart) -> Self {
        let (provider, authorization_url, redirect_uri, expires_in_seconds, listener_ready) =
            start.into_parts();
        Self {
            provider,
            authorization_url,
            redirect_uri,
            expires_in_seconds,
            loopback_listener_ready: listener_ready,
            manual_callback_supported: true,
        }
    }
}

/// 远程管理场景提交的完整 loopback callback URL。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthManualCallbackRequest)]
pub(crate) struct AdminOAuthManualCallbackRequest {
    provider: AdminOAuthProvider,
    #[schema(format = "uri", min_length = 1, max_length = 16384, write_only)]
    callback_url: String,
}

/// OAuth 凭据连接成功响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminOAuthCompletionResponse)]
pub(crate) struct AdminOAuthCompletionResponse {
    status: AdminOAuthCompletionStatus,
}

/// OAuth 凭据连接终态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminOAuthCompletionStatus, rename_all = "snake_case")]
pub(crate) enum AdminOAuthCompletionStatus {
    Connected,
}

/// 构建仅管理员可访问的 OAuth provider 查询、发起和手动完成路由。
pub fn build_admin_oauth_connection_router(
    service: Arc<dyn AdminOAuthConnectionService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/admin/oauth/providers", get(list_oauth_providers))
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/oauth-authorizations",
            post(begin_oauth_authorization),
        )
        .route(
            "/api/admin/oauth/authorizations/manual-callback",
            post(complete_oauth_manual_callback),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .layer(DefaultBodyLimit::max(MAX_ADMIN_OAUTH_CALLBACK_BODY_BYTES))
        .layer(RequestBodyLimitLayer::new(
            MAX_ADMIN_OAUTH_CALLBACK_BODY_BYTES,
        ))
        .with_state(AdminOAuthHttpState { service })
}

async fn list_oauth_providers(
    State(state): State<AdminOAuthHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let providers = state
        .service
        .provider_statuses(authentication.principal())
        .map_err(map_oauth_error)?
        .into_iter()
        .map(AdminOAuthProviderResponse::from)
        .collect();
    Ok(no_store_json(AdminOAuthProviderListResponse { providers }))
}

async fn begin_oauth_authorization(
    State(state): State<AdminOAuthHttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthAuthorizationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let start = state
        .service
        .begin(
            authentication.principal(),
            channel_id,
            credential_id,
            request.provider,
        )
        .await
        .map_err(map_oauth_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminOAuthAuthorizationResponse::from(start),
    ))
}

async fn complete_oauth_manual_callback(
    State(state): State<AdminOAuthHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminOAuthManualCallbackRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let provider = request.provider;
    let outcome = tokio::time::timeout(
        OAUTH_CALLBACK_COMPLETION_TIMEOUT,
        state
            .service
            .complete_manual(authentication.principal(), provider, request.callback_url),
    )
    .await
    .map_err(|_| {
        tracing::warn!(
            provider = provider.as_str(),
            stage = "manual_callback",
            error_kind = "upstream_timeout",
            "OAuth provider 操作超时"
        );
        ManagementError::OauthUpstreamTimeout
    })?
    .map_err(map_oauth_error)?;
    completion_response(outcome)
}

fn completion_response(outcome: AdminOAuthCompletionOutcome) -> Result<Response, ManagementError> {
    match outcome {
        AdminOAuthCompletionOutcome::Connected => Ok(no_store_json(AdminOAuthCompletionResponse {
            status: AdminOAuthCompletionStatus::Connected,
        })),
        AdminOAuthCompletionOutcome::TargetNotFound => Err(ManagementError::CredentialNotFound),
        AdminOAuthCompletionOutcome::CredentialProviderMismatch => {
            Err(ManagementError::OauthCredentialProviderMismatch)
        }
    }
}

fn parse_channel_id(value: &str) -> Result<ChannelId, ManagementError> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| ChannelId::new(value).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_credential_id(value: &str) -> Result<CredentialId, ManagementError> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| CredentialId::new(value).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn map_oauth_error(error: AdminOAuthConnectionError) -> ManagementError {
    match error {
        AdminOAuthConnectionError::InvalidInput => ManagementError::InvalidRequest,
        AdminOAuthConnectionError::Forbidden => ManagementError::Forbidden,
        AdminOAuthConnectionError::ProviderNotConfigured => {
            ManagementError::OauthProviderNotConfigured
        }
        AdminOAuthConnectionError::TargetNotFound => ManagementError::CredentialNotFound,
        AdminOAuthConnectionError::TargetKindMismatch => ManagementError::InvalidRequest,
        AdminOAuthConnectionError::CredentialProviderMismatch => {
            ManagementError::OauthCredentialProviderMismatch
        }
        AdminOAuthConnectionError::AuthorizationCapacityExceeded => {
            ManagementError::OauthAuthorizationCapacityExceeded
        }
        AdminOAuthConnectionError::AuthorizationNotFound => {
            ManagementError::OauthAuthorizationNotFound
        }
        AdminOAuthConnectionError::AuthorizationExpired => {
            ManagementError::OauthAuthorizationExpired
        }
        AdminOAuthConnectionError::AuthorizationDenied => ManagementError::OauthAuthorizationDenied,
        AdminOAuthConnectionError::UpstreamTimeout => ManagementError::OauthUpstreamTimeout,
        AdminOAuthConnectionError::UpstreamRejected => ManagementError::OauthUpstreamRejected,
        AdminOAuthConnectionError::UpstreamInvalidResponse => {
            ManagementError::OauthUpstreamInvalidResponse
        }
        AdminOAuthConnectionError::Unavailable => ManagementError::OauthUnavailable,
        AdminOAuthConnectionError::Internal => ManagementError::Internal,
    }
}

#[derive(Clone)]
struct OAuthLoopbackHttpState {
    service: Arc<dyn AdminOAuthConnectionService>,
    binding: OAuthLoopbackBinding,
}

/// 为单个 provider 构建不带 CORS、Cookie、JWT 或通用请求日志的专用回调 Router。
pub fn build_oauth_loopback_callback_router(
    service: Arc<dyn AdminOAuthConnectionService>,
    binding: OAuthLoopbackBinding,
) -> HttpRouter {
    let callback_path = binding.callback_path().to_owned();
    let state = OAuthLoopbackHttpState { service, binding };
    HttpRouter::new(
        Router::new()
            .route(&callback_path, any(complete_oauth_loopback_callback))
            .fallback(complete_oauth_loopback_not_found)
            .layer(DefaultBodyLimit::max(0))
            .layer(RequestBodyLimitLayer::new(0))
            .layer(middleware::from_fn(reject_loopback_request_body))
            .with_state(state),
    )
}

async fn complete_oauth_loopback_not_found() -> Response {
    loopback_html(StatusCode::NOT_FOUND, false)
}

async fn complete_oauth_loopback_callback(
    State(state): State<OAuthLoopbackHttpState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if method != Method::GET {
        return loopback_html(StatusCode::METHOD_NOT_ALLOWED, false);
    }
    if peer.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
        || !has_expected_host(&headers, state.binding.expected_authority())
    {
        return loopback_html(StatusCode::BAD_REQUEST, false);
    }
    let Some(raw_query) = raw_query else {
        return loopback_html(StatusCode::BAD_REQUEST, false);
    };
    if raw_query.is_empty() || raw_query.len() > MAX_OAUTH_LOOPBACK_QUERY_BYTES {
        return loopback_html(StatusCode::BAD_REQUEST, false);
    }

    // Host 永远不参与 callback URL 构造，避免本机恶意请求覆盖已注册 redirect URI。
    let mut callback_url = state.binding.redirect_uri().clone();
    callback_url.set_query(Some(&raw_query));
    let completion = tokio::time::timeout(
        OAUTH_CALLBACK_COMPLETION_TIMEOUT,
        state
            .service
            .complete_loopback(state.binding.provider(), callback_url.to_string()),
    )
    .await;
    match completion {
        Ok(Ok(AdminOAuthCompletionOutcome::Connected)) => loopback_html(StatusCode::OK, true),
        Ok(Ok(
            AdminOAuthCompletionOutcome::TargetNotFound
            | AdminOAuthCompletionOutcome::CredentialProviderMismatch,
        ))
        | Ok(Err(_)) => loopback_html(StatusCode::BAD_REQUEST, false),
        Err(_) => {
            tracing::warn!(
                provider = state.binding.provider().as_str(),
                stage = "loopback_callback",
                error_kind = "upstream_timeout",
                "OAuth provider 操作超时"
            );
            loopback_html(StatusCode::GATEWAY_TIMEOUT, false)
        }
    }
}

async fn reject_loopback_request_body(request: Request, next: Next) -> Response {
    let has_transfer_encoding = request.headers().contains_key(TRANSFER_ENCODING);
    let has_body = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value != "0");
    if has_transfer_encoding || has_body {
        return loopback_html(StatusCode::BAD_REQUEST, false);
    }
    next.run(request).await
}

fn has_expected_host(headers: &HeaderMap, expected_authority: &str) -> bool {
    let values = headers.get_all(HOST);
    let mut values = values.iter();
    let Some(host) = values.next() else {
        return false;
    };
    values.next().is_none()
        && host
            .to_str()
            .is_ok_and(|host| host.eq_ignore_ascii_case(expected_authority))
}

fn loopback_html(status: StatusCode, succeeded: bool) -> Response {
    let document = if succeeded {
        r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>OAuth 授权完成</title></head><body><main><h1>授权已完成</h1><p>凭据已经安全更新，可以关闭此窗口。</p></main></body></html>"#
    } else {
        r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>OAuth 授权失败</title></head><body><main><h1>授权未完成</h1><p>请返回 AnyFlows 后重试，或提交浏览器地址栏中的完整回调地址。</p></main></body></html>"#
    };
    let mut response = (status, Html(document)).into_response();
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'; base-uri 'none'"),
    );
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}
