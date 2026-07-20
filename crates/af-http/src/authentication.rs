use std::sync::Arc;

use af_admin::{TokenAuthenticationError, TokenAuthenticator};
use af_domain::AfError;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{OpenAiHttpError, QueryApiKeyPolicy, extract_presented_api_key};

/// HTTP API Key 鉴权中间件所需的不可变状态。
#[derive(Clone)]
pub(crate) struct AuthenticationState {
    authenticator: Arc<dyn TokenAuthenticator>,
    query_api_key_policy: QueryApiKeyPolicy,
}

impl AuthenticationState {
    /// 绑定真实认证器与查询参数凭据策略。
    pub(crate) fn new(
        authenticator: Arc<dyn TokenAuthenticator>,
        query_api_key_policy: QueryApiKeyPolicy,
    ) -> Self {
        Self {
            authenticator,
            query_api_key_policy,
        }
    }
}

/// 从唯一凭据载体完成认证，并把已验证身份注入请求扩展。
pub(crate) async fn authenticate_api_key(
    State(state): State<AuthenticationState>,
    mut request: Request,
    next: Next,
) -> Response {
    let presented = match extract_presented_api_key(&mut request, state.query_api_key_policy) {
        Ok(presented) => presented,
        Err(error) => return OpenAiHttpError::from(error).into_response(),
    };

    // 明文只活到摘要计算完成；数据库查询期间不得继续持有它。
    let digest = presented.digest();
    drop(presented);

    let authentication = state.authenticator.authenticate(&digest).await;
    // 鉴权结束即释放摘要，不让它跟随 handler 和转发生命周期。
    drop(digest);

    match authentication {
        Ok(principal) => {
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(TokenAuthenticationError::InvalidApiKey) => {
            OpenAiHttpError::from(AfError::InvalidApiKey).into_response()
        }
        Err(TokenAuthenticationError::Internal) => {
            OpenAiHttpError::from(AfError::Internal).into_response()
        }
    }
}
