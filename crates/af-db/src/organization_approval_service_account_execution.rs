use crate::{
    entity::organization_approval_service_account_executions,
    organization::mutation,
    organization_approval_execution::{
        OrganizationApprovalExecutionRepositoryError, lock_decision, lock_request,
        require_current_member_permission,
    },
    organization_service_account::{
        OrganizationServiceAccountRepositoryError, set_status_in_transaction,
    },
    organization_service_account_budget::{
        OrganizationServiceAccountBudgetBindingRepositoryError,
        set_binding_in_transaction_with_account_version,
    },
};
use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationBudgetPolicyId, OrganizationDepartmentId,
    OrganizationId, OrganizationPermission, OrganizationServiceAccountId,
    OrganizationServiceAccountLocator, OrganizationServiceAccountStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use sha2::{Digest as _, Sha256};

const STATUS_SUMMARY_PREFIX: &str = "service-account-status-sha256:";
const BUDGET_BINDING_SUMMARY_PREFIX: &str = "service-account-budget-binding-sha256:";

/// 服务账号审批允许的受控动作；密钥创建和轮换不在动作集合内。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalServiceAccountExecutionAction {
    SetStatus {
        target_status: OrganizationServiceAccountStatus,
        expected_account_version: i64,
    },
    SetBudgetBinding {
        policy_id: OrganizationBudgetPolicyId,
        expected_policy_version: i64,
        expected_binding_version: Option<i64>,
        expected_account_version: i64,
    },
}

impl OrganizationApprovalServiceAccountExecutionAction {
    #[must_use]
    pub const fn kind(self) -> i16 {
        match self {
            Self::SetStatus { .. } => 1,
            Self::SetBudgetBinding { .. } => 2,
        }
    }

    #[must_use]
    pub const fn expected_account_version(self) -> i64 {
        match self {
            Self::SetStatus {
                expected_account_version,
                ..
            }
            | Self::SetBudgetBinding {
                expected_account_version,
                ..
            } => expected_account_version,
        }
    }
}

/// 服务账号审批执行写入命令；部门归属是创建/认证时固化的快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalServiceAccountExecutionWrite {
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    service_account_id: OrganizationServiceAccountId,
    service_account_department_id: Option<OrganizationDepartmentId>,
    action: OrganizationApprovalServiceAccountExecutionAction,
    executed_by_membership_id: af_domain::OrganizationMembershipId,
    executed_by_user_id: UserId,
}

impl OrganizationApprovalServiceAccountExecutionWrite {
    /// 构造服务账号审批执行命令并校验版本边界。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        service_account_id: OrganizationServiceAccountId,
        service_account_department_id: Option<OrganizationDepartmentId>,
        action: OrganizationApprovalServiceAccountExecutionAction,
        executed_by_membership_id: af_domain::OrganizationMembershipId,
        executed_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if action.expected_account_version() <= 0
            || action.expected_account_version() == i64::MAX
            || matches!(
                action,
                OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                    expected_policy_version,
                    expected_binding_version: Some(expected_binding_version),
                    ..
                } if expected_policy_version <= 0
                    || expected_binding_version <= 0
                    || expected_binding_version == i64::MAX
            )
            || matches!(
                action,
                OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                    expected_policy_version,
                    expected_binding_version: None,
                    ..
                } if expected_policy_version <= 0
            )
        {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            request_id,
            service_account_id,
            service_account_department_id,
            action,
            executed_by_membership_id,
            executed_by_user_id,
        })
    }

    #[must_use]
    pub const fn action(&self) -> OrganizationApprovalServiceAccountExecutionAction {
        self.action
    }

    /// 返回创建审批申请时必须保存的规范化目标摘要。
    #[must_use]
    pub fn resource_summary(&self) -> String {
        match self.action {
            OrganizationApprovalServiceAccountExecutionAction::SetStatus {
                target_status,
                expected_account_version,
            } => organization_approval_service_account_status_resource_summary(
                self.organization_id,
                self.service_account_id,
                self.service_account_department_id,
                target_status,
                expected_account_version,
            ),
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                policy_id,
                expected_policy_version,
                expected_binding_version,
                expected_account_version,
            } => organization_approval_service_account_budget_binding_resource_summary(
                self.organization_id,
                self.service_account_id,
                self.service_account_department_id,
                policy_id,
                expected_policy_version,
                expected_binding_version,
                expected_account_version,
            ),
        }
    }
}

