use std::{fmt, time::Duration};

use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{STATUS_LEASED, STATUS_PENDING, STATUS_PUBLISHED, SchedulerCatalogSubject};
use crate::{DatabasePool, entity::scheduler_outbox_events};

const CLAIM_CANDIDATE_LIMIT: u64 = 16;
const DELIVERY_LEASE_SECONDS: u64 = 120;

/// 已由数据库版本 CAS 独占领取的一条调度目录变更事件。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerOutboxLease {
    event_id: i64,
    subject: SchedulerCatalogSubject,
    attempt_count: i16,
    version: i64,
}

impl SchedulerOutboxLease {
    /// 返回全局单调的 outbox 事件标识。
    #[must_use]
    pub const fn event_id(&self) -> i64 {
        self.event_id
    }

    /// 返回本次失效广播对应的闭合调度主体。
    #[must_use]
    pub const fn subject(&self) -> SchedulerCatalogSubject {
        self.subject
    }

    /// 返回包含本次领取在内的累计投递次数。
    #[must_use]
    pub const fn attempt_count(&self) -> i16 {
        self.attempt_count
    }
}

impl fmt::Debug for SchedulerOutboxLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let subject_kind = match self.subject {
            SchedulerCatalogSubject::Channel(_) => "channel",
            SchedulerCatalogSubject::Group(_) => "group",
        };
        formatter
            .debug_struct("SchedulerOutboxLease")
            .field("event_id", &self.event_id)
            .field("subject_kind", &subject_kind)
            .field("attempt_count", &self.attempt_count)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 领取下一条到期 outbox 事件的结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerOutboxClaimOutcome {
    /// 当前实例取得了独占数据库租约。
    Claimed(SchedulerOutboxLease),
    /// 当前没有可安全领取的事件。
    Empty,
}

/// 按租约版本推进投递结果的 CAS 结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerOutboxCompletionOutcome {
    /// 当前租约完成了状态推进。
    Completed,
    /// 租约已过期或被其他实例推进，当前结果不得覆盖新事实。
    Stale,
}

/// 调度 outbox 仓储构造错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerOutboxRepositoryConfigError {
    /// 零截止时间会绕过数据库操作边界。
    #[error("调度 outbox 数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 调度 outbox 仓储错误；不携带主体、模型或数据库诊断。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerOutboxRepositoryError {
    /// 查询、领取或状态推进失败。
    #[error("调度 outbox 数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("调度 outbox 数据库操作超时")]
    Timeout,
    /// 事件主体、版本或状态违反持久化不变量。
    #[error("调度 outbox 持久化状态损坏")]
    Invariant,
}

