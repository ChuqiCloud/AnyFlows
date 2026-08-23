use std::fmt;

use af_domain::{Quota, UserId, UserSubscriptionId};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use thiserror::Error;

/// 已由版本 CAS 独占领取的一次订阅窗口预警投递。
#[derive(Clone, Eq, PartialEq)]
pub struct SubscriptionBalanceAlertDeliveryLease {
    pub(super) event_id: i64,
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) user_id: UserId,
    pub(super) recipient: String,
    pub(super) username: String,
    pub(super) plan_name: String,
    pub(super) quota_amount: Quota,
    pub(super) quota_used: Quota,
    pub(super) window_ends_at: TimeDateTimeWithTimeZone,
    pub(super) threshold_percent: i16,
    pub(super) attempt_count: i16,
    pub(super) version: i64,
    pub(super) notification_source_key: String,
}

impl SubscriptionBalanceAlertDeliveryLease {
    #[must_use]
    pub const fn event_id(&self) -> i64 {
        self.event_id
    }

    #[must_use]
    pub const fn subscription_id(&self) -> UserSubscriptionId {
        self.subscription_id
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub fn plan_name(&self) -> &str {
        &self.plan_name
    }

    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    #[must_use]
    pub const fn quota_used(&self) -> Quota {
        self.quota_used
    }

    #[must_use]
    pub const fn window_ends_at(&self) -> TimeDateTimeWithTimeZone {
        self.window_ends_at
    }

    #[must_use]
    pub const fn threshold_percent(&self) -> i16 {
        self.threshold_percent
    }

    #[must_use]
    pub const fn attempt_count(&self) -> i16 {
        self.attempt_count
    }
}

impl fmt::Debug for SubscriptionBalanceAlertDeliveryLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionBalanceAlertDeliveryLease")
            .field("event_id", &self.event_id)
            .field("subscription_id", &self.subscription_id)
            .field("user_id", &self.user_id)
            .field("recipient", &"<已脱敏>")
            .field("username", &"<已脱敏>")
            .field("plan_name", &"<已脱敏>")
            .field("attempt_count", &self.attempt_count)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 单轮订阅窗口低余额候选发现结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionBalanceAlertEnqueueReport {
    pub(super) eligible: usize,
}

impl SubscriptionBalanceAlertEnqueueReport {
    #[must_use]
    pub const fn eligible(self) -> usize {
        self.eligible
    }
}

/// 领取下一条订阅预警投递事件的结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscriptionBalanceAlertClaimOutcome {
    Claimed(SubscriptionBalanceAlertDeliveryLease),
    Skipped,
    Empty,
}

/// 订阅预警事件仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionBalanceAlertRepositoryConfigError {
    #[error("订阅预警事件数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 订阅预警事件仓储错误；不携带邮箱或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionBalanceAlertRepositoryError {
    #[error("订阅预警事件批次大小无效")]
    InvalidBatchSize,
    #[error("订阅预警剩余额度百分比无效")]
    InvalidThreshold,
    #[error("订阅预警事件数据库操作失败")]
    Query,
    #[error("订阅预警事件数据库操作超时")]
    Timeout,
    #[error("订阅预警事件持久化状态损坏")]
    Invariant,
}
