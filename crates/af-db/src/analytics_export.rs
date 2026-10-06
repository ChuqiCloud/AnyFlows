use std::{fmt, time::Duration};

use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{analytics_export_outbox_events, request_outcome_logs, usage_logs},
};

const FACT_USAGE: i16 = 1;
const FACT_REQUEST_OUTCOME: i16 = 2;
const STATUS_PENDING: i16 = 1;
const STATUS_LEASED: i16 = 2;
const STATUS_PUBLISHED: i16 = 3;
const CLAIM_CANDIDATE_LIMIT: u64 = 32;
const DELIVERY_LEASE_SECONDS: u64 = 120;
const MAX_BACKFILL_BATCH: u64 = 256;
const MAX_REPLAY_BATCH: u64 = 256;

/// 可异步投递到 ClickHouse 的主库事实类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalyticsExportFactKind {
    /// 用量与计费事实。
    UsageLog,
    /// 请求终态事实。
    RequestOutcome,
}

impl AnalyticsExportFactKind {
    const fn database_value(self) -> i16 {
        match self {
            Self::UsageLog => FACT_USAGE,
            Self::RequestOutcome => FACT_REQUEST_OUTCOME,
        }
    }

    fn from_database(value: i16) -> Option<Self> {
        match value {
            FACT_USAGE => Some(Self::UsageLog),
            FACT_REQUEST_OUTCOME => Some(Self::RequestOutcome),
            _ => None,
        }
    }
}

/// 已由数据库版本 CAS 独占领取的一条事实投递租约。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyticsExportLease {
    event_id: i64,
    fact_kind: AnalyticsExportFactKind,
    fact_id: i64,
    attempt_count: i16,
    version: i64,
}

impl AnalyticsExportLease {
    /// 返回 outbox 事件标识。
    #[must_use]
    pub const fn event_id(&self) -> i64 {
        self.event_id
    }

    /// 返回事实类型。
    #[must_use]
    pub const fn fact_kind(&self) -> AnalyticsExportFactKind {
        self.fact_kind
    }

    /// 返回主库事实标识。
    #[must_use]
    pub const fn fact_id(&self) -> i64 {
        self.fact_id
    }

    /// 返回包含本次领取在内的累计尝试次数。
    #[must_use]
    pub const fn attempt_count(&self) -> i16 {
        self.attempt_count
    }
}

/// 投递租约状态推进结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalyticsExportCompletionOutcome {
    /// 当前租约成功推进状态。
    Completed,
    /// 租约已经过期或被其他实例推进。
    Stale,
}

/// ClickHouse 异步事实 outbox 的状态计数快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalyticsExportQueueCounts {
    pending_count: u64,
    leased_count: u64,
    published_count: u64,
}

impl AnalyticsExportQueueCounts {
    /// 构造状态计数快照；计数由数据库聚合后传入。
    #[must_use]
    pub const fn new(pending_count: u64, leased_count: u64, published_count: u64) -> Self {
        Self {
            pending_count,
            leased_count,
            published_count,
        }
    }

    #[must_use]
    pub const fn pending_count(self) -> u64 {
        self.pending_count
    }

    #[must_use]
    pub const fn leased_count(self) -> u64 {
        self.leased_count
    }

    #[must_use]
    pub const fn published_count(self) -> u64 {
        self.published_count
    }

    /// 返回尚未闭合为已发布的事件数量。
    #[must_use]
    pub const fn backlog_count(self) -> u64 {
        self.pending_count.saturating_add(self.leased_count)
    }
}

/// 待写入 ClickHouse 的脱敏事实载荷。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyticsExportFact {
    kind: AnalyticsExportFactKind,
    fact_id: i64,
    deduplication_token: String,
    payload: Value,
}

impl AnalyticsExportFact {
    /// 返回事实类型。
    #[must_use]
    pub const fn kind(&self) -> AnalyticsExportFactKind {
        self.kind
    }

    /// 返回主库事实标识。
    #[must_use]
    pub const fn fact_id(&self) -> i64 {
        self.fact_id
    }

    /// 返回 ClickHouse 稳定去重 token。
    #[must_use]
    pub fn deduplication_token(&self) -> &str {
        &self.deduplication_token
    }

    /// 返回不含用户 token、凭据、正文或 Header 的 JSON 载荷。
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }
}

