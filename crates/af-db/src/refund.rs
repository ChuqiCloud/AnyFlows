use std::time::Duration;

use sha2::{Digest, Sha256};

use af_domain::{
    MAX_REFUND_APPROVAL_REASON_BYTES, OrganizationId, RefundApprovalStatus, RefundManualCompletion,
    RefundManualResult, RefundOrderKind, RefundRequestCreate, RefundRequestCreateOutcome,
    RefundRequestId, RefundRequestKey, RefundRequestRecord, RefundRequestStatus,
    SubscriptionOrderStatus, TopupOrderStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, refund_manual_completions, refund_provider_events,
        refund_reconciliation_entries, refund_requests, subscription_orders, topup_orders,
    },
};

/// 退款请求事实仓储。
///
/// 首切片只建立和读取事实，不调用 Provider、不修改钱包账本；后续 Provider 切片
/// 以 `status + version` 为 CAS 边界继续推进状态。
#[derive(Clone)]
pub struct RefundRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl RefundRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, RefundRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(RefundRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等创建退款请求；同一用户的幂等键和订单各自只能绑定同一事实。
    pub async fn create_request(
        &self,
        write: RefundRequestCreate,
    ) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
        let operation = create_request_inner(self.pool.connection(), &write);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 按稳定退款请求标识读取事实。
    pub async fn get_request(
        &self,
        request_id: RefundRequestId,
    ) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
        let operation = load_by_request_key(self.pool.connection(), request_id);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 按当前用户和幂等键读取事实，供结果未知恢复使用。
    pub async fn get_by_idempotency_key(
        &self,
        user_id: UserId,
        key: RefundRequestKey,
    ) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
        let operation = load_by_idempotency_key(self.pool.connection(), user_id, key);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 按数据库 ID 倒序分页读取管理员可见的退款请求。
    pub async fn list_admin_requests(
        &self,
        query: RefundAdminListQuery,
    ) -> Result<RefundAdminPage, RefundRepositoryError> {
        let operation = list_admin_requests_inner(self.pool.connection(), query);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 以版本和审批状态 CAS 批准退款请求，重复批准只返回已有事实。
    pub async fn approve_request(
        &self,
        request_id: RefundRequestId,
        actor_id: UserId,
        reason: Option<String>,
        updated_at: u64,
    ) -> Result<RefundApprovalOutcome, RefundRepositoryError> {
        let operation = apply_approval_inner(
            self.pool.connection(),
            request_id,
            actor_id,
            reason,
            RefundApprovalStatus::Approved,
            updated_at,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 以版本和审批状态 CAS 拒绝退款请求，并关闭尚未提交的 Provider 生命周期。
    pub async fn reject_request(
        &self,
        request_id: RefundRequestId,
        actor_id: UserId,
        reason: Option<String>,
        updated_at: u64,
    ) -> Result<RefundApprovalOutcome, RefundRepositoryError> {
        let operation = apply_approval_inner(
            self.pool.connection(),
            request_id,
            actor_id,
            reason,
            RefundApprovalStatus::Rejected,
            updated_at,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 把请求占位推进到 Submitted；Provider 调用发生在该 CAS 之后，避免并发重复出资。
    pub async fn claim_submission(
        &self,
        request_id: RefundRequestId,
        expected_version: u64,
        updated_at: u64,
    ) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
        let operation = claim_submission_inner(
            self.pool.connection(),
            request_id,
            expected_version,
            updated_at,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 绑定 Provider 退款标识；只接受 Submitted 状态的严格版本 CAS。
    pub async fn bind_provider_refund_id(
        &self,
        request_id: RefundRequestId,
        expected_version: u64,
        provider_refund_id: String,
        updated_at: u64,
    ) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
        let operation = bind_provider_refund_id_inner(
            self.pool.connection(),
            request_id,
            expected_version,
            provider_refund_id,
            updated_at,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 记录 Provider 明确失败；结果未知不应调用此方法。
    pub async fn mark_failed(
        &self,
        request_id: RefundRequestId,
        expected_version: u64,
        updated_at: u64,
    ) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
        let operation = transition_submission(
            self.pool.connection(),
            request_id,
            expected_version,
            RefundRequestStatus::Failed,
            updated_at,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 在事务内追加已验签回执并 CAS 推进退款状态。
    pub async fn apply_receipt(
        &self,
        write: RefundReceiptWrite,
    ) -> Result<RefundReceiptOutcome, RefundRepositoryError> {
        let operation = apply_receipt_inner(self.pool.connection(), &write);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 记录易支付线下退款结果；人工事实与请求状态在同一事务内提交。
    pub async fn complete_manual_refund(
        &self,
        completion: RefundManualCompletion,
    ) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
        let operation = complete_manual_refund_inner(self.pool.connection(), &completion);
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::OutcomeUnknown),
        }
    }

    /// 按当前用户读取个人退款对账事实；企业订单不会混入个人结果。
    pub async fn list_user_reconciliations(
        &self,
        user_id: UserId,
        query: RefundReconciliationQuery,
    ) -> Result<RefundReconciliationPage, RefundRepositoryError> {
        let operation = list_reconciliations_inner(
            self.pool.connection(),
            RefundReconciliationScope::User(user_id),
            query,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 按企业资金主体读取退款对账事实，调用方不能省略企业作用域。
    pub async fn list_organization_reconciliations(
        &self,
        organization_id: OrganizationId,
        query: RefundReconciliationQuery,
    ) -> Result<RefundReconciliationPage, RefundRepositoryError> {
        let operation = list_reconciliations_inner(
            self.pool.connection(),
            RefundReconciliationScope::Organization(organization_id),
            query,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }

    /// 读取平台管理员可见的全局退款对账事实。
    ///
    /// 该入口不自行授予权限，只能由已校验固定退款管理权限的服务层调用。
    pub async fn list_admin_reconciliations(
        &self,
        query: RefundReconciliationQuery,
    ) -> Result<RefundReconciliationPage, RefundRepositoryError> {
        let operation = list_reconciliations_inner(
            self.pool.connection(),
            RefundReconciliationScope::Admin,
            query,
        );
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(RefundRepositoryError::Timeout),
        }
    }
}

/// 管理员退款列表默认分页边界。
pub const DEFAULT_REFUND_ADMIN_PAGE_SIZE: usize = 50;
pub const MAX_REFUND_ADMIN_PAGE_SIZE: usize = 100;

/// 管理员退款列表查询，使用 ID 游标避免时间戳并列漂移。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefundAdminListQuery {
    after_id: Option<i64>,
    approval_status: Option<RefundApprovalStatus>,
    limit: usize,
}

impl RefundAdminListQuery {
    #[must_use]
    pub const fn new(
        after_id: Option<i64>,
        approval_status: Option<RefundApprovalStatus>,
        limit: usize,
    ) -> Self {
        Self {
            after_id,
            approval_status,
            limit,
        }
    }
    #[must_use]
    pub const fn after_id(self) -> Option<i64> {
        self.after_id
    }
    #[must_use]
    pub const fn approval_status(self) -> Option<RefundApprovalStatus> {
        self.approval_status
    }
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for RefundAdminListQuery {
    fn default() -> Self {
        Self::new(None, None, DEFAULT_REFUND_ADMIN_PAGE_SIZE)
    }
}

/// 管理员退款列表页，仅返回脱敏领域快照。
#[derive(Debug)]
pub struct RefundAdminPage {
    entries: Vec<RefundRequestRecord>,
    next_cursor: Option<i64>,
}

impl RefundAdminPage {
    #[must_use]
    pub fn into_parts(self) -> (Vec<RefundRequestRecord>, Option<i64>) {
        (self.entries, self.next_cursor)
    }
    #[must_use]
    pub fn entries(&self) -> &[RefundRequestRecord] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

/// 审批 CAS 的结果，重复请求不再次写入或触发资金动作。
#[derive(Debug)]
pub enum RefundApprovalOutcome {
    Applied(RefundRequestRecord),
    Existing(RefundRequestRecord),
}

pub enum RefundSubmissionOutcome {
    Applied(RefundRequestRecord),
    Existing(RefundRequestRecord),
}

/// 已验签回执的规范持久化输入；不允许携带原始 payload 或签名。
#[derive(Clone, Debug)]
pub struct RefundReceiptWrite {
    pub event_key: String,
    pub request_id: RefundRequestId,
    pub provider: String,
    pub provider_event_id: String,
    pub provider_refund_id: String,
    pub status: RefundRequestStatus,
    pub amount_minor: i64,
    pub currency: String,
    pub signature_key_fingerprint: String,
    pub payload_sha256: String,
    pub received_at: u64,
    pub processed_at: u64,
    pub created_at: u64,
}

/// 回执应用结果。
#[derive(Debug)]
pub enum RefundReceiptOutcome {
    Applied(RefundRequestRecord),
    Existing(RefundRequestRecord),
    NotFound,
}

/// 退款对账列表的稳定主键游标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefundReconciliationQuery {
    before_id: Option<i64>,
    limit: usize,
}

impl RefundReconciliationQuery {
    #[must_use]
    pub const fn new(before_id: Option<i64>, limit: usize) -> Self {
        Self { before_id, limit }
    }

    #[must_use]
    pub const fn before_id(self) -> Option<i64> {
        self.before_id
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for RefundReconciliationQuery {
    fn default() -> Self {
        Self::new(None, DEFAULT_REFUND_ADMIN_PAGE_SIZE)
    }
}

/// 一条不可变退款负向现金对账事实。
pub struct RefundReconciliationRecord {
    id: i64,
    request_id: RefundRequestId,
    provider_event_id: Option<i64>,
    manual_completion_id: Option<i64>,
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

impl RefundReconciliationRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn request_id(&self) -> RefundRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn provider_event_id(&self) -> Option<i64> {
        self.provider_event_id
    }
    #[must_use]
    pub const fn manual_completion_id(&self) -> Option<i64> {
        self.manual_completion_id
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

impl std::fmt::Debug for RefundReconciliationRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RefundReconciliationRecord(<redacted>)")
    }
}

/// 一页作用域内退款对账事实。
#[derive(Debug)]
pub struct RefundReconciliationPage {
    entries: Vec<RefundReconciliationRecord>,
    next_cursor: Option<i64>,
}

impl RefundReconciliationPage {
    #[must_use]
    pub fn into_parts(self) -> (Vec<RefundReconciliationRecord>, Option<i64>) {
        (self.entries, self.next_cursor)
    }
    #[must_use]
    pub fn entries(&self) -> &[RefundReconciliationRecord] {
        &self.entries
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

#[derive(Clone, Copy)]
enum RefundReconciliationScope {
    User(UserId),
    Organization(OrganizationId),
    Admin,
}

async fn list_reconciliations_inner(
    connection: &DatabaseConnection,
    scope: RefundReconciliationScope,
    query: RefundReconciliationQuery,
) -> Result<RefundReconciliationPage, RefundRepositoryError> {
    if query.before_id().is_some_and(|value| value <= 0) {
        return Err(RefundRepositoryError::Invariant);
    }
    let limit = query.limit().clamp(1, MAX_REFUND_ADMIN_PAGE_SIZE);
    let mut statement = refund_reconciliation_entries::Entity::find();
    statement = match scope {
        RefundReconciliationScope::User(user_id) => statement
            .filter(refund_reconciliation_entries::Column::UserId.eq(user_id.get()))
            .filter(refund_reconciliation_entries::Column::OrganizationId.is_null()),
        RefundReconciliationScope::Organization(organization_id) => statement.filter(
            refund_reconciliation_entries::Column::OrganizationId.eq(organization_id.get()),
        ),
        RefundReconciliationScope::Admin => statement,
    };
    if let Some(before_id) = query.before_id() {
        statement = statement.filter(refund_reconciliation_entries::Column::Id.lt(before_id));
    }
    let mut models = statement
        .order_by_desc(refund_reconciliation_entries::Column::Id)
        .limit((limit + 1) as u64)
        .all(connection)
        .await
        .map_err(map_query)?;
    let has_more = models.len() > limit;
    models.truncate(limit);
    let next_cursor = has_more.then(|| models.last().expect("非空分页必须有末条记录").id);
    let entries = models
        .into_iter()
        .map(to_reconciliation_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RefundReconciliationPage {
        entries,
        next_cursor,
    })
}

async fn list_admin_requests_inner(
    connection: &DatabaseConnection,
    query: RefundAdminListQuery,
) -> Result<RefundAdminPage, RefundRepositoryError> {
    if query.after_id().is_some_and(|value| value <= 0) {
        return Err(RefundRepositoryError::Invariant);
    }
    let limit = query.limit().clamp(1, MAX_REFUND_ADMIN_PAGE_SIZE);
    let mut statement = refund_requests::Entity::find();
    if let Some(after_id) = query.after_id() {
        statement = statement.filter(refund_requests::Column::Id.lt(after_id));
    }
    if let Some(status) = query.approval_status() {
        statement = statement.filter(refund_requests::Column::ApprovalStatus.eq(status.code()));
    }
    let mut models = statement
        .order_by_desc(refund_requests::Column::Id)
        .limit((limit + 1) as u64)
        .all(connection)
        .await
        .map_err(map_query)?;
    let has_more = models.len() > limit;
    models.truncate(limit);
    let next_cursor = has_more.then(|| models.last().expect("非空分页必须有末条记录").id);
    let entries = models
        .into_iter()
        .map(to_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RefundAdminPage {
        entries,
        next_cursor,
    })
}

async fn apply_approval_inner(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
    actor_id: UserId,
    reason: Option<String>,
    target: RefundApprovalStatus,
    updated_at: u64,
) -> Result<RefundApprovalOutcome, RefundRepositoryError> {
    if reason.as_deref().is_some_and(|value| {
        value.is_empty()
            || value.len() > MAX_REFUND_APPROVAL_REASON_BYTES
            || value.trim() != value
            || value.chars().any(char::is_control)
    }) {
        return Err(RefundRepositoryError::Invariant);
    }
    let Some(current) = load_model_by_request_key(connection, request_id).await? else {
        return Err(RefundRepositoryError::NotFound);
    };
    let record = to_record(current.clone())?;
    if updated_at < record.created_at() {
        return Err(RefundRepositoryError::Invariant);
    }
    if record.approval_status() == target {
        return Ok(RefundApprovalOutcome::Existing(record));
    }
    if record.approval_status() != RefundApprovalStatus::Pending
        || record.status() != RefundRequestStatus::Requested
    {
        return Err(RefundRepositoryError::Conflict);
    }
    let next_version = record
        .version()
        .checked_add(1)
        .ok_or(RefundRepositoryError::Invariant)?;
    let updated_at = to_database_time(updated_at)?;
    let mut update = refund_requests::Entity::update_many()
        .filter(refund_requests::Column::Id.eq(record.database_id()))
        .filter(refund_requests::Column::Version.eq(record.version() as i64))
        .filter(refund_requests::Column::ApprovalStatus.eq(RefundApprovalStatus::Pending.code()))
        .filter(refund_requests::Column::Status.eq(RefundRequestStatus::Requested.code()))
        .col_expr(
            refund_requests::Column::ApprovalStatus,
            Expr::value(target.code()),
        )
        .col_expr(
            refund_requests::Column::ApprovalActorId,
            Expr::value(actor_id.get()),
        )
        .col_expr(refund_requests::Column::ApprovalReason, Expr::value(reason))
        .col_expr(
            refund_requests::Column::Version,
            Expr::value(i64::try_from(next_version).map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .col_expr(refund_requests::Column::UpdatedAt, Expr::value(updated_at));
    if target == RefundApprovalStatus::Rejected {
        update = update.col_expr(
            refund_requests::Column::Status,
            Expr::value(RefundRequestStatus::Canceled.code()),
        );
    }
    let result = update.exec(connection).await.map_err(map_query)?;
    if result.rows_affected != 1 {
        return Err(RefundRepositoryError::Conflict);
    }
    let model = load_model_by_request_key(connection, request_id)
        .await?
        .ok_or(RefundRepositoryError::OutcomeUnknown)?;
    Ok(RefundApprovalOutcome::Applied(to_record(model)?))
}

async fn claim_submission_inner(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
    expected_version: u64,
    updated_at: u64,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    let Some(current) = load_model_by_request_key(connection, request_id).await? else {
        return Err(RefundRepositoryError::NotFound);
    };
    let record = to_record(current.clone())?;
    if updated_at < record.created_at() {
        return Err(RefundRepositoryError::Invariant);
    }
    if record.status() == RefundRequestStatus::Submitted
        && (record.version() == expected_version
            || record.version() == expected_version.saturating_add(1))
    {
        return Ok(RefundSubmissionOutcome::Existing(record));
    }
    if record.version() != expected_version
        || !matches!(
            record.status(),
            RefundRequestStatus::Requested | RefundRequestStatus::Failed
        )
        || record.provider_refund_id().is_some()
    {
        return Err(RefundRepositoryError::Conflict);
    }
    transition_submission(
        connection,
        request_id,
        expected_version,
        RefundRequestStatus::Submitted,
        updated_at,
    )
    .await
}

async fn bind_provider_refund_id_inner(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
    expected_version: u64,
    provider_refund_id: String,
    updated_at: u64,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    if !valid_provider_refund_id(&provider_refund_id) {
        return Err(RefundRepositoryError::Invariant);
    }
    let Some(current) = load_model_by_request_key(connection, request_id).await? else {
        return Err(RefundRepositoryError::NotFound);
    };
    let record = to_record(current.clone())?;
    if updated_at < record.created_at() {
        return Err(RefundRepositoryError::Invariant);
    }
    if record.status() == RefundRequestStatus::Submitted
        && record.provider_refund_id() == Some(provider_refund_id.as_str())
        && record.version() == expected_version.saturating_add(1)
    {
        return Ok(RefundSubmissionOutcome::Existing(record));
    }
    if record.version() != expected_version
        || record.status() != RefundRequestStatus::Submitted
        || record.provider_refund_id().is_some()
    {
        return Err(RefundRepositoryError::Conflict);
    }
    let updated_at = to_database_time(updated_at)?;
    let next_version = expected_version
        .checked_add(1)
        .ok_or(RefundRepositoryError::Invariant)?;
    let result = refund_requests::Entity::update_many()
        .filter(refund_requests::Column::RequestKey.eq(request_id.persistence_key()))
        .filter(
            refund_requests::Column::Version
                .eq(i64::try_from(expected_version)
                    .map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .filter(refund_requests::Column::Status.eq(RefundRequestStatus::Submitted.code()))
        .filter(refund_requests::Column::ProviderRefundId.is_null())
        .col_expr(
            refund_requests::Column::ProviderRefundId,
            Expr::value(provider_refund_id),
        )
        .col_expr(
            refund_requests::Column::Version,
            Expr::value(i64::try_from(next_version).map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .col_expr(refund_requests::Column::UpdatedAt, Expr::value(updated_at))
        .exec(connection)
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(RefundRepositoryError::Conflict);
    }
    let model = load_model_by_request_key(connection, request_id)
        .await?
        .ok_or(RefundRepositoryError::Invariant)?;
    Ok(RefundSubmissionOutcome::Applied(to_record(model)?))
}

async fn transition_submission(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
    expected_version: u64,
    target: RefundRequestStatus,
    updated_at: u64,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    let Some(current) = load_model_by_request_key(connection, request_id).await? else {
        return Err(RefundRepositoryError::NotFound);
    };
    let record = to_record(current)?;
    if updated_at < record.created_at() {
        return Err(RefundRepositoryError::Invariant);
    }
    if record.status() == target
        && (record.version() == expected_version
            || record.version() == expected_version.saturating_add(1))
    {
        return Ok(RefundSubmissionOutcome::Existing(record));
    }
    if record.version() != expected_version || !record.status().can_transition_to(target) {
        return Err(RefundRepositoryError::Conflict);
    }
    let updated_at = to_database_time(updated_at)?;
    let next_version = expected_version
        .checked_add(1)
        .ok_or(RefundRepositoryError::Invariant)?;
    let result = refund_requests::Entity::update_many()
        .filter(refund_requests::Column::RequestKey.eq(request_id.persistence_key()))
        .filter(
            refund_requests::Column::Version
                .eq(i64::try_from(expected_version)
                    .map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .filter(refund_requests::Column::Status.eq(record.status().code()))
        .col_expr(refund_requests::Column::Status, Expr::value(target.code()))
        .col_expr(
            refund_requests::Column::Version,
            Expr::value(i64::try_from(next_version).map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .col_expr(refund_requests::Column::UpdatedAt, Expr::value(updated_at))
        .exec(connection)
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(RefundRepositoryError::Conflict);
    }
    let model = load_model_by_request_key(connection, request_id)
        .await?
        .ok_or(RefundRepositoryError::Invariant)?;
    Ok(RefundSubmissionOutcome::Applied(to_record(model)?))
}

async fn complete_manual_refund_inner(
    connection: &DatabaseConnection,
    completion: &RefundManualCompletion,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    let reference_sha256 = format!("{:x}", Sha256::digest(completion.reference().as_bytes()));
    let transaction = connection
        .begin()
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    let result =
        complete_manual_refund_transaction(&transaction, completion, &reference_sha256).await;
    match result {
        Ok(outcome) => {
            transaction
                .commit()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            Ok(outcome)
        }
        Err(error) => {
            transaction
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            Err(error)
        }
    }
}

async fn complete_manual_refund_transaction(
    transaction: &DatabaseTransaction,
    completion: &RefundManualCompletion,
    reference_sha256: &str,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    let request_key = completion.request_id().persistence_key();
    if let Some(existing) = refund_manual_completions::Entity::find()
        .filter(
            refund_manual_completions::Column::CompletionKey
                .eq(completion.completion_key().persistence_key()),
        )
        .one(transaction)
        .await
        .map_err(map_query)?
    {
        if existing.request_key != request_key
            || existing.expected_version
                != i64::try_from(completion.expected_version())
                    .map_err(|_| RefundRepositoryError::Invariant)?
            || existing.result != completion.result().code()
            || existing.reference_sha256 != reference_sha256
        {
            return Err(RefundRepositoryError::Conflict);
        }
        let model = load_model_by_request_key(transaction, completion.request_id())
            .await?
            .ok_or(RefundRepositoryError::Invariant)?;
        return Ok(RefundSubmissionOutcome::Existing(to_record(model)?));
    }
    if let Some(existing) = refund_manual_completions::Entity::find()
        .filter(refund_manual_completions::Column::RequestKey.eq(request_key.clone()))
        .one(transaction)
        .await
        .map_err(map_query)?
    {
        let same = existing.completion_key == completion.completion_key().persistence_key()
            && existing.expected_version
                == i64::try_from(completion.expected_version())
                    .map_err(|_| RefundRepositoryError::Invariant)?
            && existing.result == completion.result().code()
            && existing.reference_sha256 == reference_sha256;
        return if same {
            let model = load_model_by_request_key(transaction, completion.request_id())
                .await?
                .ok_or(RefundRepositoryError::Invariant)?;
            Ok(RefundSubmissionOutcome::Existing(to_record(model)?))
        } else {
            Err(RefundRepositoryError::Conflict)
        };
    }

    let current = load_model_by_request_key(transaction, completion.request_id())
        .await?
        .ok_or(RefundRepositoryError::NotFound)?;
    let record = to_record(current)?;
    if record.provider() != "epay"
        || record.approval_status() != RefundApprovalStatus::Approved
        || !matches!(
            record.status(),
            RefundRequestStatus::Requested | RefundRequestStatus::Failed
        )
        || completion.completed_at() < record.created_at()
        || completion.expected_version() != record.version()
    {
        return Err(RefundRepositoryError::Conflict);
    }
    let completed_at = to_database_time(completion.completed_at())?;
    let manual = refund_manual_completions::ActiveModel {
        id: sea_orm::NotSet,
        completion_key: Set(completion.completion_key().persistence_key()),
        request_key: Set(request_key.clone()),
        expected_version: Set(i64::try_from(completion.expected_version())
            .map_err(|_| RefundRepositoryError::Invariant)?),
        actor_user_id: Set(completion.actor_id().get()),
        result: Set(completion.result().code()),
        reference_sha256: Set(reference_sha256.to_owned()),
        completed_at: Set(completed_at),
        created_at: Set(completed_at),
    };
    let savepoint = transaction
        .begin()
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    let inserted = match manual.insert(&savepoint).await {
        Ok(value) => {
            savepoint
                .commit()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            value
        }
        Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return resolve_manual_unique_conflict(
                transaction,
                completion,
                reference_sha256,
                &request_key,
            )
            .await;
        }
        Err(_) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return Err(RefundRepositoryError::Query);
        }
    };
    let facts = if completion.result() == RefundManualResult::Completed {
        Some(derive_reconciliation_facts(transaction, &record).await?)
    } else {
        None
    };
    if let Some(facts) = facts {
        insert_reconciliation(
            transaction,
            &record,
            None,
            Some(inserted.id),
            facts,
            completed_at,
        )
        .await?;
    }
    let target = match completion.result() {
        RefundManualResult::Completed => RefundRequestStatus::ManuallySucceeded,
        RefundManualResult::Failed => RefundRequestStatus::ManuallyFailed,
    };
    let expected_version = i64::try_from(completion.expected_version())
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let next_version = record
        .version()
        .checked_add(1)
        .ok_or(RefundRepositoryError::Invariant)?;
    let changed = refund_requests::Entity::update_many()
        .filter(refund_requests::Column::Id.eq(record.database_id()))
        .filter(refund_requests::Column::Version.eq(expected_version))
        .filter(refund_requests::Column::Status.eq(record.status().code()))
        .col_expr(refund_requests::Column::Status, Expr::value(target.code()))
        .col_expr(
            refund_requests::Column::Version,
            Expr::value(i64::try_from(next_version).map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .col_expr(
            refund_requests::Column::UpdatedAt,
            Expr::value(completed_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    if changed.rows_affected != 1 {
        return Err(RefundRepositoryError::Conflict);
    }
    let updated = load_model_by_request_key(transaction, completion.request_id())
        .await?
        .ok_or(RefundRepositoryError::Invariant)?;
    Ok(RefundSubmissionOutcome::Applied(to_record(updated)?))
}

/// 唯一键并发竞争后回读已提交事实，保证重试同一完成键仍然幂等。
async fn resolve_manual_unique_conflict(
    transaction: &DatabaseTransaction,
    completion: &RefundManualCompletion,
    reference_sha256: &str,
    request_key: &str,
) -> Result<RefundSubmissionOutcome, RefundRepositoryError> {
    let expected_version = i64::try_from(completion.expected_version())
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let completion_key = completion.completion_key().persistence_key();
    let matches = |existing: &refund_manual_completions::Model| {
        existing.request_key == request_key
            && existing.expected_version == expected_version
            && existing.result == completion.result().code()
            && existing.reference_sha256 == reference_sha256
    };
    if let Some(existing) = refund_manual_completions::Entity::find()
        .filter(refund_manual_completions::Column::CompletionKey.eq(completion_key.clone()))
        .one(transaction)
        .await
        .map_err(map_query)?
    {
        if !matches(&existing) {
            return Err(RefundRepositoryError::Conflict);
        }
        let model = load_model_by_request_key(transaction, completion.request_id())
            .await?
            .ok_or(RefundRepositoryError::Invariant)?;
        return Ok(RefundSubmissionOutcome::Existing(to_record(model)?));
    }
    if let Some(existing) = refund_manual_completions::Entity::find()
        .filter(refund_manual_completions::Column::RequestKey.eq(request_key.to_owned()))
        .one(transaction)
        .await
        .map_err(map_query)?
    {
        if !matches(&existing) || existing.completion_key != completion_key {
            return Err(RefundRepositoryError::Conflict);
        }
        let model = load_model_by_request_key(transaction, completion.request_id())
            .await?
            .ok_or(RefundRepositoryError::Invariant)?;
        return Ok(RefundSubmissionOutcome::Existing(to_record(model)?));
    }
    // 唯一键错误但两个业务键都不存在，说明数据库返回了未知约束冲突。
    Err(RefundRepositoryError::Query)
}

async fn apply_receipt_inner(
    connection: &DatabaseConnection,
    write: &RefundReceiptWrite,
) -> Result<RefundReceiptOutcome, RefundRepositoryError> {
    validate_receipt_write(write)?;
    let transaction = connection
        .begin()
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    let result = apply_receipt_transaction(&transaction, write).await;
    match result {
        Ok(outcome) => {
            transaction
                .commit()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            Ok(outcome)
        }
        Err(RefundRepositoryError::Conflict) => {
            transaction
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            Err(RefundRepositoryError::Conflict)
        }
        Err(error) => {
            transaction
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            Err(error)
        }
    }
}

async fn apply_receipt_transaction(
    transaction: &DatabaseTransaction,
    write: &RefundReceiptWrite,
) -> Result<RefundReceiptOutcome, RefundRepositoryError> {
    let event_type = refund_receipt_event_type(write.status)?;
    let Some(current) = load_model_by_request_key(transaction, write.request_id).await? else {
        return Ok(RefundReceiptOutcome::NotFound);
    };
    let record = to_record(current.clone())?;
    if record.provider() != write.provider
        || record.currency() != write.currency
        || record.refund_amount_minor() != write.amount_minor
        || record
            .provider_refund_id()
            .is_some_and(|value| value != write.provider_refund_id)
    {
        return Err(RefundRepositoryError::Conflict);
    }
    if write.processed_at < record.created_at() {
        return Err(RefundRepositoryError::Invariant);
    }
    if !matches!(
        write.status,
        RefundRequestStatus::Succeeded | RefundRequestStatus::Failed
    ) || record.status() != RefundRequestStatus::Submitted
    {
        if record.status() == write.status {
            return duplicate_receipt_outcome(transaction, write, record, event_type).await;
        }
        return Err(RefundRepositoryError::Conflict);
    }
    let reconciliation = if write.status == RefundRequestStatus::Succeeded {
        Some(derive_reconciliation_facts(transaction, &record).await?)
    } else {
        None
    };
    let received_at = to_database_time(write.received_at)?;
    let processed_at = to_database_time(write.processed_at)?;
    let created_at = to_database_time(write.created_at)?;
    let event = refund_provider_events::ActiveModel {
        id: sea_orm::NotSet,
        event_key: Set(write.event_key.clone()),
        request_key: Set(write.request_id.persistence_key()),
        provider: Set(write.provider.clone()),
        provider_event_id: Set(SensitiveString::from(write.provider_event_id.clone())),
        provider_refund_id: Set(SensitiveString::from(write.provider_refund_id.clone())),
        event_type: Set(event_type),
        amount_minor: Set(write.amount_minor),
        currency: Set(write.currency.clone()),
        signature_key_fingerprint: Set(SensitiveString::from(
            write.signature_key_fingerprint.clone(),
        )),
        payload_sha256: Set(SensitiveString::from(write.payload_sha256.clone())),
        received_at: Set(received_at),
        processed_at: Set(Some(processed_at)),
        created_at: Set(created_at),
    };
    // 唯一约束冲突只代表重复回执；用保存点隔离插入失败，不能吞掉真实数据库错误。
    let savepoint = transaction
        .begin()
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    let insert_result = event.insert(&savepoint).await;
    let inserted_event = match insert_result {
        Ok(inserted) => {
            savepoint
                .commit()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            inserted
        }
        Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return duplicate_receipt_outcome(transaction, write, record, event_type).await;
        }
        Err(_) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return Err(RefundRepositoryError::Query);
        }
    };
    if let Some(reconciliation) = reconciliation {
        insert_reconciliation(
            transaction,
            &record,
            Some(inserted_event.id),
            None,
            reconciliation,
            processed_at,
        )
        .await?;
    }
    let expected_version =
        i64::try_from(record.version()).map_err(|_| RefundRepositoryError::Invariant)?;
    let next_version = record
        .version()
        .checked_add(1)
        .ok_or(RefundRepositoryError::Invariant)?;
    let result = refund_requests::Entity::update_many()
        .filter(refund_requests::Column::Id.eq(record.database_id()))
        .filter(refund_requests::Column::Version.eq(expected_version))
        .filter(refund_requests::Column::Status.eq(RefundRequestStatus::Submitted.code()))
        .col_expr(
            refund_requests::Column::ProviderRefundId,
            Expr::value(write.provider_refund_id.clone()),
        )
        .col_expr(
            refund_requests::Column::Status,
            Expr::value(write.status.code()),
        )
        .col_expr(
            refund_requests::Column::Version,
            Expr::value(i64::try_from(next_version).map_err(|_| RefundRepositoryError::Invariant)?),
        )
        .col_expr(
            refund_requests::Column::UpdatedAt,
            Expr::value(processed_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| RefundRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(RefundRepositoryError::Conflict);
    }
    let updated = load_model_by_request_key(transaction, write.request_id)
        .await?
        .ok_or(RefundRepositoryError::Invariant)?;
    Ok(RefundReceiptOutcome::Applied(to_record(updated)?))
}

async fn duplicate_receipt_outcome(
    transaction: &DatabaseTransaction,
    write: &RefundReceiptWrite,
    record: RefundRequestRecord,
    event_type: i16,
) -> Result<RefundReceiptOutcome, RefundRepositoryError> {
    let by_event_key = refund_provider_events::Entity::find()
        .filter(refund_provider_events::Column::EventKey.eq(write.event_key.clone()))
        .one(transaction)
        .await
        .map_err(map_query)?;
    let existing = match by_event_key {
        Some(event) => Some(event),
        None => refund_provider_events::Entity::find()
            .filter(refund_provider_events::Column::Provider.eq(write.provider.clone()))
            .filter(
                refund_provider_events::Column::ProviderEventId
                    .eq(SensitiveString::from(write.provider_event_id.clone())),
            )
            .one(transaction)
            .await
            .map_err(map_query)?,
    };
    let Some(existing) = existing else {
        return Err(RefundRepositoryError::Query);
    };
    if existing.request_key == write.request_id.persistence_key()
        && existing.provider == write.provider
        && existing.provider_refund_id.as_str() == write.provider_refund_id
        && existing.event_type == event_type
        && existing.amount_minor == write.amount_minor
        && existing.currency == write.currency
    {
        if write.status == RefundRequestStatus::Succeeded {
            validate_existing_reconciliation(transaction, write, &record, existing.id).await?;
        }
        Ok(RefundReceiptOutcome::Existing(record))
    } else {
        Err(RefundRepositoryError::Conflict)
    }
}

#[derive(Clone, Copy)]
struct ReconciliationFacts {
    organization_id: Option<OrganizationId>,
    approval_actor_id: UserId,
}

/// 成功退款只能从已支付原订单推导资金主体，不能信任 webhook 或调用方提供的租户字段。
async fn derive_reconciliation_facts(
    transaction: &DatabaseTransaction,
    record: &RefundRequestRecord,
) -> Result<ReconciliationFacts, RefundRepositoryError> {
    if record.approval_status() != RefundApprovalStatus::Approved {
        return Err(RefundRepositoryError::Conflict);
    }
    let approval_actor_id = record
        .approval_actor_id()
        .ok_or(RefundRepositoryError::Invariant)?;
    let organization_id = match record.order_kind() {
        RefundOrderKind::Topup => {
            let order = topup_orders::Entity::find()
                .filter(topup_orders::Column::OrderKey.eq(record.order_key()))
                .one(transaction)
                .await
                .map_err(map_query)?
                .ok_or(RefundRepositoryError::Conflict)?;
            if order.status != TopupOrderStatus::Paid.code()
                || order.user_id != record.user_id().get()
                || order.provider != record.provider()
                || order.amount_minor != record.original_amount_minor()
                || order.currency != record.currency()
                || order.trade_no.as_ref().map(SensitiveString::as_str)
                    != record.payment_reference()
            {
                return Err(RefundRepositoryError::Conflict);
            }
            order
                .organization_id
                .map(OrganizationId::new)
                .transpose()
                .map_err(|_| RefundRepositoryError::Invariant)?
        }
        RefundOrderKind::Subscription => {
            let order = subscription_orders::Entity::find()
                .filter(subscription_orders::Column::OrderKey.eq(record.order_key()))
                .one(transaction)
                .await
                .map_err(map_query)?
                .ok_or(RefundRepositoryError::Conflict)?;
            if order.status != SubscriptionOrderStatus::Paid.code()
                || order.user_id != record.user_id().get()
                || order.provider != record.provider()
                || order.amount_minor != record.original_amount_minor()
                || order.currency != record.currency()
                || order.trade_no.as_ref().map(SensitiveString::as_str)
                    != record.payment_reference()
            {
                return Err(RefundRepositoryError::Conflict);
            }
            None
        }
    };
    Ok(ReconciliationFacts {
        organization_id,
        approval_actor_id,
    })
}

async fn insert_reconciliation(
    transaction: &DatabaseTransaction,
    record: &RefundRequestRecord,
    provider_event_id: Option<i64>,
    manual_completion_id: Option<i64>,
    facts: ReconciliationFacts,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<(), RefundRepositoryError> {
    let amount_delta_minor = record
        .refund_amount_minor()
        .checked_neg()
        .filter(|value| *value != i64::MIN)
        .ok_or(RefundRepositoryError::Invariant)?;
    refund_reconciliation_entries::ActiveModel {
        id: sea_orm::NotSet,
        request_key: Set(record.request_id().persistence_key()),
        provider_event_id: Set(provider_event_id),
        manual_completion_id: Set(manual_completion_id),
        user_id: Set(record.user_id().get()),
        organization_id: Set(facts.organization_id.map(OrganizationId::get)),
        approval_actor_id: Set(facts.approval_actor_id.get()),
        order_kind: Set(record.order_kind().code()),
        order_key: Set(record.order_key().to_owned()),
        provider: Set(record.provider().to_owned()),
        amount_delta_minor: Set(amount_delta_minor),
        currency: Set(record.currency().to_owned()),
        created_at: Set(created_at),
    }
    .insert(transaction)
    .await
    .map(|_| ())
    .map_err(|error| {
        if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
            RefundRepositoryError::Conflict
        } else {
            RefundRepositoryError::Query
        }
    })
}

async fn validate_existing_reconciliation(
    transaction: &DatabaseTransaction,
    write: &RefundReceiptWrite,
    record: &RefundRequestRecord,
    provider_event_id: i64,
) -> Result<(), RefundRepositoryError> {
    let facts = derive_reconciliation_facts(transaction, record).await?;
    let existing = refund_reconciliation_entries::Entity::find()
        .filter(
            refund_reconciliation_entries::Column::RequestKey
                .eq(record.request_id().persistence_key()),
        )
        .one(transaction)
        .await
        .map_err(map_query)?
        .ok_or(RefundRepositoryError::Invariant)?;
    let expected_delta = record
        .refund_amount_minor()
        .checked_neg()
        .filter(|value| *value != i64::MIN)
        .ok_or(RefundRepositoryError::Invariant)?;
    if existing.provider_event_id != Some(provider_event_id)
        || existing.user_id != record.user_id().get()
        || existing.organization_id != facts.organization_id.map(OrganizationId::get)
        || existing.approval_actor_id != facts.approval_actor_id.get()
        || existing.order_kind != record.order_kind().code()
        || existing.order_key != record.order_key()
        || existing.provider != write.provider
        || existing.amount_delta_minor != expected_delta
        || existing.currency != write.currency
    {
        return Err(RefundRepositoryError::Conflict);
    }
    Ok(())
}

fn to_reconciliation_record(
    model: refund_reconciliation_entries::Model,
) -> Result<RefundReconciliationRecord, RefundRepositoryError> {
    if model.id <= 0
        || (model.provider_event_id.is_none() == model.manual_completion_id.is_none())
        || model.amount_delta_minor >= 0
        || model.amount_delta_minor == i64::MIN
    {
        return Err(RefundRepositoryError::Invariant);
    }
    Ok(RefundReconciliationRecord {
        id: model.id,
        request_id: RefundRequestId::from_persistence_key(&model.request_key)
            .map_err(|_| RefundRepositoryError::Invariant)?,
        provider_event_id: model.provider_event_id,
        manual_completion_id: model.manual_completion_id,
        user_id: UserId::new(model.user_id).map_err(|_| RefundRepositoryError::Invariant)?,
        organization_id: model
            .organization_id
            .map(OrganizationId::new)
            .transpose()
            .map_err(|_| RefundRepositoryError::Invariant)?,
        approval_actor_id: UserId::new(model.approval_actor_id)
            .map_err(|_| RefundRepositoryError::Invariant)?,
        order_kind: RefundOrderKind::try_from(model.order_kind)
            .map_err(|_| RefundRepositoryError::Invariant)?,
        order_key: model.order_key,
        provider: model.provider,
        amount_delta_minor: model.amount_delta_minor,
        currency: model.currency,
        created_at: unix_seconds(model.created_at)?,
    })
}

fn validate_receipt_write(write: &RefundReceiptWrite) -> Result<(), RefundRepositoryError> {
    if !valid_hex_text(&write.event_key, 32)
        || write.provider.is_empty()
        || write.provider.len() > 64
        || write.provider_event_id.is_empty()
        || write.provider_event_id.len() > 128
        || write.provider_refund_id.is_empty()
        || write.provider_refund_id.len() > 128
        || write.amount_minor <= 0
        || !valid_currency(&write.currency)
        || !valid_hex_text(&write.signature_key_fingerprint, 64)
        || !valid_hex_text(&write.payload_sha256, 64)
        || write.received_at > i64::MAX as u64
        || write.processed_at > i64::MAX as u64
        || write.created_at > i64::MAX as u64
    {
        return Err(RefundRepositoryError::Invariant);
    }
    Ok(())
}

fn refund_receipt_event_type(status: RefundRequestStatus) -> Result<i16, RefundRepositoryError> {
    match status {
        RefundRequestStatus::Succeeded => Ok(1),
        RefundRequestStatus::Failed => Ok(2),
        _ => Err(RefundRepositoryError::Invariant),
    }
}

async fn load_model_by_request_key<C: ConnectionTrait>(
    connection: &C,
    request_id: RefundRequestId,
) -> Result<Option<refund_requests::Model>, RefundRepositoryError> {
    refund_requests::Entity::find()
        .filter(refund_requests::Column::RequestKey.eq(request_id.persistence_key()))
        .one(connection)
        .await
        .map_err(map_query)
}

fn valid_provider_refund_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= af_domain::MAX_REFUND_PROVIDER_REFUND_ID_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_hex_text(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

async fn create_request_inner(
    connection: &DatabaseConnection,
    write: &RefundRequestCreate,
) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
    if let Some(existing) =
        find_by_idempotency_key(connection, write.user_id(), write.idempotency_key())
            .await
            .map_err(map_query)?
    {
        return existing_outcome(existing, write);
    }
    if let Some(existing) = find_by_order(connection, write.order_kind(), write.order_key())
        .await
        .map_err(map_query)?
    {
        return existing_outcome(existing, write);
    }

    let created_at = to_database_time(write.created_at())?;
    let model = refund_requests::ActiveModel {
        id: sea_orm::NotSet,
        request_key: Set(write.request_id().persistence_key()),
        idempotency_key: Set(write.idempotency_key().persistence_key()),
        user_id: Set(write.user_id().get()),
        order_kind: Set(write.order_kind().code()),
        order_key: Set(write.order_key().to_owned()),
        provider: Set(write.provider().to_owned()),
        payment_reference: Set(Some(write.payment_reference().to_owned())),
        currency: Set(write.currency().to_owned()),
        original_amount_minor: Set(write.original_amount_minor()),
        refund_amount_minor: Set(write.refund_amount_minor()),
        provider_refund_id: Set(None),
        status: Set(RefundRequestStatus::Requested.code()),
        approval_status: Set(RefundApprovalStatus::Pending.code()),
        approval_actor_id: Set(None),
        approval_reason: Set(None),
        version: Set(1),
        created_at: Set(created_at),
        updated_at: Set(created_at),
    };

    match model.insert(connection).await {
        Ok(model) => Ok(RefundRequestCreateOutcome::Created(to_record(model)?)),
        Err(_) => {
            // 并发请求可能在首次检查后先完成；重新按两个唯一事实读取，避免把可恢复重试
            // 错误地报告为内部错误，同时不吞掉不同事实的冲突。
            if let Some(existing) =
                find_by_idempotency_key(connection, write.user_id(), write.idempotency_key())
                    .await
                    .map_err(map_query)?
            {
                return existing_outcome(existing, write);
            }
            if let Some(existing) = find_by_order(connection, write.order_kind(), write.order_key())
                .await
                .map_err(map_query)?
            {
                return existing_outcome(existing, write);
            }
            Err(RefundRepositoryError::Query)
        }
    }
}

fn existing_outcome(
    model: refund_requests::Model,
    write: &RefundRequestCreate,
) -> Result<RefundRequestCreateOutcome, RefundRepositoryError> {
    let record = to_record(model)?;
    if record.matches_create(write) {
        Ok(RefundRequestCreateOutcome::Existing(record))
    } else {
        Err(RefundRepositoryError::Conflict)
    }
}

async fn load_by_request_key(
    connection: &DatabaseConnection,
    request_id: RefundRequestId,
) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
    let model = refund_requests::Entity::find()
        .filter(refund_requests::Column::RequestKey.eq(request_id.persistence_key()))
        .one(connection)
        .await
        .map_err(map_query)?;
    model.map(to_record).transpose()
}

async fn load_by_idempotency_key(
    connection: &DatabaseConnection,
    user_id: UserId,
    key: RefundRequestKey,
) -> Result<Option<RefundRequestRecord>, RefundRepositoryError> {
    let model = find_by_idempotency_key(connection, user_id, key)
        .await
        .map_err(map_query)?;
    model.map(to_record).transpose()
}

async fn find_by_idempotency_key(
    connection: &DatabaseConnection,
    user_id: UserId,
    key: RefundRequestKey,
) -> Result<Option<refund_requests::Model>, sea_orm::DbErr> {
    refund_requests::Entity::find()
        .filter(
            refund_requests::Column::UserId
                .eq(user_id.get())
                .and(refund_requests::Column::IdempotencyKey.eq(key.persistence_key())),
        )
        .one(connection)
        .await
}

async fn find_by_order(
    connection: &DatabaseConnection,
    order_kind: RefundOrderKind,
    order_key: &str,
) -> Result<Option<refund_requests::Model>, sea_orm::DbErr> {
    refund_requests::Entity::find()
        .filter(
            refund_requests::Column::OrderKind
                .eq(order_kind.code())
                .and(refund_requests::Column::OrderKey.eq(order_key)),
        )
        .one(connection)
        .await
}

fn to_record(model: refund_requests::Model) -> Result<RefundRequestRecord, RefundRepositoryError> {
    let request_id = RefundRequestId::from_persistence_key(&model.request_key)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let idempotency_key = RefundRequestKey::from_persistence_key(&model.idempotency_key)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let order_kind = RefundOrderKind::try_from(model.order_kind)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let status = RefundRequestStatus::try_from(model.status)
        .map_err(|_| RefundRepositoryError::Invariant)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    RefundRequestRecord::from_persistence(
        model.id,
        request_id,
        idempotency_key,
        UserId::new(model.user_id).map_err(|_| RefundRepositoryError::Invariant)?,
        order_kind,
        model.order_key,
        model.provider,
        model.payment_reference,
        model.currency,
        model.original_amount_minor,
        model.refund_amount_minor,
        model.provider_refund_id,
        status,
        RefundApprovalStatus::try_from(model.approval_status)
            .map_err(|_| RefundRepositoryError::Invariant)?,
        model
            .approval_actor_id
            .map(|value| UserId::new(value).map_err(|_| RefundRepositoryError::Invariant))
            .transpose()?,
        model.approval_reason,
        model.version,
        created_at,
        updated_at,
    )
    .map_err(|_| RefundRepositoryError::Invariant)
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, RefundRepositoryError> {
    let value = i64::try_from(value).map_err(|_| RefundRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| RefundRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, RefundRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| RefundRepositoryError::Invariant)
}

fn map_query(error: sea_orm::DbErr) -> RefundRepositoryError {
    let _ = error;
    RefundRepositoryError::Query
}

/// 仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("退款仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 退款请求仓储错误；不携带订单、Provider 或支付材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundRepositoryError {
    /// 相同幂等键或订单已经绑定不同事实。
    #[error("退款请求事实冲突")]
    Conflict,
    /// 请求标识不存在。
    #[error("退款请求不存在")]
    NotFound,
    /// 数据库查询或写入失败。
    #[error("退款数据库操作失败")]
    Query,
    /// 写入结果未知，只能复用原退款请求恢复。
    #[error("退款操作结果未知")]
    OutcomeUnknown,
    /// 只读操作超过硬截止时间。
    #[error("退款数据库操作超时")]
    Timeout,
    /// 持久化快照违反退款不变量。
    #[error("退款持久化状态损坏")]
    Invariant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_receipt_event_codes_are_independent_from_request_status_codes() {
        assert_eq!(
            refund_receipt_event_type(RefundRequestStatus::Succeeded),
            Ok(1)
        );
        assert_eq!(
            refund_receipt_event_type(RefundRequestStatus::Failed),
            Ok(2)
        );
        assert_eq!(
            refund_receipt_event_type(RefundRequestStatus::Submitted),
            Err(RefundRepositoryError::Invariant)
        );
    }
}
