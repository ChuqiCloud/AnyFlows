use std::{collections::BTreeSet, fmt, time::Duration};

use af_domain::{ChannelId, CredentialId, CredentialKind, Status, UpstreamRetryAfter};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set,
    TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::credentials};

const DEFAULT_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
const AUTH_EXPIRED_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);
const QUOTA_EXHAUSTED_COOLDOWN: Duration = Duration::from_secs(15 * 60);
const OVERLOAD_COOLDOWN: Duration = Duration::from_secs(30);

/// 单次请求最多写回的凭据状态数量，覆盖影子成功时的本地与共享双事件。
pub const MAX_CREDENTIAL_STATE_BATCH: usize = 128;

/// 凭据请求结束后允许持久化的闭合状态变化。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialStateChange {
    /// 请求通过协议验证，记录最近使用时间并清除历史冷却。
    Succeeded,
    /// 共享密钥已通过认证，只清理由认证产生的临时冷却。
    AuthenticationSucceeded,
    /// 未能确认永久失效的认证错误，进入有限冷却。
    AuthExpired,
    /// OAuth 认证已失效且没有 refresh token，无法自动恢复。
    MissingRefreshToken,
    /// 上游结构化信号确认凭据已经吊销。
    AuthRevoked,
    /// 上游结构化信号确认账号、组织或工作区已经停用。
    AccountDisabled,
    /// 凭据级限流，记录限流时间和短重置窗口。
    RateLimited {
        /// 上游提供且经过领域上限校验的相对等待时间。
        retry_after: Option<UpstreamRetryAfter>,
    },
    /// 凭据额度暂时耗尽，进入较长冷却等待额度恢复。
    QuotaExhausted,
    /// 上游过载，进入短冷却。
    Overloaded {
        /// 上游提供且经过领域上限校验的相对等待时间。
        retry_after: Option<UpstreamRetryAfter>,
    },
}

/// 一条不含密钥、URL、Header 或上游正文的凭据状态事件。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialStateEvent {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_kind: CredentialKind,
    change: CredentialStateChange,
}

impl CredentialStateEvent {
    /// 校验当前运行时支持的凭据类型后构造状态事件。
    pub fn new(
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_kind: CredentialKind,
        change: CredentialStateChange,
    ) -> Result<Self, CredentialStateRepositoryError> {
        let supported_kind = matches!(
            credential_kind,
            CredentialKind::ApiKey | CredentialKind::Oauth
        );
        let invalid_missing_refresh = matches!(change, CredentialStateChange::MissingRefreshToken)
            && !matches!(credential_kind, CredentialKind::Oauth);
        if !supported_kind || invalid_missing_refresh {
            return Err(CredentialStateRepositoryError::InvalidEvent);
        }
        Ok(Self {
            channel_id,
            credential_id,
            credential_kind,
            change,
        })
    }
}

/// 凭据状态批量写回结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialStateWriteReport {
    event_count: usize,
    changed_count: usize,
}

impl CredentialStateWriteReport {
    /// 返回已校验的事件数量。
    #[must_use]
    pub const fn event_count(self) -> usize {
        self.event_count
    }

    /// 返回实际改变持久化状态的凭据数量。
    #[must_use]
    pub const fn changed_count(self) -> usize {
        self.changed_count
    }
}

/// 凭据运行时状态写回失败。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialStateRepositoryError {
    /// 状态写回截止时间不能为零。
    #[error("凭据状态写回超时必须大于零")]
    InvalidConfiguration,
    /// 批次超过上限或包含重复凭据。
    #[error("凭据状态写回批次无效")]
    InvalidBatch,
    /// 状态事件携带当前运行时不支持的凭据类型。
    #[error("凭据状态事件无效")]
    InvalidEvent,
    /// 获取连接、读取凭据或提交更新失败。
    #[error("写回凭据状态失败")]
    Query,
    /// 状态写回超过硬截止时间。
    #[error("写回凭据状态超时")]
    Timeout,
    /// 事件归属或持久化状态违背不变量。
    #[error("凭据状态写回不变量损坏")]
    Invariant,
}

/// 以有界事务维护凭据最近使用、冷却和自动停用状态。
#[derive(Clone)]
pub struct CredentialStateRepository {
    pool: DatabasePool,
    mutation_timeout: Duration,
}

impl CredentialStateRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            mutation_timeout: DEFAULT_MUTATION_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_mutation_timeout(
        pool: DatabasePool,
        mutation_timeout: Duration,
    ) -> Result<Self, CredentialStateRepositoryError> {
        if mutation_timeout.is_zero() {
            return Err(CredentialStateRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            mutation_timeout,
        })
    }

    /// 在单个事务内校验归属并批量写回凭据状态。
    pub async fn apply(
        &self,
        events: &[CredentialStateEvent],
    ) -> Result<CredentialStateWriteReport, CredentialStateRepositoryError> {
        validate_events(events)?;
        if events.is_empty() {
            return Ok(CredentialStateWriteReport {
                event_count: 0,
                changed_count: 0,
            });
        }
        let operation = async {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| CredentialStateRepositoryError::Query)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let mut changed_count = 0_usize;
            for event in events {
                if apply_event(&transaction, *event, now).await? {
                    changed_count = changed_count
                        .checked_add(1)
                        .ok_or(CredentialStateRepositoryError::Invariant)?;
                }
            }
            transaction
                .commit()
                .await
                .map_err(|_| CredentialStateRepositoryError::Query)?;
            Ok(CredentialStateWriteReport {
                event_count: events.len(),
                changed_count,
            })
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.mutation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                CredentialStateRepositoryError::Timeout,
            )),
        }
    }
}