/// 事实投递 outbox 的数据库错误；不携带业务标识或 SQL 诊断。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AnalyticsExportRepositoryError {
    /// 数据库查询或状态推进失败。
    #[error("分析事实 outbox 数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("分析事实 outbox 数据库操作超时")]
    Timeout,
    /// 持久化状态不满足闭合不变量。
    #[error("分析事实 outbox 状态损坏")]
    Invariant,
}

/// 负责事实入队、租约和事实读取的数据库仓储。
#[derive(Clone)]
pub struct AnalyticsExportRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AnalyticsExportRepository {
    /// 使用显式非零数据库截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AnalyticsExportRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(AnalyticsExportRepositoryError::Invariant);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在事实事务中插入唯一 outbox 指针；重复调用只保留一条待投递事件。
    pub(crate) async fn enqueue_in_transaction(
        transaction: &DatabaseTransaction,
        fact_kind: AnalyticsExportFactKind,
        fact_id: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<(), AnalyticsExportRepositoryError> {
        if fact_id <= 0 {
            return Err(AnalyticsExportRepositoryError::Invariant);
        }
        let result = analytics_export_outbox_events::Entity::insert(
            analytics_export_outbox_events::ActiveModel {
                fact_kind: Set(fact_kind.database_value()),
                fact_id: Set(fact_id),
                status: Set(STATUS_PENDING),
                attempt_count: Set(0),
                next_attempt_at: Set(now),
                lease_expires_at: Set(None),
                published_at: Set(None),
                version: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .on_conflict(
            OnConflict::columns([
                analytics_export_outbox_events::Column::FactKind,
                analytics_export_outbox_events::Column::FactId,
            ])
            .do_nothing_on([
                analytics_export_outbox_events::Column::FactKind,
                analytics_export_outbox_events::Column::FactId,
            ])
            .to_owned(),
        )
        .exec_without_returning(transaction)
        .await;
        result
            .map(|_| ())
            .map_err(|_| AnalyticsExportRepositoryError::Query)
    }

    /// 确保事实已有投递指针，用于修复启用前写入或异常重放造成的缺口。
    pub(crate) async fn ensure_pointer(
        &self,
        fact_kind: AnalyticsExportFactKind,
        fact_id: i64,
    ) -> Result<(), AnalyticsExportRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| AnalyticsExportRepositoryError::Query)?;
        let result = Self::enqueue_in_transaction(
            &transaction,
            fact_kind,
            fact_id,
            TimeDateTimeWithTimeZone::now_utc(),
        )
        .await;
        match result {
            Ok(()) => transaction
                .commit()
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query),
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    /// 领取最早到期的有界 outbox 事件；过期租约允许其他实例重试。
    pub async fn claim_next(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<Option<AnalyticsExportLease>, AnalyticsExportRepositoryError> {
        self.run(self.claim_next_inner(now)).await
    }

    /// 使用数据库统一的 UTC 当前时间领取下一条事件。
    pub async fn claim_next_due(
        &self,
    ) -> Result<Option<AnalyticsExportLease>, AnalyticsExportRepositoryError> {
        self.claim_next(TimeDateTimeWithTimeZone::now_utc()).await
    }

    /// 读取未发布积压数量，供运行时日志和健康观测使用。
    pub async fn backlog_count(&self) -> Result<u64, AnalyticsExportRepositoryError> {
        self.run(async {
            let count = analytics_export_outbox_events::Entity::find()
                .filter(analytics_export_outbox_events::Column::Status.ne(STATUS_PUBLISHED))
                .count(self.pool.connection())
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query)?;
            Ok(count)
        })
        .await
    }

    /// 按状态读取 outbox 计数，供管理员健康面板使用。
    pub async fn queue_counts(
        &self,
    ) -> Result<AnalyticsExportQueueCounts, AnalyticsExportRepositoryError> {
        self.run(async {
            let rows = self
                .pool
                .connection()
                .query_all(Statement::from_string(
                    self.pool.connection().get_database_backend(),
                    "SELECT status, COUNT(*) AS row_count FROM analytics_export_outbox_events GROUP BY status".to_owned(),
                ))
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query)?;
            let mut counts = AnalyticsExportQueueCounts::new(0, 0, 0);
            for row in rows {
                let status = row
                    .try_get::<i16>("", "status")
                    .map_err(|_| AnalyticsExportRepositoryError::Invariant)?;
                let value = row
                    .try_get::<i64>("", "row_count")
                    .map_err(|_| AnalyticsExportRepositoryError::Invariant)?;
                let value = u64::try_from(value)
                    .map_err(|_| AnalyticsExportRepositoryError::Invariant)?;
                counts = match status {
                    STATUS_PENDING => AnalyticsExportQueueCounts::new(
                        value,
                        counts.leased_count,
                        counts.published_count,
                    ),
                    STATUS_LEASED => AnalyticsExportQueueCounts::new(
                        counts.pending_count,
                        value,
                        counts.published_count,
                    ),
                    STATUS_PUBLISHED => AnalyticsExportQueueCounts::new(
                        counts.pending_count,
                        counts.leased_count,
                        value,
                    ),
                    _ => return Err(AnalyticsExportRepositoryError::Invariant),
                };
            }
            Ok(counts)
        })
        .await
    }

    /// 有界重排待处理或已过期租约，使后台 worker 立即重新尝试。
    pub async fn replay(
        &self,
        limit: u64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, AnalyticsExportRepositoryError> {
        let limit = limit.clamp(1, MAX_REPLAY_BATCH);
        self.run(replay_inner(self.pool.clone(), limit, now)).await
    }

    /// 使用数据库统一的 UTC 当前时间重排积压事件。
    pub async fn replay_now(&self, limit: u64) -> Result<u64, AnalyticsExportRepositoryError> {
        self.replay(limit, TimeDateTimeWithTimeZone::now_utc())
            .await
    }

    /// 读取租约对应的事实；主库读取失败时不会推进 outbox 状态。
    pub async fn load_fact(
        &self,
        lease: &AnalyticsExportLease,
    ) -> Result<AnalyticsExportFact, AnalyticsExportRepositoryError> {
        self.run(load_fact_inner(self.pool.clone(), lease.clone()))
            .await
    }

    /// 成功写入 ClickHouse 后按租约版本 CAS 闭合事件。
    pub async fn mark_published(
        &self,
        lease: &AnalyticsExportLease,
        published_at: TimeDateTimeWithTimeZone,
    ) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
        self.run(mark_published_inner(
            self.pool.clone(),
            lease.clone(),
            published_at,
        ))
        .await
    }

    /// 使用数据库统一的 UTC 当前时间确认成功投递。
    pub async fn mark_published_now(
        &self,
        lease: &AnalyticsExportLease,
    ) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
        self.mark_published(lease, TimeDateTimeWithTimeZone::now_utc())
            .await
    }

