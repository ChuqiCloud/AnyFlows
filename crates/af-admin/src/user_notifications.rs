use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    NotificationChannel as DatabaseNotificationChannel,
    NotificationDeliveryState as DatabaseNotificationDeliveryState,
    NotificationKind as DatabaseNotificationKind,
    UserNotificationCursor as DatabaseUserNotificationCursor, UserNotificationListOutcome,
    UserNotificationListQuery as DatabaseUserNotificationListQuery,
    UserNotificationMarkReadOutcome as DatabaseUserNotificationMarkReadOutcome,
    UserNotificationPageRecord, UserNotificationRecord, UserNotificationRepository,
    UserNotificationRepositoryError,
};
use thiserror::Error;

use crate::SessionPrincipal;

pub const DEFAULT_USER_NOTIFICATION_PAGE_SIZE: usize = 25;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserNotificationKind {
    BalanceAlert,
    SubscriptionBalanceAlert,
    SubscriptionPurchase,
    ProductUpdate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserNotificationChannel {
    Email,
    InApp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserNotificationDeliveryState {
    Queued,
    Accepted,
    Failed,
    Canceled,
    Available,
}

/// 管理服务层公开的稳定复合游标，不向 HTTP 层泄漏数据库契约。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationCursor {
    occurred_at: i64,
    id: i64,
}

impl UserNotificationCursor {
    pub const fn new(occurred_at: i64, id: i64) -> Result<Self, UserNotificationError> {
        if occurred_at <= 0 || id <= 0 {
            return Err(UserNotificationError::InvalidInput);
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

/// 当前用户通知历史的管理服务查询参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationListQuery {
    before: Option<UserNotificationCursor>,
    limit: usize,
}

/// 当前用户批量标记已读命令；不接受重复或空 ID。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserNotificationMarkReadCommand {
    ids: Vec<i64>,
}

impl UserNotificationMarkReadCommand {
    pub fn new(ids: Vec<i64>) -> Result<Self, UserNotificationError> {
        if ids.is_empty()
            || ids.len() > af_db::MAX_USER_NOTIFICATION_READ_BATCH
            || ids.iter().any(|id| *id <= 0)
        {
            return Err(UserNotificationError::InvalidInput);
        }
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != ids.len() {
            return Err(UserNotificationError::InvalidInput);
        }
        Ok(Self { ids })
    }
}

impl UserNotificationListQuery {
    pub fn new(
        before: Option<UserNotificationCursor>,
        limit: usize,
    ) -> Result<Self, UserNotificationError> {
        if !(1..=af_db::MAX_USER_NOTIFICATION_PAGE_SIZE).contains(&limit) {
            return Err(UserNotificationError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

pub struct UserNotification {
    record: UserNotificationRecord,
}

impl UserNotification {
    fn from_record(record: UserNotificationRecord) -> Self {
        Self { record }
    }
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.record.id()
    }
    #[must_use]
    pub const fn kind(&self) -> UserNotificationKind {
        if self.record.is_subscription_purchase() {
            return UserNotificationKind::SubscriptionPurchase;
        }
        match self.record.kind() {
            DatabaseNotificationKind::BalanceAlert => UserNotificationKind::BalanceAlert,
            DatabaseNotificationKind::SubscriptionBalanceAlert => {
                UserNotificationKind::SubscriptionBalanceAlert
            }
            DatabaseNotificationKind::ProductUpdate => UserNotificationKind::ProductUpdate,
        }
    }
    #[must_use]
    pub const fn channel(&self) -> UserNotificationChannel {
        match self.record.channel() {
            DatabaseNotificationChannel::Email => UserNotificationChannel::Email,
            DatabaseNotificationChannel::InApp => UserNotificationChannel::InApp,
        }
    }
    #[must_use]
    pub fn template_version(&self) -> &str {
        self.record.template_version()
    }
    #[must_use]
    pub const fn occurred_at(&self) -> i64 {
        self.record.occurred_at()
    }
    #[must_use]
    pub const fn delivery_state(&self) -> UserNotificationDeliveryState {
        match self.record.delivery_state() {
            DatabaseNotificationDeliveryState::Queued => UserNotificationDeliveryState::Queued,
            DatabaseNotificationDeliveryState::Accepted => UserNotificationDeliveryState::Accepted,
            DatabaseNotificationDeliveryState::Failed => UserNotificationDeliveryState::Failed,
            DatabaseNotificationDeliveryState::Canceled => UserNotificationDeliveryState::Canceled,
            DatabaseNotificationDeliveryState::Available => {
                UserNotificationDeliveryState::Available
            }
        }
    }
    #[must_use]
    pub const fn delivery_attempts(&self) -> i16 {
        self.record.delivery_attempts()
    }
    #[must_use]
    pub const fn observed_quota(&self) -> Option<i64> {
        self.record.observed_quota()
    }
    #[must_use]
    pub const fn threshold_quota(&self) -> Option<i64> {
        self.record.threshold_quota()
    }
    #[must_use]
    pub fn subscription_id(&self) -> Option<&str> {
        self.record.subscription_id()
    }
    #[must_use]
    pub const fn window_ends_at(&self) -> Option<i64> {
        self.record.window_ends_at()
    }
    #[must_use]
    pub const fn quota_amount(&self) -> Option<i64> {
        self.record.quota_amount()
    }
    #[must_use]
    pub const fn quota_used(&self) -> Option<i64> {
        self.record.quota_used()
    }
    #[must_use]
    pub const fn threshold_percent(&self) -> Option<i16> {
        self.record.threshold_percent()
    }
    #[must_use]
    pub const fn read_at(&self) -> Option<i64> {
        self.record.read_at()
    }

    #[must_use]
    pub const fn announcement_id(&self) -> Option<i64> {
        self.record.announcement_id()
    }

    #[must_use]
    pub const fn announcement_version(&self) -> Option<i64> {
        self.record.announcement_version()
    }
    #[must_use]
    pub fn announcement_title_zh(&self) -> Option<&str> {
        self.record.announcement_title_zh()
    }
    #[must_use]
    pub fn announcement_title_en(&self) -> Option<&str> {
        self.record.announcement_title_en()
    }
    #[must_use]
    pub fn announcement_body_zh(&self) -> Option<&str> {
        self.record.announcement_body_zh()
    }
    #[must_use]
    pub fn announcement_body_en(&self) -> Option<&str> {
        self.record.announcement_body_en()
    }
    #[must_use]
    pub const fn announcement_status(&self) -> Option<i16> {
        self.record.announcement_status()
    }
    #[must_use]
    pub const fn announcement_visible_until(&self) -> Option<i64> {
        self.record.announcement_visible_until()
    }
}

impl fmt::Debug for UserNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserNotification(<redacted>)")
    }
}

pub struct UserNotificationPage {
    entries: Vec<UserNotification>,
    next_cursor: Option<UserNotificationCursor>,
    unread_count: u64,
}

impl UserNotificationPage {
    fn from_record(page: UserNotificationPageRecord) -> Self {
        let next_cursor = page.next_cursor().map(|cursor| UserNotificationCursor {
            occurred_at: cursor.occurred_at(),
            id: cursor.id(),
        });
        Self {
            entries: page
                .entries()
                .iter()
                .map(|record| UserNotification::from_record(record.clone_for_service()))
                .collect(),
            next_cursor,
            unread_count: page.unread_count(),
        }
    }
    #[must_use]
    pub fn entries(&self) -> &[UserNotification] {
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

/// 标记已读后的当前用户摘要。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationMarkReadResult {
    marked_count: usize,
    unread_count: u64,
}

impl UserNotificationMarkReadResult {
    fn from_record(record: DatabaseUserNotificationMarkReadOutcome) -> Self {
        Self {
            marked_count: record.marked_count(),
            unread_count: record.unread_count(),
        }
    }
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
pub enum UserNotificationError {
    #[error("通知历史查询参数无效")]
    InvalidInput,
    #[error("当前用户通知历史服务内部失败")]
    Internal,
}

pub type UserNotificationListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserNotificationPage, UserNotificationError>> + Send + 'a>>;
pub type UserNotificationMarkReadFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<UserNotificationMarkReadResult, UserNotificationError>>
            + Send
            + 'a,
    >,
>;

pub trait UserNotificationService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserNotificationListQuery,
    ) -> UserNotificationListFuture<'a>;
    fn mark_read<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserNotificationMarkReadCommand,
    ) -> UserNotificationMarkReadFuture<'a>;
}

pub struct DatabaseUserNotificationService {
    repository: Arc<UserNotificationRepository>,
}

impl DatabaseUserNotificationService {
    #[must_use]
    pub fn new(repository: Arc<UserNotificationRepository>) -> Self {
        Self { repository }
    }
}

impl UserNotificationService for DatabaseUserNotificationService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserNotificationListQuery,
    ) -> UserNotificationListFuture<'a> {
        Box::pin(async move {
            let before = query
                .before
                .map(|cursor| DatabaseUserNotificationCursor::new(cursor.occurred_at, cursor.id))
                .transpose()
                .map_err(map_error)?;
            let query =
                DatabaseUserNotificationListQuery::new(before, query.limit).map_err(map_error)?;
            match self
                .repository
                .list(principal.user_id().get(), query)
                .await
                .map_err(map_error)?
            {
                UserNotificationListOutcome::Found(page) => {
                    Ok(UserNotificationPage::from_record(page))
                }
                UserNotificationListOutcome::NotFound => Err(UserNotificationError::Internal),
            }
        })
    }

    fn mark_read<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserNotificationMarkReadCommand,
    ) -> UserNotificationMarkReadFuture<'a> {
        Box::pin(async move {
            let result = self
                .repository
                .mark_read(principal.user_id().get(), command.ids)
                .await
                .map_err(map_error)?;
            Ok(UserNotificationMarkReadResult::from_record(result))
        })
    }
}

impl fmt::Debug for DatabaseUserNotificationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserNotificationService(<redacted>)")
    }
}

fn map_error(error: UserNotificationRepositoryError) -> UserNotificationError {
    match error {
        UserNotificationRepositoryError::InvalidInput => UserNotificationError::InvalidInput,
        UserNotificationRepositoryError::Query
        | UserNotificationRepositoryError::Invariant
        | UserNotificationRepositoryError::Timeout => UserNotificationError::Internal,
        UserNotificationRepositoryError::NotFound => UserNotificationError::InvalidInput,
    }
}
