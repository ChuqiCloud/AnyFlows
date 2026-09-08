use std::fmt;

use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationBudgetPolicyId, OrganizationBudgetScopeKind, OrganizationDepartmentId,
    OrganizationId, OrganizationMembershipId, OrganizationPermission, OrganizationTeamId, Quota,
    UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use sha2::{Digest as _, Sha256};

use crate::{
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepository, OrganizationBudgetRepositoryError,
    OrganizationBudgetScope,
    entity::organization_approval_budget_executions,
    organization::mutation,
    organization_approval_execution::{
        OrganizationApprovalExecutionRepositoryError, lock_decision, lock_request,
        require_current_member_permission,
    },
};

const BUDGET_SUMMARY_PREFIX: &str = "budget-policy-sha256:";

/// 预算审批执行的完整策略目标快照。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetExecutionWrite {
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: TimeDateTimeWithTimeZone,
    expected_policy_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_by_user_id: UserId,
}

impl OrganizationApprovalBudgetExecutionWrite {
    /// 构造带策略版本 CAS 和当前操作者双重身份的预算执行命令。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        scope: OrganizationBudgetScope,
        period: OrganizationBudgetPeriod,
        limit: OrganizationBudgetLimit,
        effective_from: TimeDateTimeWithTimeZone,
        expected_policy_version: i64,
        executed_by_membership_id: OrganizationMembershipId,
        executed_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if expected_policy_version < 0 || effective_from.unix_timestamp() < 0 {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            request_id,
            scope,
            period,
            limit,
            effective_from,
            expected_policy_version,
            executed_by_membership_id,
            executed_by_user_id,
        })
    }

    /// 返回创建审批申请时必须保存的规范化命令摘要。
    #[must_use]
    pub fn resource_summary(&self) -> String {
        organization_approval_budget_resource_summary(
            self.organization_id,
            self.scope,
            self.period,
            self.limit,
            self.effective_from.unix_timestamp(),
            self.expected_policy_version,
        )
    }
}

/// 已追加的预算审批执行事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetExecutionRecord {
    id: i64,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    decision_id: i64,
    policy_id: OrganizationBudgetPolicyId,
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
    expected_policy_version: i64,
    applied_policy_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_at: i64,
}

