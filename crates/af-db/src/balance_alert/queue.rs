use std::{fmt, time::Duration};

use af_domain::{Quota, UserId};
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Func, OnConflict, Query},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::BalanceAlertSettingsRecord;
use crate::{
    DatabasePool,
    entity::{balance_alert_events, users},
    notification::{
        NotificationDeliveryState, NotificationKind, UserNotificationWrite, update_delivery_state,
    },
};

const STATUS_PENDING: i16 = 1;
const STATUS_SENDING: i16 = 2;
const STATUS_SENT: i16 = 3;
const STATUS_FAILED: i16 = 4;
const STATUS_CANCELED: i16 = 5;
const ENABLED_USER_STATUS: i16 = 1;
const DELIVERY_LEASE_SECONDS: u64 = 120;
const CLAIM_CANDIDATE_LIMIT: u64 = 16;
pub const MAX_BALANCE_ALERT_BATCH_SIZE: usize = 100;
pub const MAX_BALANCE_ALERT_ATTEMPTS: i16 = 5;

/// SMTP 投递失败的持久化分类；不保存服务器响应正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceAlertDeliveryFailureKind {
    Timeout,
    Transport,
    Configuration,
}

impl BalanceAlertDeliveryFailureKind {
    const fn code(self) -> i16 {
        match self {
            Self::Timeout => 1,
            Self::Transport => 2,
            Self::Configuration => 3,
        }
    }

    const fn terminal(self) -> bool {
        matches!(self, Self::Configuration)
    }
}

/// 已由版本 CAS 独占领取的一次余额预警投递。
#[derive(Clone, Eq, PartialEq)]
pub struct BalanceAlertDeliveryLease {
    event_id: i64,
    user_id: UserId,
    recipient: String,
    username: String,
    current_quota: Quota,
    threshold: Quota,
    attempt_count: i16,
    version: i64,
    notification_source_key: String,
}

impl BalanceAlertDeliveryLease {
    #[must_use]
    pub const fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub const fn current_quota(&self) -> Quota {
        self.current_quota
    }

    #[must_use]
    pub const fn threshold(&self) -> Quota {
        self.threshold
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i16 {
        self.attempt_count
    }
}

impl fmt::Debug for BalanceAlertDeliveryLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BalanceAlertDeliveryLease")
            .field("event_id", &self.event_id)
            .field("user_id", &self.user_id)
            .field("recipient", &"<已脱敏>")
            .field("username", &"<已脱敏>")
            .field("attempt_count", &self.attempt_count)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 单轮低余额候选发现结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BalanceAlertEnqueueReport {
    eligible: usize,
}

impl BalanceAlertEnqueueReport {
    #[must_use]
    pub const fn eligible(self) -> usize {
        self.eligible
    }
}

/// 领取下一条投递事件的结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceAlertClaimOutcome {
    Claimed(BalanceAlertDeliveryLease),
    Skipped,
    Empty,
}

/// 以租约版本结束一次投递的 CAS 结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceAlertCompletionOutcome {
    Completed,
    Stale,
}

/// 余额预警事件仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BalanceAlertRepositoryConfigError {
    #[error("余额预警事件数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 余额预警事件仓储错误；不携带邮箱或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BalanceAlertRepositoryError {
    #[error("余额预警事件批次大小无效")]
    InvalidBatchSize,
    #[error("余额预警事件数据库操作失败")]
    Query,
    #[error("余额预警事件数据库操作超时")]
    Timeout,
    #[error("余额预警事件持久化状态损坏")]
    Invariant,
}

