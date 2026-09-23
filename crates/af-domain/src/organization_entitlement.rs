use thiserror::Error;

use crate::organization::OrganizationStatus;

/// 审计保留期最大天数。
pub const MAX_ORGANIZATION_AUDIT_RETENTION_DAYS: i64 = 3_650;

/// 企业商业授权的服务端推导状态。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrganizationEntitlementStatus {
    /// 授权在有效期内，允许已授权的企业管理写操作。
    Active,
    /// 正常授权已过期但仍处于宽限期，保留既有调用并冻结新增资源。
    Grace,
    /// 宽限期结束，仅保留控制台、审计、账本和导出读取。
    ReadOnly,
    /// 企业生命周期被平台暂停。
    Suspended,
    /// 企业已关闭，只允许按保留策略读取审计数据。
    Closed,
}

impl OrganizationEntitlementStatus {
    /// 返回稳定的接口状态码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Grace => "grace",
            Self::ReadOnly => "read_only",
            Self::Suspended => "suspended",
            Self::Closed => "closed",
        }
    }

    /// 判断当前状态是否允许新增成员、团队或企业 Key。
    #[must_use]
    pub const fn allows_provisioning(self) -> bool {
        matches!(self, Self::Active)
    }

    /// 判断当前状态是否允许既有企业 Key 发起消费请求。
    #[must_use]
    pub const fn allows_existing_consumption(self) -> bool {
        matches!(self, Self::Active | Self::Grace)
    }
}

/// 授权状态推导失败时返回的关闭式错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationEntitlementStateError {
    /// 授权时间边界不满足严格顺序。
    #[error("企业授权时间边界无效")]
    InvalidTimeRange,
    /// 容量字段不是正整数。
    #[error("企业授权容量必须为正整数")]
    InvalidCapacity,
    /// 审计保留期不在服务端允许范围内。
    #[error("企业授权审计保留期无效")]
    InvalidAuditRetention,
    /// 首版尚未实现的商业能力不能提前开启。
    #[error("企业授权包含尚未实现的能力")]
    UnsupportedFeature,
}

/// 企业授权容量快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementCapacity {
    seat_limit: i64,
    team_limit: i64,
    organization_key_limit: i64,
}

impl OrganizationEntitlementCapacity {
    /// 构造并校验企业授权容量。
    pub fn new(
        seat_limit: i64,
        team_limit: i64,
        organization_key_limit: i64,
    ) -> Result<Self, OrganizationEntitlementStateError> {
        if seat_limit <= 0 || team_limit <= 0 || organization_key_limit <= 0 {
            return Err(OrganizationEntitlementStateError::InvalidCapacity);
        }
        Ok(Self {
            seat_limit,
            team_limit,
            organization_key_limit,
        })
    }

    /// 返回席位上限。
    #[must_use]
    pub const fn seat_limit(self) -> i64 {
        self.seat_limit
    }

    /// 返回团队上限。
    #[must_use]
    pub const fn team_limit(self) -> i64 {
        self.team_limit
    }

    /// 返回企业 Key 上限。
    #[must_use]
    pub const fn organization_key_limit(self) -> i64 {
        self.organization_key_limit
    }
}

/// 授权记录的只读领域快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementSnapshot {
    capacity: OrganizationEntitlementCapacity,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    audit_retention_days: i64,
    valid_from: i64,
    valid_until: i64,
    grace_until: Option<i64>,
}

