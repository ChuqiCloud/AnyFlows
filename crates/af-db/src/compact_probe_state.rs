use std::time::Duration;

use af_domain::{ChannelId, ResponsesCompactProbeResult, Status};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    ChannelProbeTargetRevision, DatabasePool, SchedulerCatalogSubject,
    channel_settings::{responses_compact_probe_record, set_responses_compact_probe_record},
    entity::{SensitiveJson, channels},
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Compact 探测事实写回的幂等结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactProbeStateWriteOutcome {
    /// 事实已在配置版本未变化时写入。
    Recorded,
    /// 渠道配置已变化，旧探测被拒绝。
    Stale,
    /// 渠道已不存在或删除。
    NotEligible,
}

/// Compact 探测事实持久化失败。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CompactProbeStateRepositoryError {
    /// 写入截止时间配置为零。
    #[error("Compact 探测事实写入超时必须大于零")]
    InvalidConfiguration,
    /// 不能把未知状态或非法状态码写成确定性事实。
    #[error("Compact 探测事实输入无效")]
    InvalidInput,
    /// 获取连接或事务写入失败。
    #[error("写入 Compact 探测事实失败")]
    Query,
    /// 写入超过硬截止时间。
    #[error("写入 Compact 探测事实超时")]
    Timeout,
    /// 数据库状态违反持久化不变量。
    #[error("Compact 探测事实持久化状态损坏")]
    Invariant,
}

/// 以渠道配置时间戳为 CAS 条件写入 Compact 探测事实。
#[derive(Clone)]
pub struct CompactProbeStateRepository {
    pool: DatabasePool,
    write_timeout: Duration,
}

impl CompactProbeStateRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_write_timeout(
        pool: DatabasePool,
        write_timeout: Duration,
    ) -> Result<Self, CompactProbeStateRepositoryError> {
        if write_timeout.is_zero() {
            return Err(CompactProbeStateRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            write_timeout,
        })
    }

    /// 仅在探活开始时的渠道版本仍然有效时写入确定性结论。
    pub async fn record(
        &self,
        channel_id: ChannelId,
        revision: &ChannelProbeTargetRevision,
        result: ResponsesCompactProbeResult,
        checked_at: i64,
        http_status: Option<u16>,
    ) -> Result<CompactProbeStateWriteOutcome, CompactProbeStateRepositoryError> {
        if result == ResponsesCompactProbeResult::Unknown
            || checked_at <= 0
            || http_status.is_some_and(|status| !(100..=599).contains(&status))
        {
            return Err(CompactProbeStateRepositoryError::InvalidInput);
        }
        let expected_updated_at = revision.timestamp();
        let operation = async {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| CompactProbeStateRepositoryError::Query)?;
            // 先以不改变审计时间的写语句取得行锁，串行化同一渠道的探测与管理更新。
            channels::Entity::update_many()
                .col_expr(
                    channels::Column::UpdatedAt,
                    Expr::col(channels::Column::UpdatedAt).into(),
                )
                .filter(channels::Column::Id.eq(channel_id.get()))
                .filter(channels::Column::DeletedAt.is_null())
                .exec(&transaction)
                .await
                .map_err(|_| CompactProbeStateRepositoryError::Query)?;
            let Some(channel) = channels::Entity::find_by_id(channel_id.get())
                .filter(channels::Column::DeletedAt.is_null())
                .one(&transaction)
                .await
                .map_err(|_| CompactProbeStateRepositoryError::Query)?
            else {
                return Ok(CompactProbeStateWriteOutcome::NotEligible);
            };
            Status::try_from(channel.status)
                .map_err(|_| CompactProbeStateRepositoryError::Invariant)?;
            if channel.updated_at != expected_updated_at {
                return Ok(CompactProbeStateWriteOutcome::Stale);
            }
            let mut settings = channel.settings.into_inner();
            if responses_compact_probe_record(&settings)
                .map_err(|_| CompactProbeStateRepositoryError::Invariant)?
                .is_some_and(|record| record.checked_at() >= checked_at)
            {
                return Ok(CompactProbeStateWriteOutcome::Stale);
            }
            set_responses_compact_probe_record(&mut settings, result, checked_at, http_status)
                .map_err(|_| CompactProbeStateRepositoryError::Invariant)?;
            let now = TimeDateTimeWithTimeZone::now_utc();
            let update = channels::Entity::update_many()
                .col_expr(
                    channels::Column::Settings,
                    Expr::value(SensitiveJson::from(settings)),
                )
                // 不修改 updated_at；自动禁用恢复租约仍绑定原始渠道状态版本。
                .filter(channels::Column::Id.eq(channel_id.get()))
                .filter(channels::Column::DeletedAt.is_null())
                .exec(&transaction)
                .await
                .map_err(|_| CompactProbeStateRepositoryError::Query)?;
            // 行锁后已用数据库读回值完成 CAS，避免 SQLite 时间文本与绑定参数格式差异。
            if update.rows_affected != 1 {
                return Err(CompactProbeStateRepositoryError::Invariant);
            }
            enqueue_scheduler_catalog_change(
                &transaction,
                SchedulerCatalogSubject::Channel(channel_id),
                now,
            )
            .await
            .map_err(|_| CompactProbeStateRepositoryError::Query)?;
            transaction
                .commit()
                .await
                .map_err(|_| CompactProbeStateRepositoryError::Query)?;
            Ok(CompactProbeStateWriteOutcome::Recorded)
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.write_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                CompactProbeStateRepositoryError::Timeout,
            )),
        }
    }
}

impl std::fmt::Debug for CompactProbeStateRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompactProbeStateRepository")
            .field("write_timeout", &self.write_timeout)
            .finish_non_exhaustive()
    }
}

fn record_internal_error(
    error: CompactProbeStateRepositoryError,
) -> CompactProbeStateRepositoryError {
    let error_kind = match error {
        CompactProbeStateRepositoryError::InvalidConfiguration
        | CompactProbeStateRepositoryError::InvalidInput => return error,
        CompactProbeStateRepositoryError::Query => "compact_probe_state_query",
        CompactProbeStateRepositoryError::Timeout => "compact_probe_state_timeout",
        CompactProbeStateRepositoryError::Invariant => "compact_probe_state_invariant",
    };
    tracing::error!(
        target: "af_db::compact_probe_state",
        error_kind,
        "Compact 探活事实仓储发生内部错误"
    );
    error
}
