use std::{collections::HashMap, fmt, sync::Arc};

use af_domain::UserId;
use af_httpclient::HttpClientProvider;
use thiserror::Error;

use super::{
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationSessionStore, OAuthAuthorizationStart, OAuthLoopbackRedirect,
    OAuthProviderProfile, OAuthTokenExchangeError, OAuthTokenPersistenceError,
    OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService, UpstreamOAuthProvider,
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
    profiles: HashMap<UpstreamOAuthProvider, Arc<OAuthProviderProfile>>,
    sessions: OAuthAuthorizationSessionStore,
    http_clients: HttpClientProvider,
    persistence: OAuthTokenPersistenceService,
}

impl OAuthConnectionCoordinator {
    /// 组合闭合 provider 集合、一次性会话、受控 Client 与加密持久化服务。
    pub fn new<P>(
        profiles: impl IntoIterator<Item = P>,
        sessions: OAuthAuthorizationSessionStore,
        http_clients: HttpClientProvider,
        persistence: OAuthTokenPersistenceService,
    ) -> Result<Self, OAuthConnectionError>
    where
        P: Into<Arc<OAuthProviderProfile>>,
    {
        let mut indexed_profiles = HashMap::with_capacity(4);
        for profile in profiles {
            let profile = profile.into();
            let provider = profile.provider();
            if indexed_profiles.insert(provider, profile).is_some() {
                return Err(OAuthConnectionError::DuplicateProviderProfile { provider });
            }
        }
        Ok(Self {
            profiles: indexed_profiles,
            sessions,
            http_clients,
            persistence,
        })
    }

    /// 创建绑定业务主体的授权会话并返回浏览器授权地址。
    pub fn begin(
        &self,
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
    ) -> Result<OAuthAuthorizationStart, OAuthConnectionError> {
        let profile = self.profile(provider)?;
        let request = profile.authorization_request(context)?;
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
        // 完成侧不接受 user/channel/credential 覆盖，持久化目标只能来自一次性会话。
        let grant = self.sessions.consume(callback)?;
        self.exchange_and_persist(profile, grant).await
    }

    /// 仅允许发起授权的同一管理用户完成手动回调。
    pub async fn complete_for(
        &self,
        user_id: UserId,
        callback: OAuthAuthorizationCallback,
    ) -> Result<OAuthConnectionOutcome, OAuthConnectionError> {
        let provider = callback.provider();
        let profile = self.profile(provider)?;
        let grant = self.sessions.consume_for(callback, user_id)?;
        self.exchange_and_persist(profile, grant).await
    }

    async fn exchange_and_persist(
        &self,
        profile: &OAuthProviderProfile,
        grant: super::OAuthAuthorizationGrant,
    ) -> Result<OAuthConnectionOutcome, OAuthConnectionError> {
        let http_client = self
            .http_clients
            .get(None)
            .map_err(|_| OAuthConnectionError::HttpClientUnavailable)?;
        let token_set = profile.exchange(&http_client, grant).await?;
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

    /// 清理已过期的一次性授权会话，供服务端后台任务周期调用。
    pub fn cleanup_expired_sessions(&self) -> Result<usize, OAuthConnectionError> {
        self.sessions.cleanup_expired().map_err(Into::into)
    }

    /// 返回所有已配置 provider 的固定 loopback 合约。
    #[must_use]
    pub fn configured_loopback_redirects(&self) -> Vec<OAuthLoopbackRedirect> {
        [
            UpstreamOAuthProvider::ClaudeCode,
            UpstreamOAuthProvider::Codex,
            UpstreamOAuthProvider::Gemini,
            UpstreamOAuthProvider::Antigravity,
        ]
        .into_iter()
        .filter_map(|provider| self.profiles.get(&provider))
        .map(|profile| profile.loopback_redirect().clone())
        .collect()
    }

    fn profile(
        &self,
        provider: UpstreamOAuthProvider,
    ) -> Result<&OAuthProviderProfile, OAuthConnectionError> {
        self.profiles
            .get(&provider)
            .map(Arc::as_ref)
            .ok_or(OAuthConnectionError::ProviderProfileNotConfigured { provider })
    }
}

impl fmt::Debug for OAuthConnectionCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthConnectionCoordinator")
            .field("profile_count", &self.profiles.len())
            .field("sessions", &self.sessions)
            .field("http_clients", &"<已脱敏>")
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
    /// 当前全局网络配置无法取得受控 HTTP Client。
    #[error("OAuth HTTP Client 当前不可用")]
    HttpClientUnavailable,
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
