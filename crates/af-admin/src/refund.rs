use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    DEFAULT_REFUND_ADMIN_PAGE_SIZE, MAX_REFUND_ADMIN_PAGE_SIZE,
    RefundAdminListQuery as DbRefundAdminListQuery, RefundAdminPage, RefundApprovalOutcome,
    RefundRepository, RefundRepositoryError,
};
use af_domain::{
    MAX_REFUND_APPROVAL_REASON_BYTES, RefundApprovalStatus, RefundOrderKind, RefundRequestId,
    RefundRequestRecord, RefundRequestStatus, UserId,
};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

pub const DEFAULT_ADMIN_REFUND_PAGE_SIZE: usize = DEFAULT_REFUND_ADMIN_PAGE_SIZE;
pub const MAX_ADMIN_REFUND_PAGE_SIZE: usize = MAX_REFUND_ADMIN_PAGE_SIZE;

/// 管理员可见的退款请求脱敏快照。
pub struct AdminRefundRequest {
    request_id: RefundRequestId,
    user_id: UserId,
    order_kind: RefundOrderKind,
    order_key: String,
    provider: String,
    currency: String,
    original_amount_minor: i64,
    refund_amount_minor: i64,
    provider_refund_id: Option<String>,
    status: RefundRequestStatus,
    approval_status: RefundApprovalStatus,
    approval_actor_id: Option<UserId>,
    approval_reason: Option<String>,
    version: u64,
    created_at: u64,
    updated_at: u64,
}

impl AdminRefundRequest {
    fn from_record(record: RefundRequestRecord) -> Self {
        Self {
            request_id: record.request_id(),
            user_id: record.user_id(),
            order_kind: record.order_kind(),
            order_key: record.order_key().to_owned(),
            provider: record.provider().to_owned(),
            currency: record.currency().to_owned(),
            original_amount_minor: record.original_amount_minor(),
            refund_amount_minor: record.refund_amount_minor(),
            provider_refund_id: record.provider_refund_id().map(str::to_owned),
            status: record.status(),
            approval_status: record.approval_status(),
            approval_actor_id: record.approval_actor_id(),
            approval_reason: record.approval_reason().map(str::to_owned),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    #[must_use]
    pub const fn request_id(&self) -> RefundRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn order_kind(&self) -> RefundOrderKind {
        self.order_kind
    }
    #[must_use]
    pub fn order_key(&self) -> &str {
        &self.order_key
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn original_amount_minor(&self) -> i64 {
        self.original_amount_minor
    }
    #[must_use]
    pub const fn refund_amount_minor(&self) -> i64 {
        self.refund_amount_minor
    }
    #[must_use]
    pub fn provider_refund_id(&self) -> Option<&str> {
        self.provider_refund_id.as_deref()
    }
    #[must_use]
    pub const fn status(&self) -> RefundRequestStatus {
        self.status
    }
    #[must_use]
    pub const fn approval_status(&self) -> RefundApprovalStatus {
        self.approval_status
    }
    #[must_use]
    pub const fn approval_actor_id(&self) -> Option<UserId> {
        self.approval_actor_id
    }
    #[must_use]
    pub fn approval_reason(&self) -> Option<&str> {
        self.approval_reason.as_deref()
    }
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminRefundRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRefundRequest(<redacted>)")
    }
}

pub struct AdminRefundPage {
    entries: Vec<AdminRefundRequest>,
    next_cursor: Option<i64>,
}

impl AdminRefundPage {
    #[must_use]
    pub fn entries(&self) -> &[AdminRefundRequest] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminRefundPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRefundPage(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRefundListQuery {
    after_id: Option<i64>,
    approval_status: Option<RefundApprovalStatus>,
    limit: usize,
}

impl AdminRefundListQuery {
    pub fn new(
        after_id: Option<i64>,
        approval_status: Option<RefundApprovalStatus>,
        limit: usize,
    ) -> Result<Self, AdminRefundError> {
        if after_id.is_some_and(|value| value <= 0)
            || !(1..=MAX_ADMIN_REFUND_PAGE_SIZE).contains(&limit)
        {
            return Err(AdminRefundError::InvalidInput);
        }
        Ok(Self {
            after_id,
            approval_status,
            limit,
        })
    }
}

impl Default for AdminRefundListQuery {
    fn default() -> Self {
        Self {
            after_id: None,
            approval_status: None,
            limit: DEFAULT_ADMIN_REFUND_PAGE_SIZE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AdminRefundDecisionCommand {
    reason: Option<String>,
}

impl AdminRefundDecisionCommand {
    pub fn new(reason: Option<String>) -> Result<Self, AdminRefundError> {
        if reason.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.trim() != value
                || value.len() > MAX_REFUND_APPROVAL_REASON_BYTES
                || value.chars().any(char::is_control)
        }) {
            return Err(AdminRefundError::InvalidInput);
        }
        Ok(Self { reason })
    }
}

pub type AdminRefundListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRefundPage, AdminRefundError>> + Send + 'a>>;
pub type AdminRefundActionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRefundPage, AdminRefundError>> + Send + 'a>>;

/// 将已批准退款提交给当前运行时 Provider，并复用同一 CAS 编排器。
pub trait AdminRefundSubmitter: Send + Sync {
    fn auto_submit_enabled<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<bool, AdminRefundError>> + Send + 'a>>;
    fn submit<'a>(
        &'a self,
        record: &'a RefundRequestRecord,
        now: u64,
    ) -> Pin<Box<dyn Future<Output = Result<RefundRequestRecord, AdminRefundError>> + Send + 'a>>;
}

pub trait AdminRefundService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRefundListQuery,
    ) -> AdminRefundListFuture<'a>;
    fn approve<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundDecisionCommand,
    ) -> AdminRefundActionFuture<'a>;
    fn reject<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundDecisionCommand,
    ) -> AdminRefundActionFuture<'a>;
    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
    ) -> AdminRefundActionFuture<'a>;
}

