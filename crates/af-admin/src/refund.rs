use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    DEFAULT_REFUND_ADMIN_PAGE_SIZE, MAX_REFUND_ADMIN_PAGE_SIZE,
    RefundAdminListQuery as DbRefundAdminListQuery, RefundAdminPage, RefundApprovalOutcome,
    RefundReconciliationPage as DbRefundReconciliationPage,
    RefundReconciliationQuery as DbRefundReconciliationQuery, RefundReconciliationRecord,
    RefundRepository, RefundRepositoryError,
};
use af_domain::{
    MAX_REFUND_APPROVAL_REASON_BYTES, OrganizationId, RefundApprovalStatus, RefundManualCompletion,
    RefundManualResult, RefundOrderKind, RefundRequestId, RefundRequestKey, RefundRequestRecord,
    RefundRequestStatus, UserId,
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

/// 退款对账读取默认分页数量。
pub const DEFAULT_REFUND_RECONCILIATION_PAGE_SIZE: usize = 50;
/// 退款对账单页硬上限。
pub const MAX_REFUND_RECONCILIATION_PAGE_SIZE: usize = 100;

/// 已校验的退款对账稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefundReconciliationListQuery {
    before_id: Option<i64>,
    limit: usize,
}

impl RefundReconciliationListQuery {
    pub fn new(before_id: Option<i64>, limit: usize) -> Result<Self, AdminRefundError> {
        if before_id.is_some_and(|value| value <= 0)
            || !(1..=MAX_REFUND_RECONCILIATION_PAGE_SIZE).contains(&limit)
        {
            return Err(AdminRefundError::InvalidInput);
        }
        Ok(Self { before_id, limit })
    }
}

impl Default for RefundReconciliationListQuery {
    fn default() -> Self {
        Self {
            before_id: None,
            limit: DEFAULT_REFUND_RECONCILIATION_PAGE_SIZE,
        }
    }
}

/// HTTP 层可读取的一条脱敏退款对账事实。
pub struct RefundReconciliationEntry {
    id: i64,
    request_id: RefundRequestId,
    user_id: UserId,
    organization_id: Option<OrganizationId>,
    approval_actor_id: UserId,
    order_kind: RefundOrderKind,
    order_key: String,
    provider: String,
    amount_delta_minor: i64,
    currency: String,
    created_at: u64,
}

