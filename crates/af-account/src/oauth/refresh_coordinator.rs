use std::{collections::HashMap, fmt, num::NonZeroUsize, sync::Arc, time::Duration};

use af_httpclient::HttpClientProvider;
use thiserror::Error;

use super::{
    OAuthProviderProfile, OAuthRefreshCandidate, OAuthRefreshFailurePersistenceOutcome,
    OAuthRefreshPersistenceError, OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceService,
    OAuthRefreshWriteGuard, OAuthTokenExchangeError, UpstreamOAuthProvider,
    refresh_failure::classify_refresh_failure,
};

mod singleflight;

use singleflight::{
    OAuthRefreshFlightAcquisition, OAuthRefreshFlightKey, OAuthRefreshSingleflight,
    OAuthRefreshSingleflightError,
};

/// 单进程最多同时保留的 OAuth 刷新 flight 数量。
pub const DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS: usize = 1_024;
/// 已完成结果的默认短暂保留时间，用于吸收携带旧版本的迟到候选。
pub const DEFAULT_OAUTH_REFRESH_RESULT_RETENTION: Duration = Duration::from_secs(1);

const DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS_NON_ZERO: NonZeroUsize =
    NonZeroUsize::new(DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS).expect("默认 OAuth flight 容量必须非零");

/// 一次 OAuth 刷新协调后的闭合业务结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthRefreshCoordinatorOutcome {
    /// 新 token 集合已经加密并原子写入。
    Stored,
    /// 上游调用期间凭据已经变化，旧结果被安全丢弃。
    Stale,
    /// 目标不存在、已删除、不属于原渠道或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经切换到其他 Provider。
    ProviderMismatch,
}

impl From<OAuthRefreshPersistenceOutcome> for OAuthRefreshCoordinatorOutcome {
    fn from(outcome: OAuthRefreshPersistenceOutcome) -> Self {
        match outcome {
            OAuthRefreshPersistenceOutcome::Stored => Self::Stored,
            OAuthRefreshPersistenceOutcome::Stale => Self::Stale,
            OAuthRefreshPersistenceOutcome::TargetNotFound => Self::TargetNotFound,
            OAuthRefreshPersistenceOutcome::ProviderMismatch => Self::ProviderMismatch,
        }
    }
}

