use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationApprovalNotificationCursor as DatabaseCursor,
    OrganizationApprovalNotificationListQuery as DatabaseQuery,
    OrganizationApprovalNotificationMarkReadOutcome as DatabaseMarkReadOutcome,
    OrganizationApprovalNotificationPage as DatabasePage,
    OrganizationApprovalNotificationRecord as DatabaseRecord,
    OrganizationApprovalNotificationRepository, OrganizationApprovalNotificationRepositoryError,
};
use af_domain::OrganizationApprovalDecisionKind;
use thiserror::Error;

use crate::SessionPrincipal;

pub const DEFAULT_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE: usize = 25;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalNotificationDecision {
    Approved,
    Rejected,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalNotificationCursor {
    occurred_at: i64,
    id: i64,
}

impl OrganizationApprovalNotificationCursor {
    pub const fn new(
        occurred_at: i64,
        id: i64,
    ) -> Result<Self, OrganizationApprovalNotificationError> {
        if occurred_at <= 0 || id <= 0 {
            return Err(OrganizationApprovalNotificationError::InvalidInput);
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
    ) -> Result<Self, OrganizationApprovalNotificationError> {
        if !(1..=af_db::MAX_ORGANIZATION_APPROVAL_NOTIFICATION_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationApprovalNotificationError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

pub struct OrganizationApprovalNotification {
    record: DatabaseRecord,
}

impl OrganizationApprovalNotification {
    fn from_record(record: DatabaseRecord) -> Result<Self, OrganizationApprovalNotificationError> {
        OrganizationApprovalDecisionKind::try_from(record.decision_kind())
            .map_err(|_| OrganizationApprovalNotificationError::Internal)?;
        Ok(Self { record })
    }
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.record.id()
    }
    #[must_use]
    pub const fn organization_id(&self) -> i64 {
        self.record.organization_id()
    }
    #[must_use]
    pub const fn request_id(&self) -> i64 {
        self.record.request_id()
    }
    #[must_use]
    pub const fn decision_id(&self) -> i64 {
        self.record.decision_id()
    }
    #[must_use]
    pub fn decision(&self) -> OrganizationApprovalNotificationDecision {
        match self.record.decision_kind() {
            1 => OrganizationApprovalNotificationDecision::Approved,
            2 => OrganizationApprovalNotificationDecision::Rejected,
            _ => OrganizationApprovalNotificationDecision::TimedOut,
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
    pub const fn read_at(&self) -> Option<i64> {
        self.record.read_at()
    }
}

pub struct OrganizationApprovalNotificationPage {
    entries: Vec<OrganizationApprovalNotification>,
    next_cursor: Option<OrganizationApprovalNotificationCursor>,
    unread_count: u64,
}

impl OrganizationApprovalNotificationPage {
    fn from_record(page: DatabasePage) -> Result<Self, OrganizationApprovalNotificationError> {
        let entries = page
            .entries()
            .iter()
            .cloned()
            .map(OrganizationApprovalNotification::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = page
            .next_cursor()
            .map(|cursor| OrganizationApprovalNotificationCursor {
                occurred_at: cursor.occurred_at(),
                id: cursor.id(),
            });
        Ok(Self {
            entries,
            next_cursor,
            unread_count: page.unread_count(),
        })
    }
    #[must_use]
    pub fn entries(&self) -> &[OrganizationApprovalNotification] {
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
pub struct OrganizationApprovalNotificationMarkReadResult {
    marked_count: usize,
    unread_count: u64,
}

impl OrganizationApprovalNotificationMarkReadResult {
    fn from_record(value: DatabaseMarkReadOutcome) -> Self {
        Self {
            marked_count: value.marked_count(),
            unread_count: value.unread_count(),
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
pub enum OrganizationApprovalNotificationError {
    #[error("企业审批通知请求无效")]
    InvalidInput,
    #[error("企业审批通知服务内部错误")]
    Internal,
}

pub type OrganizationApprovalNotificationListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalNotificationPage,
                    OrganizationApprovalNotificationError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationApprovalNotificationMarkReadFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalNotificationMarkReadResult,
                    OrganizationApprovalNotificationError,
                >,
            > + Send
            + 'a,
    >,
>;

pub trait OrganizationApprovalNotificationService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationApprovalNotificationListQuery,
    ) -> OrganizationApprovalNotificationListFuture<'a>;
    fn mark_read<'a>(
        &'a self,
        principal: SessionPrincipal,
        ids: Vec<i64>,
    ) -> OrganizationApprovalNotificationMarkReadFuture<'a>;
}

pub struct DatabaseOrganizationApprovalNotificationService {
    repository: Arc<OrganizationApprovalNotificationRepository>,
}

impl DatabaseOrganizationApprovalNotificationService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationApprovalNotificationRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationApprovalNotificationService for DatabaseOrganizationApprovalNotificationService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationApprovalNotificationListQuery,
    ) -> OrganizationApprovalNotificationListFuture<'a> {
        Box::pin(async move {
            let before = query
                .before
                .map(|cursor| DatabaseCursor::new(cursor.occurred_at, cursor.id))
                .transpose()
                .map_err(map_error)?;
            let query = DatabaseQuery::new(before, query.limit).map_err(map_error)?;
            let page = self
                .repository
                .list(principal.user_id().get(), query)
                .await
                .map_err(map_error)?;
            OrganizationApprovalNotificationPage::from_record(page)
        })
    }

    fn mark_read<'a>(
        &'a self,
        principal: SessionPrincipal,
        ids: Vec<i64>,
    ) -> OrganizationApprovalNotificationMarkReadFuture<'a> {
        Box::pin(async move {
            self.repository
                .mark_read(principal.user_id().get(), ids)
                .await
                .map(OrganizationApprovalNotificationMarkReadResult::from_record)
                .map_err(map_error)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationApprovalNotificationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationApprovalNotificationService(<redacted>)")
    }
}

fn map_error(
    error: OrganizationApprovalNotificationRepositoryError,
) -> OrganizationApprovalNotificationError {
    match error {
        OrganizationApprovalNotificationRepositoryError::InvalidInput
        | OrganizationApprovalNotificationRepositoryError::NotFound => {
            OrganizationApprovalNotificationError::InvalidInput
        }
        OrganizationApprovalNotificationRepositoryError::Query
        | OrganizationApprovalNotificationRepositoryError::Invariant
        | OrganizationApprovalNotificationRepositoryError::Timeout => {
            OrganizationApprovalNotificationError::Internal
        }
    }
}