impl RefundReconciliationEntry {
    fn from_record(record: RefundReconciliationRecord) -> Self {
        Self {
            id: record.id(),
            request_id: record.request_id(),
            user_id: record.user_id(),
            organization_id: record.organization_id(),
            approval_actor_id: record.approval_actor_id(),
            order_kind: record.order_kind(),
            order_key: record.order_key().to_owned(),
            provider: record.provider().to_owned(),
            amount_delta_minor: record.amount_delta_minor(),
            currency: record.currency().to_owned(),
            created_at: record.created_at(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
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
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }
    #[must_use]
    pub const fn approval_actor_id(&self) -> UserId {
        self.approval_actor_id
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
    pub const fn amount_delta_minor(&self) -> i64 {
        self.amount_delta_minor
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

impl fmt::Debug for RefundReconciliationEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundReconciliationEntry(<redacted>)")
    }
}

/// 一页已经按个人、企业或平台管理员作用域裁剪的退款对账事实。
pub struct RefundReconciliationPage {
    entries: Vec<RefundReconciliationEntry>,
    next_cursor: Option<i64>,
}

impl RefundReconciliationPage {
    #[must_use]
    pub fn entries(&self) -> &[RefundReconciliationEntry] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for RefundReconciliationPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundReconciliationPage(<redacted>)")
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

/// 管理员登记易支付线下退款结果的命令。
#[derive(Clone, Debug)]
pub struct AdminRefundManualCompletionCommand {
    completion_key: RefundRequestKey,
    expected_version: u64,
    result: RefundManualResult,
    reference: String,
}

impl AdminRefundManualCompletionCommand {
    pub fn new(
        completion_key: RefundRequestKey,
        expected_version: u64,
        result: RefundManualResult,
        reference: String,
    ) -> Result<Self, AdminRefundError> {
        if expected_version == 0
            || reference.is_empty()
            || reference.trim() != reference
            || reference.len() > af_domain::MAX_REFUND_MANUAL_REFERENCE_BYTES
            || reference.chars().any(char::is_control)
        {
            return Err(AdminRefundError::InvalidInput);
        }
        Ok(Self {
            completion_key,
            expected_version,
            result,
            reference,
        })
    }
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
pub type RefundReconciliationListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RefundReconciliationPage, AdminRefundError>> + Send + 'a>>;

/// 将已批准退款提交给当前运行时 Provider，并复用同一 CAS 编排器。
pub trait AdminRefundSubmitter: Send + Sync {
    /// 返回指定 Provider 是否允许进入人工退款登记边界。
    fn manual_refund_enabled<'a>(
        &'a self,
        provider: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, AdminRefundError>> + Send + 'a>> {
        let _ = provider;
        Box::pin(async { Ok(false) })
    }

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
    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundManualCompletionCommand,
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
    fn list_user_reconciliations<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: RefundReconciliationListQuery,
    ) -> RefundReconciliationListFuture<'a>;
    fn list_admin_reconciliations<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: RefundReconciliationListQuery,
    ) -> RefundReconciliationListFuture<'a>;
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

    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundManualCompletionCommand,
    ) -> AdminRefundActionFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let submitter = self
                .submitter
                .as_ref()
                .ok_or(AdminRefundError::Unavailable)?;
            if !submitter.manual_refund_enabled("epay").await? {
                return Err(AdminRefundError::Unavailable);
            }
            // 不在服务层预读并拒绝终态：仓储必须先按完成键回读，保证重试幂等。
            let completion = RefundManualCompletion::new(
                request_id,
                command.completion_key,
                command.expected_version,
                principal.user_id(),
                command.result,
                command.reference,
                now_seconds(),
            )
            .map_err(|_| AdminRefundError::InvalidInput)?;
            let result = self
                .repository
                .complete_manual_refund(completion)
                .await
                .map_err(map_repository_error)?;
            let record = match result {
                af_db::RefundSubmissionOutcome::Applied(record)
                | af_db::RefundSubmissionOutcome::Existing(record) => record,
            };
            Ok(to_page_from_record(record))
        })
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

    fn complete_manual<'a>(
        &'a self,
        principal: SessionPrincipal,
        request_id: RefundRequestId,
        command: AdminRefundManualCompletionCommand,
    ) -> AdminRefundActionFuture<'a> {
        DatabaseAdminRefundService::complete_manual(self, principal, request_id, command)
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
            if record.provider() == "epay" {
                return Err(AdminRefundError::Conflict);
            }
            let submitter = self
                .submitter
                .as_ref()
                .ok_or(AdminRefundError::Unavailable)?;
            let record = submitter.submit(&record, now_seconds()).await?;
            Ok(to_page_from_record(record))
        })
    }

    fn list_user_reconciliations<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: RefundReconciliationListQuery,
    ) -> RefundReconciliationListFuture<'a> {
        Box::pin(async move {
            let page = self
                .repository
                .list_user_reconciliations(
                    principal.user_id(),
                    DbRefundReconciliationQuery::new(query.before_id, query.limit),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(to_reconciliation_page(page))
        })
    }

    fn list_admin_reconciliations<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: RefundReconciliationListQuery,
    ) -> RefundReconciliationListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list_admin_reconciliations(DbRefundReconciliationQuery::new(
                    query.before_id,
                    query.limit,
                ))
                .await
                .map_err(map_repository_error)?;
            Ok(to_reconciliation_page(page))
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
            let (record, applied) = match outcome.map_err(map_repository_error)? {
                RefundApprovalOutcome::Applied(record) => (record, true),
                RefundApprovalOutcome::Existing(record) => (record, false),
            };
            if approve
                && applied
                && record.provider() != "epay"
                && let Some(submitter) = &self.submitter
                && submitter.auto_submit_enabled().await?
            {
                let record = submitter.submit(&record, now_seconds()).await?;
                return Ok(to_page_from_record(record));
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

fn to_reconciliation_page(page: DbRefundReconciliationPage) -> RefundReconciliationPage {
    let (entries, next_cursor) = page.into_parts();
    RefundReconciliationPage {
        entries: entries
            .into_iter()
            .map(RefundReconciliationEntry::from_record)
            .collect(),
        next_cursor,
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
