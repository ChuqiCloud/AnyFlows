use std::{collections::HashMap, fmt, time::Duration};

use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, OnConflict},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{user_notification_events, user_notification_receipts},
};

/// 通知历史单页允许返回的最大事实数量。
pub const MAX_USER_NOTIFICATION_PAGE_SIZE: usize = 100;
/// 单次标记已读允许携带的通知数量。
pub const MAX_USER_NOTIFICATION_READ_BATCH: usize = 100;

/// 第一切片支持的通知类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum NotificationKind {
    BalanceAlert = 1,
    SubscriptionBalanceAlert = 2,
}

impl NotificationKind {
    fn from_code(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::BalanceAlert),
            2 => Some(Self::SubscriptionBalanceAlert),
            _ => None,
        }
    }
}

/// 第一切片固定使用邮件渠道。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum NotificationChannel {
    Email = 1,
}

impl NotificationChannel {
    fn from_code(value: i16) -> Option<Self> {
        (value == 1).then_some(Self::Email)
    }
}

/// 系统已知的邮件投递事实状态，不代表 Provider 最终送达或用户已读。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum NotificationDeliveryState {
    Queued = 1,
    Accepted = 2,
    Failed = 3,
    Canceled = 4,
}

impl NotificationDeliveryState {
    fn from_code(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::Queued),
            2 => Some(Self::Accepted),
            3 => Some(Self::Failed),
            4 => Some(Self::Canceled),
            _ => None,
        }
    }
}

/// 用户可见通知中允许携带的有限业务快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserNotificationSnapshot {
    Balance {
        observed_quota: i64,
        threshold_quota: i64,
    },
    Subscription {
        subscription_id: String,
        window_ends_at: TimeDateTimeWithTimeZone,
        quota_amount: i64,
        quota_used: i64,
        threshold_percent: i16,
    },
}

/// 与预警入队事务一起写入的通知事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserNotificationWrite {
    pub user_id: i64,
    pub kind: NotificationKind,
    pub occurred_at: TimeDateTimeWithTimeZone,
    pub source_key: String,
    pub snapshot: UserNotificationSnapshot,
}

impl UserNotificationWrite {
    /// 构造余额预警的稳定来源键和快照。
    #[must_use]
    pub fn balance_alert(
        user_id: i64,
        window_started_at_epoch: i64,
        observed_quota: i64,
        threshold_quota: i64,
        occurred_at: TimeDateTimeWithTimeZone,
    ) -> Self {
        Self {
            user_id,
            kind: NotificationKind::BalanceAlert,
            occurred_at,
            source_key: format!("balance:{user_id}:{window_started_at_epoch}"),
            snapshot: UserNotificationSnapshot::Balance {
                observed_quota,
                threshold_quota,
            },
        }
    }

    /// 构造订阅窗口预警的稳定来源键和快照。
    #[must_use]
    #[allow(clippy::too_many_arguments, reason = "字段与订阅通知事实一一对应")]
    pub fn subscription_balance_alert(
        user_id: i64,
        internal_subscription_id: i64,
        subscription_id: String,
        window_started_at: TimeDateTimeWithTimeZone,
        window_ends_at: TimeDateTimeWithTimeZone,
        quota_amount: i64,
        quota_used: i64,
        threshold_percent: i16,
        occurred_at: TimeDateTimeWithTimeZone,
    ) -> Self {
        Self {
            user_id,
            kind: NotificationKind::SubscriptionBalanceAlert,
            occurred_at,
            source_key: format!(
                "subscription:{internal_subscription_id}:{}",
                window_started_at.unix_timestamp()
            ),
            snapshot: UserNotificationSnapshot::Subscription {
                subscription_id,
                window_ends_at,
                quota_amount,
                quota_used,
                threshold_percent,
            },
        }
    }
}

/// 用户通知事实记录，已移除邮箱、正文和 Provider 响应。
#[derive(Clone)]
pub struct UserNotificationRecord {
    id: i64,
    kind: NotificationKind,
    channel: NotificationChannel,
    template_version: String,
    occurred_at: i64,
    delivery_state: NotificationDeliveryState,
    delivery_attempts: i16,
    observed_quota: Option<i64>,
    threshold_quota: Option<i64>,
    subscription_id: Option<String>,
    window_ends_at: Option<i64>,
    quota_amount: Option<i64>,
    quota_used: Option<i64>,
    threshold_percent: Option<i16>,
    read_at: Option<i64>,
}