/// 负责调度目录 outbox 租约领取、发布确认和失败退避的数据库仓储。
#[derive(Clone, Debug)]
pub struct SchedulerOutboxRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl SchedulerOutboxRepository {
    /// 使用显式非零数据库截止时间创建仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, SchedulerOutboxRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(SchedulerOutboxRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 领取最早到期事件；并发实例通过版本 CAS 竞争，过期租约允许重新领取。
    pub async fn claim_next(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxClaimOutcome, SchedulerOutboxRepositoryError> {
        match timeout(self.operation_timeout, self.claim_next_inner(now)).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerOutboxRepositoryError::Timeout,
            )),
        }
    }

    /// 读取当前已经提交的最大事件标识；空表返回零。
    ///
    /// 全量快照必须先读取该高水位再读取业务目录。这样高水位以内的事务一定已经提交，
    /// 随后的目录查询至少能看到这些变更，迟到投影便不能覆盖更新的全量事实。
    pub async fn latest_event_id(&self) -> Result<u64, SchedulerOutboxRepositoryError> {
        match timeout(self.operation_timeout, self.latest_event_id_inner()).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerOutboxRepositoryError::Timeout,
            )),
        }
    }

    /// 仅在租约版本仍为当前事实时，把已经广播的事件闭合为已发布。
    pub async fn mark_published(
        &self,
        lease: &SchedulerOutboxLease,
        published_at: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxCompletionOutcome, SchedulerOutboxRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.mark_published_inner(lease, published_at),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerOutboxRepositoryError::Timeout,
            )),
        }
    }

    /// 记录一次广播失败，并按有界退避把当前租约退回待投递状态。
    pub async fn record_failure(
        &self,
        lease: &SchedulerOutboxLease,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxCompletionOutcome, SchedulerOutboxRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.record_failure_inner(lease, failed_at),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerOutboxRepositoryError::Timeout,
            )),
        }
    }

    async fn claim_next_inner(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxClaimOutcome, SchedulerOutboxRepositoryError> {
        let candidates = scheduler_outbox_events::Entity::find()
            .filter(due_condition(now))
            .order_by_asc(scheduler_outbox_events::Column::NextAttemptAt)
            .order_by_asc(scheduler_outbox_events::Column::Id)
            .limit(CLAIM_CANDIDATE_LIMIT)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SchedulerOutboxRepositoryError::Query)?;

        for candidate in candidates {
            let next_attempt_count = candidate.attempt_count.saturating_add(1);
            let next_version = candidate
                .version
                .checked_add(1)
                .ok_or(SchedulerOutboxRepositoryError::Invariant)?;
            let lease_expires_at = now + Duration::from_secs(DELIVERY_LEASE_SECONDS);
            let result = scheduler_outbox_events::Entity::update_many()
                .col_expr(
                    scheduler_outbox_events::Column::Status,
                    Expr::value(STATUS_LEASED),
                )
                .col_expr(
                    scheduler_outbox_events::Column::AttemptCount,
                    Expr::value(next_attempt_count),
                )
                // 租约到期时间同时作为下一次扫描时间，保持到期索引可用。
                .col_expr(
                    scheduler_outbox_events::Column::NextAttemptAt,
                    Expr::value(lease_expires_at),
                )
                .col_expr(
                    scheduler_outbox_events::Column::LeaseExpiresAt,
                    Expr::value(Some(lease_expires_at)),
                )
                .col_expr(
                    scheduler_outbox_events::Column::Version,
                    Expr::value(next_version),
                )
                .col_expr(
                    scheduler_outbox_events::Column::UpdatedAt,
                    Expr::value(now),
                )
                .filter(scheduler_outbox_events::Column::Id.eq(candidate.id))
                .filter(scheduler_outbox_events::Column::Version.eq(candidate.version))
                .filter(due_condition(now))
                .exec(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| SchedulerOutboxRepositoryError::Query)?;
            if result.rows_affected == 0 {
                continue;
            }
            if result.rows_affected != 1 {
                return Err(SchedulerOutboxRepositoryError::Invariant);
            }
            let subject = SchedulerCatalogSubject::try_from_parts(
                candidate.subject_kind,
                candidate.subject_id,
            )
            .map_err(|_| SchedulerOutboxRepositoryError::Invariant)?;
            return Ok(SchedulerOutboxClaimOutcome::Claimed(SchedulerOutboxLease {
                event_id: candidate.id,
                subject,
                attempt_count: next_attempt_count,
                version: next_version,
            }));
        }
        Ok(SchedulerOutboxClaimOutcome::Empty)
    }

    async fn latest_event_id_inner(&self) -> Result<u64, SchedulerOutboxRepositoryError> {
        let event = scheduler_outbox_events::Entity::find()
            .order_by_desc(scheduler_outbox_events::Column::Id)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SchedulerOutboxRepositoryError::Query)?;
        event.map_or(Ok(0), |event| {
            u64::try_from(event.id).map_err(|_| SchedulerOutboxRepositoryError::Invariant)
        })
    }

    async fn mark_published_inner(
        &self,
        lease: &SchedulerOutboxLease,
        published_at: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxCompletionOutcome, SchedulerOutboxRepositoryError> {
        let next_version = lease
            .version
            .checked_add(1)
            .ok_or(SchedulerOutboxRepositoryError::Invariant)?;
        let result = scheduler_outbox_events::Entity::update_many()
            .col_expr(
                scheduler_outbox_events::Column::Status,
                Expr::value(STATUS_PUBLISHED),
            )
            .col_expr(
                scheduler_outbox_events::Column::NextAttemptAt,
                Expr::value(published_at),
            )
            .col_expr(
                scheduler_outbox_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                scheduler_outbox_events::Column::PublishedAt,
                Expr::value(Some(published_at)),
            )
            .col_expr(
                scheduler_outbox_events::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                scheduler_outbox_events::Column::UpdatedAt,
                Expr::value(published_at),
            )
            .filter(scheduler_outbox_events::Column::Id.eq(lease.event_id))
            .filter(scheduler_outbox_events::Column::Status.eq(STATUS_LEASED))
            .filter(scheduler_outbox_events::Column::Version.eq(lease.version))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SchedulerOutboxRepositoryError::Query)?;
        completion_outcome(result.rows_affected)
    }

    async fn record_failure_inner(
        &self,
        lease: &SchedulerOutboxLease,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<SchedulerOutboxCompletionOutcome, SchedulerOutboxRepositoryError> {
        let next_version = lease
            .version
            .checked_add(1)
            .ok_or(SchedulerOutboxRepositoryError::Invariant)?;
        let next_attempt_at = failed_at + retry_delay(lease.attempt_count)?;
        let result = scheduler_outbox_events::Entity::update_many()
            .col_expr(
                scheduler_outbox_events::Column::Status,
                Expr::value(STATUS_PENDING),
            )
            .col_expr(
                scheduler_outbox_events::Column::NextAttemptAt,
                Expr::value(next_attempt_at),
            )
            .col_expr(
                scheduler_outbox_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                scheduler_outbox_events::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                scheduler_outbox_events::Column::UpdatedAt,
                Expr::value(failed_at),
            )
            .filter(scheduler_outbox_events::Column::Id.eq(lease.event_id))
            .filter(scheduler_outbox_events::Column::Status.eq(STATUS_LEASED))
            .filter(scheduler_outbox_events::Column::Version.eq(lease.version))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SchedulerOutboxRepositoryError::Query)?;
        completion_outcome(result.rows_affected)
    }
}