    /// ClickHouse 请求失败时按有界退避释放租约。
    pub async fn record_failure(
        &self,
        lease: &AnalyticsExportLease,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
        self.run(record_failure_inner(
            self.pool.clone(),
            lease.clone(),
            failed_at,
        ))
        .await
    }

    /// 使用数据库统一的 UTC 当前时间记录失败并释放租约。
    pub async fn record_failure_now(
        &self,
        lease: &AnalyticsExportLease,
    ) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
        self.record_failure(lease, TimeDateTimeWithTimeZone::now_utc())
            .await
    }

    /// 有界扫描两类事实，补齐启用导出前已经落盘的历史事实指针。
    pub async fn backfill_missing(
        &self,
        limit: u64,
    ) -> Result<u64, AnalyticsExportRepositoryError> {
        let limit = limit.clamp(1, MAX_BACKFILL_BATCH);
        self.run(backfill_inner(self.pool.clone(), limit)).await
    }

    async fn run<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, AnalyticsExportRepositoryError>>,
    ) -> Result<T, AnalyticsExportRepositoryError> {
        timeout(self.operation_timeout, future)
            .await
            .map_err(|_| AnalyticsExportRepositoryError::Timeout)?
            .map_err(record_internal_error)
    }

    async fn claim_next_inner(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<Option<AnalyticsExportLease>, AnalyticsExportRepositoryError> {
        let candidates = analytics_export_outbox_events::Entity::find()
            .filter(due_condition(now))
            .order_by_asc(analytics_export_outbox_events::Column::NextAttemptAt)
            .order_by_asc(analytics_export_outbox_events::Column::Id)
            .limit(CLAIM_CANDIDATE_LIMIT)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AnalyticsExportRepositoryError::Query)?;
        for candidate in candidates {
            // 尝试次数只用于退避分类，达到字段上限后保持饱和，不能让一条坏事件永久阻塞队列。
            let attempt_count = candidate.attempt_count.saturating_add(1);
            let version = candidate
                .version
                .checked_add(1)
                .ok_or(AnalyticsExportRepositoryError::Invariant)?;
            let lease_expires_at = now + Duration::from_secs(DELIVERY_LEASE_SECONDS);
            let result = analytics_export_outbox_events::Entity::update_many()
                .col_expr(
                    analytics_export_outbox_events::Column::Status,
                    Expr::value(STATUS_LEASED),
                )
                .col_expr(
                    analytics_export_outbox_events::Column::AttemptCount,
                    Expr::value(attempt_count),
                )
                .col_expr(
                    analytics_export_outbox_events::Column::NextAttemptAt,
                    Expr::value(lease_expires_at),
                )
                .col_expr(
                    analytics_export_outbox_events::Column::LeaseExpiresAt,
                    Expr::value(Some(lease_expires_at)),
                )
                .col_expr(
                    analytics_export_outbox_events::Column::Version,
                    Expr::value(version),
                )
                .col_expr(
                    analytics_export_outbox_events::Column::UpdatedAt,
                    Expr::value(now),
                )
                .filter(analytics_export_outbox_events::Column::Id.eq(candidate.id))
                .filter(analytics_export_outbox_events::Column::Version.eq(candidate.version))
                .filter(due_condition(now))
                .exec(self.pool.connection())
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query)?;
            if result.rows_affected == 1 {
                let fact_kind = AnalyticsExportFactKind::from_database(candidate.fact_kind)
                    .ok_or(AnalyticsExportRepositoryError::Invariant)?;
                return Ok(Some(AnalyticsExportLease {
                    event_id: candidate.id,
                    fact_kind,
                    fact_id: candidate.fact_id,
                    attempt_count,
                    version,
                }));
            }
            if result.rows_affected > 1 {
                return Err(AnalyticsExportRepositoryError::Invariant);
            }
        }
        Ok(None)
    }
}

