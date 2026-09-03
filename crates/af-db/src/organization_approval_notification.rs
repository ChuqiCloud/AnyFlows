use std::time::Duration;

use sea_orm::{
    ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::OnConflict,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{organization_approval_notification_receipts, organization_approval_notifications},
};

pub const MAX_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE: usize = 100;
pub const MAX_ORGANIZATION_APPROVAL_NOTIFICATION_READ_BATCH: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalNotificationCursor {
    occurred_at: i64,
    id: i64,
}

impl OrganizationApprovalNotificationCursor {
    pub const fn new(
        occurred_at: i64,
        id: i64,
    ) -> Result<Self, OrganizationApprovalNotificationRepositoryError> {
        if occurred_at <= 0 || id <= 0 {
            return Err(OrganizationApprovalNotificationRepositoryError::InvalidInput);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalNotificationListQuery {
    before: Option<OrganizationApprovalNotificationCursor>,
    limit: usize,
}

impl OrganizationApprovalNotificationListQuery {
    pub fn new(
        before: Option<OrganizationApprovalNotificationCursor>,
        limit: usize,
    ) -> Result<Self, OrganizationApprovalNotificationRepositoryError> {
        if !(1..=MAX_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationApprovalNotificationRepositoryError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalNotificationRecord {
    id: i64,
    organization_id: i64,
    request_id: i64,
    decision_id: i64,
    decision_kind: i16,
    template_version: String,
    occurred_at: i64,
    read_at: Option<i64>,
}

impl OrganizationApprovalNotificationRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> i64 {
        self.organization_id
    }
    #[must_use]
    pub const fn request_id(&self) -> i64 {
        self.request_id
    }
    #[must_use]
    pub const fn decision_id(&self) -> i64 {
        self.decision_id
    }
    #[must_use]
    pub const fn decision_kind(&self) -> i16 {
        self.decision_kind
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
    pub const fn read_at(&self) -> Option<i64> {
        self.read_at
    }
}

#[derive(Debug)]
pub struct OrganizationApprovalNotificationPage {
    entries: Vec<OrganizationApprovalNotificationRecord>,
    next_cursor: Option<OrganizationApprovalNotificationCursor>,
    unread_count: u64,
}

impl OrganizationApprovalNotificationPage {
    #[must_use]
    pub fn entries(&self) -> &[OrganizationApprovalNotificationRecord] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationApprovalNotificationCursor> {
        self.next_cursor
    }
    #[must_use]
    pub const fn unread_count(&self) -> u64 {
        self.unread_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalNotificationMarkReadOutcome {
    marked_count: usize,
    unread_count: u64,
}

impl OrganizationApprovalNotificationMarkReadOutcome {
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
pub enum OrganizationApprovalNotificationRepositoryConfigError {
    #[error("企业审批通知操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalNotificationRepositoryError {
    #[error("企业审批通知查询参数无效")]
    InvalidInput,
    #[error("企业审批通知事实查询失败")]
    Query,
    #[error("企业审批通知事实数据损坏")]
    Invariant,
    #[error("企业审批通知不存在或不属于当前用户")]
    NotFound,
    #[error("企业审批通知操作超时")]
    Timeout,
}

#[derive(Clone)]
pub struct OrganizationApprovalNotificationRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationApprovalNotificationRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationApprovalNotificationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(
                OrganizationApprovalNotificationRepositoryConfigError::ZeroOperationTimeout,
            );
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn list(
        &self,
        recipient_user_id: i64,
        query: OrganizationApprovalNotificationListQuery,
    ) -> Result<OrganizationApprovalNotificationPage, OrganizationApprovalNotificationRepositoryError>
    {
        if recipient_user_id <= 0 {
            return Err(OrganizationApprovalNotificationRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.list_inner(recipient_user_id, query),
        )
        .await
        .map_err(|_| OrganizationApprovalNotificationRepositoryError::Timeout)?
    }

    pub async fn mark_read(
        &self,
        recipient_user_id: i64,
        notification_ids: Vec<i64>,
    ) -> Result<
        OrganizationApprovalNotificationMarkReadOutcome,
        OrganizationApprovalNotificationRepositoryError,
    > {
        if recipient_user_id <= 0
            || notification_ids.is_empty()
            || notification_ids.len() > MAX_ORGANIZATION_APPROVAL_NOTIFICATION_READ_BATCH
            || notification_ids.iter().any(|id| *id <= 0)
        {
            return Err(OrganizationApprovalNotificationRepositoryError::InvalidInput);
        }
        let mut unique_ids = notification_ids.clone();
        unique_ids.sort_unstable();
        unique_ids.dedup();
        if unique_ids.len() != notification_ids.len() {
            return Err(OrganizationApprovalNotificationRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.mark_read_inner(recipient_user_id, unique_ids),
        )
        .await
        .map_err(|_| OrganizationApprovalNotificationRepositoryError::Timeout)?
    }

    async fn list_inner(
        &self,
        recipient_user_id: i64,
        query: OrganizationApprovalNotificationListQuery,
    ) -> Result<OrganizationApprovalNotificationPage, OrganizationApprovalNotificationRepositoryError>
    {
        let mut select = organization_approval_notifications::Entity::find().filter(
            organization_approval_notifications::Column::RecipientUserId.eq(recipient_user_id),
        );
        if let Some(cursor) = query.before {
            let occurred_at = TimeDateTimeWithTimeZone::from_unix_timestamp(cursor.occurred_at)
                .map_err(|_| OrganizationApprovalNotificationRepositoryError::InvalidInput)?;
            select = select.filter(
                sea_orm::Condition::any()
                    .add(organization_approval_notifications::Column::OccurredAt.lt(occurred_at))
                    .add(
                        sea_orm::Condition::all()
                            .add(
                                organization_approval_notifications::Column::OccurredAt
                                    .eq(occurred_at),
                            )
                            .add(organization_approval_notifications::Column::Id.lt(cursor.id)),
                    ),
            );
        }
        let mut models = select
            .order_by_desc(organization_approval_notifications::Column::OccurredAt)
            .order_by_desc(organization_approval_notifications::Column::Id)
            .limit((query.limit + 1) as u64)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        let has_more = models.len() > query.limit;
        if has_more {
            models.truncate(query.limit);
        }
        let next_cursor = if has_more {
            let last = models
                .last()
                .ok_or(OrganizationApprovalNotificationRepositoryError::Invariant)?;
            Some(OrganizationApprovalNotificationCursor::new(
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
            organization_approval_notification_receipts::Entity::find()
                .filter(
                    organization_approval_notification_receipts::Column::RecipientUserId
                        .eq(recipient_user_id),
                )
                .filter(
                    organization_approval_notification_receipts::Column::NotificationId.is_in(ids),
                )
                .all(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?
        };
        let read_at = receipts
            .into_iter()
            .map(|receipt| (receipt.notification_id, receipt.read_at.unix_timestamp()))
            .collect::<std::collections::HashMap<_, _>>();
        let total = organization_approval_notifications::Entity::find()
            .filter(
                organization_approval_notifications::Column::RecipientUserId.eq(recipient_user_id),
            )
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        let read = organization_approval_notification_receipts::Entity::find()
            .filter(
                organization_approval_notification_receipts::Column::RecipientUserId
                    .eq(recipient_user_id),
            )
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        let entries = models
            .into_iter()
            .map(|model| OrganizationApprovalNotificationRecord {
                id: model.id,
                organization_id: model.organization_id,
                request_id: model.request_id,
                decision_id: model.decision_id,
                decision_kind: model.decision_kind,
                template_version: model.template_version,
                occurred_at: model.occurred_at.unix_timestamp(),
                read_at: read_at.get(&model.id).copied(),
            })
            .collect();
        Ok(OrganizationApprovalNotificationPage {
            entries,
            next_cursor,
            unread_count: total.saturating_sub(read),
        })
    }

    async fn mark_read_inner(
        &self,
        recipient_user_id: i64,
        notification_ids: Vec<i64>,
    ) -> Result<
        OrganizationApprovalNotificationMarkReadOutcome,
        OrganizationApprovalNotificationRepositoryError,
    > {
        let matching = organization_approval_notifications::Entity::find()
            .filter(
                organization_approval_notifications::Column::RecipientUserId.eq(recipient_user_id),
            )
            .filter(organization_approval_notifications::Column::Id.is_in(notification_ids.clone()))
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        if matching != notification_ids.len() as u64 {
            return Err(OrganizationApprovalNotificationRepositoryError::NotFound);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let receipts = notification_ids
            .iter()
            .map(
                |notification_id| organization_approval_notification_receipts::ActiveModel {
                    recipient_user_id: Set(recipient_user_id),
                    notification_id: Set(*notification_id),
                    read_at: Set(now),
                },
            )
            .collect::<Vec<_>>();
        organization_approval_notification_receipts::Entity::insert_many(receipts)
            .on_conflict(
                OnConflict::columns([
                    organization_approval_notification_receipts::Column::RecipientUserId,
                    organization_approval_notification_receipts::Column::NotificationId,
                ])
                .do_nothing_on([
                    organization_approval_notification_receipts::Column::RecipientUserId,
                    organization_approval_notification_receipts::Column::NotificationId,
                ])
                .to_owned(),
            )
            .exec_without_returning(self.pool.connection())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        let read = organization_approval_notification_receipts::Entity::find()
            .filter(
                organization_approval_notification_receipts::Column::RecipientUserId
                    .eq(recipient_user_id),
            )
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        let total = organization_approval_notifications::Entity::find()
            .filter(
                organization_approval_notifications::Column::RecipientUserId.eq(recipient_user_id),
            )
            .count(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| OrganizationApprovalNotificationRepositoryError::Query)?;
        Ok(OrganizationApprovalNotificationMarkReadOutcome {
            marked_count: notification_ids.len(),
            unread_count: total.saturating_sub(read),
        })
    }
}
