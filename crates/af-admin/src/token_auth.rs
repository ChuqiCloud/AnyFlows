use std::{fmt, future::Future, pin::Pin};

use af_db::{TokenAuthLookup, TokenAuthLookupOutcome, TokenAuthRepository};
use af_domain::{GatewayPrincipal, TrustedClientIp};
use thiserror::Error;

use crate::ApiKeyDigest;

/// 一次令牌认证调用的对象安全 Future。
pub type TokenAuthenticationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<GatewayPrincipal, TokenAuthenticationError>> + Send + 'a>>;

/// HTTP 层可安全公开的令牌认证失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenAuthenticationError {
    /// 凭据不存在，或令牌、用户、有效分组、客户端 IP 不允许继续访问。
    #[error("API Key 无效")]
    InvalidApiKey,
    /// 持久化查询失败、超时或数据违反鉴权不变量。
    #[error("令牌认证内部失败")]
    Internal,
}

/// 下游 API Key 认证端口；HTTP 层只提交摘要，不跨异步边界保留明文。
pub trait TokenAuthenticator: Send + Sync {
    /// 校验 API Key 摘要和可信客户端 IP，并返回已验证的网关请求主体。
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a>;
}

/// 使用 `af-db` 仓储完成令牌与客户端 IP 认证的生产实现。
pub struct DatabaseTokenAuthenticator {
    repository: TokenAuthRepository,
}

impl DatabaseTokenAuthenticator {
    /// 绑定已配置硬超时的令牌仓储。
    #[must_use]
    pub const fn new(repository: TokenAuthRepository) -> Self {
        Self { repository }
    }
}

impl TokenAuthenticator for DatabaseTokenAuthenticator {
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        Box::pin(async move {
            let lookup = TokenAuthLookup::new(digest.as_str())
                .map_err(|_| TokenAuthenticationError::Internal)?;
            match self.repository.lookup(&lookup, client_ip).await {
                Ok(TokenAuthLookupOutcome::Authenticated(principal)) => Ok(principal),
                Ok(TokenAuthLookupOutcome::Rejected) => {
                    Err(TokenAuthenticationError::InvalidApiKey)
                }
                Err(_) => Err(TokenAuthenticationError::Internal),
            }
        })
    }
}

impl fmt::Debug for DatabaseTokenAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseTokenAuthenticator(<redacted>)")
    }
}