impl UserNotificationRecord {
    fn from_model(
        model: user_notification_events::Model,
        read_at: Option<i64>,
    ) -> Result<Self, UserNotificationRepositoryError> {
        Ok(Self {
            id: model.id,
            kind: NotificationKind::from_code(model.kind)
                .ok_or(UserNotificationRepositoryError::Invariant)?,
            channel: NotificationChannel::from_code(model.channel)
                .ok_or(UserNotificationRepositoryError::Invariant)?,
            template_version: model.template_version,
            occurred_at: model.occurred_at.unix_timestamp(),
            delivery_state: NotificationDeliveryState::from_code(model.delivery_state)
                .ok_or(UserNotificationRepositoryError::Invariant)?,
            delivery_attempts: model.delivery_attempts,
            observed_quota: model.observed_quota,
            threshold_quota: model.threshold_quota,
            subscription_id: model.subscription_id,
            window_ends_at: model.window_ends_at.map(|value| value.unix_timestamp()),
            quota_amount: model.quota_amount,
            quota_used: model.quota_used,
            threshold_percent: model.threshold_percent,
            read_at,
        })
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn kind(&self) -> NotificationKind {
        self.kind
    }
    #[must_use]
    pub const fn channel(&self) -> NotificationChannel {
        self.channel
    }
    #[must_use]
    pub fn template_version(&self) -> &str {
        &self.template_version
    }
    #[must_use]
    pub const fn occurred_at(&self) -> i64 {
        self.occurred_at
    }
    #[must_use]
    pub const fn delivery_state(&self) -> NotificationDeliveryState {
        self.delivery_state
    }
    #[must_use]
    pub const fn delivery_attempts(&self) -> i16 {
        self.delivery_attempts
    }
    #[must_use]
    pub const fn observed_quota(&self) -> Option<i64> {
        self.observed_quota
    }
    #[must_use]
    pub const fn threshold_quota(&self) -> Option<i64> {
        self.threshold_quota
    }
    #[must_use]
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    #[must_use]
    pub const fn window_ends_at(&self) -> Option<i64> {
        self.window_ends_at
    }
    #[must_use]
    pub const fn quota_amount(&self) -> Option<i64> {
        self.quota_amount
    }
    #[must_use]
    pub const fn quota_used(&self) -> Option<i64> {
        self.quota_used
    }
    #[must_use]
    pub const fn threshold_percent(&self) -> Option<i16> {
        self.threshold_percent
    }
    #[must_use]
    pub const fn read_at(&self) -> Option<i64> {
        self.read_at
    }

    #[must_use]
    pub fn clone_for_service(&self) -> Self {
        self.clone()
    }
}

impl fmt::Debug for UserNotificationRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserNotificationRecord(<redacted>)")
    }
}

/// 通知历史的稳定复合游标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationCursor {
    occurred_at: i64,
    id: i64,
}

impl UserNotificationCursor {
    pub const fn new(occurred_at: i64, id: i64) -> Result<Self, UserNotificationRepositoryError> {
        if occurred_at <= 0 || id <= 0 {
            return Err(UserNotificationRepositoryError::InvalidInput);
        }
        Ok(Self { occurred_at, id })
    }

    #[must_use]
    pub const fn occurred_at(self) -> i64 {
        self.occurred_at
    }
    #[must_use]
    pub const fn id(self) -> i64 {
        self.id
    }
}

/// 通知历史查询参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationListQuery {
    before: Option<UserNotificationCursor>,
    limit: usize,
}

impl UserNotificationListQuery {
    pub fn new(
        before: Option<UserNotificationCursor>,
        limit: usize,
    ) -> Result<Self, UserNotificationRepositoryError> {
        if !(1..=MAX_USER_NOTIFICATION_PAGE_SIZE).contains(&limit) {
            return Err(UserNotificationRepositoryError::InvalidInput);
        }
        Ok(Self { before, limit })
    }

    #[must_use]
    pub const fn before(self) -> Option<UserNotificationCursor> {
        self.before
    }
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for UserNotificationListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: 25,
        }
    }
}

