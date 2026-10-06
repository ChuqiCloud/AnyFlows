use std::fmt;

use af_domain::{AsyncTaskId, BillingReservationId, GroupId, Quota, UserId};

use crate::async_task::AsyncTaskInputError;

/// 持久化任务计费使用的有效分辨率；它与可空审计分辨率相互独立。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncTaskBillingResolution {
    P480,
    P720,
    P1080,
}

impl AsyncTaskBillingResolution {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::P480 => 1,
            Self::P720 => 2,
            Self::P1080 => 3,
        }
    }

    pub(super) const fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::P480),
            2 => Some(Self::P720),
            3 => Some(Self::P1080),
            _ => None,
        }
    }
}

/// 异步任务计费的闭合持久化状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncTaskBillingState {
    Planned,
    Reserved,
    Submitted,
    SettlementPending,
    Settled,
    ReleasePending,
    Released,
}

impl AsyncTaskBillingState {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::Planned => 1,
            Self::Reserved => 2,
            Self::Submitted => 3,
            Self::SettlementPending => 4,
            Self::Settled => 5,
            Self::ReleasePending => 6,
            Self::Released => 7,
        }
    }

    pub(super) const fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::Planned),
            2 => Some(Self::Reserved),
            3 => Some(Self::Submitted),
            4 => Some(Self::SettlementPending),
            5 => Some(Self::Settled),
            6 => Some(Self::ReleasePending),
            7 => Some(Self::Released),
            _ => None,
        }
    }
}

/// 上游发网前必须先持久化的冻结计划。
pub struct AsyncTaskBillingPlan {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) reservation_id: BillingReservationId,
    pub(super) target_group_id: GroupId,
    pub(super) price_card_version: i16,
    pub(super) resolution: AsyncTaskBillingResolution,
    pub(super) ratios: [i64; 3],
    pub(super) upper_bound: Quota,
    pub(super) observed_at: u64,
}

impl AsyncTaskBillingPlan {
    /// 校验版本、倍率、严格正数上界和审计时间后构造计划。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与不可变任务计费快照一一对应"
    )]
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        reservation_id: BillingReservationId,
        target_group_id: GroupId,
        price_card_version: u16,
        resolution: AsyncTaskBillingResolution,
        ratios: [i64; 3],
        upper_bound: Quota,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        if price_card_version == 0
            || ratios.into_iter().any(|value| value < 0)
            || upper_bound.is_zero()
        {
            return Err(AsyncTaskInputError::InvalidBilling);
        }
        Ok(Self {
            task_id,
            user_id,
            reservation_id,
            target_group_id,
            price_card_version: i16::try_from(price_card_version)
                .map_err(|_| AsyncTaskInputError::InvalidBilling)?,
            resolution,
            ratios,
            upper_bound,
            observed_at: validate_time(observed_at)?,
        })
    }
}

impl fmt::Debug for AsyncTaskBillingPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskBillingPlan(<脱敏>)")
    }
}

/// 不增加业务字段的任务计费状态 CAS 命令。
pub struct AsyncTaskBillingMark {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) expected_version: i64,
    pub(super) observed_at: u64,
}

impl AsyncTaskBillingMark {
    /// 构造带所有者、版本和观察时间的状态命令。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        Ok(Self {
            task_id,
            user_id,
            expected_version: validate_version(expected_version)?,
            observed_at: validate_time(observed_at)?,
        })
    }
}

impl fmt::Debug for AsyncTaskBillingMark {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskBillingMark(<脱敏>)")
    }
}

/// 上游接受后固化官方单价和提交 fallback 的 CAS 命令。
pub struct AsyncTaskBillingAccept {
    pub(super) mark: AsyncTaskBillingMark,
    pub(super) rate_microusd: i64,
    pub(super) fallback_quota: Quota,
}

impl AsyncTaskBillingAccept {
    /// 构造上游已接受的不可变计费事实。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        rate_microusd: i64,
        fallback_quota: Quota,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        if rate_microusd <= 0 {
            return Err(AsyncTaskInputError::InvalidBilling);
        }
        Ok(Self {
            mark: AsyncTaskBillingMark::new(task_id, user_id, expected_version, observed_at)?,
            rate_microusd,
            fallback_quota,
        })
    }
}

impl fmt::Debug for AsyncTaskBillingAccept {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskBillingAccept(<脱敏>)")
    }
}

/// 成功终态在持久化结算前固化的实际额度与真实时长。
pub struct AsyncTaskBillingSettlement {
    pub(super) mark: AsyncTaskBillingMark,
    pub(super) actual_quota: Quota,
    pub(super) actual_duration_seconds: i16,
}