/// 已追加的服务账号审批执行事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalServiceAccountExecutionRecord {
    id: i64,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    decision_id: i64,
    service_account_id: OrganizationServiceAccountId,
    service_account_department_id: Option<OrganizationDepartmentId>,
    action: OrganizationApprovalServiceAccountExecutionAction,
    applied_account_version: i64,
    applied_policy_version: Option<i64>,
    applied_binding_version: Option<i64>,
    executed_by_membership_id: af_domain::OrganizationMembershipId,
    executed_at: i64,
}

impl OrganizationApprovalServiceAccountExecutionRecord {
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
    pub const fn service_account_id(&self) -> OrganizationServiceAccountId {
        self.service_account_id
    }
    #[must_use]
    pub const fn service_account_department_id(&self) -> Option<OrganizationDepartmentId> {
        self.service_account_department_id
    }
    #[must_use]
    pub const fn action(&self) -> OrganizationApprovalServiceAccountExecutionAction {
        self.action
    }
    #[must_use]
    pub const fn applied_account_version(&self) -> i64 {
        self.applied_account_version
    }
    #[must_use]
    pub const fn applied_policy_version(&self) -> Option<i64> {
        self.applied_policy_version
    }
    #[must_use]
    pub const fn applied_binding_version(&self) -> Option<i64> {
        self.applied_binding_version
    }
    #[must_use]
    pub const fn executed_by_membership_id(&self) -> af_domain::OrganizationMembershipId {
        self.executed_by_membership_id
    }
    #[must_use]
    pub const fn executed_at(&self) -> i64 {
        self.executed_at
    }
}

/// 服务账号审批执行结果；Existing 表示命令未被再次应用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalServiceAccountExecutionOutcome {
    Applied(OrganizationApprovalServiceAccountExecutionRecord),
    Existing(OrganizationApprovalServiceAccountExecutionRecord),
}

/// 在同一事务内复验审批事实、企业边界、权限和主体/绑定版本。
pub(crate) async fn execute_service_account_transaction(
    transaction: &DatabaseTransaction,
    write: OrganizationApprovalServiceAccountExecutionWrite,
) -> Result<
    OrganizationApprovalServiceAccountExecutionOutcome,
    OrganizationApprovalExecutionRepositoryError,
> {
    let organization = mutation::lock_organization(transaction, write.organization_id)
        .await
        .map_err(map_organization_error)?;
    if organization.status != af_domain::OrganizationStatus::Active.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let request = lock_request(transaction, write.organization_id, write.request_id).await?;
    if request.resource_type != OrganizationApprovalResourceType::ServiceAccount.code() {
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
        OrganizationPermission::ServiceAccountsManage,
    )
    .await?;

    if let Some(existing) =
        find_execution(transaction, write.organization_id, write.request_id).await?
    {
        let record = execution_record(existing)?;
        return exact_replay(&record, &write, decision.id)
            .then_some(OrganizationApprovalServiceAccountExecutionOutcome::Existing(record))
            .ok_or(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    if request.expires_at <= TimeDateTimeWithTimeZone::now_utc() {
        return Err(OrganizationApprovalExecutionRepositoryError::Expired);
    }

    let locator = write.service_account_department_id.map_or_else(
        || OrganizationServiceAccountLocator::new(write.organization_id, write.service_account_id),
        |department_id| {
            OrganizationServiceAccountLocator::with_department(
                write.organization_id,
                write.service_account_id,
                department_id,
            )
        },
    );
    let (applied_account_version, applied_policy_version, applied_binding_version) =
        match write.action {
            OrganizationApprovalServiceAccountExecutionAction::SetStatus {
                target_status,
                expected_account_version,
            } => {
                let record = set_status_in_transaction(
                    transaction,
                    locator,
                    expected_account_version,
                    target_status,
                )
                .await
                .map_err(map_service_account_error)?;
                (record.version(), None, None)
            }
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                policy_id,
                expected_policy_version,
                expected_binding_version,
                expected_account_version,
            } => {
                let binding = set_binding_in_transaction_with_account_version(
                    transaction,
                    write.organization_id,
                    write.service_account_id,
                    policy_id,
                    expected_binding_version,
                    Some(expected_account_version),
                    Some(write.service_account_department_id),
                )
                .await
                .map_err(map_budget_binding_error)?;
                if binding.policy_version() != expected_policy_version {
                    return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
                }
                (
                    expected_account_version,
                    Some(binding.policy_version()),
                    Some(binding.version()),
                )
            }
        };

    let executed_at = TimeDateTimeWithTimeZone::now_utc();
    let model = organization_approval_service_account_executions::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        request_id: Set(write.request_id.get()),
        decision_id: Set(decision.id),
        service_account_id: Set(write.service_account_id.get()),
        service_account_department_id: Set(write
            .service_account_department_id
            .map(OrganizationDepartmentId::get)),
        action_kind: Set(write.action.kind()),
        target_status: Set(match write.action {
            OrganizationApprovalServiceAccountExecutionAction::SetStatus {
                target_status, ..
            } => Some(target_status.code()),
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding { .. } => None,
        }),
        policy_id: Set(match write.action {
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                policy_id,
                ..
            } => Some(policy_id.get()),
            OrganizationApprovalServiceAccountExecutionAction::SetStatus { .. } => None,
        }),
        expected_account_version: Set(write.action.expected_account_version()),
        applied_account_version: Set(applied_account_version),
        expected_policy_version: Set(match write.action {
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                expected_policy_version,
                ..
            } => Some(expected_policy_version),
            OrganizationApprovalServiceAccountExecutionAction::SetStatus { .. } => None,
        }),
        applied_policy_version: Set(applied_policy_version),
        expected_binding_version: Set(match write.action {
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                expected_binding_version,
                ..
            } => expected_binding_version,
            OrganizationApprovalServiceAccountExecutionAction::SetStatus { .. } => None,
        }),
        applied_binding_version: Set(applied_binding_version),
        executed_by_membership_id: Set(write.executed_by_membership_id.get()),
        executed_at: Set(executed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)?;
    Ok(OrganizationApprovalServiceAccountExecutionOutcome::Applied(
        execution_record(model)?,
    ))
}

