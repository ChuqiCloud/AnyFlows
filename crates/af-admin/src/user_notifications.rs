use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    NotificationChannel, NotificationDeliveryState, NotificationKind, UserNotificationCursor,
    UserNotificationListOutcome, UserNotificationListQuery, UserNotificationPageRecord,
    UserNotificationRecord, UserNotificationRepository, UserNotificationRepositoryError,
};
use thiserror::Error;

use crate::SessionPrincipal;

pub const DEFAULT_USER_NOTIFICATION_PAGE_SIZE: usize = 25;

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
    pub const fn kind(&self) -> NotificationKind {
        self.record.kind()
    }
    #[must_use]
    pub const fn channel(&self) -> NotificationChannel {
        self.record.channel()
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
    pub const fn delivery_state(&self) -> NotificationDeliveryState {
        self.record.delivery_state()
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
}

impl fmt::Debug for UserNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserNotification(<redacted>)")
    }
}

pub struct UserNotificationPage {
    entries: Vec<UserNotification>,
    next_cursor: Option<UserNotificationCursor>,
}

impl UserNotificationPage {
    fn from_record(page: UserNotificationPageRecord) -> Self {
        let next_cursor = page.next_cursor();
        Self {
            entries: page
                .entries()
                .iter()
                .map(|record| UserNotification::from_record(record.clone_for_service()))
                .collect(),
            next_cursor,
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

pub trait UserNotificationService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserNotificationListQuery,
    ) -> UserNotificationListFuture<'a>;
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
    }
}
