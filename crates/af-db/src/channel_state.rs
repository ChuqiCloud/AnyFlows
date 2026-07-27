use std::{fmt, time::Duration};

use af_domain::{ChannelId, Status};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    ability_write::{AbilityWriteError, set_channel_abilities_enabled},
    entity::channels,
};

const DEFAULT_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
/// 单轮探活最多读取的自动禁用渠道数量，防止后台任务无界占用内存与连接。
pub const MAX_CHANNEL_PROBE_BATCH: usize = 64;

/// 渠道自动禁用操作的幂等结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelAutoDisableOutcome {
    /// 渠道从启用状态原子切换为自动禁用。
    Disabled,
    /// 渠道已经处于自动禁用状态，无需重复写入。
    AlreadyDisabled,
    /// 渠道不存在、已软删除、未开启自动禁用或处于手动禁用状态。
    NotEligible,
}

/// 探活成功后恢复渠道的幂等结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelProbeRecoveryOutcome {
    /// 渠道从自动禁用状态原子恢复为启用。
    Recovered,
    /// 渠道已经处于启用状态，无需重复写入。
    AlreadyEnabled,
    /// 渠道在本次探活开始后再次变更，旧探活结果不得覆盖新状态。
    StaleProbe,
    /// 渠道不存在、已软删除或处于手动禁用状态。
    NotEligible,
}

/// 一次探活绑定的自动禁用状态快照。
///
/// 恢复时必须原样传回本值；仓储会校验禁用时间戳，拒绝旧探活覆盖更新后的状态。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelProbeLease {
    channel_id: ChannelId,
    auto_disabled_at: TimeDateTimeWithTimeZone,
}

impl ChannelProbeLease {
    /// 为非数据库仓储实现或测试构造探活租约。
    ///
    /// 生产数据库仓储仍以 `load_probe_candidates` 为标准来源；恢复阶段会再次用时间戳
    /// 做 CAS 校验，因此调用方不能仅凭渠道 ID 绕过旧探活保护。
    #[must_use]
    pub const fn new(channel_id: ChannelId, auto_disabled_at: TimeDateTimeWithTimeZone) -> Self {
        Self {
            channel_id,
            auto_disabled_at,
        }
    }

    /// 返回本次应探活的渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
}

impl fmt::Debug for ChannelProbeLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelProbeLease")
            .field("channel_id", &self.channel_id)
            .field("auto_disabled_at", &"<已脱敏>")
            .finish()
    }
}

/// 渠道自动禁用与探活恢复持久化失败。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelStateRepositoryError {
    /// 状态变更截止时间配置为零。
    #[error("渠道状态变更超时必须大于零")]
    InvalidConfiguration,
    /// 探活批次为零或超过单轮安全上限。
    #[error("渠道探活批次大小无效")]
    InvalidBatchSize,
    /// 获取连接、执行更新或读取当前状态失败。
    #[error("更新渠道状态失败")]
    Query,
    /// 状态变更超过硬截止时间。
    #[error("更新渠道状态超时")]
    Timeout,
    /// 数据库返回的状态值或受影响行数违反持久化不变量。
    #[error("渠道状态持久化状态损坏")]
    Invariant,
}

/// 维护渠道自动禁用与探活恢复状态的数据库仓储。
///
/// 自动禁用使用条件更新保护管理员状态；探活恢复使用事务锁与租约时间戳，只恢复
/// 本轮观察到的自动禁用状态。仓储不保存上游错误正文或探活响应内容。
#[derive(Clone)]
pub struct ChannelStateRepository {
    pool: DatabasePool,
    mutation_timeout: Duration,
}