/// 一页通知历史和下一页复合游标。
pub struct UserNotificationPageRecord {
    entries: Vec<UserNotificationRecord>,
    next_cursor: Option<UserNotificationCursor>,
    unread_count: u64,
}

impl UserNotificationPageRecord {
    fn from_parts(
        entries: Vec<UserNotificationRecord>,
        next_cursor: Option<UserNotificationCursor>,
        unread_count: u64,
    ) -> Self {
        Self {
            entries,
            next_cursor,
            unread_count,
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[UserNotificationRecord] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<UserNotificationCursor> {
        self.next_cursor
    }
    #[must_use]
    pub const fn unread_count(&self) -> u64 {
        self.unread_count
    }
}

impl fmt::Debug for UserNotificationPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserNotificationPageRecord(<redacted>)")
    }
}

/// 通知历史列表的数据库结果。
#[derive(Debug)]
pub enum UserNotificationListOutcome {
    Found(UserNotificationPageRecord),
    NotFound,
}

/// 标记已读后的稳定摘要。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationMarkReadOutcome {
    marked_count: usize,
    unread_count: u64,
}

impl UserNotificationMarkReadOutcome {
    #[must_use]
    pub const fn marked_count(self) -> usize {
        self.marked_count
    }
    #[must_use]
    pub const fn unread_count(self) -> u64 {
        self.unread_count
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserNotificationRepositoryConfigError {
    #[error("用户通知事实库操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserNotificationRepositoryError {
    #[error("用户通知历史查询参数无效")]
    InvalidInput,
    #[error("用户通知事实库查询失败")]
    Query,
    #[error("用户通知事实数据损坏")]
    Invariant,
    #[error("用户通知事实不存在或不属于当前用户")]
    NotFound,
    #[error("用户通知事实库操作超时")]
    Timeout,
}

/// 用户通知事实库，读取时始终由调用方传入当前会话用户 ID。
#[derive(Clone)]
pub struct UserNotificationRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl UserNotificationRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, UserNotificationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(UserNotificationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按当前用户和稳定复合游标读取通知历史。
    pub async fn list(
        &self,
        user_id: i64,
        query: UserNotificationListQuery,
    ) -> Result<UserNotificationListOutcome, UserNotificationRepositoryError> {
        if user_id <= 0 {
            return Err(UserNotificationRepositoryError::InvalidInput);
        }
        match timeout(self.operation_timeout, self.list_inner(user_id, query)).await {
            Ok(result) => result,
            Err(_) => Err(UserNotificationRepositoryError::Timeout),
        }
    }

    /// 在当前用户范围内幂等写入已读回执，并返回剩余未读数量。
    pub async fn mark_read(
        &self,
        user_id: i64,
        notification_ids: Vec<i64>,
    ) -> Result<UserNotificationMarkReadOutcome, UserNotificationRepositoryError> {
        if user_id <= 0
            || notification_ids.is_empty()
            || notification_ids.len() > MAX_USER_NOTIFICATION_READ_BATCH
            || notification_ids.iter().any(|id| *id <= 0)
        {
            return Err(UserNotificationRepositoryError::InvalidInput);
        }
        let mut unique_ids = notification_ids.clone();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        if unique_ids.len() != notification_ids.len() {
            return Err(UserNotificationRepositoryError::InvalidInput);
        }
        match timeout(
            self.operation_timeout,
            self.mark_read_inner(user_id, unique_ids),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(UserNotificationRepositoryError::Timeout),
        }
    }

    async fn list_inner(
        &self,
        user_id: i64,
        query: UserNotificationListQuery,
    ) -> Result<UserNotificationListOutcome, UserNotificationRepositoryError> {
        let mut select = user_notification_events::Entity::find()
            .filter(user_notification_events::Column::UserId.eq(user_id));
        if let Some(cursor) = query.before {
            let cursor_occurred_at =
                TimeDateTimeWithTimeZone::from_unix_timestamp(cursor.occurred_at)
                    .map_err(|_| UserNotificationRepositoryError::InvalidInput)?;
            select = select.filter(
                Condition::any()
                    .add(user_notification_events::Column::OccurredAt.lt(cursor_occurred_at))
                    .add(
                        Condition::all()
                            .add(
                                user_notification_events::Column::OccurredAt.eq(cursor_occurred_at),
                            )
                            .add(user_notification_events::Column::Id.lt(cursor.id)),
                    ),
            );
        }
        let fetch_limit = query
            .limit
            .checked_add(1)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(UserNotificationRepositoryError::Invariant)?;
        let mut models = select
            .order_by_desc(user_notification_events::Column::OccurredAt)
            .order_by_desc(user_notification_events::Column::Id)
            .limit(fetch_limit)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        let has_more = models.len() > query.limit;
        if has_more {
            models.truncate(query.limit);
        }
        let next_cursor = if has_more {
            let last = models
                .last()
                .ok_or(UserNotificationRepositoryError::Invariant)?;
            Some(UserNotificationCursor::new(
                last.occurred_at.unix_timestamp(),
                last.id,
            )?)
        } else {
            None
        };
        let ids = models.iter().map(|model| model.id).collect::<Vec<_>>();
        let receipts = if ids.is_empty() {
            Vec::new()
        } else {
            user_notification_receipts::Entity::find()
                .filter(user_notification_receipts::Column::UserId.eq(user_id))
                .filter(user_notification_receipts::Column::NotificationId.is_in(ids))
                .all(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| UserNotificationRepositoryError::Query)?
        };
        let read_at_by_id = receipts
            .into_iter()
            .map(|receipt| (receipt.notification_id, receipt.read_at.unix_timestamp()))
            .collect::<HashMap<_, _>>();
        let total_count = user_notification_events::Entity::find()
            .filter(user_notification_events::Column::UserId.eq(user_id))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        let read_count = user_notification_receipts::Entity::find()
            .filter(user_notification_receipts::Column::UserId.eq(user_id))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        let unread_count = total_count.saturating_sub(read_count);
        let entries = models
            .into_iter()
            .map(|model| {
                let read_at = read_at_by_id.get(&model.id).copied();
                UserNotificationRecord::from_model(model, read_at)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UserNotificationListOutcome::Found(
            UserNotificationPageRecord::from_parts(entries, next_cursor, unread_count),
        ))
    }

    async fn mark_read_inner(
        &self,
        user_id: i64,
        notification_ids: Vec<i64>,
    ) -> Result<UserNotificationMarkReadOutcome, UserNotificationRepositoryError> {
        let matching_count = user_notification_events::Entity::find()
            .filter(user_notification_events::Column::UserId.eq(user_id))
            .filter(user_notification_events::Column::Id.is_in(notification_ids.clone()))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        if matching_count != notification_ids.len() as u64 {
            return Err(UserNotificationRepositoryError::NotFound);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let models = notification_ids
            .iter()
            .map(|notification_id| user_notification_receipts::ActiveModel {
                user_id: Set(user_id),
                notification_id: Set(*notification_id),
                read_at: Set(now),
            })
            .collect::<Vec<_>>();
        user_notification_receipts::Entity::insert_many(models)
            .on_conflict(
                OnConflict::columns([
                    user_notification_receipts::Column::UserId,
                    user_notification_receipts::Column::NotificationId,
                ])
                .do_nothing_on([
                    user_notification_receipts::Column::UserId,
                    user_notification_receipts::Column::NotificationId,
                ])
                .to_owned(),
            )
            .exec_without_returning(self.pool.connection())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        let total_count = user_notification_events::Entity::find()
            .filter(user_notification_events::Column::UserId.eq(user_id))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        let read_count = user_notification_receipts::Entity::find()
            .filter(user_notification_receipts::Column::UserId.eq(user_id))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserNotificationRepositoryError::Query)?;
        Ok(UserNotificationMarkReadOutcome {
            marked_count: notification_ids.len(),
            unread_count: total_count.saturating_sub(read_count),
        })
    }
}

impl fmt::Debug for UserNotificationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserNotificationRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 在预警发现事务中幂等写入通知事实；该函数不接触收件人或正文。
pub(crate) async fn insert_queued<C: ConnectionTrait>(
    connection: &C,
    write: &UserNotificationWrite,
) -> Result<(), sea_orm::DbErr> {
    // 游标对外只暴露 Unix 秒，因此账本时间统一截断到秒，避免数据库保留子秒后翻页漏记。
    let occurred_at =
        TimeDateTimeWithTimeZone::from_unix_timestamp(write.occurred_at.unix_timestamp())
            .map_err(|_| sea_orm::DbErr::Custom("通知事实时间戳无效".to_owned()))?;
    let (
        observed_quota,
        threshold_quota,
        subscription_id,
        window_ends_at,
        quota_amount,
        quota_used,
        threshold_percent,
    ) = match &write.snapshot {
        UserNotificationSnapshot::Balance {
            observed_quota,
            threshold_quota,
        } => (
            Some(*observed_quota),
            Some(*threshold_quota),
            None,
            None,
            None,
            None,
            None,
        ),
        UserNotificationSnapshot::Subscription {
            subscription_id,
            window_ends_at,
            quota_amount,
            quota_used,
            threshold_percent,
        } => (
            None,
            None,
            Some(subscription_id.clone()),
            Some(*window_ends_at),
            Some(*quota_amount),
            Some(*quota_used),
            Some(*threshold_percent),
        ),
    };
    user_notification_events::Entity::insert(user_notification_events::ActiveModel {
        id: sea_orm::NotSet,
        user_id: Set(write.user_id),
        kind: Set(write.kind as i16),
        channel: Set(NotificationChannel::Email as i16),
        template_version: Set("v1".to_owned()),
        occurred_at: Set(occurred_at),
        delivery_state: Set(NotificationDeliveryState::Queued as i16),
        delivery_attempts: Set(0),
        source_kind: Set(write.kind as i16),
        source_key: Set(write.source_key.clone()),
        observed_quota: Set(observed_quota),
        threshold_quota: Set(threshold_quota),
        subscription_id: Set(subscription_id),
        window_ends_at: Set(window_ends_at),
        quota_amount: Set(quota_amount),
        quota_used: Set(quota_used),
        threshold_percent: Set(threshold_percent),
        updated_at: Set(occurred_at),
    })
    .on_conflict(
        OnConflict::columns([
            user_notification_events::Column::SourceKind,
            user_notification_events::Column::SourceKey,
        ])
        .do_nothing_on([
            user_notification_events::Column::SourceKind,
            user_notification_events::Column::SourceKey,
        ])
        .to_owned(),
    )
    .exec_without_returning(connection)
    .await
    .map(|_| ())
}

/// 在预警队列状态事务中同步用户可见投递事实；旧队列事件没有账本行时允许零更新。
pub(crate) async fn update_delivery_state<C: ConnectionTrait>(
    connection: &C,
    kind: NotificationKind,
    source_key: &str,
    state: NotificationDeliveryState,
    attempts: i16,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), sea_orm::DbErr> {
    if source_key.is_empty() || source_key.len() > 128 || !(0..=5).contains(&attempts) {
        return Err(sea_orm::DbErr::Custom(
            "通知事实投递状态参数无效".to_owned(),
        ));
    }
    let result = user_notification_events::Entity::update_many()
        .col_expr(
            user_notification_events::Column::DeliveryState,
            Expr::value(state as i16),
        )
        .col_expr(
            user_notification_events::Column::DeliveryAttempts,
            Expr::value(attempts),
        )
        .col_expr(
            user_notification_events::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .filter(user_notification_events::Column::Kind.eq(kind as i16))
        .filter(user_notification_events::Column::SourceKey.eq(source_key))
        .exec(connection)
        .await?;
    if result.rows_affected > 1 {
        return Err(sea_orm::DbErr::Custom("通知事实来源唯一性损坏".to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sea_orm::{ActiveModelTrait, EntityTrait, Set, entity::prelude::TimeDateTimeWithTimeZone};

    use super::*;
    use crate::{
        DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
        MigrationOptions, connect_and_migrate,
    };

    async fn fixture() -> Result<(DatabasePool, i64), Box<dyn std::error::Error>> {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let InitialSetupOutcome::Initialized { user_id } =
            InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
                .initialize(InitialSetupRecord::new(
                    "notification-owner".to_owned(),
                    "a secure notification password".to_owned(),
                ))
                .await?
        else {
            return Err("notification fixture must initialize".into());
        };
        Ok((pool, user_id.get()))
    }

    #[tokio::test]
    async fn source_is_idempotent_and_cursor_is_stable_per_user()
    -> Result<(), Box<dyn std::error::Error>> {
        let (pool, user_id) = fixture().await?;
        let occurred_at = TimeDateTimeWithTimeZone::now_utc();
        let repository = UserNotificationRepository::new(pool.clone(), Duration::from_secs(5))?;

        for window in 1..=3 {
            insert_queued(
                pool.connection(),
                &UserNotificationWrite::balance_alert(
                    user_id,
                    window,
                    100 + window,
                    1_000,
                    occurred_at,
                ),
            )
            .await?;
        }
        insert_queued(
            pool.connection(),
            &UserNotificationWrite::balance_alert(user_id, 1, 999, 1_000, occurred_at),
        )
        .await?;

        let first = repository
            .list(user_id, UserNotificationListQuery::new(None, 2)?)
            .await?;
        let UserNotificationListOutcome::Found(first) = first else {
            return Err("notification page must exist".into());
        };
        assert_eq!(first.entries().len(), 2);
        let cursor = first
            .next_cursor()
            .expect("two entries must expose a cursor");
        let second = repository
            .list(user_id, UserNotificationListQuery::new(Some(cursor), 2)?)
            .await?;
        let UserNotificationListOutcome::Found(second) = second else {
            return Err("notification page must exist".into());
        };
        assert_eq!(second.entries().len(), 1);

        let other_user = repository
            .list(2, UserNotificationListQuery::new(None, 10)?)
            .await?;
        let UserNotificationListOutcome::Found(other_user) = other_user else {
            return Err("notification page must exist".into());
        };
        assert!(other_user.entries().is_empty());
        pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn mark_read_is_idempotent_and_rejects_invalid_or_cross_user_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let (pool, user_id) = fixture().await?;
        let owner = crate::entity::users::Entity::find_by_id(user_id)
            .one(pool.connection())
            .await?
            .expect("notification owner must exist");
        let other_user = crate::entity::users::ActiveModel {
            username: Set("notification-other".to_owned()),
            email: Set(Some("notification-other@example.com".to_owned())),
            default_group_id: Set(owner.default_group_id),
            aff_code: Set("notification-other-aff".to_owned()),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let occurred_at = TimeDateTimeWithTimeZone::now_utc();
        insert_queued(
            pool.connection(),
            &UserNotificationWrite::balance_alert(user_id, 10, 100, 1_000, occurred_at),
        )
        .await?;
        insert_queued(
            pool.connection(),
            &UserNotificationWrite::balance_alert(other_user.id, 11, 100, 1_000, occurred_at),
        )
        .await?;
        let repository = UserNotificationRepository::new(pool.clone(), Duration::from_secs(5))?;
        let UserNotificationListOutcome::Found(page) = repository
            .list(user_id, UserNotificationListQuery::default())
            .await?
        else {
            return Err("notification page must exist".into());
        };
        let notification_id = page.entries()[0].id();
        assert_eq!(page.unread_count(), 1);
        assert_eq!(
            repository.mark_read(user_id, vec![]).await,
            Err(UserNotificationRepositoryError::InvalidInput)
        );
        assert_eq!(
            repository
                .mark_read(user_id, vec![notification_id, notification_id])
                .await,
            Err(UserNotificationRepositoryError::InvalidInput)
        );
        let UserNotificationListOutcome::Found(other_page) = repository
            .list(other_user.id, UserNotificationListQuery::default())
            .await?
        else {
            return Err("other notification page must exist".into());
        };
        let other_notification_id = other_page.entries()[0].id();
        assert_eq!(
            repository
                .mark_read(user_id, vec![other_notification_id])
                .await,
            Err(UserNotificationRepositoryError::NotFound)
        );
        let marked = repository.mark_read(user_id, vec![notification_id]).await?;
        assert_eq!(marked.marked_count(), 1);
        assert_eq!(marked.unread_count(), 0);
        let repeated = repository.mark_read(user_id, vec![notification_id]).await?;
        assert_eq!(repeated.marked_count(), 1);
        assert_eq!(repeated.unread_count(), 0);
        let UserNotificationListOutcome::Found(page) = repository
            .list(user_id, UserNotificationListQuery::default())
            .await?
        else {
            return Err("notification page must exist".into());
        };
        assert!(page.entries()[0].read_at().is_some());
        assert_eq!(page.unread_count(), 0);
        pool.close().await?;
        Ok(())
    }
}
