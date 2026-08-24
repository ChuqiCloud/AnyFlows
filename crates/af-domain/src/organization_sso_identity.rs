use thiserror::Error;

use crate::{
    OrganizationEntitlementStatus, OrganizationMembershipStatus, OrganizationSsoDomainStatus,
    OrganizationSsoProviderStatus, OrganizationStatus,
};

/// 企业 SSO 身份绑定来源；不允许用邮箱或域名相似度推导绑定。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoIdentityBindingMethod {
    /// 已登录成员完成二次验证后的显式绑定。
    Explicit = 1,
    /// 受控企业邀请事实约束下的绑定。
    Invited = 2,
}

/// 企业 SSO 身份绑定持久化状态解析错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoIdentityStateCodeError {
    /// 持久化值不属于服务端闭合集合。
    #[error("企业 SSO 身份绑定来源无效")]
    InvalidBindingMethod,
    /// 持久化值不属于服务端闭合集合。
    #[error("企业 SSO 身份状态无效")]
    InvalidStatus,
}

impl OrganizationSsoIdentityBindingMethod {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for OrganizationSsoIdentityBindingMethod {
    type Error = OrganizationSsoIdentityStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Explicit),
            2 => Ok(Self::Invited),
            _ => Err(Self::Error::InvalidBindingMethod),
        }
    }
}

/// 企业 SSO 身份绑定的生命周期状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i16)]
pub enum OrganizationSsoIdentityStatus {
    /// 身份可参与企业 SSO 登录。
    Active = 1,
    /// 成员停用或移除后保留的历史身份，不可登录。
    Disabled = 2,
}

impl OrganizationSsoIdentityStatus {
    /// 返回数据库使用的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 判断身份是否可参与企业 SSO 登录。
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl TryFrom<i16> for OrganizationSsoIdentityStatus {
    type Error = OrganizationSsoIdentityStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Active),
            2 => Ok(Self::Disabled),
            _ => Err(Self::Error::InvalidStatus),
        }
    }
}

/// 已验证服务端事实的企业 SSO 登录资格输入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoIdentityLoginFacts {
    organization_status: OrganizationStatus,
    commercial_entitlement_status: OrganizationEntitlementStatus,
    sso_enabled: bool,
    provider_status: OrganizationSsoProviderStatus,
    domain_status: OrganizationSsoDomainStatus,
    identity_status: OrganizationSsoIdentityStatus,
    membership_status: OrganizationMembershipStatus,
}

impl OrganizationSsoIdentityLoginFacts {
    /// 组装登录资格事实；调用方必须传入服务端已核验的状态，不接受客户端声明。
    #[must_use]
    pub const fn new(
        organization_status: OrganizationStatus,
        commercial_entitlement_status: OrganizationEntitlementStatus,
        sso_enabled: bool,
        provider_status: OrganizationSsoProviderStatus,
        domain_status: OrganizationSsoDomainStatus,
        identity_status: OrganizationSsoIdentityStatus,
        membership_status: OrganizationMembershipStatus,
    ) -> Self {
        Self {
            organization_status,
            commercial_entitlement_status,
            sso_enabled,
            provider_status,
            domain_status,
            identity_status,
            membership_status,
        }
    }
}

/// 企业 SSO 登录资格拒绝原因；对外层应统一映射为不泄露租户状态的错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoIdentityLoginRejection {
    /// 企业已停用或生命周期状态不可用。
    #[error("企业 SSO 登录资格无效")]
    OrganizationInactive,
    /// 商业授权不可用。
    #[error("企业 SSO 登录资格无效")]
    CommercialEntitlementInactive,
    /// 企业未启用 SSO。
    #[error("企业 SSO 登录资格无效")]
    SsoDisabled,
    /// Provider 不在可用状态。
    #[error("企业 SSO 登录资格无效")]
    ProviderInactive,
    /// 域名所有权事实不可用。
    #[error("企业 SSO 登录资格无效")]
    DomainUnverified,
    /// 身份绑定已停用。
    #[error("企业 SSO 登录资格无效")]
    IdentityDisabled,
    /// 成员不再处于活动状态。
    #[error("企业 SSO 登录资格无效")]
    MembershipInactive,
}