impl fmt::Debug for AnalyticsExportRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AnalyticsExportRepository(<redacted>)")
    }
}

fn due_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(analytics_export_outbox_events::Column::Status.eq(STATUS_PENDING))
                .add(analytics_export_outbox_events::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(analytics_export_outbox_events::Column::Status.eq(STATUS_LEASED))
                .add(analytics_export_outbox_events::Column::NextAttemptAt.lte(now))
                .add(analytics_export_outbox_events::Column::LeaseExpiresAt.lte(now)),
        )
}

fn replay_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(analytics_export_outbox_events::Column::Status.eq(STATUS_PENDING))
        .add(
            Condition::all()
                .add(analytics_export_outbox_events::Column::Status.eq(STATUS_LEASED))
                .add(analytics_export_outbox_events::Column::LeaseExpiresAt.lte(now)),
        )
}

async fn replay_inner(
    pool: DatabasePool,
    limit: u64,
    now: TimeDateTimeWithTimeZone,
) -> Result<u64, AnalyticsExportRepositoryError> {
    let transaction = pool
        .connection()
        .begin()
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    let candidates = analytics_export_outbox_events::Entity::find()
        .filter(replay_condition(now))
        .order_by_asc(analytics_export_outbox_events::Column::Id)
        .limit(limit)
        .all(&transaction)
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    let mut replayed = 0_u64;
    for candidate in candidates {
        let version = candidate
            .version
            .checked_add(1)
            .ok_or(AnalyticsExportRepositoryError::Invariant)?;
        let result = analytics_export_outbox_events::Entity::update_many()
            .col_expr(
                analytics_export_outbox_events::Column::Status,
                Expr::value(STATUS_PENDING),
            )
            .col_expr(
                analytics_export_outbox_events::Column::NextAttemptAt,
                Expr::value(now),
            )
            .col_expr(
                analytics_export_outbox_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                analytics_export_outbox_events::Column::Version,
                Expr::value(version),
            )
            .col_expr(
                analytics_export_outbox_events::Column::UpdatedAt,
                Expr::value(now),
            )
            .filter(analytics_export_outbox_events::Column::Id.eq(candidate.id))
            .filter(analytics_export_outbox_events::Column::Version.eq(candidate.version))
            .filter(replay_condition(now))
            .exec(&transaction)
            .await
            .map_err(|_| AnalyticsExportRepositoryError::Query)?;
        if result.rows_affected == 1 {
            replayed = replayed.saturating_add(1);
        } else if result.rows_affected > 1 {
            return Err(AnalyticsExportRepositoryError::Invariant);
        }
    }
    transaction
        .commit()
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    Ok(replayed)
}