impl AsyncTaskBillingSettlement {
    /// 构造只能原参数重放的结算事实。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        actual_quota: Quota,
        actual_duration_seconds: u8,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        if !(1..=15).contains(&actual_duration_seconds) {
            return Err(AsyncTaskInputError::InvalidBilling);
        }
        Ok(Self {
            mark: AsyncTaskBillingMark::new(task_id, user_id, expected_version, observed_at)?,
            actual_quota,
            actual_duration_seconds: i16::from(actual_duration_seconds),
        })
    }
}

impl fmt::Debug for AsyncTaskBillingSettlement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskBillingSettlement(<脱敏>)")
    }
}

/// 只删除已释放且尚未产生上游任务的计费计划。
pub struct AsyncTaskBillingClear {
    pub(super) mark: AsyncTaskBillingMark,
    pub(super) reservation_id: BillingReservationId,
}

impl AsyncTaskBillingClear {
    /// 构造已释放计划的受控清理命令。
    pub fn new(
        task_id: AsyncTaskId,
        user_id: UserId,
        expected_version: u64,
        reservation_id: BillingReservationId,
        observed_at: u64,
    ) -> Result<Self, AsyncTaskInputError> {
        Ok(Self {
            mark: AsyncTaskBillingMark::new(task_id, user_id, expected_version, observed_at)?,
            reservation_id,
        })
    }
}

impl fmt::Debug for AsyncTaskBillingClear {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AsyncTaskBillingClear(<脱敏>)")
    }
}

/// 所有者范围内的任务计费持久化快照。
#[derive(Clone, Eq, PartialEq)]
pub struct AsyncTaskBillingRecord {
    pub(super) task_id: AsyncTaskId,
    pub(super) user_id: UserId,
    pub(super) reservation_id: BillingReservationId,
    pub(super) target_group_id: GroupId,
    pub(super) state: AsyncTaskBillingState,
    pub(super) price_card_version: u16,
    pub(super) resolution: AsyncTaskBillingResolution,
    pub(super) ratios: [i64; 3],
    pub(super) upper_bound: Quota,
    pub(super) rate_microusd: Option<i64>,
    pub(super) fallback_quota: Option<Quota>,
    pub(super) actual_quota: Option<Quota>,
    pub(super) actual_duration_seconds: Option<u8>,
    pub(super) version: u64,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl AsyncTaskBillingRecord {
    #[must_use]
    pub const fn task_id(&self) -> AsyncTaskId {
        self.task_id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn reservation_id(&self) -> BillingReservationId {
        self.reservation_id
    }
    #[must_use]
    pub const fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }
    #[must_use]
    pub const fn state(&self) -> AsyncTaskBillingState {
        self.state
    }
    #[must_use]
    pub const fn price_card_version(&self) -> u16 {
        self.price_card_version
    }
    #[must_use]
    pub const fn resolution(&self) -> AsyncTaskBillingResolution {
        self.resolution
    }
    #[must_use]
    pub const fn ratios(&self) -> [i64; 3] {
        self.ratios
    }
    #[must_use]
    pub const fn upper_bound(&self) -> Quota {
        self.upper_bound
    }
    #[must_use]
    pub const fn rate_microusd(&self) -> Option<i64> {
        self.rate_microusd
    }
    #[must_use]
    pub const fn fallback_quota(&self) -> Option<Quota> {
        self.fallback_quota
    }
    #[must_use]
    pub const fn actual_quota(&self) -> Option<Quota> {
        self.actual_quota
    }
    #[must_use]
    pub const fn actual_duration_seconds(&self) -> Option<u8> {
        self.actual_duration_seconds
    }
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_plan(&self, write: &AsyncTaskBillingPlan) -> bool {
        self.task_id == write.task_id
            && self.user_id == write.user_id
            && self.reservation_id == write.reservation_id
            && self.target_group_id == write.target_group_id
            && self.price_card_version == u16::try_from(write.price_card_version).unwrap_or(0)
            && self.resolution == write.resolution
            && self.ratios == write.ratios
            && self.upper_bound == write.upper_bound
    }
}

impl fmt::Debug for AsyncTaskBillingRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncTaskBillingRecord")
            .field("state", &self.state)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

pub enum AsyncTaskBillingPlanOutcome {
    Created(AsyncTaskBillingRecord),
    Existing(AsyncTaskBillingRecord),
    NotFound,
}

pub enum AsyncTaskBillingMutationOutcome {
    Applied(AsyncTaskBillingRecord),
    Existing(AsyncTaskBillingRecord),
    NotFound,
}

fn validate_version(value: u64) -> Result<i64, AsyncTaskInputError> {
    let value = i64::try_from(value).map_err(|_| AsyncTaskInputError::InvalidVersion)?;
    if value <= 0 || value == i64::MAX {
        Err(AsyncTaskInputError::InvalidVersion)
    } else {
        Ok(value)
    }
}

fn validate_time(value: u64) -> Result<u64, AsyncTaskInputError> {
    i64::try_from(value)
        .map(|_| value)
        .map_err(|_| AsyncTaskInputError::InvalidTiming)
}
