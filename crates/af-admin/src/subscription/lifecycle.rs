use std::fmt;

use af_domain::UserSubscriptionStatus;

use super::types::{AdminUserSubscription, SubscriptionServiceError};

/// 管理员可执行的闭合订阅生命周期动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminUserSubscriptionLifecycleAction {
    /// 暂停有效订阅并保留当前窗口与已用额度。
    Suspend,
    /// 恢复暂停订阅；陈旧窗口由服务端推进。
    Resume,
    /// 取消有效或暂停订阅并停止后续续期。
    Cancel,
}

impl AdminUserSubscriptionLifecycleAction {
    pub(super) fn target_status(
        self,
        current_status: UserSubscriptionStatus,
    ) -> Result<UserSubscriptionStatus, SubscriptionServiceError> {
        match (self, current_status) {
            (Self::Suspend, UserSubscriptionStatus::Active) => {
                Ok(UserSubscriptionStatus::Suspended)
            }
            (Self::Resume, UserSubscriptionStatus::Suspended) => Ok(UserSubscriptionStatus::Active),
            (Self::Cancel, UserSubscriptionStatus::Active | UserSubscriptionStatus::Suspended) => {
                Ok(UserSubscriptionStatus::Canceled)
            }
            _ => Err(SubscriptionServiceError::InvalidTransition),
        }
    }
}

/// 管理员以当前版本提交一个闭合生命周期动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUserSubscriptionLifecycleCommand {
    pub(super) action: AdminUserSubscriptionLifecycleAction,
    pub(super) expected_version: u64,
}

impl AdminUserSubscriptionLifecycleCommand {
    /// 校验正版本后构造生命周期命令。
    pub fn new(
        action: AdminUserSubscriptionLifecycleAction,
        expected_version: i64,
    ) -> Result<Self, SubscriptionServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionServiceError::InvalidInput);
        }
        Ok(Self {
            action,
            expected_version: u64::try_from(expected_version)
                .map_err(|_| SubscriptionServiceError::InvalidInput)?,
        })
    }
}

/// 生命周期迁移后的订阅事实与恢复窗口推进结果。
pub struct AdminUserSubscriptionLifecycleResult {
    subscription: AdminUserSubscription,
    periods_elapsed: u32,
}

impl AdminUserSubscriptionLifecycleResult {
    pub(super) const fn new(subscription: AdminUserSubscription, periods_elapsed: u32) -> Self {
        Self {
            subscription,
            periods_elapsed,
        }
    }

    /// 返回迁移后的订阅事实。
    #[must_use]
    pub const fn subscription(&self) -> &AdminUserSubscription {
        &self.subscription
    }

    /// 返回恢复陈旧订阅时跨越的完整周期数。
    #[must_use]
    pub const fn periods_elapsed(&self) -> u32 {
        self.periods_elapsed
    }
}

impl fmt::Debug for AdminUserSubscriptionLifecycleResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminUserSubscriptionLifecycleResult")
            .field("periods_elapsed", &self.periods_elapsed)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_actions_only_allow_the_public_management_graph() {
        assert_eq!(
            AdminUserSubscriptionLifecycleAction::Suspend
                .target_status(UserSubscriptionStatus::Active),
            Ok(UserSubscriptionStatus::Suspended)
        );
        assert_eq!(
            AdminUserSubscriptionLifecycleAction::Resume
                .target_status(UserSubscriptionStatus::Suspended),
            Ok(UserSubscriptionStatus::Active)
        );
        for status in [
            UserSubscriptionStatus::Active,
            UserSubscriptionStatus::Suspended,
        ] {
            assert_eq!(
                AdminUserSubscriptionLifecycleAction::Cancel.target_status(status),
                Ok(UserSubscriptionStatus::Canceled)
            );
        }
        assert_eq!(
            AdminUserSubscriptionLifecycleAction::Cancel
                .target_status(UserSubscriptionStatus::Canceled),
            Err(SubscriptionServiceError::InvalidTransition)
        );
        assert_eq!(
            AdminUserSubscriptionLifecycleAction::Resume
                .target_status(UserSubscriptionStatus::Expired),
            Err(SubscriptionServiceError::InvalidTransition)
        );
    }
}
