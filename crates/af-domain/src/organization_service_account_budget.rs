use thiserror::Error;

use crate::{
    OrganizationBudgetPolicyId, OrganizationId, OrganizationServiceAccountId, OrganizationTeamId,
};

/// 服务账号允许绑定的企业预算作用域；成员预算不在服务账号范围内。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationServiceAccountBudgetScope {
    Organization,
    Team(OrganizationTeamId),
}

impl OrganizationServiceAccountBudgetScope {
    /// 返回持久化使用的预算作用域编码。
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::Organization => 1,
            Self::Team(_) => 2,
        }
    }

    /// 返回团队作用域的标识；企业作用域返回空值。
    #[must_use]
    pub const fn team_id(self) -> Option<OrganizationTeamId> {
        match self {
            Self::Organization => None,
            Self::Team(team_id) => Some(team_id),
        }
    }

    /// 从持久化字段恢复闭合的作用域组合。
    pub fn from_persistence(
        code: i16,
        team_id: Option<OrganizationTeamId>,
    ) -> Result<Self, OrganizationServiceAccountBudgetBindingError> {
        match (code, team_id) {
            (1, None) => Ok(Self::Organization),
            (2, Some(team_id)) => Ok(Self::Team(team_id)),
            _ => Err(OrganizationServiceAccountBudgetBindingError::InvalidScope),
        }
    }
}

/// 服务账号预算绑定领域错误；错误不携带主体或额度内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountBudgetBindingError {
    #[error("服务账号预算绑定作用域无效")]
    InvalidScope,
    #[error("服务账号预算绑定版本无效")]
    InvalidVersion,
    #[error("服务账号预算绑定主体不一致")]
    OrganizationMismatch,
}

/// 已通过企业与服务账号双重边界校验的预算绑定事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountBudgetBinding {
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    policy_id: OrganizationBudgetPolicyId,
    policy_version: i64,
    scope: OrganizationServiceAccountBudgetScope,
    version: i64,
}

impl OrganizationServiceAccountBudgetBinding {
    /// 组装预算绑定事实；策略版本和 CAS 版本必须为正数。
    pub fn new(
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
        policy_id: OrganizationBudgetPolicyId,
        policy_version: i64,
        scope: OrganizationServiceAccountBudgetScope,
        version: i64,
    ) -> Result<Self, OrganizationServiceAccountBudgetBindingError> {
        if policy_version <= 0 || version <= 0 || version == i64::MAX {
            return Err(OrganizationServiceAccountBudgetBindingError::InvalidVersion);
        }
        if scope.team_id().is_some_and(|team_id| team_id.get() <= 0) {
            return Err(OrganizationServiceAccountBudgetBindingError::InvalidScope);
        }
        Ok(Self {
            organization_id,
            service_account_id,
            policy_id,
            policy_version,
            scope,
            version,
        })
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn service_account_id(self) -> OrganizationServiceAccountId {
        self.service_account_id
    }

    #[must_use]
    pub const fn policy_id(self) -> OrganizationBudgetPolicyId {
        self.policy_id
    }

    #[must_use]
    pub const fn policy_version(self) -> i64 {
        self.policy_version
    }

    #[must_use]
    pub const fn scope(self) -> OrganizationServiceAccountBudgetScope {
        self.scope
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }
}