async fn find_execution(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<
    Option<organization_approval_service_account_executions::Model>,
    OrganizationApprovalExecutionRepositoryError,
> {
    let mut query = organization_approval_service_account_executions::Entity::find()
        .filter(
            organization_approval_service_account_executions::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(
            organization_approval_service_account_executions::Column::RequestId
                .eq(request_id.get()),
        );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

fn exact_replay(
    record: &OrganizationApprovalServiceAccountExecutionRecord,
    write: &OrganizationApprovalServiceAccountExecutionWrite,
    decision_id: i64,
) -> bool {
    record.organization_id == write.organization_id
        && record.request_id == write.request_id
        && record.decision_id == decision_id
        && record.service_account_id == write.service_account_id
        && record.service_account_department_id == write.service_account_department_id
        && record.action == write.action
        && record.executed_by_membership_id == write.executed_by_membership_id
}

fn execution_record(
    model: organization_approval_service_account_executions::Model,
) -> Result<
    OrganizationApprovalServiceAccountExecutionRecord,
    OrganizationApprovalExecutionRepositoryError,
> {
    if model.id <= 0
        || model.decision_id <= 0
        || model.expected_account_version <= 0
        || model.expected_account_version == i64::MAX
        || model.applied_account_version <= 0
        || model.executed_by_membership_id <= 0
        || model.executed_at.unix_timestamp() < 0
    {
        return Err(invariant());
    }
    let action = match model.action_kind {
        1 if model.policy_id.is_none()
            && model.expected_policy_version.is_none()
            && model.applied_policy_version.is_none()
            && model.expected_binding_version.is_none()
            && model.applied_binding_version.is_none() =>
        {
            OrganizationApprovalServiceAccountExecutionAction::SetStatus {
                target_status: OrganizationServiceAccountStatus::from_code(
                    model.target_status.ok_or_else(invariant)?,
                )
                .map_err(|_| invariant())?,
                expected_account_version: model.expected_account_version,
            }
        }
        2 if model.target_status.is_none()
            && model
                .expected_policy_version
                .is_some_and(|version| version > 0)
            && model
                .applied_policy_version
                .is_some_and(|version| version > 0)
            && model
                .expected_binding_version
                .is_none_or(|version| version > 0 && version < i64::MAX)
            && model
                .applied_binding_version
                .is_some_and(|version| version > 0) =>
        {
            OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                policy_id: OrganizationBudgetPolicyId::new(model.policy_id.ok_or_else(invariant)?)
                    .map_err(|_| invariant())?,
                expected_policy_version: model.expected_policy_version.ok_or_else(invariant)?,
                expected_binding_version: model.expected_binding_version,
                expected_account_version: model.expected_account_version,
            }
        }
        _ => return Err(invariant()),
    };
    if matches!(
        action,
        OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
            expected_policy_version,
            ..
        } if model.applied_policy_version != Some(expected_policy_version)
    ) {
        return Err(invariant());
    }
    let binding_version_valid = match action {
        OrganizationApprovalServiceAccountExecutionAction::SetStatus { .. } => {
            model.expected_binding_version.is_none() && model.applied_binding_version.is_none()
        }
        OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
            expected_binding_version,
            ..
        } => match expected_binding_version {
            None => model.applied_binding_version == Some(1),
            Some(expected) => model.applied_binding_version.is_some_and(|applied| {
                applied == expected || expected.checked_add(1) == Some(applied)
            }),
        },
    };
    if !binding_version_valid {
        return Err(invariant());
    }
    let account_version_valid = match action {
        OrganizationApprovalServiceAccountExecutionAction::SetStatus {
            expected_account_version,
            ..
        } => {
            model.applied_account_version == expected_account_version
                || expected_account_version
                    .checked_add(1)
                    .is_some_and(|version| model.applied_account_version == version)
        }
        OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
            expected_account_version,
            ..
        } => model.applied_account_version == expected_account_version,
    };
    if !account_version_valid {
        return Err(invariant());
    }
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let service_account_id =
        OrganizationServiceAccountId::new(model.service_account_id).map_err(|_| invariant())?;
    let department_id = model
        .service_account_department_id
        .map(OrganizationDepartmentId::new)
        .transpose()
        .map_err(|_| invariant())?;
    if model.executed_at.unix_timestamp() < 0 {
        return Err(invariant());
    }
    Ok(OrganizationApprovalServiceAccountExecutionRecord {
        id: model.id,
        organization_id,
        request_id: OrganizationApprovalRequestId::new(model.request_id)
            .map_err(|_| invariant())?,
        decision_id: model.decision_id,
        service_account_id,
        service_account_department_id: department_id,
        action,
        applied_account_version: model.applied_account_version,
        applied_policy_version: model.applied_policy_version,
        applied_binding_version: model.applied_binding_version,
        executed_by_membership_id: af_domain::OrganizationMembershipId::new(
            model.executed_by_membership_id,
        )
        .map_err(|_| invariant())?,
        executed_at: model.executed_at.unix_timestamp(),
    })
}

