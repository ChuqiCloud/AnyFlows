use std::fmt;

use af_domain::{
    BillingReservationId, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationBudgetPolicyId, OrganizationBudgetScopeKind, OrganizationBudgetWindowId,
    OrganizationId, OrganizationMembershipId, OrganizationTeamId, Quota, UserId,
};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use thiserror::Error;

/// 预算策略作用主体；枚举形状保证团队与成员标识不会同时出现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationBudgetScope {
    Organization,
    Team(OrganizationTeamId),
    Member(OrganizationMembershipId),
}

impl OrganizationBudgetScope {
    pub(crate) const fn kind(self) -> OrganizationBudgetScopeKind {
        match self {
            Self::Organization => OrganizationBudgetScopeKind::Organization,
            Self::Team(_) => OrganizationBudgetScopeKind::Team,
            Self::Member(_) => OrganizationBudgetScopeKind::Member,
        }
    }

    pub(crate) const fn team_id(self) -> Option<OrganizationTeamId> {
        match self {
            Self::Team(id) => Some(id),
            Self::Organization | Self::Member(_) => None,
        }
    }

    pub(crate) const fn membership_id(self) -> Option<OrganizationMembershipId> {
        match self {
            Self::Member(id) => Some(id),
            Self::Organization | Self::Team(_) => None,
        }
    }
}

/// 创建一个新预算策略版本所需的完整输入。
#[derive(Clone)]
pub struct OrganizationBudgetPolicyWrite {
    pub(crate) organization_id: OrganizationId,
    pub(crate) scope: OrganizationBudgetScope,
    pub(crate) period: OrganizationBudgetPeriod,
    pub(crate) limit: OrganizationBudgetLimit,
    pub(crate) effective_from: TimeDateTimeWithTimeZone,
    pub(crate) actor_user_id: UserId,
}

impl OrganizationBudgetPolicyWrite {
    #[must_use]
    pub const fn new(
        organization_id: OrganizationId,
        scope: OrganizationBudgetScope,
        period: OrganizationBudgetPeriod,
        limit: OrganizationBudgetLimit,
        effective_from: TimeDateTimeWithTimeZone,
        actor_user_id: UserId,
    ) -> Self {
        Self {
            organization_id,
            scope,
            period,
            limit,
            effective_from,
            actor_user_id,
        }
    }
}

impl fmt::Debug for OrganizationBudgetPolicyWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationBudgetPolicyWrite(<redacted>)")
    }
}

/// 已持久化的不可变预算策略版本投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationBudgetPolicyRecord {
    pub(crate) id: OrganizationBudgetPolicyId,
    pub(crate) organization_id: OrganizationId,
    pub(crate) scope: OrganizationBudgetScope,
    pub(crate) period: OrganizationBudgetPeriod,
    pub(crate) limit: OrganizationBudgetLimit,
    pub(crate) effective_from: TimeDateTimeWithTimeZone,
    pub(crate) effective_until: Option<TimeDateTimeWithTimeZone>,
    pub(crate) version: i64,
}

impl OrganizationBudgetPolicyRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationBudgetPolicyId {
        self.id
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
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
    pub const fn effective_from(&self) -> TimeDateTimeWithTimeZone {
        self.effective_from
    }

    #[must_use]
    pub const fn effective_until(&self) -> Option<TimeDateTimeWithTimeZone> {
        self.effective_until
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 企业请求的资金与预算归属；请求期间不得重新解析这些主体。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OrganizationBillingPrincipal {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    team_id: Option<OrganizationTeamId>,
    user_id: UserId,
}

impl OrganizationBillingPrincipal {
    #[must_use]
    pub const fn new(
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        team_id: Option<OrganizationTeamId>,
        user_id: UserId,
    ) -> Self {
        Self {
            organization_id,
            membership_id,
            team_id,
            user_id,
        }
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.membership_id
    }

    #[must_use]
    pub const fn team_id(self) -> Option<OrganizationTeamId> {
        self.team_id
    }

    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }
}

impl fmt::Debug for OrganizationBillingPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationBillingPrincipal(<redacted>)")
    }
}

/// 企业计费预留的持久化状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationBillingReservationStatus {
    Reserved = 1,
    SettlementPending = 2,
    Settled = 3,
    Released = 4,
}

impl OrganizationBillingReservationStatus {
    pub(crate) const fn code(self) -> i16 {
        self as i16
    }

    pub(crate) fn from_code(code: i16) -> Result<Self, OrganizationBudgetRepositoryError> {
        match code {
            1 => Ok(Self::Reserved),
            2 => Ok(Self::SettlementPending),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Released),
            _ => Err(OrganizationBudgetRepositoryError::Invariant),
        }
    }
}

/// 幂等企业预留状态转换结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationBillingMutationOutcome {
    Applied(OrganizationBillingReservationStatus),
    Existing(OrganizationBillingReservationStatus),
}

/// 企业预留固化的一层预算窗口快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationBudgetAllocationRecord {
    pub(crate) policy_id: OrganizationBudgetPolicyId,
    pub(crate) window_id: OrganizationBudgetWindowId,
    pub(crate) policy_version: i64,
    pub(crate) window_version: i64,
    pub(crate) reserved_quota: Quota,
}

/// 企业计费预留的安全投影，不暴露钱包余额或账本内容。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationBillingReservationRecord {
    pub(crate) id: BillingReservationId,
    pub(crate) principal: OrganizationBillingPrincipal,
    pub(crate) status: OrganizationBillingReservationStatus,
    pub(crate) reserved_quota: Quota,
    pub(crate) actual_quota: Option<Quota>,
    pub(crate) organization: Option<OrganizationBudgetAllocationRecord>,
    pub(crate) team: Option<OrganizationBudgetAllocationRecord>,
    pub(crate) member: Option<OrganizationBudgetAllocationRecord>,
}

impl OrganizationBillingReservationRecord {
    #[must_use]
    pub const fn status(&self) -> OrganizationBillingReservationStatus {
        self.status
    }

    #[must_use]
    pub const fn reserved_quota(&self) -> Quota {
        self.reserved_quota
    }

    #[must_use]
    pub const fn actual_quota(&self) -> Option<Quota> {
        self.actual_quota
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationBudgetRepositoryConfigError {
    #[error("企业预算操作超时必须大于零")]
    ZeroOperationTimeout,
    #[error("企业预留期限必须大于零")]
    ZeroReservationTtl,
}

/// 企业预算与预留仓储错误；不携带主体、额度或底层数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationBudgetRepositoryError {
    #[error("企业预算输入无效")]
    InvalidInput,
    #[error("企业预算资源不存在")]
    NotFound,
    #[error("企业预算操作权限不足")]
    PermissionDenied,
    #[error("企业预算状态冲突")]
    Conflict,
    #[error("企业钱包余额不足")]
    WalletQuotaInsufficient,
    #[error("企业预算窗口额度不足")]
    BudgetQuotaInsufficient,
    #[error("企业计费保护已启用")]
    BillingProtected,
    #[error("实际用量超过预留且暂未完成补扣")]
    SettlementPending,
    #[error("企业预算数据库操作失败")]
    Query,
    #[error("企业预算操作结果未知")]
    OutcomeUnknown,
    #[error("企业预算操作超时")]
    Timeout,
    #[error("企业预算持久化状态损坏")]
    Invariant,
}
