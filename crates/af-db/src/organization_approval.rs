use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionId, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateId,
    OrganizationApprovalTemplateKey, OrganizationApprovalTemplateSnapshot, OrganizationId,
    OrganizationMembershipStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_approval_decisions, organization_approval_requests,
        organization_approval_templates, organization_memberships, organizations,
    },
};

const ACTIVE_ORGANIZATION: i16 = 1;

/// 企业审批模板版本写入参数；版本由仓储在企业范围锁内分配。
pub struct OrganizationApprovalTemplateWrite {
    organization_id: OrganizationId,
    template_key: OrganizationApprovalTemplateKey,
    resource_type: OrganizationApprovalResourceType,
    definition_digest: OrganizationApprovalTemplateDigest,
    created_by_user_id: UserId,
    expected_version: Option<i64>,
}

impl OrganizationApprovalTemplateWrite {
    /// 构造模板版本写入；`expected_version` 用于并发 CAS，首次版本为零。
    pub fn new(
        organization_id: OrganizationId,
        template_key: OrganizationApprovalTemplateKey,
        resource_type: OrganizationApprovalResourceType,
        definition_digest: OrganizationApprovalTemplateDigest,
        created_by_user_id: UserId,
        expected_version: Option<i64>,
    ) -> Self {
        Self {
            organization_id,
            template_key,
            resource_type,
            definition_digest,
            created_by_user_id,
            expected_version,
        }
    }
}

impl fmt::Debug for OrganizationApprovalTemplateWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalTemplateWrite")
            .field("organization_id", &self.organization_id)
            .field("resource_type", &self.resource_type)
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 企业审批申请写入参数；申请本身不改变任何业务资源。
pub struct OrganizationApprovalRequestWrite {
    request: OrganizationApprovalRequest,
}

impl OrganizationApprovalRequestWrite {
    /// 包装已通过领域校验的申请事实。
    #[must_use]
    pub fn new(request: OrganizationApprovalRequest) -> Self {
        Self { request }
    }
}

impl fmt::Debug for OrganizationApprovalRequestWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalRequestWrite")
            .field("organization_id", &self.request.organization_id())
            .field("resource_type", &self.request.resource_type())
            .field("template_id", &self.request.template_id())
            .field("template_version", &self.request.template_version())
            .finish_non_exhaustive()
    }
}

/// 企业审批决定写入参数；决定提交后不可更新或撤销。
pub struct OrganizationApprovalDecisionWrite {
    decision: OrganizationApprovalDecision,
}

impl OrganizationApprovalDecisionWrite {
    /// 包装已通过领域校验的决定事实。
    #[must_use]
    pub fn new(decision: OrganizationApprovalDecision) -> Self {
        Self { decision }
    }
}

impl fmt::Debug for OrganizationApprovalDecisionWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalDecisionWrite")
            .field("organization_id", &self.decision.organization_id())
            .field("request_id", &self.decision.request_id())
            .field("kind", &self.decision.kind())
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业审批模板版本安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalTemplateRecord {
    id: OrganizationApprovalTemplateId,
    snapshot: OrganizationApprovalTemplateSnapshot,
    created_at: TimeDateTimeWithTimeZone,
}

impl OrganizationApprovalTemplateRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationApprovalTemplateId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.snapshot.organization_id()
    }
    #[must_use]
    pub fn template_key(&self) -> &OrganizationApprovalTemplateKey {
        self.snapshot.template_key()
    }
    #[must_use]
    pub const fn resource_type(&self) -> OrganizationApprovalResourceType {
        self.snapshot.resource_type()
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.snapshot.version()
    }
    #[must_use]
    pub const fn definition_digest(&self) -> OrganizationApprovalTemplateDigest {
        self.snapshot.definition_digest()
    }
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.snapshot.created_by_user_id()
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
}

/// 已持久化的企业审批申请安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalRequestRecord {
    id: OrganizationApprovalRequestId,
    request: OrganizationApprovalRequest,
    created_at: TimeDateTimeWithTimeZone,
}

impl OrganizationApprovalRequestRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationApprovalRequestId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.request.organization_id()
    }
    #[must_use]
    pub const fn request_key(&self) -> OrganizationApprovalRequestKey {
        self.request.request_key()
    }
    #[must_use]
    pub const fn resource_type(&self) -> OrganizationApprovalResourceType {
        self.request.resource_type()
    }
    #[must_use]
    pub fn resource_summary(&self) -> &str {
        self.request.resource_summary()
    }
    #[must_use]
    pub const fn template_id(&self) -> OrganizationApprovalTemplateId {
        self.request.template_id()
    }
    #[must_use]
    pub const fn template_version(&self) -> i64 {
        self.request.template_version()
    }
    #[must_use]
    pub const fn requested_by_user_id(&self) -> UserId {
        self.request.requested_by_user_id()
    }
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.request.expires_at()
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
}

/// 已持久化的企业审批决定安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalDecisionRecord {
    id: OrganizationApprovalDecisionId,
    decision: OrganizationApprovalDecision,
    created_at: TimeDateTimeWithTimeZone,
}

impl OrganizationApprovalDecisionRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationApprovalDecisionId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.decision.organization_id()
    }
    #[must_use]
    pub const fn decision_key(&self) -> OrganizationApprovalDecisionKey {
        self.decision.decision_key()
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationApprovalRequestId {
        self.decision.request_id()
    }
    #[must_use]
    pub const fn kind(&self) -> OrganizationApprovalDecisionKind {
        self.decision.kind()
    }
    #[must_use]
    pub const fn decided_by_user_id(&self) -> Option<UserId> {
        self.decision.decided_by_user_id()
    }
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.decision.reason()
    }
    #[must_use]
    pub const fn decided_at(&self) -> i64 {
        self.decision.decided_at()
    }
    #[must_use]
    pub const fn created_at(&self) -> TimeDateTimeWithTimeZone {
        self.created_at
    }
}

/// 模板版本创建结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalTemplateCreateOutcome {
    Created(OrganizationApprovalTemplateRecord),
}

/// 申请写入的幂等结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalRequestCreateOutcome {
    Created(OrganizationApprovalRequestRecord),
    Existing(OrganizationApprovalRequestRecord),
}

/// 决定写入的幂等结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalDecisionOutcome {
    Created(OrganizationApprovalDecisionRecord),
    Existing(OrganizationApprovalDecisionRecord),
}

/// 企业审批仓储错误；错误不携带资源摘要、模板键或理由原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalRepositoryError {
    #[error("企业审批输入无效")]
    InvalidInput,
    #[error("企业审批事实不存在")]
    NotFound,
    #[error("企业不可用")]
    OrganizationUnavailable,
    #[error("企业成员不可用")]
    MemberUnavailable,
    #[error("企业审批模板资源类型冲突")]
    TemplateResourceMismatch,
    #[error("企业审批模板版本冲突")]
    Conflict,
    #[error("企业审批申请已决定")]
    AlreadyDecided,
    #[error("企业审批申请已超时")]
    Expired,
    #[error("企业审批数据库操作失败")]
    Query,
    #[error("企业审批操作超时")]
    Timeout,
    #[error("企业审批持久化状态损坏")]
    Invariant,
}

/// 企业审批仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalRepositoryConfigError {
    #[error("企业审批仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业范围审批事实仓储；所有读取和写入都带企业边界。
#[derive(Clone)]
pub struct OrganizationApprovalRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationApprovalRepository {
    /// 使用共享连接池和非零操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationApprovalRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationApprovalRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在企业范围锁内追加一个模板版本事实。
    pub async fn create_template(
        &self,
        write: OrganizationApprovalTemplateWrite,
    ) -> Result<OrganizationApprovalTemplateRecord, OrganizationApprovalRepositoryError> {
        self.run(self.create_template_inner(write)).await
    }

    /// 幂等追加一个企业审批申请事实。
    pub async fn create_request(
        &self,
        write: OrganizationApprovalRequestWrite,
    ) -> Result<OrganizationApprovalRequestCreateOutcome, OrganizationApprovalRepositoryError> {
        self.run(self.create_request_inner(write)).await
    }

