use thiserror::Error;

use crate::organization_entitlement::OrganizationEntitlementSnapshot;

/// 企业授权续费期限计算失败时的关闭式错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationEntitlementRenewalError {
    /// 续费时长必须为正数。
    #[error("企业授权续费时长无效")]
    InvalidExtension,
    /// Unix 秒加法超出有符号 64 位范围。
    #[error("企业授权续费时间溢出")]
    Overflow,
    /// 计算结果无法重新构造成合法授权快照。
    #[error("企业授权续费结果无效")]
    InvalidSnapshot,
}

impl OrganizationEntitlementSnapshot {
    /// 计算续费后的授权快照，不改变能力和容量，只延展有效期。
    ///
    /// 活动或尚未生效的授权保留原 `valid_from`，从原 `valid_until` 继续延长；
    /// 已过期授权从当前时间恢复，避免生成过去的生效窗口。宽限时长保持不变。
    pub fn renewed(
        self,
        now: i64,
        extension_seconds: i64,
    ) -> Result<Self, OrganizationEntitlementRenewalError> {
        if extension_seconds <= 0 {
            return Err(OrganizationEntitlementRenewalError::InvalidExtension);
        }

        let valid_from = if now >= self.valid_until() {
            now
        } else {
            self.valid_from()
        };
        let renewal_anchor = self.valid_until().max(now);
        let valid_until = renewal_anchor
            .checked_add(extension_seconds)
            .ok_or(OrganizationEntitlementRenewalError::Overflow)?;
        let grace_until = self
            .grace_until()
            .map(|grace_until| {
                let grace_duration = grace_until
                    .checked_sub(self.valid_until())
                    .ok_or(OrganizationEntitlementRenewalError::InvalidSnapshot)?;
                valid_until
                    .checked_add(grace_duration)
                    .ok_or(OrganizationEntitlementRenewalError::Overflow)
            })
            .transpose()?;

        Self::new(
            self.capacity(),
            self.team_management_enabled(),
            self.sso_enabled(),
            self.custom_roles_enabled(),
            self.audit_retention_days(),
            valid_from,
            valid_until,
            grace_until,
        )
        .map_err(|_| OrganizationEntitlementRenewalError::InvalidSnapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OrganizationEntitlementCapacity;

    fn snapshot(
        valid_from: i64,
        valid_until: i64,
        grace_until: Option<i64>,
    ) -> OrganizationEntitlementSnapshot {
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(20, 5, 3).expect("容量有效"),
            true,
            true,
            false,
            180,
            valid_from,
            valid_until,
            grace_until,
        )
        .expect("授权窗口有效")
    }

    #[test]
    fn 活动授权续费从原结束时间延展并保留宽限时长() {
        let current = snapshot(100, 200, Some(230));
        let renewed = current.renewed(150, 86_400).expect("续费计算成功");

        assert_eq!(renewed.valid_from(), 100);
        assert_eq!(renewed.valid_until(), 86_600);
        assert_eq!(renewed.grace_until(), Some(86_630));
        assert_eq!(renewed.capacity(), current.capacity());
        assert!(renewed.team_management_enabled());
        assert!(renewed.sso_enabled());
    }

    #[test]
    fn 已过期授权续费从当前时间恢复() {
        let current = snapshot(100, 200, None);
        let renewed = current.renewed(250, 100).expect("续费计算成功");

        assert_eq!(renewed.valid_from(), 250);
        assert_eq!(renewed.valid_until(), 350);
        assert_eq!(renewed.grace_until(), None);
    }

    #[test]
    fn 到期瞬间续费也从当前时间恢复() {
        let current = snapshot(100, 200, None);
        let renewed = current.renewed(200, 100).expect("续费计算成功");

        assert_eq!(renewed.valid_from(), 200);
        assert_eq!(renewed.valid_until(), 300);
    }

    #[test]
    fn 非正续费时长失败且不接受领域错误替代() {
        let current = snapshot(100, 200, None);
        assert_eq!(
            current.renewed(150, 0),
            Err(OrganizationEntitlementRenewalError::InvalidExtension)
        );
        assert_eq!(
            current.renewed(150, -1),
            Err(OrganizationEntitlementRenewalError::InvalidExtension)
        );
    }

    #[test]
    fn 续费时间溢出失败关闭() {
        let current = snapshot(i64::MAX - 2, i64::MAX - 1, Some(i64::MAX - 1));
        assert_eq!(
            current.renewed(i64::MAX - 2, 2),
            Err(OrganizationEntitlementRenewalError::Overflow)
        );
    }
}
