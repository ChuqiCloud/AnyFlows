use std::sync::Arc;

use af_admin::{SessionAuthentication, TokenAuthenticationError, TokenAuthenticator};
use af_domain::{AfError, Protocol};
use axum::{
    extract::{Extension, Request, State},
    middleware::Next,
    response::Response,
};

use crate::client_ip::{ClientIpResolutionError, ClientIpResolver};
use crate::error_response::{api_error_response, rate_limit_error_response};
use crate::{QueryApiKeyPolicy, extract_presented_api_key};

/// HTTP API Key 鉴权中间件所需的不可变状态。
#[derive(Clone)]
pub(crate) struct AuthenticationState {
    authenticator: Arc<dyn TokenAuthenticator>,
    query_api_key_policy: QueryApiKeyPolicy,
    client_ip_resolver: ClientIpResolver,
    error_protocol: Protocol,
}

impl AuthenticationState {
    /// 绑定真实认证器与查询参数凭据策略。
    pub(crate) fn new(
        authenticator: Arc<dyn TokenAuthenticator>,
        query_api_key_policy: QueryApiKeyPolicy,
        server_config: &af_config::ServerConfig,
    ) -> Self {
        Self {
            authenticator,
            query_api_key_policy,
            client_ip_resolver: ClientIpResolver::from_config(server_config),
            error_protocol: Protocol::OpenAiChat,
        }
    }

    /// 为独立协议入口选择对应的公开错误 wire。
    pub(crate) const fn with_error_protocol(mut self, protocol: Protocol) -> Self {
        self.error_protocol = protocol;
        self
    }
}

/// 从已验证管理会话解析试炼场内部主体，并注入现有网关认证快照。
pub(crate) async fn authenticate_playground_session(
    State(state): State<AuthenticationState>,
    Extension(session): Extension<SessionAuthentication>,
    mut request: Request,
    next: Next,
) -> Response {
    match state
        .authenticator
        .authenticate_playground(session.principal().user_id())
        .await
    {
        Ok(authentication) => {
            request.headers_mut().remove(http::header::AUTHORIZATION);
            request.headers_mut().remove("x-api-key");
            request.headers_mut().remove("x-goog-api-key");
            request.extensions_mut().insert(authentication);
            next.run(request).await
        }
        Err(TokenAuthenticationError::InvalidApiKey) => {
            api_error_response(state.error_protocol, AfError::InvalidApiKey)
        }
        Err(TokenAuthenticationError::Internal)
        | Err(TokenAuthenticationError::RateLimitUnavailable) => {
            api_error_response(state.error_protocol, AfError::Internal)
        }
        Err(TokenAuthenticationError::RateLimited { retry_after }) => {
            rate_limit_error_response(state.error_protocol, retry_after)
        }
        Err(TokenAuthenticationError::RequestLimitReached) => {
            api_error_response(state.error_protocol, AfError::InsufficientQuota)
        }
    }
}

/// 从 TCP/代理边界和唯一凭据载体完成认证，并把已验证身份注入请求扩展。
pub(crate) async fn authenticate_api_key(
    State(state): State<AuthenticationState>,
    mut request: Request,
    next: Next,
) -> Response {
    let client_ip = match state.client_ip_resolver.resolve(&mut request) {
        Ok(client_ip) => client_ip,
        Err(ClientIpResolutionError::MissingPeer) => {
            tracing::error!(
                target: "af_http::authentication",
                error_kind = "client_ip_missing_peer",
                "HTTP 服务入口缺少 TCP 对端信息"
            );
            return api_error_response(state.error_protocol, AfError::Internal);
        }
        Err(ClientIpResolutionError::InvalidProxyChain) => {
            return api_error_response(state.error_protocol, AfError::InvalidApiKey);
        }
    };
    let presented = match extract_presented_api_key(&mut request, state.query_api_key_policy) {
        Ok(presented) => presented,
        Err(_) => return api_error_response(state.error_protocol, AfError::InvalidApiKey),
    };

    // 明文只活到摘要计算完成；数据库查询期间不得继续持有它。
    let digest = presented.digest();
    drop(presented);

    let authentication = state.authenticator.authenticate(&digest, client_ip).await;
    // 鉴权结束即释放摘要，不让它跟随 handler 和转发生命周期。
    drop(digest);

    match authentication {
        Ok(authentication) => {
            request.extensions_mut().insert(authentication);
            next.run(request).await
        }
        Err(TokenAuthenticationError::InvalidApiKey) => {
            api_error_response(state.error_protocol, AfError::InvalidApiKey)
        }
        Err(TokenAuthenticationError::Internal) => {
            api_error_response(state.error_protocol, AfError::Internal)
        }
        Err(TokenAuthenticationError::RateLimitUnavailable) => {
            api_error_response(state.error_protocol, AfError::Internal)
        }
        Err(TokenAuthenticationError::RateLimited { retry_after }) => {
            rate_limit_error_response(state.error_protocol, retry_after)
        }
        Err(TokenAuthenticationError::RequestLimitReached) => {
            // 累计请求上限不会自动恢复，因此不能伪造 Retry-After。
            api_error_response(state.error_protocol, AfError::InsufficientQuota)
        }
    }
}