async fn mark_published_inner(
    pool: DatabasePool,
    lease: AnalyticsExportLease,
    at: TimeDateTimeWithTimeZone,
) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
    let version = lease
        .version
        .checked_add(1)
        .ok_or(AnalyticsExportRepositoryError::Invariant)?;
    let result = analytics_export_outbox_events::Entity::update_many()
        .col_expr(
            analytics_export_outbox_events::Column::Status,
            Expr::value(STATUS_PUBLISHED),
        )
        .col_expr(
            analytics_export_outbox_events::Column::NextAttemptAt,
            Expr::value(at),
        )
        .col_expr(
            analytics_export_outbox_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            analytics_export_outbox_events::Column::PublishedAt,
            Expr::value(Some(at)),
        )
        .col_expr(
            analytics_export_outbox_events::Column::Version,
            Expr::value(version),
        )
        .col_expr(
            analytics_export_outbox_events::Column::UpdatedAt,
            Expr::value(at),
        )
        .filter(analytics_export_outbox_events::Column::Id.eq(lease.event_id))
        .filter(analytics_export_outbox_events::Column::Status.eq(STATUS_LEASED))
        .filter(analytics_export_outbox_events::Column::Version.eq(lease.version))
        .exec(pool.connection())
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    completion_outcome(result.rows_affected)
}

async fn record_failure_inner(
    pool: DatabasePool,
    lease: AnalyticsExportLease,
    at: TimeDateTimeWithTimeZone,
) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
    let version = lease
        .version
        .checked_add(1)
        .ok_or(AnalyticsExportRepositoryError::Invariant)?;
    let next_attempt_at = at + retry_delay(lease.attempt_count)?;
    let result = analytics_export_outbox_events::Entity::update_many()
        .col_expr(
            analytics_export_outbox_events::Column::Status,
            Expr::value(STATUS_PENDING),
        )
        .col_expr(
            analytics_export_outbox_events::Column::NextAttemptAt,
            Expr::value(next_attempt_at),
        )
        .col_expr(
            analytics_export_outbox_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            analytics_export_outbox_events::Column::Version,
            Expr::value(version),
        )
        .col_expr(
            analytics_export_outbox_events::Column::UpdatedAt,
            Expr::value(at),
        )
        .filter(analytics_export_outbox_events::Column::Id.eq(lease.event_id))
        .filter(analytics_export_outbox_events::Column::Status.eq(STATUS_LEASED))
        .filter(analytics_export_outbox_events::Column::Version.eq(lease.version))
        .exec(pool.connection())
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    completion_outcome(result.rows_affected)
}

async fn load_fact_inner(
    pool: DatabasePool,
    lease: AnalyticsExportLease,
) -> Result<AnalyticsExportFact, AnalyticsExportRepositoryError> {
    let payload = match lease.fact_kind {
        AnalyticsExportFactKind::UsageLog => {
            let row = usage_logs::Entity::find_by_id(lease.fact_id)
                .one(pool.connection())
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query)?
                .ok_or(AnalyticsExportRepositoryError::Invariant)?;
            json!({
                "fact_id": row.id,
                "kind": "usage_log",
                "event_type": row.event_type,
                "group_id": row.group_id,
                "organization_id": row.organization_id,
                "organization_team_id": row.organization_team_id,
                "billing_mode": row.billing_mode,
                "input_tokens": row.input_tokens,
                "output_tokens": row.output_tokens,
                "cache_read": row.cache_read,
                "cache_creation_5m": row.cache_creation_5m,
                "cache_creation_1h": row.cache_creation_1h,
                "reasoning_tokens": row.reasoning_tokens,
                "audio_input_tokens": row.audio_input_tokens,
                "audio_output_tokens": row.audio_output_tokens,
                "audio_duration_nanoseconds": row.audio_duration_nanoseconds,
                "video_duration_seconds": row.video_duration_seconds,
                "video_resolution": row.video_resolution,
                "model": row.model,
                "protocol": row.protocol,
                "operation": row.operation,
                "is_stream": row.is_stream,
                "reasoning_effort": row.reasoning_effort,
                "reasoning_budget_tokens": row.reasoning_budget_tokens,
                "first_token_ms": row.first_token_ms,
                "duration_ms": row.duration_ms,
                "usage_source": row.usage_source,
                "usage_semantics": row.usage_semantics,
                "quota": row.quota,
                "created_at": row.created_at.to_string(),
            })
        }
        AnalyticsExportFactKind::RequestOutcome => {
            let row = request_outcome_logs::Entity::find_by_id(lease.fact_id)
                .one(pool.connection())
                .await
                .map_err(|_| AnalyticsExportRepositoryError::Query)?
                .ok_or(AnalyticsExportRepositoryError::Invariant)?;
            json!({
                "fact_id": row.id,
                "kind": "request_outcome",
                "protocol": row.protocol,
                "operation": row.operation,
                "model": row.model,
                "outcome": row.outcome,
                "error_kind": row.error_kind,
                "channel_id": row.channel_id,
                "duration_ms": row.duration_ms,
                "created_at": row.created_at.to_string(),
            })
        }
    };
    Ok(AnalyticsExportFact {
        kind: lease.fact_kind,
        fact_id: lease.fact_id,
        deduplication_token: deduplication_token(lease.fact_kind, lease.fact_id),
        payload,
    })
}