/// 负责低余额候选去重、租约领取和投递结果推进的数据库仓储。
#[derive(Clone)]
pub struct BalanceAlertRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl BalanceAlertRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, BalanceAlertRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(BalanceAlertRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 扫描一批低余额用户，并按用户与提醒窗口唯一键写入待投递事件。
    pub async fn enqueue_due(
        &self,
        settings: BalanceAlertSettingsRecord,
        now: TimeDateTimeWithTimeZone,
        limit: usize,
    ) -> Result<BalanceAlertEnqueueReport, BalanceAlertRepositoryError> {
        if limit == 0 || limit > MAX_BALANCE_ALERT_BATCH_SIZE {
            return Err(BalanceAlertRepositoryError::InvalidBatchSize);
        }
        match timeout(
            self.operation_timeout,
            self.enqueue_due_inner(settings, now, limit),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    /// 领取当前窗口的一条到期事件；失效用户会在事务内取消而不返回身份信息。
    pub async fn claim_next(
        &self,
        settings: BalanceAlertSettingsRecord,
        window_started_at_epoch: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertClaimOutcome, BalanceAlertRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.claim_next_inner(settings, window_started_at_epoch, now),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    /// 取消旧提醒窗口中尚未完成的事件，避免停机恢复后发送过期提醒。
    pub async fn cancel_stale(
        &self,
        current_window_started_at_epoch: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, BalanceAlertRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.cancel_stale_inner(current_window_started_at_epoch, now),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    /// 把已经耗尽五次尝试且不再持有有效租约的事件闭合为失败。
    pub async fn fail_exhausted(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, BalanceAlertRepositoryError> {
        match timeout(self.operation_timeout, self.fail_exhausted_inner(now)).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    /// 按租约版本把投递闭合为成功。
    pub async fn mark_sent(
        &self,
        lease: &BalanceAlertDeliveryLease,
        sent_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, BalanceAlertRepositoryError> {
        match timeout(self.operation_timeout, self.mark_sent_inner(lease, sent_at)).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    /// 记录归一化失败；可重试错误退回待处理，配置错误或第五次失败闭合终态。
    pub async fn record_failure(
        &self,
        lease: &BalanceAlertDeliveryLease,
        failure: BalanceAlertDeliveryFailureKind,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, BalanceAlertRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.record_failure_inner(lease, failure, failed_at),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(BalanceAlertRepositoryError::Timeout)),
        }
    }

    async fn enqueue_due_inner(
        &self,
        settings: BalanceAlertSettingsRecord,
        now: TimeDateTimeWithTimeZone,
        limit: usize,
    ) -> Result<BalanceAlertEnqueueReport, BalanceAlertRepositoryError> {
        if !settings.enabled() {
            return Ok(BalanceAlertEnqueueReport::default());
        }
        let window_started_at_epoch = settings
            .window_started_at_epoch(now)
            .map_err(|_| BalanceAlertRepositoryError::Invariant)?;
        let default_threshold = settings.default_threshold().units();
        let effective_threshold = Func::coalesce([
            Expr::col(users::Column::BalanceAlertThreshold).into(),
            Expr::value(default_threshold),
        ]);
        let limit = u64::try_from(limit).map_err(|_| BalanceAlertRepositoryError::Invariant)?;
        let transaction = begin_transaction(&self.pool).await?;
        let mut queued_users = Query::select();
        queued_users
            .column(balance_alert_events::Column::UserId)
            .from(balance_alert_events::Entity)
            .and_where(
                Expr::col(balance_alert_events::Column::WindowStartedAtEpoch)
                    .eq(window_started_at_epoch),
            );
        let candidates = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .column(users::Column::Quota)
            .expr_as(
                effective_threshold.clone(),
                users::Column::BalanceAlertThreshold,
            )
            .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::Email.is_not_null())
            .filter(users::Column::Email.ne(""))
            .filter(users::Column::EmailUsageAlerts.eq(true))
            .filter(Expr::col(users::Column::Quota).lt(effective_threshold))
            // 在 LIMIT 前排除本窗口已有事件，避免首批用户长期饿死后续候选。
            .filter(Expr::col(users::Column::Id).not_in_subquery(queued_users))
            .order_by_asc(users::Column::Id)
            .limit(limit)
            .into_tuple::<(i64, i64, i64)>()
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        for (user_id, observed_quota, threshold_quota) in &candidates {
            balance_alert_events::Entity::insert(balance_alert_events::ActiveModel {
                id: sea_orm::NotSet,
                user_id: Set(*user_id),
                window_started_at_epoch: Set(window_started_at_epoch),
                threshold_quota: Set(*threshold_quota),
                observed_quota: Set(*observed_quota),
                status: Set(STATUS_PENDING),
                attempt_count: Set(0),
                next_attempt_at: Set(now),
                lease_expires_at: Set(None),
                last_error_kind: Set(None),
                version: Set(1),
                sent_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            })
            .on_conflict(balance_alert_on_conflict())
            .exec_without_returning(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        }
        for (user_id, observed_quota, threshold_quota) in &candidates {
            crate::notification::insert_queued(
                &transaction,
                &UserNotificationWrite::balance_alert(
                    *user_id,
                    window_started_at_epoch,
                    *observed_quota,
                    *threshold_quota,
                    now,
                ),
            )
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        }
        commit_transaction(transaction).await?;
        Ok(BalanceAlertEnqueueReport {
            eligible: candidates.len(),
        })
    }

    async fn claim_next_inner(
        &self,
        settings: BalanceAlertSettingsRecord,
        window_started_at_epoch: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertClaimOutcome, BalanceAlertRepositoryError> {
        let candidates = balance_alert_events::Entity::find()
            .filter(
                Condition::all()
                    .add(due_condition(now))
                    .add(balance_alert_events::Column::AttemptCount.lt(MAX_BALANCE_ALERT_ATTEMPTS)),
            )
            .filter(balance_alert_events::Column::WindowStartedAtEpoch.eq(window_started_at_epoch))
            .order_by_asc(balance_alert_events::Column::NextAttemptAt)
            .order_by_asc(balance_alert_events::Column::Id)
            .limit(CLAIM_CANDIDATE_LIMIT)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;

        for candidate in candidates {
            let transaction = begin_transaction(&self.pool).await?;
            let next_attempt_count = candidate
                .attempt_count
                .checked_add(1)
                .ok_or(BalanceAlertRepositoryError::Invariant)?;
            let next_version = candidate
                .version
                .checked_add(1)
                .ok_or(BalanceAlertRepositoryError::Invariant)?;
            let result = balance_alert_events::Entity::update_many()
                .col_expr(
                    balance_alert_events::Column::Status,
                    Expr::value(STATUS_SENDING),
                )
                .col_expr(
                    balance_alert_events::Column::AttemptCount,
                    Expr::value(next_attempt_count),
                )
                .col_expr(
                    balance_alert_events::Column::LeaseExpiresAt,
                    Expr::value(Some(now + Duration::from_secs(DELIVERY_LEASE_SECONDS))),
                )
                .col_expr(
                    balance_alert_events::Column::Version,
                    Expr::value(next_version),
                )
                .col_expr(balance_alert_events::Column::UpdatedAt, Expr::value(now))
                .filter(balance_alert_events::Column::Id.eq(candidate.id))
                .filter(balance_alert_events::Column::Version.eq(candidate.version))
                .filter(due_condition(now))
                .filter(balance_alert_events::Column::AttemptCount.lt(MAX_BALANCE_ALERT_ATTEMPTS))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| BalanceAlertRepositoryError::Query)?;
            if result.rows_affected == 0 {
                rollback_transaction(transaction).await?;
                continue;
            }
            if result.rows_affected != 1 {
                return Err(BalanceAlertRepositoryError::Invariant);
            }
            let event = balance_alert_events::Entity::find_by_id(candidate.id)
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| BalanceAlertRepositoryError::Query)?
                .ok_or(BalanceAlertRepositoryError::Invariant)?;
            let user = users::Entity::find_by_id(event.user_id)
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| BalanceAlertRepositoryError::Query)?;
            let Some(user) = user else {
                cancel_claimed(&transaction, &event, now).await?;
                commit_transaction(transaction).await?;
                return Ok(BalanceAlertClaimOutcome::Skipped);
            };
            let threshold_units = user
                .balance_alert_threshold
                .unwrap_or(settings.default_threshold().units());
            let eligible = user.status == ENABLED_USER_STATUS
                && user.deleted_at.is_none()
                && user.email_usage_alerts
                && user.quota >= 0
                && threshold_units > 0
                && user.quota < threshold_units
                && threshold_units == event.threshold_quota
                && user.email.as_deref().is_some_and(valid_recipient);
            if !eligible {
                cancel_claimed(&transaction, &event, now).await?;
                commit_transaction(transaction).await?;
                return Ok(BalanceAlertClaimOutcome::Skipped);
            }
            let lease = BalanceAlertDeliveryLease {
                event_id: event.id,
                user_id: UserId::new(event.user_id)
                    .map_err(|_| BalanceAlertRepositoryError::Invariant)?,
                recipient: user.email.ok_or(BalanceAlertRepositoryError::Invariant)?,
                username: user.username,
                current_quota: Quota::new(user.quota)
                    .map_err(|_| BalanceAlertRepositoryError::Invariant)?,
                threshold: Quota::new(threshold_units)
                    .map_err(|_| BalanceAlertRepositoryError::Invariant)?,
                attempt_count: event.attempt_count,
                version: event.version,
                notification_source_key: format!(
                    "balance:{}:{}",
                    event.user_id, event.window_started_at_epoch
                ),
            };
            commit_transaction(transaction).await?;
            return Ok(BalanceAlertClaimOutcome::Claimed(lease));
        }
        Ok(BalanceAlertClaimOutcome::Empty)
    }

    async fn cancel_stale_inner(
        &self,
        current_window_started_at_epoch: i64,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, BalanceAlertRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let candidates = balance_alert_events::Entity::find()
            .filter(
                balance_alert_events::Column::WindowStartedAtEpoch
                    .lt(current_window_started_at_epoch),
            )
            .filter(cancelable_condition(now))
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        let mut canceled = 0_u64;
        for event in candidates {
            let result = balance_alert_events::Entity::update_many()
                .col_expr(
                    balance_alert_events::Column::Status,
                    Expr::value(STATUS_CANCELED),
                )
                .col_expr(
                    balance_alert_events::Column::LeaseExpiresAt,
                    Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
                )
                .col_expr(
                    balance_alert_events::Column::Version,
                    Expr::value(
                        event
                            .version
                            .checked_add(1)
                            .ok_or(BalanceAlertRepositoryError::Invariant)?,
                    ),
                )
                .col_expr(balance_alert_events::Column::UpdatedAt, Expr::value(now))
                .filter(balance_alert_events::Column::Id.eq(event.id))
                .filter(balance_alert_events::Column::Version.eq(event.version))
                .filter(cancelable_condition(now))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| BalanceAlertRepositoryError::Query)?;
            if result.rows_affected == 0 {
                continue;
            }
            if result.rows_affected != 1 {
                return Err(BalanceAlertRepositoryError::Invariant);
            }
            sync_notification_state(
                &transaction,
                &balance_notification_source_key(&event),
                NotificationDeliveryState::Canceled,
                event.attempt_count,
                now,
            )
            .await?;
            canceled = canceled
                .checked_add(1)
                .ok_or(BalanceAlertRepositoryError::Invariant)?;
        }
        commit_transaction(transaction).await?;
        Ok(canceled)
    }

    async fn fail_exhausted_inner(
        &self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<u64, BalanceAlertRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let candidates = balance_alert_events::Entity::find()
            .filter(balance_alert_events::Column::AttemptCount.gte(MAX_BALANCE_ALERT_ATTEMPTS))
            .filter(cancelable_condition(now))
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        let mut failed = 0_u64;
        for event in candidates {
            let result = balance_alert_events::Entity::update_many()
                .col_expr(
                    balance_alert_events::Column::Status,
                    Expr::value(STATUS_FAILED),
                )
                .col_expr(
                    balance_alert_events::Column::LeaseExpiresAt,
                    Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
                )
                .col_expr(
                    balance_alert_events::Column::Version,
                    Expr::value(
                        event
                            .version
                            .checked_add(1)
                            .ok_or(BalanceAlertRepositoryError::Invariant)?,
                    ),
                )
                .col_expr(balance_alert_events::Column::UpdatedAt, Expr::value(now))
                .filter(balance_alert_events::Column::Id.eq(event.id))
                .filter(balance_alert_events::Column::Version.eq(event.version))
                .filter(cancelable_condition(now))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| BalanceAlertRepositoryError::Query)?;
            if result.rows_affected == 0 {
                continue;
            }
            if result.rows_affected != 1 {
                return Err(BalanceAlertRepositoryError::Invariant);
            }
            sync_notification_state(
                &transaction,
                &balance_notification_source_key(&event),
                NotificationDeliveryState::Failed,
                event.attempt_count,
                now,
            )
            .await?;
            failed = failed
                .checked_add(1)
                .ok_or(BalanceAlertRepositoryError::Invariant)?;
        }
        commit_transaction(transaction).await?;
        Ok(failed)
    }

    async fn mark_sent_inner(
        &self,
        lease: &BalanceAlertDeliveryLease,
        sent_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, BalanceAlertRepositoryError> {
        let next_version = lease
            .version
            .checked_add(1)
            .ok_or(BalanceAlertRepositoryError::Invariant)?;
        let transaction = begin_transaction(&self.pool).await?;
        let result = balance_alert_events::Entity::update_many()
            .col_expr(
                balance_alert_events::Column::Status,
                Expr::value(STATUS_SENT),
            )
            .col_expr(
                balance_alert_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                balance_alert_events::Column::SentAt,
                Expr::value(Some(sent_at)),
            )
            .col_expr(
                balance_alert_events::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                balance_alert_events::Column::UpdatedAt,
                Expr::value(sent_at),
            )
            .filter(balance_alert_events::Column::Id.eq(lease.event_id))
            .filter(balance_alert_events::Column::Status.eq(STATUS_SENDING))
            .filter(balance_alert_events::Column::Version.eq(lease.version))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        match completion_outcome(result.rows_affected)? {
            BalanceAlertCompletionOutcome::Stale => {
                rollback_transaction(transaction).await?;
                Ok(BalanceAlertCompletionOutcome::Stale)
            }
            BalanceAlertCompletionOutcome::Completed => {
                sync_notification_state(
                    &transaction,
                    &lease.notification_source_key,
                    NotificationDeliveryState::Accepted,
                    lease.attempt_count,
                    sent_at,
                )
                .await?;
                commit_transaction(transaction).await?;
                Ok(BalanceAlertCompletionOutcome::Completed)
            }
        }
    }

    async fn record_failure_inner(
        &self,
        lease: &BalanceAlertDeliveryLease,
        failure: BalanceAlertDeliveryFailureKind,
        failed_at: TimeDateTimeWithTimeZone,
    ) -> Result<BalanceAlertCompletionOutcome, BalanceAlertRepositoryError> {
        let terminal = failure.terminal() || lease.attempt_count >= MAX_BALANCE_ALERT_ATTEMPTS;
        let status = if terminal {
            STATUS_FAILED
        } else {
            STATUS_PENDING
        };
        let next_attempt_at = if terminal {
            failed_at
        } else {
            failed_at + retry_delay(lease.attempt_count)?
        };
        let next_version = lease
            .version
            .checked_add(1)
            .ok_or(BalanceAlertRepositoryError::Invariant)?;
        let transaction = begin_transaction(&self.pool).await?;
        let result = balance_alert_events::Entity::update_many()
            .col_expr(balance_alert_events::Column::Status, Expr::value(status))
            .col_expr(
                balance_alert_events::Column::NextAttemptAt,
                Expr::value(next_attempt_at),
            )
            .col_expr(
                balance_alert_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                balance_alert_events::Column::LastErrorKind,
                Expr::value(Some(failure.code())),
            )
            .col_expr(
                balance_alert_events::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                balance_alert_events::Column::UpdatedAt,
                Expr::value(failed_at),
            )
            .filter(balance_alert_events::Column::Id.eq(lease.event_id))
            .filter(balance_alert_events::Column::Status.eq(STATUS_SENDING))
            .filter(balance_alert_events::Column::Version.eq(lease.version))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| BalanceAlertRepositoryError::Query)?;
        match completion_outcome(result.rows_affected)? {
            BalanceAlertCompletionOutcome::Stale => {
                rollback_transaction(transaction).await?;
                Ok(BalanceAlertCompletionOutcome::Stale)
            }
            BalanceAlertCompletionOutcome::Completed => {
                sync_notification_state(
                    &transaction,
                    &lease.notification_source_key,
                    if terminal {
                        NotificationDeliveryState::Failed
                    } else {
                        NotificationDeliveryState::Queued
                    },
                    lease.attempt_count,
                    failed_at,
                )
                .await?;
                commit_transaction(transaction).await?;
                Ok(BalanceAlertCompletionOutcome::Completed)
            }
        }
    }
}

fn balance_alert_on_conflict() -> OnConflict {
    OnConflict::columns([
        balance_alert_events::Column::UserId,
        balance_alert_events::Column::WindowStartedAtEpoch,
    ])
    // MySQL 需要显式冲突列生成合法的无变化更新语句。
    .do_nothing_on([
        balance_alert_events::Column::UserId,
        balance_alert_events::Column::WindowStartedAtEpoch,
    ])
    .to_owned()
}

fn due_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(balance_alert_events::Column::Status.eq(STATUS_PENDING))
                .add(balance_alert_events::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(balance_alert_events::Column::Status.eq(STATUS_SENDING))
                .add(balance_alert_events::Column::LeaseExpiresAt.lte(now)),
        )
}

fn cancelable_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(balance_alert_events::Column::Status.eq(STATUS_PENDING))
        .add(
            Condition::all()
                .add(balance_alert_events::Column::Status.eq(STATUS_SENDING))
                .add(balance_alert_events::Column::LeaseExpiresAt.lte(now)),
        )
}