impl OrganizationEntitlementSnapshot {
    /// 构造并校验授权的能力、容量和时间边界。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capacity: OrganizationEntitlementCapacity,
        team_management_enabled: bool,
        sso_enabled: bool,
        custom_roles_enabled: bool,
        audit_retention_days: i64,
        valid_from: i64,
        valid_until: i64,
        grace_until: Option<i64>,
    ) -> Result<Self, OrganizationEntitlementStateError> {
        if !(1..=MAX_ORGANIZATION_AUDIT_RETENTION_DAYS).contains(&audit_retention_days) {
            return Err(OrganizationEntitlementStateError::InvalidAuditRetention);
        }
        if valid_from >= valid_until || grace_until.is_some_and(|value| value < valid_until) {
            return Err(OrganizationEntitlementStateError::InvalidTimeRange);
        }
        Ok(Self {
            capacity,
            team_management_enabled,
            sso_enabled,
            custom_roles_enabled,
            audit_retention_days,
            valid_from,
            valid_until,
            grace_until,
        })
    }

    /// 根据企业生命周期和 UTC Unix 秒推导当前授权状态。
    #[must_use]
    pub fn status(
        self,
        organization_status: OrganizationStatus,
        now: i64,
    ) -> OrganizationEntitlementStatus {
        match organization_status {
            OrganizationStatus::Closed => OrganizationEntitlementStatus::Closed,
            OrganizationStatus::Suspended => OrganizationEntitlementStatus::Suspended,
            OrganizationStatus::Active if now >= self.valid_from && now < self.valid_until => {
                OrganizationEntitlementStatus::Active
            }
            OrganizationStatus::Active
                if now >= self.valid_until && self.grace_until.is_some_and(|until| now < until) =>
            {
                OrganizationEntitlementStatus::Grace
            }
            OrganizationStatus::Active => OrganizationEntitlementStatus::ReadOnly,
        }
    }

    /// 返回授权容量。
    #[must_use]
    pub const fn capacity(self) -> OrganizationEntitlementCapacity {
        self.capacity
    }

    /// 是否开启团队管理能力。
    #[must_use]
    pub const fn team_management_enabled(self) -> bool {
        self.team_management_enabled
    }

    /// 是否开启单点登录能力。
    #[must_use]
    pub const fn sso_enabled(self) -> bool {
        self.sso_enabled
    }

    /// 是否开启自定义角色能力。
    #[must_use]
    pub const fn custom_roles_enabled(self) -> bool {
        self.custom_roles_enabled
    }

    /// 返回审计保留天数。
    #[must_use]
    pub const fn audit_retention_days(self) -> i64 {
        self.audit_retention_days
    }

    /// 返回授权开始时间。
    #[must_use]
    pub const fn valid_from(self) -> i64 {
        self.valid_from
    }

    /// 返回正常授权结束时间。
    #[must_use]
    pub const fn valid_until(self) -> i64 {
        self.valid_until
    }

    /// 返回宽限期结束时间。
    #[must_use]
    pub const fn grace_until(self) -> Option<i64> {
        self.grace_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(grace_until: Option<i64>) -> OrganizationEntitlementSnapshot {
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 4, 2).expect("容量有效"),
            true,
            false,
            false,
            90,
            100,
            200,
            grace_until,
        )
        .expect("授权有效")
    }

    #[test]
    fn 状态按企业生命周期优先推导() {
        let entitlement = snapshot(Some(250));
        assert_eq!(
            entitlement.status(OrganizationStatus::Active, 150),
            OrganizationEntitlementStatus::Active
        );
        assert_eq!(
            entitlement.status(OrganizationStatus::Active, 220),
            OrganizationEntitlementStatus::Grace
        );
        assert_eq!(
            entitlement.status(OrganizationStatus::Active, 300),
            OrganizationEntitlementStatus::ReadOnly
        );
        assert_eq!(
            entitlement.status(OrganizationStatus::Active, 50),
            OrganizationEntitlementStatus::ReadOnly
        );
        assert_eq!(
            entitlement.status(OrganizationStatus::Suspended, 150),
            OrganizationEntitlementStatus::Suspended
        );
        assert_eq!(
            entitlement.status(OrganizationStatus::Closed, 150),
            OrganizationEntitlementStatus::Closed
        );
    }

    #[test]
    fn 无效时间边界失败关闭() {
        assert_eq!(
            OrganizationEntitlementSnapshot::new(
                OrganizationEntitlementCapacity::new(1, 1, 1).expect("容量有效"),
                false,
                false,
                false,
                1,
                2,
                2,
                None,
            ),
            Err(OrganizationEntitlementStateError::InvalidTimeRange)
        );
    }

    #[test]
    fn 自定义角色能力可由授权启停() {
        for enabled in [false, true] {
            let snapshot = OrganizationEntitlementSnapshot::new(
                OrganizationEntitlementCapacity::new(1, 1, 1).expect("容量有效"),
                true, true, enabled, 90, 1, 2, None,
            ).expect("已实现的自定义角色允许授权");
            assert_eq!(snapshot.custom_roles_enabled(), enabled);
        }
    }
}