#[derive(Clone)]
pub struct DatabaseAdminRefundService {
    repository: RefundRepository,
    submitter: Option<Arc<dyn AdminRefundSubmitter>>,
}

impl DatabaseAdminRefundService {
    #[must_use]
    pub fn new(
        repository: RefundRepository,
        submitter: Option<Arc<dyn AdminRefundSubmitter>>,
    ) -> Self {
        Self {
            repository,
            submitter,
        }
    }
}

impl AdminRefundService for DatabaseAdminRefundService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRefundListQuery,
    ) -> AdminRefundListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list_admin_requests(DbRefundAdminListQuery::new(
                    query.after_id,
                    query.approval_status,
                    query.limit,
                ))
                .await
                .map_err(map_repository_error)?;
            Ok(to_page(page))
        })
    }

    fn approve<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundDecisionCommand,
    ) -> AdminRefundActionFuture<'a> {
        self.decide(principal, request_id, command, true)
    }

    fn reject<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundDecisionCommand,
    ) -> AdminRefundActionFuture<'a> {
        self.decide(principal, request_id, command, false)
    }

    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
    ) -> AdminRefundActionFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let record = self
                .repository
                .get_request(request_id)
                .await
                .map_err(map_repository_error)?
                .ok_or(AdminRefundError::NotFound)?;
            let submitter = self
                .submitter
                .as_ref()
                .ok_or(AdminRefundError::Unavailable)?;
            let record = submitter.submit(&record, now_seconds()).await?;
            Ok(to_page_from_record(record))
        })
    }
}

impl DatabaseAdminRefundService {
    fn decide<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundDecisionCommand,
        approve: bool,
    ) -> AdminRefundActionFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let actor_id = principal.user_id();
            let outcome = if approve {
                self.repository
                    .approve_request(request_id, actor_id, command.reason, now_seconds())
                    .await
            } else {
                self.repository
                    .reject_request(request_id, actor_id, command.reason, now_seconds())
                    .await
            };
            let record = match outcome.map_err(map_repository_error)? {
                RefundApprovalOutcome::Applied(record)
                | RefundApprovalOutcome::Existing(record) => record,
            };
            if approve {
                if let Some(submitter) = &self.submitter {
                    if submitter.auto_submit_enabled().await? {
                        let record = submitter.submit(&record, now_seconds()).await?;
                        return Ok(to_page_from_record(record));
                    }
                }
            }
            Ok(to_page_from_record(record))
        })
    }
}

fn to_page(page: RefundAdminPage) -> AdminRefundPage {
    let (entries, next_cursor) = page.into_parts();
    AdminRefundPage {
        entries: entries
            .into_iter()
            .map(AdminRefundRequest::from_record)
            .collect(),
        next_cursor,
    }
}

fn to_page_from_record(record: RefundRequestRecord) -> AdminRefundPage {
    AdminRefundPage {
        entries: vec![AdminRefundRequest::from_record(record)],
        next_cursor: None,
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminRefundError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminRefundError::Forbidden)
    }
}

fn now_seconds() -> u64 {
    u64::try_from(af_db::DatabaseTimestamp::now_utc().unix_timestamp()).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminRefundError {
    #[error("退款管理请求无效")]
    InvalidInput,
    #[error("退款管理权限不足")]
    Forbidden,
    #[error("退款请求不存在")]
    NotFound,
    #[error("退款请求状态冲突")]
    Conflict,
    #[error("退款能力不可用")]
    Unavailable,
    #[error("退款自动提交失败")]
    AutoSubmitFailed,
    #[error("退款结果未知")]
    OutcomeUnknown,
    #[error("退款管理内部失败")]
    Internal,
}

fn map_repository_error(error: RefundRepositoryError) -> AdminRefundError {
    match error {
        RefundRepositoryError::Conflict => AdminRefundError::Conflict,
        RefundRepositoryError::NotFound => AdminRefundError::NotFound,
        RefundRepositoryError::OutcomeUnknown => AdminRefundError::OutcomeUnknown,
        RefundRepositoryError::Query
        | RefundRepositoryError::Timeout
        | RefundRepositoryError::Invariant => AdminRefundError::Internal,
    }
}

impl fmt::Debug for DatabaseAdminRefundService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminRefundService(<redacted>)")
    }
}
