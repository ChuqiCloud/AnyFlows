use std::{collections::HashMap, fmt};

use af_httpclient::PooledClient;
use thiserror::Error;

use super::{
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationSessionStore, OAuthAuthorizationStart, OAuthProviderProfile,
    OAuthTokenExchangeError, OAuthTokenPersistenceError, OAuthTokenPersistenceOutcome,
    OAuthTokenPersistenceService, UpstreamOAuthProvider,
};

/// OAuth 连接流程的业务结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthConnectionOutcome {
    /// token 集合已加密写入目标 OAuth 凭据。
    Connected,
    /// 授权期间目标被删除、转移或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经绑定其他 provider。
    CredentialProviderMismatch,
}

/// 不含 HTTP/UI 的 OAuth 应用层连接协调器。
pub struct OAuthConnectionCoordinator {
    profiles: HashMap<UpstreamOAuthProvider, OAuthProviderProfile>,
    sessions: OAuthAuthorizationSessionStore,
    http_client: PooledClient,
    persistence: OAuthTokenPersistenceService,
}

impl OAuthConnectionCoordinator {
    /// 组合闭合 provider 集合、一次性会话、受控 Client 与加密持久化服务。
    pub fn new(
        profiles: impl IntoIterator<Item = OAuthProviderProfile>,
        sessions: OAuthAuthorizationSessionStore,
        http_client: PooledClient,
        persistence: OAuthTokenPersistenceService,
    ) -> Result<Self, OAuthConnectionError> {
        let mut indexed_profiles = HashMap::with_capacity(4);
        for profile in profiles {
            let provider = profile.provider();
            if indexed_profiles.insert(provider, profile).is_some() {
                return Err(OAuthConnectionError::DuplicateProviderProfile { provider });
            }
        }
        Ok(Self {
            profiles: indexed_profiles,
            sessions,
            http_client,
            persistence,
        })
    }

    /// 创建绑定业务主体的授权会话并返回浏览器授权地址。
    pub fn begin(
        &self,
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        redirect_uri: String,
    ) -> Result<OAuthAuthorizationStart, OAuthConnectionError> {
        let profile = self.profile(provider)?;
        let request = profile.authorization_request(context, redirect_uri)?;
        self.sessions.begin(&request).map_err(Into::into)
    }

    /// 一次性消费回调，交换 token 并加密持久化到授权时绑定的凭据。
    pub async fn complete(
        &self,
        callback: OAuthAuthorizationCallback,
    ) -> Result<OAuthConnectionOutcome, OAuthConnectionError> {
        let provider = callback.provider();
        // 缺少 profile 时先失败，避免错误接线消耗仍可由正确 provider 完成的会话。
        let profile = self.profile(provider)?;
        let grant = self.sessions.consume(callback)?;
        let token_set = profile.exchange(&self.http_client, grant).await?;
        let outcome = self.persistence.persist(token_set).await?;
        Ok(match outcome {
            OAuthTokenPersistenceOutcome::Stored => OAuthConnectionOutcome::Connected,
            OAuthTokenPersistenceOutcome::TargetNotFound => OAuthConnectionOutcome::TargetNotFound,
            OAuthTokenPersistenceOutcome::ProviderMismatch => {
                OAuthConnectionOutcome::CredentialProviderMismatch
            }
        })
    }

    /// 返回已配置 provider 数量，供启动检查和有界指标使用。
    #[must_use]
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    /// 返回当前未过期的待授权会话数。
    pub fn pending_count(&self) -> Result<usize, OAuthConnectionError> {
        self.sessions.pending_count().map_err(Into::into)
    }

    fn profile(
        &self,
        provider: UpstreamOAuthProvider,
    ) -> Result<&OAuthProviderProfile, OAuthConnectionError> {
        self.profiles
            .get(&provider)
            .ok_or(OAuthConnectionError::ProviderProfileNotConfigured { provider })
    }
}

impl fmt::Debug for OAuthConnectionCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthConnectionCoordinator")
            .field("profile_count", &self.profiles.len())
            .field("sessions", &self.sessions)
            .field("http_client", &"<已脱敏>")
            .field("persistence", &"<已脱敏>")
            .finish()
    }
}

/// OAuth 连接协调错误；不携带 state、授权码、verifier、token、scope 或端点正文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthConnectionError {
    /// 同一个 provider 不能注册多份相互竞争的 profile。
    #[error("OAuth provider profile 重复：{provider}")]
    DuplicateProviderProfile { provider: UpstreamOAuthProvider },
    /// 回调 provider 必须在消费一次性会话前存在。
    #[error("OAuth provider profile 未配置：{provider}")]
    ProviderProfileNotConfigured { provider: UpstreamOAuthProvider },
    /// 授权请求、回调或会话校验失败。
    #[error("OAuth 授权流程失败")]
    Authorization(#[from] OAuthAuthorizationError),
    /// token endpoint 请求或响应失败。
    #[error("OAuth token 交换失败")]
    TokenExchange(#[from] OAuthTokenExchangeError),
    /// token 集合加密或数据库持久化失败。
    #[error("OAuth token 持久化失败")]
    Persistence(#[from] OAuthTokenPersistenceError),
}

#[cfg(test)]
mod tests;