/// 依据企业、授权、Provider、域名、身份和成员事实判断 SSO 登录资格。
pub fn authorize_organization_sso_identity_login(
    facts: OrganizationSsoIdentityLoginFacts,
) -> Result<(), OrganizationSsoIdentityLoginRejection> {
    if !facts.organization_status.is_active() {
        return Err(OrganizationSsoIdentityLoginRejection::OrganizationInactive);
    }
    if !facts.commercial_entitlement_status.allows_provisioning() {
        return Err(OrganizationSsoIdentityLoginRejection::CommercialEntitlementInactive);
    }
    if !facts.sso_enabled {
        return Err(OrganizationSsoIdentityLoginRejection::SsoDisabled);
    }
    if facts.provider_status != OrganizationSsoProviderStatus::Active {
        return Err(OrganizationSsoIdentityLoginRejection::ProviderInactive);
    }
    if facts.domain_status != OrganizationSsoDomainStatus::Verified {
        return Err(OrganizationSsoIdentityLoginRejection::DomainUnverified);
    }
    if !facts.identity_status.is_active() {
        return Err(OrganizationSsoIdentityLoginRejection::IdentityDisabled);
    }
    if facts.membership_status != OrganizationMembershipStatus::Active {
        return Err(OrganizationSsoIdentityLoginRejection::MembershipInactive);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> OrganizationSsoIdentityLoginFacts {
        OrganizationSsoIdentityLoginFacts::new(
            OrganizationStatus::Active,
            OrganizationEntitlementStatus::Active,
            true,
            OrganizationSsoProviderStatus::Active,
            OrganizationSsoDomainStatus::Verified,
            OrganizationSsoIdentityStatus::Active,
            OrganizationMembershipStatus::Active,
        )
    }

    #[test]
    fn identity_persistence_states_are_closed_and_stable() {
        assert_eq!(OrganizationSsoIdentityBindingMethod::Explicit.code(), 1);
        assert_eq!(OrganizationSsoIdentityBindingMethod::Invited.code(), 2);
        assert_eq!(
            OrganizationSsoIdentityBindingMethod::try_from(1),
            Ok(OrganizationSsoIdentityBindingMethod::Explicit)
        );
        assert_eq!(
            OrganizationSsoIdentityBindingMethod::try_from(3),
            Err(OrganizationSsoIdentityStateCodeError::InvalidBindingMethod)
        );
        assert_eq!(OrganizationSsoIdentityStatus::Active.code(), 1);
        assert_eq!(OrganizationSsoIdentityStatus::Disabled.code(), 2);
        assert!(OrganizationSsoIdentityStatus::Active.is_active());
        assert!(!OrganizationSsoIdentityStatus::Disabled.is_active());
        assert_eq!(
            OrganizationSsoIdentityStatus::try_from(2),
            Ok(OrganizationSsoIdentityStatus::Disabled)
        );
        assert_eq!(
            OrganizationSsoIdentityStatus::try_from(3),
            Err(OrganizationSsoIdentityStateCodeError::InvalidStatus)
        );
    }

    #[test]
    fn login_authorization_is_fail_closed_for_each_fact() {
        let cases = [
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Suspended,
                    OrganizationEntitlementStatus::Active,
                    true,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::OrganizationInactive,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Grace,
                    true,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::CommercialEntitlementInactive,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Active,
                    false,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::SsoDisabled,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Active,
                    true,
                    OrganizationSsoProviderStatus::Draft,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::ProviderInactive,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Active,
                    true,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Pending,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::DomainUnverified,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Active,
                    true,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Disabled,
                    OrganizationMembershipStatus::Active,
                ),
                OrganizationSsoIdentityLoginRejection::IdentityDisabled,
            ),
            (
                OrganizationSsoIdentityLoginFacts::new(
                    OrganizationStatus::Active,
                    OrganizationEntitlementStatus::Active,
                    true,
                    OrganizationSsoProviderStatus::Active,
                    OrganizationSsoDomainStatus::Verified,
                    OrganizationSsoIdentityStatus::Active,
                    OrganizationMembershipStatus::Suspended,
                ),
                OrganizationSsoIdentityLoginRejection::MembershipInactive,
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                authorize_organization_sso_identity_login(input),
                Err(expected)
            );
        }
        assert_eq!(authorize_organization_sso_identity_login(facts()), Ok(()));
    }
}
