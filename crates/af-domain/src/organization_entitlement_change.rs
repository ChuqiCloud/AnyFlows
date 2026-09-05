use thiserror::Error;

use crate::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationPlanSnapshot,
};

/// 企业授权计划使用的 Unix 秒/天换算常量。
pub const ORGANIZATION_ENTITLEMENT_DAY_SECONDS: i64 = 86_400;

/// 企业授权升级或降级计划的构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationEntitlementChangeError {
    /// 宽限期长度不能为负数。
    #[error("企业授权宽限期长度无效")]
    InvalidGracePeriod,
    /// 目标授权时间计算超出有符号 64 位范围。
    #[error("企业授权生效时间溢出")]
    Overflow,
    /// 目标套餐不能构造成合法的授权快照。
    #[error("企业授权计划快照无效")]
    InvalidSnapshot,
}

/// 降级前必须在同一事务中重新读取的企业资源占用。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementUsage {
    active_seats: i64,
    active_teams: i64,
    organization_keys: i64,
    active_sso_providers: i64,
    active_custom_roles: i64,
}

impl OrganizationEntitlementUsage {
    /// 构造资源占用快照；所有计数均不能为负数。
    pub fn new(
        active_seats: i64,
        active_teams: i64,
        organization_keys: i64,
        active_sso_providers: i64,
        active_custom_roles: i64,
    ) -> Result<Self, OrganizationEntitlementPreflightError> {
        if [
            active_seats,
            active_teams,
            organization_keys,
            active_sso_providers,
            active_custom_roles,
        ]
        .iter()
        .any(|value| *value < 0)
        {
            return Err(OrganizationEntitlementPreflightError::InvalidUsage);
        }
        Ok(Self {
            active_seats,
            active_teams,
            organization_keys,
            active_sso_providers,
            active_custom_roles,
        })
    }

    #[must_use]
    pub const fn active_seats(self) -> i64 {
        self.active_seats
    }

    #[must_use]
    pub const fn active_teams(self) -> i64 {
        self.active_teams
    }

    #[must_use]
    pub const fn organization_keys(self) -> i64 {
        self.organization_keys
    }

    #[must_use]
    pub const fn active_sso_providers(self) -> i64 {
        self.active_sso_providers
    }

    #[must_use]
    pub const fn active_custom_roles(self) -> i64 {
        self.active_custom_roles
    }
}

/// 授权计划在目标生效时间生成的版本快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementChangePlan {
    snapshot: OrganizationEntitlementSnapshot,
    effective_at: i64,
}

impl OrganizationEntitlementChangePlan {
    /// 从不可变套餐快照生成计划；生效时间不会提前写入当前授权。
    pub fn from_plan(
        plan: OrganizationPlanSnapshot,
        effective_at: i64,
        grace_seconds: i64,
    ) -> Result<Self, OrganizationEntitlementChangeError> {
        if grace_seconds < 0 {
            return Err(OrganizationEntitlementChangeError::InvalidGracePeriod);
        }
        let validity_seconds = plan
            .validity_days()
            .checked_mul(ORGANIZATION_ENTITLEMENT_DAY_SECONDS)
            .ok_or(OrganizationEntitlementChangeError::Overflow)?;
        let valid_until = effective_at
            .checked_add(validity_seconds)
            .ok_or(OrganizationEntitlementChangeError::Overflow)?;
        let grace_until = valid_until
            .checked_add(grace_seconds)
            .ok_or(OrganizationEntitlementChangeError::Overflow)?;
        let snapshot = OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(
                plan.seat_limit(),
                plan.team_limit(),
                plan.organization_key_limit(),
            )
            .map_err(|_| OrganizationEntitlementChangeError::InvalidSnapshot)?,
            plan.features().team_management_enabled(),
            plan.features().sso_enabled(),
            plan.features().custom_roles_enabled(),
            plan.audit_retention_days(),
            effective_at,
            valid_until,
            Some(grace_until),
        )
        .map_err(|_| OrganizationEntitlementChangeError::InvalidSnapshot)?;
        Ok(Self {
            snapshot,
            effective_at,
        })
    }

    /// 返回定时生效的授权快照。
    #[must_use]
    pub const fn snapshot(self) -> OrganizationEntitlementSnapshot {
        self.snapshot
    }

    /// 返回计划生效时间。
    #[must_use]
    pub const fn effective_at(self) -> i64 {
        self.effective_at
    }

    /// 在最终写入前校验所有企业资源仍符合目标套餐。
    pub fn preflight(
        self,
        usage: OrganizationEntitlementUsage,
    ) -> Result<(), OrganizationEntitlementPreflightError> {
        self.snapshot.preflight_change(usage)
    }
}

impl OrganizationEntitlementSnapshot {
    /// 在最终授权替换前校验企业资源仍符合目标快照。
    pub fn preflight_change(
        self,
        usage: OrganizationEntitlementUsage,
    ) -> Result<(), OrganizationEntitlementPreflightError> {
        let capacity = self.capacity();
        if usage.active_seats > capacity.seat_limit() {
            return Err(OrganizationEntitlementPreflightError::SeatCapacityExceeded);
        }
        if usage.active_teams > capacity.team_limit() {
            return Err(OrganizationEntitlementPreflightError::TeamCapacityExceeded);
        }
        if usage.organization_keys > capacity.organization_key_limit() {
            return Err(OrganizationEntitlementPreflightError::OrganizationKeyCapacityExceeded);
        }
        if !self.sso_enabled() && usage.active_sso_providers > 0 {
            return Err(OrganizationEntitlementPreflightError::SsoStillInUse);
        }
        if !self.custom_roles_enabled() && usage.active_custom_roles > 0 {
            return Err(OrganizationEntitlementPreflightError::CustomRolesStillInUse);
        }
        Ok(())
    }
}