async fn cancel_claimed(
    transaction: &DatabaseTransaction,
    event: &balance_alert_events::Model,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), BalanceAlertRepositoryError> {
    let next_version = event
        .version
        .checked_add(1)
        .ok_or(BalanceAlertRepositoryError::Invariant)?;
    let result = balance_alert_events::Entity::update_many()
        .col_expr(
            balance_alert_events::Column::Status,
            Expr::value(STATUS_CANCELED),
        )
        .col_expr(
            balance_alert_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            balance_alert_events::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(balance_alert_events::Column::UpdatedAt, Expr::value(now))
        .filter(balance_alert_events::Column::Id.eq(event.id))
        .filter(balance_alert_events::Column::Status.eq(STATUS_SENDING))
        .filter(balance_alert_events::Column::Version.eq(event.version))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| BalanceAlertRepositoryError::Query)?;
    if result.rows_affected == 1 {
        sync_notification_state(
            transaction,
            &balance_notification_source_key(event),
            NotificationDeliveryState::Canceled,
            event.attempt_count,
            now,
        )
        .await?;
        Ok(())
    } else {
        Err(BalanceAlertRepositoryError::Invariant)
    }
}

fn balance_notification_source_key(event: &balance_alert_events::Model) -> String {
    format!(
        "balance:{}:{}",
        event.user_id, event.window_started_at_epoch
    )
}