    /// 追加一次审批决定；同一申请不能写入第二个决定。
    pub async fn record_decision(
        &self,
        write: OrganizationApprovalDecisionWrite,
    ) -> Result<OrganizationApprovalDecisionOutcome, OrganizationApprovalRepositoryError> {
        self.run(self.record_decision_inner(write)).await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationApprovalRepositoryError>>,
    ) -> Result<T, OrganizationApprovalRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationApprovalRepositoryError::Timeout)?
    }

    async fn create_template_inner(
        &self,
        write: OrganizationApprovalTemplateWrite,
    ) -> Result<OrganizationApprovalTemplateRecord, OrganizationApprovalRepositoryError> {
        if write.expected_version.is_some_and(|value| value < 0) {
            return Err(OrganizationApprovalRepositoryError::InvalidInput);
        }
        let transaction = begin(&self.pool).await?;
        lock_active_organization(&transaction, write.organization_id).await?;
        ensure_active_member(
            &transaction,
            write.organization_id,
            write.created_by_user_id,
        )
        .await?;
        let existing = lock_template_scope(
            &transaction,
            write.organization_id,
            write.template_key.as_str(),
        )
        .await?;
        if existing
            .iter()
            .any(|row| row.resource_type != write.resource_type.code())
        {
            rollback(transaction).await?;
            return Err(OrganizationApprovalRepositoryError::TemplateResourceMismatch);
        }
        let current_version = existing.iter().map(|row| row.version).max().unwrap_or(0);
        if write
            .expected_version
            .is_some_and(|value| value != current_version)
        {
            rollback(transaction).await?;
            return Err(OrganizationApprovalRepositoryError::Conflict);
        }
        let version = current_version
            .checked_add(1)
            .filter(|value| *value > 0)
            .ok_or_else(invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_approval_templates::ActiveModel {
            organization_id: Set(write.organization_id.get()),
            template_key: Set(write.template_key.as_str().to_owned()),
            resource_type: Set(write.resource_type.code()),
            version: Set(version),
            definition_digest: Set(SensitiveString::from(
                write.definition_digest.persistence_key(),
            )),
            created_by_user_id: Set(write.created_by_user_id.get()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(query_error)?;
        let record = template_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn create_request_inner(
        &self,
        write: OrganizationApprovalRequestWrite,
    ) -> Result<OrganizationApprovalRequestCreateOutcome, OrganizationApprovalRepositoryError> {
        let request = write.request;
        if let Some(existing) =
            find_request_by_key(&self.pool, request.organization_id(), request.request_key())
                .await?
        {
            return existing_request_outcome(existing, &request);
        }
        let expires_at = to_time(request.expires_at())?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        if expires_at <= now {
            return Err(OrganizationApprovalRepositoryError::InvalidInput);
        }
        let transaction = begin(&self.pool).await?;
        lock_active_organization(&transaction, request.organization_id()).await?;
        ensure_active_member(
            &transaction,
            request.organization_id(),
            request.requested_by_user_id(),
        )
        .await?;
        let template =
            organization_approval_templates::Entity::find_by_id(request.template_id().get())
                .filter(
                    organization_approval_templates::Column::OrganizationId
                        .eq(request.organization_id().get()),
                )
                .filter(
                    organization_approval_templates::Column::Version.eq(request.template_version()),
                )
                .one(&transaction)
                .await
                .map_err(query_error)?
                .ok_or(OrganizationApprovalRepositoryError::NotFound)?;
        if template.resource_type != request.resource_type().code() {
            rollback(transaction).await?;
            return Err(OrganizationApprovalRepositoryError::TemplateResourceMismatch);
        }
        let model = organization_approval_requests::ActiveModel {
            request_key: Set(request.request_key().persistence_key()),
            organization_id: Set(request.organization_id().get()),
            resource_type: Set(request.resource_type().code()),
            resource_summary: Set(SensitiveString::from(request.resource_summary().to_owned())),
            template_id: Set(request.template_id().get()),
            template_version: Set(request.template_version()),
            requested_by_user_id: Set(request.requested_by_user_id().get()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            ..Default::default()
        };
        let model = match model.insert(&transaction).await {
            Ok(model) => model,
            Err(_) => {
                rollback(transaction).await?;
                let existing = find_request_by_key(
                    &self.pool,
                    request.organization_id(),
                    request.request_key(),
                )
                .await?;
                return existing
                    .map(|record| existing_request_outcome(record, &request))
                    .unwrap_or(Err(OrganizationApprovalRepositoryError::Conflict));
            }
        };
        let record = request_record(model)?;
        commit(transaction).await?;
        Ok(OrganizationApprovalRequestCreateOutcome::Created(record))
    }

    async fn record_decision_inner(
        &self,
        write: OrganizationApprovalDecisionWrite,
    ) -> Result<OrganizationApprovalDecisionOutcome, OrganizationApprovalRepositoryError> {
        let decision = write.decision;
        if let Some(existing) = find_decision_by_key(
            &self.pool,
            decision.organization_id(),
            decision.decision_key(),
        )
        .await?
        {
            return existing_decision_outcome(existing, &decision);
        }
        let decided_at = to_time(decision.decided_at())?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        if decided_at > now {
            return Err(OrganizationApprovalRepositoryError::InvalidInput);
        }
        let transaction = begin(&self.pool).await?;
        lock_active_organization(&transaction, decision.organization_id()).await?;
        let request = lock_request(
            &transaction,
            decision.organization_id(),
            decision.request_id(),
        )
        .await?;
        if decision.decided_at() < request.created_at.unix_timestamp() {
            rollback(transaction).await?;
            return Err(OrganizationApprovalRepositoryError::InvalidInput);
        }
        let request_expires_at = request.expires_at.unix_timestamp();
        match decision.kind() {
            OrganizationApprovalDecisionKind::TimedOut
                if decision.decided_at() < request_expires_at =>
            {
                rollback(transaction).await?;
                return Err(OrganizationApprovalRepositoryError::Expired);
            }
            OrganizationApprovalDecisionKind::Approved
            | OrganizationApprovalDecisionKind::Rejected
                if decision.decided_at() >= request_expires_at =>
            {
                rollback(transaction).await?;
                return Err(OrganizationApprovalRepositoryError::Expired);
            }
            _ => {}
        }
        if let Some(existing) = find_decision_for_request(
            &transaction,
            decision.organization_id(),
            decision.request_id(),
        )
        .await?
        {
            rollback(transaction).await?;
            let record = decision_record(existing)?;
            return Err(if record.decision_key() == decision.decision_key() {
                OrganizationApprovalRepositoryError::Conflict
            } else {
                OrganizationApprovalRepositoryError::AlreadyDecided
            });
        }
        if let Some(user_id) = decision.decided_by_user_id() {
            ensure_active_member(&transaction, decision.organization_id(), user_id).await?;
        }
        let model = organization_approval_decisions::ActiveModel {
            decision_key: Set(decision.decision_key().persistence_key()),
            organization_id: Set(decision.organization_id().get()),
            request_id: Set(decision.request_id().get()),
            decision_kind: Set(decision.kind().code()),
            decided_by_user_id: Set(decision.decided_by_user_id().map(UserId::get)),
            reason: Set(decision
                .reason()
                .map(|value| SensitiveString::from(value.to_owned()))),
            decided_at: Set(decided_at),
            created_at: Set(now),
            ..Default::default()
        };
        let model = match model.insert(&transaction).await {
            Ok(model) => model,
            Err(_) => {
                rollback(transaction).await?;
                if let Some(existing) = find_decision_by_key(
                    &self.pool,
                    decision.organization_id(),
                    decision.decision_key(),
                )
                .await?
                {
                    return existing_decision_outcome(existing, &decision);
                }
                return Err(OrganizationApprovalRepositoryError::AlreadyDecided);
            }
        };
        let record = decision_record(model)?;
        commit(transaction).await?;
        Ok(OrganizationApprovalDecisionOutcome::Created(record))
    }
}

impl fmt::Debug for OrganizationApprovalRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_active_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationApprovalRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(ACTIVE_ORGANIZATION));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    if query.one(transaction).await.map_err(query_error)?.is_none() {
        return Err(OrganizationApprovalRepositoryError::OrganizationUnavailable);
    }
    Ok(())
}

async fn ensure_active_member(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<(), OrganizationApprovalRepositoryError> {
    let member = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .one(transaction)
        .await
        .map_err(query_error)?;
    member
        .map(|_| ())
        .ok_or(OrganizationApprovalRepositoryError::MemberUnavailable)
}

async fn lock_template_scope(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    template_key: &str,
) -> Result<Vec<organization_approval_templates::Model>, OrganizationApprovalRepositoryError> {
    let mut query = organization_approval_templates::Entity::find()
        .filter(organization_approval_templates::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_approval_templates::Column::TemplateKey.eq(template_key))
        .order_by_asc(organization_approval_templates::Column::Version);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.all(transaction).await.map_err(query_error)
}

async fn lock_request(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<organization_approval_requests::Model, OrganizationApprovalRepositoryError> {
    let mut query = organization_approval_requests::Entity::find_by_id(request_id.get())
        .filter(organization_approval_requests::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationApprovalRepositoryError::NotFound)
}

async fn find_request_by_key(
    pool: &DatabasePool,
    organization_id: OrganizationId,
    key: OrganizationApprovalRequestKey,
) -> Result<Option<OrganizationApprovalRequestRecord>, OrganizationApprovalRepositoryError> {
    organization_approval_requests::Entity::find()
        .filter(organization_approval_requests::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_approval_requests::Column::RequestKey.eq(key.persistence_key()))
        .one(pool.connection())
        .await
        .map_err(query_error)?
        .map(request_record)
        .transpose()
}

async fn find_decision_by_key(
    pool: &DatabasePool,
    organization_id: OrganizationId,
    key: OrganizationApprovalDecisionKey,
) -> Result<Option<OrganizationApprovalDecisionRecord>, OrganizationApprovalRepositoryError> {
    organization_approval_decisions::Entity::find()
        .filter(organization_approval_decisions::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_approval_decisions::Column::DecisionKey.eq(key.persistence_key()))
        .one(pool.connection())
        .await
        .map_err(query_error)?
        .map(decision_record)
        .transpose()
}

async fn find_decision_for_request<C>(
    connection: &C,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<Option<organization_approval_decisions::Model>, OrganizationApprovalRepositoryError>
where
    C: ConnectionTrait,
{
    organization_approval_decisions::Entity::find()
        .filter(organization_approval_decisions::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_approval_decisions::Column::RequestId.eq(request_id.get()))
        .one(connection)
        .await
        .map_err(query_error)
}

fn existing_request_outcome(
    existing: OrganizationApprovalRequestRecord,
    request: &OrganizationApprovalRequest,
) -> Result<OrganizationApprovalRequestCreateOutcome, OrganizationApprovalRepositoryError> {
    if existing.request_key() != request.request_key()
        || existing.resource_type() != request.resource_type()
        || existing.resource_summary() != request.resource_summary()
        || existing.template_id() != request.template_id()
        || existing.template_version() != request.template_version()
        || existing.requested_by_user_id() != request.requested_by_user_id()
        || existing.expires_at() != request.expires_at()
    {
        return Err(OrganizationApprovalRepositoryError::Conflict);
    }
    Ok(OrganizationApprovalRequestCreateOutcome::Existing(existing))
}

fn existing_decision_outcome(
    existing: OrganizationApprovalDecisionRecord,
    decision: &OrganizationApprovalDecision,
) -> Result<OrganizationApprovalDecisionOutcome, OrganizationApprovalRepositoryError> {
    if existing.request_id() != decision.request_id()
        || existing.kind() != decision.kind()
        || existing.decided_by_user_id() != decision.decided_by_user_id()
        || existing.reason() != decision.reason()
        || existing.decided_at() != decision.decided_at()
    {
        return Err(OrganizationApprovalRepositoryError::Conflict);
    }
    Ok(OrganizationApprovalDecisionOutcome::Existing(existing))
}

fn template_record(
    model: organization_approval_templates::Model,
) -> Result<OrganizationApprovalTemplateRecord, OrganizationApprovalRepositoryError> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let template_key =
        OrganizationApprovalTemplateKey::new(model.template_key).map_err(|_| invariant())?;
    let resource_type =
        OrganizationApprovalResourceType::try_from(model.resource_type).map_err(|_| invariant())?;
    let digest =
        OrganizationApprovalTemplateDigest::from_persistence_key(model.definition_digest.as_str())
            .map_err(|_| invariant())?;
    let created_by_user_id = UserId::new(model.created_by_user_id).map_err(|_| invariant())?;
    let snapshot = OrganizationApprovalTemplateSnapshot::new(
        organization_id,
        template_key,
        resource_type,
        model.version,
        digest,
        created_by_user_id,
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationApprovalTemplateRecord {
        id: OrganizationApprovalTemplateId::new(model.id).map_err(|_| invariant())?,
        snapshot,
        created_at: model.created_at,
    })
}

fn request_record(
    model: organization_approval_requests::Model,
) -> Result<OrganizationApprovalRequestRecord, OrganizationApprovalRepositoryError> {
    let request = OrganizationApprovalRequest::new(
        OrganizationApprovalRequestKey::from_persistence_key(&model.request_key)
            .map_err(|_| invariant())?,
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        OrganizationApprovalResourceType::try_from(model.resource_type).map_err(|_| invariant())?,
        model.resource_summary.as_str().to_owned(),
        OrganizationApprovalTemplateId::new(model.template_id).map_err(|_| invariant())?,
        model.template_version,
        UserId::new(model.requested_by_user_id).map_err(|_| invariant())?,
        model.expires_at.unix_timestamp(),
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationApprovalRequestRecord {
        id: OrganizationApprovalRequestId::new(model.id).map_err(|_| invariant())?,
        request,
        created_at: model.created_at,
    })
}

fn decision_record(
    model: organization_approval_decisions::Model,
) -> Result<OrganizationApprovalDecisionRecord, OrganizationApprovalRepositoryError> {
    let decision = OrganizationApprovalDecision::new(
        OrganizationApprovalDecisionKey::from_persistence_key(&model.decision_key)
            .map_err(|_| invariant())?,
        OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        OrganizationApprovalRequestId::new(model.request_id).map_err(|_| invariant())?,
        OrganizationApprovalDecisionKind::try_from(model.decision_kind).map_err(|_| invariant())?,
        model
            .decided_by_user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| invariant())?,
        model.reason.map(|value| value.as_str().to_owned()),
        model.decided_at.unix_timestamp(),
    )
    .map_err(|_| invariant())?;
    Ok(OrganizationApprovalDecisionRecord {
        id: OrganizationApprovalDecisionId::new(model.id).map_err(|_| invariant())?,
        decision,
        created_at: model.created_at,
    })
}

fn to_time(seconds: i64) -> Result<TimeDateTimeWithTimeZone, OrganizationApprovalRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(seconds)
        .map_err(|_| OrganizationApprovalRepositoryError::InvalidInput)
}

async fn begin(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, OrganizationApprovalRepositoryError> {
    pool.connection().begin().await.map_err(query_error)
}

async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationApprovalRepositoryError> {
    transaction.commit().await.map_err(query_error)
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationApprovalRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

fn query_error(_: sea_orm::DbErr) -> OrganizationApprovalRepositoryError {
    OrganizationApprovalRepositoryError::Query
}

fn invariant() -> OrganizationApprovalRepositoryError {
    OrganizationApprovalRepositoryError::Invariant
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_expiration_is_rejected_before_database() {
        let result = OrganizationApprovalRequest::new(
            OrganizationApprovalRequestKey::new([1; 16]).unwrap(),
            OrganizationId::new(1).unwrap(),
            OrganizationApprovalResourceType::Budget,
            "budget change".to_owned(),
            OrganizationApprovalTemplateId::new(1).unwrap(),
            1,
            UserId::new(2).unwrap(),
            -1,
        );
        assert_eq!(result, Err(OrganizationApprovalError::InvalidTime));
    }
}
