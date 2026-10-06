use std::fmt;

use thiserror::Error;

/// 网关主体或渠道标识构造错误；不保留外部提供的原始数值。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PrincipalIdError {
    /// 数据库实体标识必须是正整数。
    #[error("领域实体标识必须大于零")]
    NonPositive,
}

macro_rules! principal_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);

        impl $name {
            /// 校验并构造正整数标识。
            pub const fn new(value: i64) -> Result<Self, PrincipalIdError> {
                if value > 0 {
                    Ok(Self(value))
                } else {
                    Err(PrincipalIdError::NonPositive)
                }
            }

            /// 返回持久化边界使用的正整数值。
            #[must_use]
            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = PrincipalIdError;

            fn try_from(value: i64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

principal_id! {
    /// 已通过鉴权的下游令牌标识。
    TokenId
}

principal_id! {
    /// 已通过鉴权的用户标识。
    UserId
}

principal_id! {
    /// 本次请求使用的有效分组标识。
    GroupId
}

principal_id! {
    /// 本次请求实际使用的上游渠道标识。
    ChannelId
}

principal_id! {
    /// 上游渠道绑定的凭据标识。
    CredentialId
}

principal_id! {
    /// 凭据绑定的专属出口代理标识。
    ProxyId
}

principal_id! {
    /// 独立模型商品元数据记录标识。
    ModelId
}

principal_id! {
    /// 管理端智能路由规则标识。
    RouteId
}

principal_id! {
    /// 智能路由中的单个渠道凭据候选标识。
    RouteChannelId
}

principal_id! {
    /// 请求计费快照使用的合同价记录标识。
    BillingContractPriceId
}

principal_id! {
    /// 已通过组织范围校验的企业空间内部标识。
    OrganizationId
}

principal_id! {
    /// 已通过组织范围校验的企业成员关系标识。
    OrganizationMembershipId
}

principal_id! {
    /// 已通过组织范围校验的企业团队标识。
    OrganizationTeamId
}

principal_id! {
    /// 已通过组织范围校验的企业部门标识。
    OrganizationDepartmentId
}

principal_id! {
    /// 已通过组织范围校验的企业自定义角色内部标识。
    OrganizationCustomRoleId
}

principal_id! {
    /// 企业预算策略的持久化标识。
    OrganizationBudgetPolicyId
}

principal_id! {
    /// 企业预算周期窗口的持久化标识。
    OrganizationBudgetWindowId
}

principal_id! {
    /// 企业合同价版本事实的持久化标识。
    OrganizationContractPriceId
}

principal_id! {
    /// 企业授信与账期策略事实的持久化标识。
    OrganizationCreditTermId
}

principal_id! {
    /// 企业账期账单事实的持久化标识。
    OrganizationCreditInvoiceId
}

principal_id! {
    /// 企业已确认还款事实的持久化标识。
    OrganizationCreditRepaymentId
}

principal_id! {
    /// 企业还款核销分配事实的持久化标识。
    OrganizationCreditRepaymentAllocationId
}

principal_id! {
    /// 企业审批模板版本事实的持久化标识。
    OrganizationApprovalTemplateId
}

principal_id! {
    /// 企业审批申请事实的持久化标识。
    OrganizationApprovalRequestId
}

principal_id! {
    /// 企业审批决定事实的持久化标识。
    OrganizationApprovalDecisionId
}

principal_id! {
    /// 企业开通申请事实的持久化标识。
    OrganizationProvisioningRequestId
}

principal_id! {
    /// 企业认证案件的持久化标识。
    OrganizationVerificationCaseId
}

principal_id! {
    /// 企业认证材料的持久化标识。
    OrganizationVerificationMaterialId
}

principal_id! {
    /// 企业 SCIM 令牌的持久化标识。
    OrganizationScimTokenId
}

principal_id! {
    /// 企业服务账号的持久化标识，与自然人用户标识保持分离。
    OrganizationServiceAccountId
}

/// 已完成令牌、用户与有效分组校验的网关请求主体。
///
/// 该值只携带稳定标识，不包含 API Key、摘要、展示前缀或可变授权策略，且不实现
/// `Display` 或序列化，避免被直接写入日志和协议响应。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct GatewayPrincipal {
    token_id: TokenId,
    user_id: UserId,
    group_id: GroupId,
    organization: Option<OrganizationGatewayPrincipal>,
    playground: bool,
}

/// 企业 API Key 在鉴权后固化的组织主体。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OrganizationGatewayPrincipal {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    team_id: Option<OrganizationTeamId>,
    department_id: Option<OrganizationDepartmentId>,
}

impl OrganizationGatewayPrincipal {
    /// 组合已通过同企业归属校验的组织、成员和可选团队。
    #[must_use]
    pub const fn new(
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        team_id: Option<OrganizationTeamId>,
    ) -> Self {
        Self {
            organization_id,
            membership_id,
            team_id,
            department_id: None,
        }
    }

    /// 组合已通过成员归属和部门状态校验的企业请求主体。
    #[must_use]
    pub const fn with_department(
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        team_id: Option<OrganizationTeamId>,
        department_id: OrganizationDepartmentId,
    ) -> Self {
        Self {
            organization_id,
            membership_id,
            team_id,
            department_id: Some(department_id),
        }
    }

    /// 返回企业内部标识。
    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    /// 返回签发成员关系标识。
    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.membership_id
    }

    /// 返回签发时固化的主团队。
    #[must_use]
    pub const fn team_id(self) -> Option<OrganizationTeamId> {
        self.team_id
    }

    /// 返回企业 Key 创建/认证时固化的部门归属。
    #[must_use]
    pub const fn department_id(self) -> Option<OrganizationDepartmentId> {
        self.department_id
    }
}

impl fmt::Debug for OrganizationGatewayPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationGatewayPrincipal(<redacted>)")
    }
}

impl GatewayPrincipal {
    /// 使用已经分别校验的强类型标识构造请求主体。
    #[must_use]
    pub const fn new(token_id: TokenId, user_id: UserId, group_id: GroupId) -> Self {
        Self {
            token_id,
            user_id,
            group_id,
            organization: None,
            playground: false,
        }
    }

    /// 构造仅供登录试炼场使用的网关主体；仍然绑定真实 Token 以复用额度与审计链路。
    #[must_use]
    pub const fn playground(token_id: TokenId, user_id: UserId, group_id: GroupId) -> Self {
        Self {
            token_id,
            user_id,
            group_id,
            organization: None,
            playground: true,
        }
    }

    /// 构造已经完成成员、授权和团队复验的企业 API Key 主体。
    #[must_use]
    pub const fn organization(
        token_id: TokenId,
        user_id: UserId,
        group_id: GroupId,
        organization: OrganizationGatewayPrincipal,
    ) -> Self {
        Self {
            token_id,
            user_id,
            group_id,
            organization: Some(organization),
            playground: false,
        }
    }

    /// 返回通过鉴权的令牌标识。
    #[must_use]
    pub const fn token_id(self) -> TokenId {
        self.token_id
    }

    /// 返回令牌所属用户标识。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    /// 返回令牌覆盖或用户默认的有效分组标识。
    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }

    /// 返回企业 API Key 的已验证组织主体；个人 Key 返回空。
    #[must_use]
    pub const fn organization_principal(self) -> Option<OrganizationGatewayPrincipal> {
        self.organization
    }

    /// 返回是否允许试炼场专用的分组降级策略。
    #[must_use]
    pub const fn is_playground(self) -> bool {
        self.playground
    }
}

impl fmt::Debug for GatewayPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GatewayPrincipal(<redacted>)")
    }
}