impl ChannelStateRepository {
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
    ) -> Result<Self, ChannelStateRepositoryError> {
        if mutation_timeout.is_zero() {
            return Err(ChannelStateRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            mutation_timeout,
        })
    }

    /// 仅在渠道启用、未软删除且允许自动禁用时切换为自动禁用。
    pub async fn auto_disable(
        &self,
        channel_id: ChannelId,
    ) -> Result<ChannelAutoDisableOutcome, ChannelStateRepositoryError> {
        let operation = async {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let result = channels::Entity::update_many()
                .col_expr(
                    channels::Column::Status,
                    Expr::value(Status::AutoDisabled.code()),
                )
                .col_expr(channels::Column::UpdatedAt, Expr::value(now))
                .filter(channels::Column::Id.eq(channel_id.get()))
                .filter(channels::Column::Status.eq(Status::Enabled.code()))
                .filter(channels::Column::AutoBan.eq(true))
                .filter(channels::Column::DeletedAt.is_null())
                .exec(&transaction)
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            let outcome = match result.rows_affected {
                1 => ChannelAutoDisableOutcome::Disabled,
                0 => match load_live_state(&transaction, channel_id).await? {
                    Some((Status::AutoDisabled, _)) => ChannelAutoDisableOutcome::AlreadyDisabled,
                    Some((Status::Enabled | Status::Disabled, _)) | None => {
                        ChannelAutoDisableOutcome::NotEligible
                    }
                },
                _ => return Err(ChannelStateRepositoryError::Invariant),
            };
            if matches!(
                outcome,
                ChannelAutoDisableOutcome::Disabled | ChannelAutoDisableOutcome::AlreadyDisabled
            ) {
                set_channel_abilities_enabled(&transaction, channel_id, false, now)
                    .await
                    .map_err(map_ability_write_error)?;
            }
            transaction
                .commit()
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            Ok(outcome)
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.mutation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ChannelStateRepositoryError::Timeout)),
        }
    }

    /// 按渠道标识稳定读取一轮有界探活租约。
    ///
    /// 调用方应把上一批最后一个渠道作为 `after_channel_id` 继续扫描；到达末尾后以
    /// `None` 重新开始，避免自动禁用渠道超过单批上限时产生饥饿。
    pub async fn load_probe_candidates(
        &self,
        after_channel_id: Option<ChannelId>,
        limit: usize,
    ) -> Result<Vec<ChannelProbeLease>, ChannelStateRepositoryError> {
        if limit == 0 || limit > MAX_CHANNEL_PROBE_BATCH {
            return Err(ChannelStateRepositoryError::InvalidBatchSize);
        }
        let operation = async {
            let limit = u64::try_from(limit).map_err(|_| ChannelStateRepositoryError::Invariant)?;
            let mut query = channels::Entity::find()
                .select_only()
                .column(channels::Column::Id)
                .column(channels::Column::UpdatedAt)
                .filter(channels::Column::Status.eq(Status::AutoDisabled.code()))
                .filter(channels::Column::DeletedAt.is_null());
            if let Some(after_channel_id) = after_channel_id {
                query = query.filter(channels::Column::Id.gt(after_channel_id.get()));
            }
            let rows = query
                .order_by_asc(channels::Column::Id)
                .limit(limit)
                .into_tuple::<(i64, TimeDateTimeWithTimeZone)>()
                .all(self.pool.connection())
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            rows.into_iter()
                .map(|(channel_id, auto_disabled_at)| {
                    Ok(ChannelProbeLease {
                        channel_id: ChannelId::new(channel_id)
                            .map_err(|_| ChannelStateRepositoryError::Invariant)?,
                        auto_disabled_at,
                    })
                })
                .collect::<Result<Vec<_>, ChannelStateRepositoryError>>()
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.mutation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ChannelStateRepositoryError::Timeout)),
        }
    }

    /// 探活成功后，以租约时间戳为 CAS 条件恢复对应的自动禁用渠道。
    pub async fn recover_after_probe(
        &self,
        lease: ChannelProbeLease,
    ) -> Result<ChannelProbeRecoveryOutcome, ChannelStateRepositoryError> {
        let operation = async {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            lock_probe_channel(&transaction, lease.channel_id).await?;
            let outcome = match load_live_state(&transaction, lease.channel_id).await? {
                Some((Status::Enabled, _)) => {
                    set_channel_abilities_enabled(
                        &transaction,
                        lease.channel_id,
                        true,
                        TimeDateTimeWithTimeZone::now_utc(),
                    )
                    .await
                    .map_err(map_ability_write_error)?;
                    ChannelProbeRecoveryOutcome::AlreadyEnabled
                }
                Some((Status::Disabled, _)) | None => ChannelProbeRecoveryOutcome::NotEligible,
                Some((Status::AutoDisabled, updated_at))
                    if updated_at != lease.auto_disabled_at =>
                {
                    ChannelProbeRecoveryOutcome::StaleProbe
                }
                Some((Status::AutoDisabled, _)) => {
                    let now = TimeDateTimeWithTimeZone::now_utc();
                    let result = channels::Entity::update_many()
                        .col_expr(
                            channels::Column::Status,
                            Expr::value(Status::Enabled.code()),
                        )
                        .col_expr(channels::Column::UpdatedAt, Expr::value(now))
                        .filter(channels::Column::Id.eq(lease.channel_id.get()))
                        .filter(channels::Column::Status.eq(Status::AutoDisabled.code()))
                        .filter(channels::Column::DeletedAt.is_null())
                        .exec(&transaction)
                        .await
                        .map_err(|_| ChannelStateRepositoryError::Query)?;
                    if result.rows_affected != 1 {
                        return Err(ChannelStateRepositoryError::Invariant);
                    }
                    set_channel_abilities_enabled(&transaction, lease.channel_id, true, now)
                        .await
                        .map_err(map_ability_write_error)?;
                    ChannelProbeRecoveryOutcome::Recovered
                }
            };
            transaction
                .commit()
                .await
                .map_err(|_| ChannelStateRepositoryError::Query)?;
            Ok(outcome)
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.mutation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ChannelStateRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for ChannelStateRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelStateRepository")
            .field("mutation_timeout", &self.mutation_timeout)
            .finish_non_exhaustive()
    }
}