impl OrganizationApprovalBudgetExecutionRecord {
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
    pub const fn policy_id(&self) -> OrganizationBudgetPolicyId {
        self.policy_id
    }
    #[must_use]
    pub const fn scope(&self) -> OrganizationBudgetScope {
        self.scope
    }
    #[must_use]
    pub const fn period(&self) -> OrganizationBudgetPeriod {
        self.period
    }
    #[must_use]
    pub const fn limit(&self) -> OrganizationBudgetLimit {
        self.limit
    }
    #[must_use]
    pub const fn effective_from(&self) -> i64 {
        self.effective_from
    }
    #[must_use]
    pub const fn expected_policy_version(&self) -> i64 {
        self.expected_policy_version
    }
    #[must_use]
    pub const fn applied_policy_version(&self) -> i64 {
        self.applied_policy_version
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

/// 预算审批执行结果；Existing 表示命令未被再次应用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalBudgetExecutionOutcome {
    Applied(OrganizationApprovalBudgetExecutionRecord),
    Existing(OrganizationApprovalBudgetExecutionRecord),
}

/// 在目标预算事务内复验审批事实、权限和策略版本，并追加执行事实。
pub(crate) async fn execute_budget_policy_transaction(
    transaction: &DatabaseTransaction,
    write: OrganizationApprovalBudgetExecutionWrite,
) -> Result<OrganizationApprovalBudgetExecutionOutcome, OrganizationApprovalExecutionRepositoryError>
{
    let organization = mutation::lock_organization(transaction, write.organization_id)
        .await
        .map_err(map_organization_error)?;
    if organization.status != af_domain::OrganizationStatus::Active.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let request = lock_request(transaction, write.organization_id, write.request_id).await?;
    if request.resource_type != OrganizationApprovalResourceType::Budget.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch);
    }
    if request.resource_summary.as_str() != write.resource_summary() {
        return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    let decision = lock_decision(transaction, write.organization_id, write.request_id).await?;
    if decision.decision_kind != OrganizationApprovalDecisionKind::Approved.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::NotApproved);
    }

    let actor = mutation::lock_membership(
        transaction,
        write.organization_id,
        write.executed_by_membership_id,
    )
    .await
    .map_err(map_organization_error)?;
    require_current_member_permission(
        transaction,
        &actor,
        write.executed_by_user_id,
        OrganizationPermission::BudgetsManage,
    )
    .await?;

    if let Some(existing) =
        find_execution(transaction, write.organization_id, write.request_id).await?
    {
        let record = execution_record(existing)?;
        return exact_replay(&record, &write, decision.id)
            .then_some(OrganizationApprovalBudgetExecutionOutcome::Existing(record))
            .ok_or(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    if request.expires_at <= TimeDateTimeWithTimeZone::now_utc() {
        return Err(OrganizationApprovalExecutionRepositoryError::Expired);
    }

    let policy = OrganizationBudgetRepository::create_policy_in_transaction(
        transaction,
        OrganizationBudgetPolicyWrite::new(
            write.organization_id,
            write.scope,
            write.period,
            write.limit,
            write.effective_from,
            write.executed_by_user_id,
        ),
        write.expected_policy_version,
    )
    .await
    .map_err(map_budget_error)?;
    let executed_at = TimeDateTimeWithTimeZone::now_utc();
    let model = organization_approval_budget_executions::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        request_id: Set(write.request_id.get()),
        decision_id: Set(decision.id),
        policy_id: Set(policy.id().get()),
        scope_kind: Set(write.scope.kind().code()),
        team_id: Set(write.scope.team_id().map(OrganizationTeamId::get)),
        membership_id: Set(write
            .scope
            .membership_id()
            .map(OrganizationMembershipId::get)),
        department_id: Set(write
            .scope
            .department_id()
            .map(OrganizationDepartmentId::get)),
        period_kind: Set(write.period.code()),
        fixed_period_seconds: Set(write.period.fixed_seconds().map(i64::from)),
        quota_limit: Set(write.limit.quota().map(Quota::units)),
        effective_from: Set(write.effective_from),
        expected_policy_version: Set(write.expected_policy_version),
        applied_policy_version: Set(policy.version()),
        executed_by_membership_id: Set(write.executed_by_membership_id.get()),
        executed_at: Set(executed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)?;
    Ok(OrganizationApprovalBudgetExecutionOutcome::Applied(
        execution_record(model)?,
    ))
}

async fn find_execution(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<
    Option<organization_approval_budget_executions::Model>,
    OrganizationApprovalExecutionRepositoryError,
> {
    let mut query = organization_approval_budget_executions::Entity::find()
        .filter(
            organization_approval_budget_executions::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(organization_approval_budget_executions::Column::RequestId.eq(request_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)
}

fn exact_replay(
    record: &OrganizationApprovalBudgetExecutionRecord,
    write: &OrganizationApprovalBudgetExecutionWrite,
    decision_id: i64,
) -> bool {
    record.organization_id == write.organization_id
        && record.request_id == write.request_id
        && record.decision_id == decision_id
        && record.scope == write.scope
        && record.period == write.period
        && record.limit == write.limit
        && record.effective_from == write.effective_from.unix_timestamp()
        && record.expected_policy_version == write.expected_policy_version
        && record.executed_by_membership_id == write.executed_by_membership_id
}

fn execution_record(
    model: organization_approval_budget_executions::Model,
) -> Result<OrganizationApprovalBudgetExecutionRecord, OrganizationApprovalExecutionRepositoryError>
{
    if model.id <= 0
        || model.decision_id <= 0
        || model.expected_policy_version < 0
        || model.applied_policy_version <= 0
    {
        return Err(invariant());
    }
    let scope_kind =
        OrganizationBudgetScopeKind::try_from(model.scope_kind).map_err(|_| invariant())?;
    let scope = match (
        scope_kind,
        model.team_id,
        model.membership_id,
        model.department_id,
    ) {
        (OrganizationBudgetScopeKind::Organization, None, None, None) => {
            OrganizationBudgetScope::Organization
        }
        (OrganizationBudgetScopeKind::Team, Some(id), None, None) => {
            OrganizationBudgetScope::Team(OrganizationTeamId::new(id).map_err(|_| invariant())?)
        }
        (OrganizationBudgetScopeKind::Member, None, Some(id), None) => {
            OrganizationBudgetScope::Member(
                OrganizationMembershipId::new(id).map_err(|_| invariant())?,
            )
        }
        (OrganizationBudgetScopeKind::Department, None, None, Some(id)) => {
            OrganizationBudgetScope::Department(
                OrganizationDepartmentId::new(id).map_err(|_| invariant())?,
            )
        }
        _ => return Err(invariant()),
    };
    let fixed_seconds = model
        .fixed_period_seconds
        .map(u32::try_from)
        .transpose()
        .map_err(|_| invariant())?;
    let period = OrganizationBudgetPeriod::from_persistence(model.period_kind, fixed_seconds)
        .map_err(|_| invariant())?;
    let limit = OrganizationBudgetLimit::new(
        model
            .quota_limit
            .map(Quota::new)
            .transpose()
            .map_err(|_| invariant())?,
    )
    .map_err(|_| invariant())?;
    if model.effective_from.unix_timestamp() < 0 || model.executed_at.unix_timestamp() < 0 {
        return Err(invariant());
    }
    if model.expected_policy_version.checked_add(1) != Some(model.applied_policy_version) {
        return Err(invariant());
    }
    Ok(OrganizationApprovalBudgetExecutionRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        request_id: OrganizationApprovalRequestId::new(model.request_id)
            .map_err(|_| invariant())?,
        decision_id: model.decision_id,
        policy_id: OrganizationBudgetPolicyId::new(model.policy_id).map_err(|_| invariant())?,
        scope,
        period,
        limit,
        effective_from: model.effective_from.unix_timestamp(),
        expected_policy_version: model.expected_policy_version,
        applied_policy_version: model.applied_policy_version,
        executed_by_membership_id: OrganizationMembershipId::new(model.executed_by_membership_id)
            .map_err(|_| invariant())?,
        executed_at: model.executed_at.unix_timestamp(),
    })
}

/// 为申请创建和执行复验生成同一份预算目标摘要。
#[must_use]
pub fn organization_approval_budget_resource_summary(
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
    expected_policy_version: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:budget-policy:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hasher.update(scope.kind().code().to_be_bytes());
    hasher.update(
        scope
            .team_id()
            .map_or(0, OrganizationTeamId::get)
            .to_be_bytes(),
    );
    hasher.update(
        scope
            .membership_id()
            .map_or(0, OrganizationMembershipId::get)
            .to_be_bytes(),
    );
    hasher.update(
        scope
            .department_id()
            .map_or(0, OrganizationDepartmentId::get)
            .to_be_bytes(),
    );
    hasher.update(period.code().to_be_bytes());
    hasher.update(
        period
            .fixed_seconds()
            .map_or(0_i64, i64::from)
            .to_be_bytes(),
    );
    hasher.update(limit.quota().map_or(0, Quota::units).to_be_bytes());
    hasher.update(effective_from.to_be_bytes());
    hasher.update(expected_policy_version.to_be_bytes());
    let digest = hasher.finalize();
    let mut summary = String::with_capacity(BUDGET_SUMMARY_PREFIX.len() + digest.len() * 2);
    summary.push_str(BUDGET_SUMMARY_PREFIX);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut summary, "{byte:02x}").expect("写入 String 不会失败");
    }
    summary
}

fn map_organization_error(
    error: crate::OrganizationRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        crate::OrganizationRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        crate::OrganizationRepositoryError::NotFound
        | crate::OrganizationRepositoryError::UserUnavailable => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        crate::OrganizationRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        crate::OrganizationRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        crate::OrganizationRepositoryError::LastOwner => {
            OrganizationApprovalExecutionRepositoryError::LastOwner
        }
        crate::OrganizationRepositoryError::EntitlementRestricted => {
            OrganizationApprovalExecutionRepositoryError::EntitlementRestricted
        }
        crate::OrganizationRepositoryError::SeatLimitReached => {
            OrganizationApprovalExecutionRepositoryError::SeatLimitReached
        }
        crate::OrganizationRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        crate::OrganizationRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        crate::OrganizationRepositoryError::Invariant
        | crate::OrganizationRepositoryError::TeamHasActiveMembers
        | crate::OrganizationRepositoryError::InvitationExpired
        | crate::OrganizationRepositoryError::InvitationNotPending
        | crate::OrganizationRepositoryError::InvitationEmailMismatch
        | crate::OrganizationRepositoryError::InvitationBatchConflict
        | crate::OrganizationRepositoryError::TeamLimitReached
        | crate::OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn map_budget_error(
    error: OrganizationBudgetRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        OrganizationBudgetRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        OrganizationBudgetRepositoryError::NotFound => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        OrganizationBudgetRepositoryError::PermissionDenied => {
            OrganizationApprovalExecutionRepositoryError::Forbidden
        }
        OrganizationBudgetRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        OrganizationBudgetRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        OrganizationBudgetRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        OrganizationBudgetRepositoryError::Invariant
        | OrganizationBudgetRepositoryError::WalletQuotaInsufficient
        | OrganizationBudgetRepositoryError::BudgetQuotaInsufficient
        | OrganizationBudgetRepositoryError::BillingProtected
        | OrganizationBudgetRepositoryError::SettlementPending
        | OrganizationBudgetRepositoryError::OutcomeUnknown => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn invariant() -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Invariant
}

impl fmt::Debug for OrganizationApprovalBudgetExecutionWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationApprovalBudgetExecutionWrite(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_summary_covers_all_target_dimensions() {
        let organization_id = OrganizationId::new(1).unwrap();
        let member_id = OrganizationMembershipId::new(2).unwrap();
        let base = organization_approval_budget_resource_summary(
            organization_id,
            OrganizationBudgetScope::Member(member_id),
            OrganizationBudgetPeriod::UtcMonth,
            OrganizationBudgetLimit::new(Some(Quota::new(100).unwrap())).unwrap(),
            1_800_000_000,
            0,
        );
        assert_eq!(base.len(), BUDGET_SUMMARY_PREFIX.len() + 64);
        assert_ne!(
            base,
            organization_approval_budget_resource_summary(
                organization_id,
                OrganizationBudgetScope::Member(member_id),
                OrganizationBudgetPeriod::UtcMonth,
                OrganizationBudgetLimit::new(Some(Quota::new(101).unwrap())).unwrap(),
                1_800_000_000,
                0,
            )
        );
        assert!(!format!("{base:?}").contains("quota"));
    }
}