fn due_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(scheduler_outbox_events::Column::Status.eq(STATUS_PENDING))
                .add(scheduler_outbox_events::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(scheduler_outbox_events::Column::Status.eq(STATUS_LEASED))
                .add(scheduler_outbox_events::Column::NextAttemptAt.lte(now))
                .add(scheduler_outbox_events::Column::LeaseExpiresAt.lte(now)),
        )
}

fn retry_delay(attempt_count: i16) -> Result<Duration, SchedulerOutboxRepositoryError> {
    let seconds = match attempt_count {
        1 => 1,
        2 => 2,
        3 => 5,
        4 => 10,
        5 => 30,
        value if value > 5 => 60,
        _ => return Err(SchedulerOutboxRepositoryError::Invariant),
    };
    Ok(Duration::from_secs(seconds))
}

fn completion_outcome(
    rows_affected: u64,
) -> Result<SchedulerOutboxCompletionOutcome, SchedulerOutboxRepositoryError> {
    match rows_affected {
        0 => Ok(SchedulerOutboxCompletionOutcome::Stale),
        1 => Ok(SchedulerOutboxCompletionOutcome::Completed),
        _ => Err(SchedulerOutboxRepositoryError::Invariant),
    }
}

fn record_internal_error(error: SchedulerOutboxRepositoryError) -> SchedulerOutboxRepositoryError {
    let error_kind = match error {
        SchedulerOutboxRepositoryError::Query => "scheduler_outbox_query",
        SchedulerOutboxRepositoryError::Timeout => "scheduler_outbox_timeout",
        SchedulerOutboxRepositoryError::Invariant => "scheduler_outbox_invariant",
    };
    tracing::error!(
        target: "af_db::scheduler_outbox",
        error_kind,
        "调度 outbox 仓储发生内部错误"
    );
    error
}