fn record_internal_error(error: ChannelStateRepositoryError) -> ChannelStateRepositoryError {
    let error_kind = match error {
        ChannelStateRepositoryError::InvalidConfiguration
        | ChannelStateRepositoryError::InvalidBatchSize => return error,
        ChannelStateRepositoryError::Query => "channel_state_query",
        ChannelStateRepositoryError::Timeout => "channel_state_timeout",
        ChannelStateRepositoryError::Invariant => "channel_state_invariant",
    };
    tracing::error!(
        target: "af_db::channel_state",
        error_kind,
        "渠道状态仓储发生内部错误"
    );
    error
}

fn map_ability_write_error(error: AbilityWriteError) -> ChannelStateRepositoryError {
    match error {
        AbilityWriteError::Query => ChannelStateRepositoryError::Query,
        AbilityWriteError::InvalidInput
        | AbilityWriteError::InvalidReference
        | AbilityWriteError::CapacityExceeded
        | AbilityWriteError::Invariant => ChannelStateRepositoryError::Invariant,
    }
}

/// 先执行不改变审计时间的写语句取得行/数据库写锁，避免探活结果与新故障交错。
async fn lock_probe_channel<C>(
    connection: &C,
    channel_id: ChannelId,
) -> Result<(), ChannelStateRepositoryError>
where
    C: ConnectionTrait,
{
    channels::Entity::update_many()
        .col_expr(
            channels::Column::UpdatedAt,
            Expr::col(channels::Column::UpdatedAt).into(),
        )
        .filter(channels::Column::Id.eq(channel_id.get()))
        .filter(channels::Column::DeletedAt.is_null())
        .exec(connection)
        .await
        .map(|_| ())
        .map_err(|_| ChannelStateRepositoryError::Query)
}

async fn load_live_state<C>(
    connection: &C,
    channel_id: ChannelId,
) -> Result<Option<(Status, TimeDateTimeWithTimeZone)>, ChannelStateRepositoryError>
where
    C: ConnectionTrait,
{
    let row = channels::Entity::find_by_id(channel_id.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(|_| ChannelStateRepositoryError::Query)?;
    row.map(|channel| {
        Ok((
            Status::try_from(channel.status).map_err(|_| ChannelStateRepositoryError::Invariant)?,
            channel.updated_at,
        ))
    })
    .transpose()
}
