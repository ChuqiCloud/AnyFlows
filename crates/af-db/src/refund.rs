use std::time::Duration;

use af_domain::{
    MAX_REFUND_APPROVAL_REASON_BYTES, RefundApprovalStatus, RefundOrderKind, RefundRequestCreate,
    RefundRequestCreateOutcome, RefundRequestId, RefundRequestKey, RefundRequestRecord,
    RefundRequestStatus, UserId,
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
    entity::{SensitiveString, refund_provider_events, refund_requests},
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

    /// 把请求占位推进到 Submitted；Provider 调用发生在该 CAS 之后，避免并发重复出资。
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
}

/// Provider 调用前成功抢到的退款请求快照。
#[derive(Debug)]
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
    let next_cursor = if models.len() > limit {
        models.pop().map(|model| model.id)
    } else {
        None
    };
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
            return Ok(RefundReceiptOutcome::Existing(record));
        }
        return Err(RefundRepositoryError::Conflict);
    }
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
        event_type: Set(write.status.code()),
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
    match insert_result {
        Ok(_) => savepoint
            .commit()
            .await
            .map_err(|_| RefundRepositoryError::OutcomeUnknown)?,
        Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return duplicate_receipt_outcome(transaction, write, record).await;
        }
        Err(_) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| RefundRepositoryError::OutcomeUnknown)?;
            return Err(RefundRepositoryError::Query);
        }
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
        && existing.event_type == write.status.code()
        && existing.amount_minor == write.amount_minor
        && existing.currency == write.currency
    {
        Ok(RefundReceiptOutcome::Existing(record))
    } else {
        Err(RefundRepositoryError::Conflict)
    }
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