impl fmt::Debug for CredentialStateRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialStateRepository")
            .field("mutation_timeout", &self.mutation_timeout)
            .finish_non_exhaustive()
    }
}

fn validate_events(events: &[CredentialStateEvent]) -> Result<(), CredentialStateRepositoryError> {
    if events.len() > MAX_CREDENTIAL_STATE_BATCH {
        return Err(CredentialStateRepositoryError::InvalidBatch);
    }
    let unique = events
        .iter()
        .map(|event| (event.channel_id, event.credential_id))
        .collect::<BTreeSet<_>>();
    if unique.len() != events.len() {
        return Err(CredentialStateRepositoryError::InvalidBatch);
    }
    Ok(())
}

async fn apply_event(
    transaction: &sea_orm::DatabaseTransaction,
    event: CredentialStateEvent,
    now: TimeDateTimeWithTimeZone,
) -> Result<bool, CredentialStateRepositoryError> {
    let model = credentials::Entity::find_by_id(event.credential_id.get())
        .filter(credentials::Column::ChannelId.eq(event.channel_id.get()))
        .filter(credentials::Column::Kind.eq(event.credential_kind.as_str()))
        .filter(credentials::Column::DeletedAt.is_null())
        .one(transaction)
        .await
        .map_err(|_| CredentialStateRepositoryError::Query)?
        .ok_or(CredentialStateRepositoryError::Invariant)?;
    let status =
        Status::try_from(model.status).map_err(|_| CredentialStateRepositoryError::Invariant)?;
    if matches!(
        event.change,
        CredentialStateChange::MissingRefreshToken
            | CredentialStateChange::AuthRevoked
            | CredentialStateChange::AccountDisabled
    ) && status != Status::Enabled
    {
        // 并发人工停用优先于运行时自动停用，已自动停用也无需重复写入。
        return Ok(false);
    }

    let authentication_cooling = matches!(
        model.temp_unschedulable_reason.as_deref(),
        Some("auth_expired" | "oauth_refresh_transient")
    );
    let mut active = model.into_active_model();
    match event.change {
        CredentialStateChange::Succeeded => {
            active.last_used_at = Set(Some(now));
            active.rate_limited_at = Set(None);
            active.rate_limit_reset_at = Set(None);
            active.overload_until = Set(None);
            active.temp_unschedulable_until = Set(None);
            active.temp_unschedulable_reason = Set(None);
        }
        CredentialStateChange::AuthenticationSucceeded => {
            active.last_used_at = Set(Some(now));
            if authentication_cooling {
                active.temp_unschedulable_until = Set(None);
                active.temp_unschedulable_reason = Set(None);
            }
        }
        CredentialStateChange::AuthExpired => {
            active.temp_unschedulable_until = Set(Some(now + AUTH_EXPIRED_COOLDOWN));
            active.temp_unschedulable_reason = Set(Some("auth_expired".to_owned()));
        }
        CredentialStateChange::MissingRefreshToken
        | CredentialStateChange::AuthRevoked
        | CredentialStateChange::AccountDisabled => {
            active.status = Set(Status::AutoDisabled.code());
            active.rate_limited_at = Set(None);
            active.rate_limit_reset_at = Set(None);
            active.overload_until = Set(None);
            active.temp_unschedulable_until = Set(None);
            active.temp_unschedulable_reason = Set(None);
        }
        CredentialStateChange::RateLimited { retry_after } => {
            active.rate_limited_at = Set(Some(now));
            active.rate_limit_reset_at =
                Set(Some(now + cooldown(RATE_LIMIT_COOLDOWN, retry_after)));
        }
        CredentialStateChange::QuotaExhausted => {
            active.temp_unschedulable_until = Set(Some(now + QUOTA_EXHAUSTED_COOLDOWN));
            active.temp_unschedulable_reason = Set(Some("quota_exhausted".to_owned()));
        }
        CredentialStateChange::Overloaded { retry_after } => {
            active.overload_until = Set(Some(now + cooldown(OVERLOAD_COOLDOWN, retry_after)));
        }
    }
    active.updated_at = Set(now);
    active
        .update(transaction)
        .await
        .map_err(|_| CredentialStateRepositoryError::Query)?;
    Ok(true)
}

const fn cooldown(fallback: Duration, retry_after: Option<UpstreamRetryAfter>) -> Duration {
    match retry_after {
        Some(retry_after) => retry_after.duration(),
        None => fallback,
    }
}

fn record_internal_error(error: CredentialStateRepositoryError) -> CredentialStateRepositoryError {
    let error_kind = match error {
        CredentialStateRepositoryError::InvalidConfiguration
        | CredentialStateRepositoryError::InvalidBatch
        | CredentialStateRepositoryError::InvalidEvent => return error,
        CredentialStateRepositoryError::Query => "credential_state_query",
        CredentialStateRepositoryError::Timeout => "credential_state_timeout",
        CredentialStateRepositoryError::Invariant => "credential_state_invariant",
    };
    tracing::error!(
        target: "af_db::credential_state",
        error_kind,
        "凭据状态仓储发生内部错误"
    );
    error
}
