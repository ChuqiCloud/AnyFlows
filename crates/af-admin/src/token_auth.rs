use std::{fmt, future::Future, num::NonZeroU32, pin::Pin};

use af_db::{TokenAuthLookup, TokenAuthLookupOutcome, TokenAuthRepository};
use af_domain::{
    ConcurrencyLimit, GatewayPrincipal, TokenModelPolicy, TrustedClientIp, UpstreamRetryAfter,
    UserId,
};
use thiserror::Error;

use crate::ApiKeyDigest;

/// 一次令牌认证调用的对象安全 Future。
pub type TokenAuthenticationFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TokenAuthentication, TokenAuthenticationError>> + Send + 'a>,
>;

/// 会话试炼场认证调用的对象安全 Future。
pub type PlaygroundAuthenticationFuture<'a> = TokenAuthenticationFuture<'a>;

/// 单次请求完成持久化校验后的身份与不可变令牌策略快照。
#[derive(Clone, Eq, PartialEq)]
pub struct TokenAuthentication {
    principal: GatewayPrincipal,
    model_policy: TokenModelPolicy,
    user_concurrency: Option<ConcurrencyLimit>,
    user_rpm_limit: Option<NonZeroU32>,
    group_rpm_limit: Option<NonZeroU32>,
}

impl TokenAuthentication {
    /// 组合已验证主体与同一次仓储查询得到的模型策略快照。
    #[must_use]
    pub const fn new(principal: GatewayPrincipal, model_policy: TokenModelPolicy) -> Self {
        Self {
            principal,
            model_policy,
            user_concurrency: None,
            user_rpm_limit: None,
            group_rpm_limit: None,
        }
    }

    /// 绑定与主体同一次数据库鉴权取得的用户并发限制快照。
    #[must_use]
    pub const fn with_user_concurrency(
        mut self,
        user_concurrency: Option<ConcurrencyLimit>,
    ) -> Self {
        self.user_concurrency = user_concurrency;
        self
    }

    /// 绑定认证查询得到的用户与分组 RPM 快照；空值表示对应主体不限速。
    #[must_use]
    pub const fn with_rpm_limits(
        mut self,
        user_rpm_limit: Option<NonZeroU32>,
        group_rpm_limit: Option<NonZeroU32>,
    ) -> Self {
        self.user_rpm_limit = user_rpm_limit;
        self.group_rpm_limit = group_rpm_limit;
        self
    }

    /// 返回只携带稳定标识的网关主体。
    #[must_use]
    pub const fn principal(&self) -> GatewayPrincipal {
        self.principal
    }

    /// 返回本次请求使用的不可变令牌模型策略。
    #[must_use]
    pub const fn model_policy(&self) -> &TokenModelPolicy {
        &self.model_policy
    }

    /// 返回用户级并发限制；空值表示不限并发。
    #[must_use]
    pub const fn user_concurrency(&self) -> Option<ConcurrencyLimit> {
        self.user_concurrency
    }

    /// 返回本次请求固定的用户 RPM 上限。
    #[must_use]
    pub const fn user_rpm_limit(&self) -> Option<NonZeroU32> {
        self.user_rpm_limit
    }

    /// 返回本次请求固定的分组 RPM 上限。
    #[must_use]
    pub const fn group_rpm_limit(&self) -> Option<NonZeroU32> {
        self.group_rpm_limit
    }
}

impl fmt::Debug for TokenAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenAuthentication(<redacted>)")
    }
}

/// HTTP 层可安全公开的令牌认证失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenAuthenticationError {
    /// 凭据不存在，或令牌、用户、有效分组、客户端 IP 不允许继续访问。
    #[error("API Key 无效")]
    InvalidApiKey,
    /// 持久化查询失败、超时或数据违反鉴权不变量。
    #[error("令牌认证内部失败")]
    Internal,
    /// 已认证请求需要限流，但权威 Redis 存储不可用或未配置。
    #[error("请求限流存储不可用")]
    RateLimitUnavailable,
    /// 已认证主体触发用户或分组 RPM 窗口。
    #[error("请求达到 RPM 限制")]
    RateLimited { retry_after: UpstreamRetryAfter },
    /// 已认证令牌达到累计请求数上限。
    #[error("令牌达到累计请求数上限")]
    RequestLimitReached,
}

/// 下游 API Key 认证端口；HTTP 层只提交摘要，不跨异步边界保留明文。
pub trait TokenAuthenticator: Send + Sync {
    /// 校验 API Key 摘要和可信客户端 IP，并返回身份与策略快照。
    fn authenticate<'a>(
        &'a self,
        digest: &'a ApiKeyDigest,
        client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a>;

    /// 按已认证管理会话解析试炼场内部主体；不会接收或返回浏览器 API Key。
    fn authenticate_playground<'a>(
        &'a self,
        _user_id: UserId,
    ) -> PlaygroundAuthenticationFuture<'a> {
        Box::pin(async { Err(TokenAuthenticationError::Internal) })
    }
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
                Ok(TokenAuthLookupOutcome::Authenticated {
                    principal,
                    model_policy,
                    user_concurrency,
                    user_rpm_limit,
                    group_rpm_limit,
                }) => Ok(TokenAuthentication::new(principal, model_policy)
                    .with_user_concurrency(user_concurrency)
                    .with_rpm_limits(user_rpm_limit, group_rpm_limit)),
                Ok(TokenAuthLookupOutcome::Rejected) => {
                    Err(TokenAuthenticationError::InvalidApiKey)
                }
                Err(_) => Err(TokenAuthenticationError::Internal),
            }
        })
    }

    fn authenticate_playground<'a>(
        &'a self,
        user_id: UserId,
    ) -> PlaygroundAuthenticationFuture<'a> {
        Box::pin(async move {
            match self.repository.lookup_playground(user_id).await {
                Ok(TokenAuthLookupOutcome::Authenticated {
                    principal,
                    model_policy,
                    user_concurrency,
                    user_rpm_limit,
                    group_rpm_limit,
                }) => Ok(TokenAuthentication::new(principal, model_policy)
                    .with_user_concurrency(user_concurrency)
                    .with_rpm_limits(user_rpm_limit, group_rpm_limit)),
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
