use std::{fmt, time::Duration};

use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationId, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationRole, OrganizationStatus,
    OrganizationTeamId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::LockType,
};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::organization_approval_budget_execution::{
    OrganizationApprovalBudgetExecutionOutcome, OrganizationApprovalBudgetExecutionWrite,
    execute_budget_policy_transaction,
};
use crate::{
    DatabasePool, OrganizationMembershipChange, OrganizationRepositoryError,
    entity::{
        organization_approval_decisions, organization_approval_member_executions,
        organization_approval_requests, organization_entitlements, organization_memberships,
    },
    organization::mutation,
    organization_custom_role_binding::active_custom_role_permissions,
};

const MEMBER_SUMMARY_PREFIX: &str = "member-change-sha256:";

/// 成员审批执行的完整结构化目标；摘要由该快照稳定派生。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberExecutionWrite {
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    target_membership_id: OrganizationMembershipId,
    target_role: OrganizationRole,
    target_status: OrganizationMembershipStatus,
    target_team_id: Option<OrganizationTeamId>,
    expected_membership_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_by_user_id: UserId,
}

impl OrganizationApprovalMemberExecutionWrite {
    /// 构造带成员版本 CAS 和当前操作者双重身份的执行命令。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        target_membership_id: OrganizationMembershipId,
        target_role: OrganizationRole,
        target_status: OrganizationMembershipStatus,
        target_team_id: Option<OrganizationTeamId>,
        expected_membership_version: i64,
        executed_by_membership_id: OrganizationMembershipId,
        executed_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if expected_membership_version <= 0 {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            request_id,
            target_membership_id,
            target_role,
            target_status,
            target_team_id,
            expected_membership_version,
            executed_by_membership_id,
            executed_by_user_id,
        })
    }

    /// 返回创建审批申请时必须保存的规范化命令摘要。
    #[must_use]
    pub fn resource_summary(&self) -> String {
        organization_approval_member_resource_summary(
            self.organization_id,
            self.target_membership_id,
            self.target_role,
            self.target_status,
            self.target_team_id,
            self.expected_membership_version,
        )
    }
}

/// 已追加的成员审批执行事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberExecutionRecord {
    id: i64,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    decision_id: i64,
    target_membership_id: OrganizationMembershipId,
    target_role: OrganizationRole,
    target_status: OrganizationMembershipStatus,
    target_team_id: Option<OrganizationTeamId>,
    expected_membership_version: i64,
    applied_membership_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_at: i64,
}

impl OrganizationApprovalMemberExecutionRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationApprovalRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn decision_id(&self) -> i64 {
        self.decision_id
    }
    #[must_use]
    pub const fn target_membership_id(&self) -> OrganizationMembershipId {
        self.target_membership_id
    }
    #[must_use]
    pub const fn target_role(&self) -> OrganizationRole {
        self.target_role
    }
    #[must_use]
    pub const fn target_status(&self) -> OrganizationMembershipStatus {
        self.target_status
    }
    #[must_use]
    pub const fn target_team_id(&self) -> Option<OrganizationTeamId> {
        self.target_team_id
    }
    #[must_use]
    pub const fn expected_membership_version(&self) -> i64 {
        self.expected_membership_version
    }
    #[must_use]
    pub const fn applied_membership_version(&self) -> i64 {
        self.applied_membership_version
    }
    #[must_use]
    pub const fn executed_by_membership_id(&self) -> OrganizationMembershipId {
        self.executed_by_membership_id
    }
    #[must_use]
    pub const fn executed_at(&self) -> i64 {
        self.executed_at
    }
}

/// 成员审批执行结果；Existing 表示命令未被再次应用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalMemberExecutionOutcome {
    Applied(OrganizationApprovalMemberExecutionRecord),
    Existing(OrganizationApprovalMemberExecutionRecord),
}

/// 审批执行仓储错误；不携带申请摘要或用户输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalExecutionRepositoryError {
    #[error("审批执行输入无效")]
    InvalidInput,
    #[error("审批执行事实不存在")]
    NotFound,
    #[error("审批申请资源类型不匹配")]
    ResourceTypeMismatch,
    #[error("审批申请未获批准")]
    NotApproved,
    #[error("审批申请已过期")]
    Expired,
    #[error("审批执行权限不足")]
    Forbidden,
    #[error("审批执行命令或版本冲突")]
    Conflict,
    #[error("成员目标状态转换无效")]
    InvalidTransition,
    #[error("企业必须保留至少一个活动所有者")]
    LastOwner,
    #[error("企业商业授权不允许当前操作")]
    EntitlementRestricted,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("审批执行数据库操作失败")]
    Query,
    #[error("审批执行操作超时")]
    Timeout,
    #[error("审批执行持久化状态损坏")]
    Invariant,
}