async fn backfill_inner(
    pool: DatabasePool,
    limit: u64,
) -> Result<u64, AnalyticsExportRepositoryError> {
    let transaction = pool
        .connection()
        .begin()
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    let mut inserted = 0_u64;
    for (table, kind) in [
        ("usage_logs", AnalyticsExportFactKind::UsageLog),
        (
            "request_outcome_logs",
            AnalyticsExportFactKind::RequestOutcome,
        ),
    ] {
        let remaining = limit.saturating_sub(inserted);
        if remaining == 0 {
            break;
        }
        let sql = format!(
            "SELECT fact_row.id FROM {table} AS fact_row WHERE NOT EXISTS (SELECT 1 FROM analytics_export_outbox_events AS outbox_row WHERE outbox_row.fact_kind = {} AND outbox_row.fact_id = fact_row.id) ORDER BY fact_row.id LIMIT {remaining}",
            kind.database_value()
        );
        let rows = transaction
            .query_all(Statement::from_string(
                transaction.get_database_backend(),
                sql,
            ))
            .await
            .map_err(|_| AnalyticsExportRepositoryError::Query)?;
        for row in rows {
            let fact_id = row
                .try_get::<i64>("", "id")
                .map_err(|_| AnalyticsExportRepositoryError::Invariant)?;
            AnalyticsExportRepository::enqueue_in_transaction(
                &transaction,
                kind,
                fact_id,
                TimeDateTimeWithTimeZone::now_utc(),
            )
            .await?;
            inserted = inserted.saturating_add(1);
        }
    }
    transaction
        .commit()
        .await
        .map_err(|_| AnalyticsExportRepositoryError::Query)?;
    Ok(inserted)
}

fn deduplication_token(kind: AnalyticsExportFactKind, fact_id: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update([kind.database_value() as u8]);
    hasher.update(fact_id.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

fn retry_delay(attempt_count: i16) -> Result<Duration, AnalyticsExportRepositoryError> {
    let seconds = match attempt_count {
        1 => 1,
        2 => 2,
        3 => 5,
        4 => 10,
        5 => 30,
        value if value > 5 => 60,
        _ => return Err(AnalyticsExportRepositoryError::Invariant),
    };
    Ok(Duration::from_secs(seconds))
}

fn completion_outcome(
    rows_affected: u64,
) -> Result<AnalyticsExportCompletionOutcome, AnalyticsExportRepositoryError> {
    match rows_affected {
        0 => Ok(AnalyticsExportCompletionOutcome::Stale),
        1 => Ok(AnalyticsExportCompletionOutcome::Completed),
        _ => Err(AnalyticsExportRepositoryError::Invariant),
    }
}

fn record_internal_error(error: AnalyticsExportRepositoryError) -> AnalyticsExportRepositoryError {
    let kind = match error {
        AnalyticsExportRepositoryError::Query => "analytics_export_query",
        AnalyticsExportRepositoryError::Timeout => "analytics_export_timeout",
        AnalyticsExportRepositoryError::Invariant => "analytics_export_invariant",
    };
    tracing::error!(target: "af_db::analytics_export", error_kind = kind, "分析事实 outbox 仓储发生内部错误");
    error
}