async fn sync_notification_state(
    transaction: &DatabaseTransaction,
    source_key: &str,
    state: NotificationDeliveryState,
    attempts: i16,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), BalanceAlertRepositoryError> {
    update_delivery_state(
        transaction,
        NotificationKind::BalanceAlert,
        source_key,
        state,
        attempts,
        updated_at,
    )
    .await
    .map_err(|_| BalanceAlertRepositoryError::Query)
}

fn retry_delay(attempt_count: i16) -> Result<Duration, BalanceAlertRepositoryError> {
    let seconds = match attempt_count {
        1 => 60,
        2 => 300,
        3 => 1_800,
        4 => 7_200,
        _ => return Err(BalanceAlertRepositoryError::Invariant),
    };
    Ok(Duration::from_secs(seconds))
}

fn completion_outcome(
    rows_affected: u64,
) -> Result<BalanceAlertCompletionOutcome, BalanceAlertRepositoryError> {
    match rows_affected {
        0 => Ok(BalanceAlertCompletionOutcome::Stale),
        1 => Ok(BalanceAlertCompletionOutcome::Completed),
        _ => Err(BalanceAlertRepositoryError::Invariant),
    }
}

fn valid_recipient(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 320
        && value.trim() == value
        && value.contains('@')
        && !value.chars().any(char::is_control)
}

async fn begin_transaction(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, BalanceAlertRepositoryError> {
    pool.connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| BalanceAlertRepositoryError::Query)
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), BalanceAlertRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| BalanceAlertRepositoryError::Query)
}

async fn rollback_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), BalanceAlertRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| BalanceAlertRepositoryError::Query)
}

fn record_internal_error(error: BalanceAlertRepositoryError) -> BalanceAlertRepositoryError {
    let error_kind = match error {
        BalanceAlertRepositoryError::InvalidBatchSize => return error,
        BalanceAlertRepositoryError::Query => "balance_alert_query",
        BalanceAlertRepositoryError::Timeout => "balance_alert_timeout",
        BalanceAlertRepositoryError::Invariant => "balance_alert_invariant",
    };
    tracing::error!(
        target: "af_db::balance_alert",
        error_kind,
        "余额预警事件仓储发生内部错误"
    );
    error
}

impl fmt::Debug for BalanceAlertRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BalanceAlertRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}