/// 企业授权降级容量预检的关闭式错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationEntitlementPreflightError {
    /// 数据库占用统计出现负数，必须失败关闭。
    #[error("企业授权资源占用无效")]
    InvalidUsage,
    /// 活动成员超过目标席位。
    #[error("企业授权席位不足")]
    SeatCapacityExceeded,
    /// 活动团队超过目标容量。
    #[error("企业授权团队容量不足")]
    TeamCapacityExceeded,
    /// 未删除企业 Key 超过目标容量。
    #[error("企业授权企业 Key 容量不足")]
    OrganizationKeyCapacityExceeded,
    /// 仍有活动 SSO Provider，不能关闭 SSO 能力。
    #[error("企业授权 SSO 能力仍在使用")]
    SsoStillInUse,
    /// 仍有活动自定义角色，不能关闭自定义角色能力。
    #[error("企业授权自定义角色仍在使用")]
    CustomRolesStillInUse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OrganizationPlanFeatures, OrganizationPlanSnapshot};

    fn plan(
        team_management_enabled: bool,
        sso_enabled: bool,
        custom_roles_enabled: bool,
        seat_limit: i64,
        team_limit: i64,
        organization_key_limit: i64,
    ) -> OrganizationPlanSnapshot {
        OrganizationPlanSnapshot::new(
            OrganizationPlanFeatures::new(
                team_management_enabled,
                sso_enabled,
                custom_roles_enabled,
            ),
            seat_limit,
            team_limit,
            organization_key_limit,
            90,
            30,
        )
        .expect("套餐快照有效")
    }

    fn usage(
        active_seats: i64,
        active_teams: i64,
        organization_keys: i64,
        active_sso_providers: i64,
        active_custom_roles: i64,
    ) -> OrganizationEntitlementUsage {
        OrganizationEntitlementUsage::new(
            active_seats,
            active_teams,
            organization_keys,
            active_sso_providers,
            active_custom_roles,
        )
        .expect("占用快照有效")
    }

    #[test]
    fn 套餐计划按明确生效时间生成有效窗口和宽限期() {
        let change = OrganizationEntitlementChangePlan::from_plan(
            plan(true, true, false, 20, 5, 3),
            1_000,
            3_600,
        )
        .expect("计划有效");
        assert_eq!(change.effective_at(), 1_000);
        assert_eq!(change.snapshot().valid_from(), 1_000);
        assert_eq!(change.snapshot().valid_until(), 1_000 + 30 * 86_400);
        assert_eq!(
            change.snapshot().grace_until(),
            Some(1_000 + 30 * 86_400 + 3_600)
        );
    }

    #[test]
    fn 负宽限期和时间溢出失败关闭() {
        assert_eq!(
            OrganizationEntitlementChangePlan::from_plan(plan(true, false, false, 1, 1, 1), 1, -1),
            Err(OrganizationEntitlementChangeError::InvalidGracePeriod)
        );
        assert_eq!(
            OrganizationEntitlementChangePlan::from_plan(
                plan(true, false, false, 1, 1, 1),
                i64::MAX,
                0,
            ),
            Err(OrganizationEntitlementChangeError::Overflow)
        );
    }

    #[test]
    fn 降级预检拒绝每一种仍在使用的容量或能力() {
        let change = OrganizationEntitlementChangePlan::from_plan(
            plan(false, false, false, 2, 1, 1),
            1_000,
            0,
        )
        .expect("计划有效");
        assert_eq!(
            change.preflight(usage(3, 0, 0, 0, 0)),
            Err(OrganizationEntitlementPreflightError::SeatCapacityExceeded)
        );
        assert_eq!(
            change.preflight(usage(1, 2, 0, 0, 0)),
            Err(OrganizationEntitlementPreflightError::TeamCapacityExceeded)
        );
        assert_eq!(
            change.preflight(usage(1, 0, 2, 0, 0)),
            Err(OrganizationEntitlementPreflightError::OrganizationKeyCapacityExceeded)
        );
        assert_eq!(
            change.preflight(usage(1, 0, 0, 1, 0)),
            Err(OrganizationEntitlementPreflightError::SsoStillInUse)
        );
        assert_eq!(
            change.preflight(usage(1, 0, 0, 0, 1)),
            Err(OrganizationEntitlementPreflightError::CustomRolesStillInUse)
        );
    }

    #[test]
    fn 资源未超限时预检通过且不改变资源快照() {
        let change = OrganizationEntitlementChangePlan::from_plan(
            plan(true, true, false, 2, 1, 1),
            1_000,
            0,
        )
        .expect("计划有效");
        let current_usage = usage(2, 1, 1, 0, 0);
        assert_eq!(change.preflight(current_usage), Ok(()));
        assert_eq!(current_usage.active_seats(), 2);
        assert_eq!(current_usage.active_teams(), 1);
    }

    #[test]
    fn 占用统计拒绝负数() {
        assert_eq!(
            OrganizationEntitlementUsage::new(-1, 0, 0, 0, 0),
            Err(OrganizationEntitlementPreflightError::InvalidUsage)
        );
    }
}