/// OAuth 刷新协调错误；不携带 token、scope、端点、响应正文或底层传输诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthRefreshCoordinatorError {
    /// 同一个 provider 不能注册多份相互竞争的 profile。
    #[error("OAuth provider profile 重复：{provider}")]
    DuplicateProviderProfile { provider: UpstreamOAuthProvider },
    /// 候选声明的 provider 没有启动期 profile。
    #[error("OAuth provider profile 未配置：{provider}")]
    ProviderProfileNotConfigured { provider: UpstreamOAuthProvider },
    /// 当前全局网络配置无法取得受控 HTTP Client。
    #[error("OAuth HTTP Client 当前不可用")]
    HttpClientUnavailable,
    /// 所有保留槽位均被其他正在执行的凭据占用。
    #[error("OAuth 刷新并发容量已满")]
    SingleflightCapacityExceeded,
    /// singleflight registry 的同步状态已经不可用。
    #[error("OAuth 刷新协调状态不可用")]
    SingleflightUnavailable,
    /// leader 在发布结果前被取消或发生 panic。
    #[error("OAuth 刷新 leader 未完成")]
    LeaderAborted,
    /// token endpoint 请求或响应失败。
    #[error("OAuth token 刷新失败")]
    TokenExchange(#[source] OAuthTokenExchangeError),
    /// token 集合加密或数据库条件写回失败。
    #[error("OAuth 刷新结果持久化失败")]
    Persistence(#[source] OAuthRefreshPersistenceError),
    /// 上游失败已经分类，但对应的凭据状态无法写入数据库。
    #[error("OAuth 刷新失败状态持久化失败")]
    FailureStatePersistence {
        exchange: OAuthTokenExchangeError,
        #[source]
        persistence: OAuthRefreshPersistenceError,
    },
}

impl From<OAuthTokenExchangeError> for OAuthRefreshCoordinatorError {
    fn from(error: OAuthTokenExchangeError) -> Self {
        Self::TokenExchange(error)
    }
}

impl From<OAuthRefreshPersistenceError> for OAuthRefreshCoordinatorError {
    fn from(error: OAuthRefreshPersistenceError) -> Self {
        Self::Persistence(error)
    }
}

/// 按渠道、凭据与 OAuth 版本合并并发刷新的应用协调器。
///
/// 本协调器只保证单进程 singleflight；多实例周期调度仍需后续分布式租约，数据库 CAS
/// 始终作为最终一致性防线。
pub struct OAuthRefreshCoordinator {
    profiles: HashMap<UpstreamOAuthProvider, Arc<OAuthProviderProfile>>,
    http_clients: HttpClientProvider,
    persistence: OAuthRefreshPersistenceService,
    singleflight: Arc<OAuthRefreshSingleflight>,
}

impl OAuthRefreshCoordinator {
    /// 组合共享 Provider profile、受控 Client、刷新持久化服务和默认有界 registry。
    pub fn new<P>(
        profiles: impl IntoIterator<Item = P>,
        http_clients: HttpClientProvider,
        persistence: OAuthRefreshPersistenceService,
    ) -> Result<Self, OAuthRefreshCoordinatorError>
    where
        P: Into<Arc<OAuthProviderProfile>>,
    {
        Self::new_with_singleflight_limits(
            profiles,
            http_clients,
            persistence,
            DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS_NON_ZERO,
            DEFAULT_OAUTH_REFRESH_RESULT_RETENTION,
        )
    }

    /// 消费刷新候选；同一凭据版本只有 leader 调用上游并执行条件写回。
    pub async fn refresh(
        &self,
        candidate: OAuthRefreshCandidate,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        let key = OAuthRefreshFlightKey::new(
            candidate.channel_id(),
            candidate.credential_id(),
            candidate.expected_revision(),
        );
        match self
            .singleflight
            .acquire(key)
            .map_err(map_singleflight_error)?
        {
            OAuthRefreshFlightAcquisition::Leader(leader) => {
                let result = self.execute(candidate).await;
                leader.complete(result)
            }
            OAuthRefreshFlightAcquisition::Follower(follower) => {
                // follower 不需要敏感刷新材料，进入等待前立即清零自己的候选。
                drop(candidate);
                follower.wait().await
            }
        }
    }

    async fn execute(
        &self,
        candidate: OAuthRefreshCandidate,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        let provider = candidate.provider();
        let profile = self
            .profiles
            .get(&provider)
            .ok_or(OAuthRefreshCoordinatorError::ProviderProfileNotConfigured { provider })?;
        let http_client = self
            .http_clients
            .get(None)
            .map_err(|_| OAuthRefreshCoordinatorError::HttpClientUnavailable)?;
        let (guard, request) = candidate.into_parts();
        match profile.refresh(&http_client, request).await {
            Ok(token_set) => Ok(self.persistence.persist(guard, token_set).await?.into()),
            Err(exchange) => self.handle_exchange_failure(guard, exchange).await,
        }
    }

    async fn handle_exchange_failure(
        &self,
        guard: OAuthRefreshWriteGuard,
        exchange: OAuthTokenExchangeError,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        let Some(failure_kind) = classify_refresh_failure(exchange) else {
            return Err(OAuthRefreshCoordinatorError::TokenExchange(exchange));
        };
        let outcome = self
            .persistence
            .record_failure(guard, failure_kind)
            .await
            .map_err(
                |persistence| OAuthRefreshCoordinatorError::FailureStatePersistence {
                    exchange,
                    persistence,
                },
            )?;
        match outcome {
            OAuthRefreshFailurePersistenceOutcome::Recorded => {
                Err(OAuthRefreshCoordinatorError::TokenExchange(exchange))
            }
            OAuthRefreshFailurePersistenceOutcome::Stale => {
                Ok(OAuthRefreshCoordinatorOutcome::Stale)
            }
            OAuthRefreshFailurePersistenceOutcome::TargetNotFound => {
                Ok(OAuthRefreshCoordinatorOutcome::TargetNotFound)
            }
            OAuthRefreshFailurePersistenceOutcome::ProviderMismatch => {
                Ok(OAuthRefreshCoordinatorOutcome::ProviderMismatch)
            }
        }
    }

    fn new_with_singleflight_limits<P>(
        profiles: impl IntoIterator<Item = P>,
        http_clients: HttpClientProvider,
        persistence: OAuthRefreshPersistenceService,
        max_entries: NonZeroUsize,
        completed_retention: Duration,
    ) -> Result<Self, OAuthRefreshCoordinatorError>
    where
        P: Into<Arc<OAuthProviderProfile>>,
    {
        let mut indexed_profiles = HashMap::with_capacity(4);
        for profile in profiles {
            let profile = profile.into();
            let provider = profile.provider();
            if indexed_profiles.insert(provider, profile).is_some() {
                return Err(OAuthRefreshCoordinatorError::DuplicateProviderProfile { provider });
            }
        }
        Ok(Self {
            profiles: indexed_profiles,
            http_clients,
            persistence,
            singleflight: OAuthRefreshSingleflight::new(max_entries, completed_retention),
        })
    }

    /// 返回已配置 provider 数量，供启动检查和有界指标使用。
    #[must_use]
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }
}

impl fmt::Debug for OAuthRefreshCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshCoordinator")
            .field("profile_count", &self.profiles.len())
            .field("http_clients", &"<已脱敏>")
            .field("persistence", &"<已脱敏>")
            .field("singleflight", &self.singleflight)
            .finish()
    }
}

const fn map_singleflight_error(
    error: OAuthRefreshSingleflightError,
) -> OAuthRefreshCoordinatorError {
    match error {
        OAuthRefreshSingleflightError::CapacityExceeded => {
            OAuthRefreshCoordinatorError::SingleflightCapacityExceeded
        }
        OAuthRefreshSingleflightError::Unavailable => {
            OAuthRefreshCoordinatorError::SingleflightUnavailable
        }
    }
}

#[cfg(test)]
mod tests;
