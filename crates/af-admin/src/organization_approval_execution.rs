use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationApprovalBudgetExecutionOutcome, OrganizationApprovalBudgetExecutionRecord,
    OrganizationApprovalBudgetExecutionWrite, OrganizationApprovalExecutionRepository,
    OrganizationApprovalExecutionRepositoryError, OrganizationApprovalMemberExecutionOutcome,
    OrganizationApprovalMemberExecutionRecord, OrganizationApprovalMemberExecutionWrite,
    OrganizationBudgetScope, organization_approval_budget_resource_summary,
    organization_approval_member_resource_summary,
};
use af_domain::{
    OrganizationApprovalRequestId, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationPermission,
    OrganizationRole, OrganizationTeamId,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 成员审批执行命令；所有可变成员字段都使用完整目标快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberChangeCommand {
    membership_id: OrganizationMembershipId,
    role: OrganizationRole,
    status: OrganizationMembershipStatus,
    team_id: Option<OrganizationTeamId>,
    expected_version: i64,
}

impl OrganizationApprovalMemberChangeCommand {
    /// 构造带 CAS 版本的成员审批目标。
    pub fn new(
        membership_id: OrganizationMembershipId,
        role: OrganizationRole,
        status: OrganizationMembershipStatus,
        team_id: Option<OrganizationTeamId>,
        expected_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if expected_version <= 0 {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            membership_id,
            role,
            status,
            team_id,
            expected_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_member_resource_summary(
            context.organization_id(),
            self.membership_id,
            self.role,
            self.status,
            self.team_id,
            self.expected_version,
        )
    }
}

/// 应用层成员审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberExecutionResult {
    record: OrganizationApprovalMemberExecutionRecord,
    replayed: bool,
}

/// 应用层预算审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetExecutionResult {
    record: OrganizationApprovalBudgetExecutionRecord,
    replayed: bool,
}

impl OrganizationApprovalBudgetExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalBudgetExecutionRecord {
        &self.record
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl OrganizationApprovalMemberExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalMemberExecutionRecord {
        &self.record
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

/// 审批执行应用层错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalExecutionError {
    #[error("审批执行请求无效")]
    InvalidRequest,
    #[error("审批执行权限不足")]
    Forbidden,
    #[error("审批事实或资源不存在")]
    NotFound,
    #[error("审批申请资源类型不匹配")]
    ResourceTypeMismatch,
    #[error("审批申请未获批准")]
    NotApproved,
    #[error("审批申请已过期")]
    Expired,
    #[error("审批执行版本或命令冲突")]
    Conflict,
    #[error("成员目标状态转换无效")]
    InvalidTransition,
    #[error("企业必须保留至少一个活动所有者")]
    LastOwner,
    #[error("企业商业授权不允许当前操作")]
    EntitlementRestricted,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("审批执行内部错误")]
    Internal,
}

pub type OrganizationApprovalMemberExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalMemberExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationApprovalBudgetExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalBudgetExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 预算审批执行命令；策略字段与版本均来自申请时的完整快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetPolicyCommand {
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
    expected_policy_version: i64,
}

impl OrganizationApprovalBudgetPolicyCommand {
    /// 构造带策略版本 CAS 的预算审批目标。
    pub fn new(
        scope: OrganizationBudgetScope,
        period: OrganizationBudgetPeriod,
        limit: OrganizationBudgetLimit,
        effective_from: i64,
        expected_policy_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if effective_from < 0 || expected_policy_version < 0 {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            scope,
            period,
            limit,
            effective_from,
            expected_policy_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_budget_resource_summary(
            context.organization_id(),
            self.scope,
            self.period,
            self.limit,
            self.effective_from,
            self.expected_policy_version,
        )
    }
}

/// 企业审批资源执行端口；按资源类型提供受控适配器，不接受宽泛资源载荷。
pub trait OrganizationApprovalExecutionService: Send + Sync {
    fn execute_member_change<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalMemberChangeCommand,
    ) -> OrganizationApprovalMemberExecutionFuture<'a>;

    fn execute_budget_policy<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalBudgetPolicyCommand,
    ) -> OrganizationApprovalBudgetExecutionFuture<'a>;
}

/// 使用数据库原子复验仓储实现成员审批执行。
pub struct DatabaseOrganizationApprovalExecutionService {
    repository: Arc<OrganizationApprovalExecutionRepository>,
}

impl DatabaseOrganizationApprovalExecutionService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationApprovalExecutionRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationApprovalExecutionService for DatabaseOrganizationApprovalExecutionService {
    fn execute_member_change<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalMemberChangeCommand,
    ) -> OrganizationApprovalMemberExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::MembersManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let write = OrganizationApprovalMemberExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.membership_id,
                command.role,
                command.status,
                command.team_id,
                command.expected_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_member_change(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalMemberExecutionOutcome::Applied(record) => (record, false),
                OrganizationApprovalMemberExecutionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationApprovalMemberExecutionResult { record, replayed })
        })
    }

    fn execute_budget_policy<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalBudgetPolicyCommand,
    ) -> OrganizationApprovalBudgetExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::BudgetsManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let effective_from =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationApprovalExecutionError::InvalidRequest)?;
            let write = OrganizationApprovalBudgetExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.scope,
                command.period,
                command.limit,
                effective_from,
                command.expected_policy_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_budget_policy(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalBudgetExecutionOutcome::Applied(record) => (record, false),
                OrganizationApprovalBudgetExecutionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationApprovalBudgetExecutionResult { record, replayed })
        })
    }
}

impl fmt::Debug for DatabaseOrganizationApprovalExecutionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationApprovalExecutionService(<repository>)")
    }
}

fn map_repository_error(
    error: OrganizationApprovalExecutionRepositoryError,
) -> OrganizationApprovalExecutionError {
    match error {
        OrganizationApprovalExecutionRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionError::InvalidRequest
        }
        OrganizationApprovalExecutionRepositoryError::NotFound => {
            OrganizationApprovalExecutionError::NotFound
        }
        OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch => {
            OrganizationApprovalExecutionError::ResourceTypeMismatch
        }
        OrganizationApprovalExecutionRepositoryError::NotApproved => {
            OrganizationApprovalExecutionError::NotApproved
        }
        OrganizationApprovalExecutionRepositoryError::Expired => {
            OrganizationApprovalExecutionError::Expired
        }
        OrganizationApprovalExecutionRepositoryError::Forbidden => {
            OrganizationApprovalExecutionError::Forbidden
        }
        OrganizationApprovalExecutionRepositoryError::Conflict => {
            OrganizationApprovalExecutionError::Conflict
        }
        OrganizationApprovalExecutionRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionError::InvalidTransition
        }
        OrganizationApprovalExecutionRepositoryError::LastOwner => {
            OrganizationApprovalExecutionError::LastOwner
        }
        OrganizationApprovalExecutionRepositoryError::EntitlementRestricted => {
            OrganizationApprovalExecutionError::EntitlementRestricted
        }
        OrganizationApprovalExecutionRepositoryError::SeatLimitReached => {
            OrganizationApprovalExecutionError::SeatLimitReached
        }
        OrganizationApprovalExecutionRepositoryError::Query
        | OrganizationApprovalExecutionRepositoryError::Timeout
        | OrganizationApprovalExecutionRepositoryError::Invariant => {
            OrganizationApprovalExecutionError::Internal
        }
    }
}