/// 在单一业务事务内复验审批事实、当前权限和目标版本的执行仓储。
#[derive(Clone)]
pub struct OrganizationApprovalExecutionRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationApprovalExecutionRepository {
    /// 使用共享数据库连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 执行或幂等恢复一个已批准的成员目标快照。
    pub async fn execute_member_change(
        &self,
        write: OrganizationApprovalMemberExecutionWrite,
    ) -> Result<
        OrganizationApprovalMemberExecutionOutcome,
        OrganizationApprovalExecutionRepositoryError,
    > {
        timeout(
            self.operation_timeout,
            self.execute_member_change_inner(write)
                .with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationApprovalExecutionRepositoryError::Timeout)?
    }

    /// 执行或幂等恢复一个已批准的预算策略目标快照。
    pub async fn execute_budget_policy(
        &self,
        write: OrganizationApprovalBudgetExecutionWrite,
    ) -> Result<
        OrganizationApprovalBudgetExecutionOutcome,
        OrganizationApprovalExecutionRepositoryError,
    > {
        timeout(
            self.operation_timeout,
            self.execute_budget_policy_inner(write)
                .with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| OrganizationApprovalExecutionRepositoryError::Timeout)?
    }

    async fn execute_member_change_inner(
        &self,
        write: OrganizationApprovalMemberExecutionWrite,
    ) -> Result<
        OrganizationApprovalMemberExecutionOutcome,
        OrganizationApprovalExecutionRepositoryError,
    > {
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        let result = execute_member_change_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                transaction.commit().await.map_err(query_error)?;
                Ok(outcome)
            }
            Err(error) => {
                transaction.rollback().await.map_err(query_error)?;
                Err(error)
            }
        }
    }

    async fn execute_budget_policy_inner(
        &self,
        write: OrganizationApprovalBudgetExecutionWrite,
    ) -> Result<
        OrganizationApprovalBudgetExecutionOutcome,
        OrganizationApprovalExecutionRepositoryError,
    > {
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        let result = execute_budget_policy_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                transaction.commit().await.map_err(query_error)?;
                Ok(outcome)
            }
            Err(error) => {
                transaction.rollback().await.map_err(query_error)?;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for OrganizationApprovalExecutionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalExecutionRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn execute_member_change_transaction(
    transaction: &DatabaseTransaction,
    write: OrganizationApprovalMemberExecutionWrite,
) -> Result<OrganizationApprovalMemberExecutionOutcome, OrganizationApprovalExecutionRepositoryError>
{
    let organization = mutation::lock_organization(transaction, write.organization_id)
        .await
        .map_err(map_organization_error)?;
    if organization.status != OrganizationStatus::Active.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let request = lock_request(transaction, write.organization_id, write.request_id).await?;
    if request.resource_type != OrganizationApprovalResourceType::Member.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch);
    }
    if request.resource_summary.as_str() != write.resource_summary() {
        return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    let decision = lock_decision(transaction, write.organization_id, write.request_id).await?;
    if decision.decision_kind != OrganizationApprovalDecisionKind::Approved.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::NotApproved);
    }

    let (actor, target) = lock_actor_and_target(transaction, &write).await?;
    require_current_member_permission(
        transaction,
        &actor,
        write.executed_by_user_id,
        OrganizationPermission::MembersManage,
    )
    .await?;
    if let Some(existing) =
        find_execution(transaction, write.organization_id, write.request_id).await?
    {
        let record = execution_record(existing)?;
        return exact_replay(&record, &write, decision.id)
            .then_some(OrganizationApprovalMemberExecutionOutcome::Existing(record))
            .ok_or(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    if request.expires_at <= TimeDateTimeWithTimeZone::now_utc() {
        return Err(OrganizationApprovalExecutionRepositoryError::Expired);
    }

    let current_role = OrganizationRole::try_from(target.role).map_err(|_| invariant())?;
    let actor_role = OrganizationRole::try_from(actor.role).map_err(|_| invariant())?;
    if current_role.is_owner() && !actor_role.is_owner() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    if current_role.is_owner() != write.target_role.is_owner() {
        return Err(OrganizationApprovalExecutionRepositoryError::InvalidTransition);
    }

    let member = mutation::change_membership_in_transaction(
        transaction,
        write.organization_id,
        write.target_membership_id,
        OrganizationMembershipChange::new(
            write.target_role,
            write.target_status,
            write.target_team_id,
            write.expected_membership_version,
        ),
    )
    .await
    .map_err(map_organization_error)?;
    let executed_at = TimeDateTimeWithTimeZone::now_utc();
    let model = organization_approval_member_executions::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        request_id: Set(write.request_id.get()),
        decision_id: Set(decision.id),
        target_membership_id: Set(write.target_membership_id.get()),
        target_role: Set(write.target_role.code()),
        target_status: Set(write.target_status.code()),
        target_team_id: Set(write.target_team_id.map(OrganizationTeamId::get)),
        expected_membership_version: Set(write.expected_membership_version),
        applied_membership_version: Set(member.version()),
        executed_by_membership_id: Set(write.executed_by_membership_id.get()),
        executed_at: Set(executed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(query_error)?;
    Ok(OrganizationApprovalMemberExecutionOutcome::Applied(
        execution_record(model)?,
    ))
}

async fn lock_actor_and_target(
    transaction: &DatabaseTransaction,
    write: &OrganizationApprovalMemberExecutionWrite,
) -> Result<
    (
        organization_memberships::Model,
        organization_memberships::Model,
    ),
    OrganizationApprovalExecutionRepositoryError,
> {
    if write.executed_by_membership_id == write.target_membership_id {
        let member = mutation::lock_membership(
            transaction,
            write.organization_id,
            write.executed_by_membership_id,
        )
        .await
        .map_err(map_organization_error)?;
        return Ok((member.clone(), member));
    }
    // 成员行按主键升序加锁，避免互相审批成员变更时形成反向锁序。
    if write.executed_by_membership_id < write.target_membership_id {
        let actor = mutation::lock_membership(
            transaction,
            write.organization_id,
            write.executed_by_membership_id,
        )
        .await
        .map_err(map_organization_error)?;
        let target = mutation::lock_membership(
            transaction,
            write.organization_id,
            write.target_membership_id,
        )
        .await
        .map_err(map_organization_error)?;
        Ok((actor, target))
    } else {
        let target = mutation::lock_membership(
            transaction,
            write.organization_id,
            write.target_membership_id,
        )
        .await
        .map_err(map_organization_error)?;
        let actor = mutation::lock_membership(
            transaction,
            write.organization_id,
            write.executed_by_membership_id,
        )
        .await
        .map_err(map_organization_error)?;
        Ok((actor, target))
    }
}

pub(crate) async fn require_current_member_permission(
    transaction: &DatabaseTransaction,
    actor: &organization_memberships::Model,
    expected_user_id: UserId,
    permission: OrganizationPermission,
) -> Result<(), OrganizationApprovalExecutionRepositoryError> {
    if actor.user_id != expected_user_id.get()
        || actor.status != OrganizationMembershipStatus::Active.code()
    {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let role = OrganizationRole::try_from(actor.role).map_err(|_| invariant())?;
    if role.permissions().contains(&permission) {
        return Ok(());
    }
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(actor.organization_id))
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    let custom =
        active_custom_role_permissions(transaction, actor, entitlement.custom_roles_enabled)
            .await
            .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)?;
    custom
        .as_deref()
        .is_some_and(|permissions| permissions.contains(&permission))
        .then_some(())
        .ok_or(OrganizationApprovalExecutionRepositoryError::Forbidden)
}

pub(crate) async fn lock_request(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<organization_approval_requests::Model, OrganizationApprovalExecutionRepositoryError> {
    let mut query = organization_approval_requests::Entity::find_by_id(request_id.get())
        .filter(organization_approval_requests::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationApprovalExecutionRepositoryError::NotFound)
}

pub(crate) async fn lock_decision(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<organization_approval_decisions::Model, OrganizationApprovalExecutionRepositoryError> {
    let mut query = organization_approval_decisions::Entity::find()
        .filter(organization_approval_decisions::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_approval_decisions::Column::RequestId.eq(request_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationApprovalExecutionRepositoryError::NotApproved)
}

async fn find_execution(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<
    Option<organization_approval_member_executions::Model>,
    OrganizationApprovalExecutionRepositoryError,
> {
    let mut query = organization_approval_member_executions::Entity::find()
        .filter(
            organization_approval_member_executions::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(organization_approval_member_executions::Column::RequestId.eq(request_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

fn exact_replay(
    record: &OrganizationApprovalMemberExecutionRecord,
    write: &OrganizationApprovalMemberExecutionWrite,
    decision_id: i64,
) -> bool {
    record.organization_id == write.organization_id
        && record.request_id == write.request_id
        && record.decision_id == decision_id
        && record.target_membership_id == write.target_membership_id
        && record.target_role == write.target_role
        && record.target_status == write.target_status
        && record.target_team_id == write.target_team_id
        && record.expected_membership_version == write.expected_membership_version
        && record.executed_by_membership_id == write.executed_by_membership_id
}

fn execution_record(
    model: organization_approval_member_executions::Model,
) -> Result<OrganizationApprovalMemberExecutionRecord, OrganizationApprovalExecutionRepositoryError>
{
    if model.id <= 0 || model.decision_id <= 0 || model.applied_membership_version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationApprovalMemberExecutionRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        request_id: OrganizationApprovalRequestId::new(model.request_id)
            .map_err(|_| invariant())?,
        decision_id: model.decision_id,
        target_membership_id: OrganizationMembershipId::new(model.target_membership_id)
            .map_err(|_| invariant())?,
        target_role: OrganizationRole::try_from(model.target_role).map_err(|_| invariant())?,
        target_status: OrganizationMembershipStatus::try_from(model.target_status)
            .map_err(|_| invariant())?,
        target_team_id: model
            .target_team_id
            .map(OrganizationTeamId::new)
            .transpose()
            .map_err(|_| invariant())?,
        expected_membership_version: model.expected_membership_version,
        applied_membership_version: model.applied_membership_version,
        executed_by_membership_id: OrganizationMembershipId::new(model.executed_by_membership_id)
            .map_err(|_| invariant())?,
        executed_at: model.executed_at.unix_timestamp(),
    })
}

/// 为申请创建和执行复验生成同一份成员目标摘要。
#[must_use]
pub fn organization_approval_member_resource_summary(
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    role: OrganizationRole,
    status: OrganizationMembershipStatus,
    team_id: Option<OrganizationTeamId>,
    expected_version: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:member-change:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hasher.update(membership_id.get().to_be_bytes());
    hasher.update(role.code().to_be_bytes());
    hasher.update(status.code().to_be_bytes());
    hasher.update(team_id.map_or(0, OrganizationTeamId::get).to_be_bytes());
    hasher.update(expected_version.to_be_bytes());
    let digest = hasher.finalize();
    let mut summary = String::with_capacity(MEMBER_SUMMARY_PREFIX.len() + digest.len() * 2);
    summary.push_str(MEMBER_SUMMARY_PREFIX);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut summary, "{byte:02x}").expect("写入 String 不会失败");
    }
    summary
}

fn map_organization_error(
    error: OrganizationRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        OrganizationRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        OrganizationRepositoryError::NotFound | OrganizationRepositoryError::UserUnavailable => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        OrganizationRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        OrganizationRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        OrganizationRepositoryError::LastOwner => {
            OrganizationApprovalExecutionRepositoryError::LastOwner
        }
        OrganizationRepositoryError::EntitlementRestricted => {
            OrganizationApprovalExecutionRepositoryError::EntitlementRestricted
        }
        OrganizationRepositoryError::SeatLimitReached => {
            OrganizationApprovalExecutionRepositoryError::SeatLimitReached
        }
        OrganizationRepositoryError::Query => OrganizationApprovalExecutionRepositoryError::Query,
        OrganizationRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        OrganizationRepositoryError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
        OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::TeamLimitReached
        | OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn query_error(_: sea_orm::DbErr) -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Query
}

fn invariant() -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Invariant
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_summary_changes_with_every_command_dimension() {
        let organization_id = OrganizationId::new(1).unwrap();
        let member_id = OrganizationMembershipId::new(2).unwrap();
        let base = organization_approval_member_resource_summary(
            organization_id,
            member_id,
            OrganizationRole::Member,
            OrganizationMembershipStatus::Active,
            None,
            1,
        );
        assert_eq!(base.len(), MEMBER_SUMMARY_PREFIX.len() + 64);
        assert_ne!(
            base,
            organization_approval_member_resource_summary(
                organization_id,
                member_id,
                OrganizationRole::Viewer,
                OrganizationMembershipStatus::Active,
                None,
                1,
            )
        );
        assert!(!format!("{base:?}").contains("member_id"));
    }
}