/// 为状态审批申请和执行复验生成同一份目标摘要。
#[must_use]
pub fn organization_approval_service_account_status_resource_summary(
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    department_id: Option<OrganizationDepartmentId>,
    target_status: OrganizationServiceAccountStatus,
    expected_account_version: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:service-account-status:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hasher.update(service_account_id.get().to_be_bytes());
    hasher.update(
        department_id
            .map_or(0, OrganizationDepartmentId::get)
            .to_be_bytes(),
    );
    hasher.update(target_status.code().to_be_bytes());
    hasher.update(expected_account_version.to_be_bytes());
    digest_summary(STATUS_SUMMARY_PREFIX, hasher.finalize())
}

/// 为预算绑定审批申请和执行复验生成同一份目标摘要。
#[must_use]
pub fn organization_approval_service_account_budget_binding_resource_summary(
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    department_id: Option<OrganizationDepartmentId>,
    policy_id: OrganizationBudgetPolicyId,
    expected_policy_version: i64,
    expected_binding_version: Option<i64>,
    expected_account_version: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:service-account-budget-binding:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hasher.update(service_account_id.get().to_be_bytes());
    hasher.update(
        department_id
            .map_or(0, OrganizationDepartmentId::get)
            .to_be_bytes(),
    );
    hasher.update(policy_id.get().to_be_bytes());
    hasher.update(expected_policy_version.to_be_bytes());
    hasher.update(expected_binding_version.unwrap_or(0).to_be_bytes());
    hasher.update(expected_account_version.to_be_bytes());
    digest_summary(BUDGET_BINDING_SUMMARY_PREFIX, hasher.finalize())
}

fn digest_summary(prefix: &str, digest: impl AsRef<[u8]>) -> String {
    let bytes = digest.as_ref();
    let mut summary = String::with_capacity(prefix.len() + bytes.len() * 2);
    summary.push_str(prefix);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut summary, "{byte:02x}").expect("写入 String 不会失败");
    }
    summary
}

fn map_service_account_error(
    error: OrganizationServiceAccountRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        OrganizationServiceAccountRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        OrganizationServiceAccountRepositoryError::NotFound => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        OrganizationServiceAccountRepositoryError::Restricted => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        OrganizationServiceAccountRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        OrganizationServiceAccountRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        OrganizationServiceAccountRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        OrganizationServiceAccountRepositoryError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn map_budget_binding_error(
    error: OrganizationServiceAccountBudgetBindingRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        OrganizationServiceAccountBudgetBindingRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::NotFound => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::AccountUnavailable
        | OrganizationServiceAccountBudgetBindingRepositoryError::TeamUnavailable => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        OrganizationServiceAccountBudgetBindingRepositoryError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
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
        crate::OrganizationRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        crate::OrganizationRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        crate::OrganizationRepositoryError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
        _ => OrganizationApprovalExecutionRepositoryError::Forbidden,
    }
}

fn query_error(_: sea_orm::DbErr) -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Query
}

fn invariant() -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Invariant
}
