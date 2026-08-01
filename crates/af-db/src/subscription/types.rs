use std::fmt;

use af_domain::{
    Quota, SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus, UserId,
    UserSubscriptionId, UserSubscriptionStatus,
};
use thiserror::Error;

/// 订阅计划名称允许的最大 UTF-8 字节数。
pub const MAX_SUBSCRIPTION_PLAN_NAME_BYTES: usize = 80;

/// 创建订阅计划时固化的不可变业务事实。
pub struct SubscriptionPlanWrite {
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) quota_amount: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) created_at: u64,
}

impl SubscriptionPlanWrite {
    /// 校验计划名称、正整数额度和审计时间边界。
    pub fn new(
        plan_id: SubscriptionPlanId,
        name: String,
        created_by_user_id: UserId,
        quota_amount: Quota,
        cycle: SubscriptionCycle,
        created_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        if !valid_name(&name) {
            return Err(SubscriptionInputError::InvalidName);
        }
        if quota_amount.is_zero() {
            return Err(SubscriptionInputError::InvalidQuota);
        }
        validate_time(created_at)?;
        Ok(Self {
            plan_id,
            name,
            created_by_user_id,
            quota_amount,
            cycle,
            created_at,
        })
    }
}

impl fmt::Debug for SubscriptionPlanWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanWrite(<redacted>)")
    }
}

/// 通过 CAS 停止订阅计划新增绑定的命令。
pub struct SubscriptionPlanDisable {
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) expected_version: i64,
    pub(super) disabled_at: u64,
}

impl SubscriptionPlanDisable {
    /// 校验可递增正版本和受信服务端停用时间。
    pub fn new(
        plan_id: SubscriptionPlanId,
        expected_version: u64,
        disabled_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionInputError::InvalidVersion);
        }
        validate_time(disabled_at)?;
        Ok(Self {
            plan_id,
            expected_version,
            disabled_at,
        })
    }
}

impl fmt::Debug for SubscriptionPlanDisable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanDisable(<redacted>)")
    }
}

/// 将用户、计划和首个额度周期绑定为一项不可变幂等事实。
pub struct UserSubscriptionBind {
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) window_started_at: u64,
    pub(super) window_ends_at: u64,
    pub(super) bound_at: u64,
}

impl UserSubscriptionBind {
    /// 校验时间窗非空，并确保绑定发生时窗口尚未结束。
    pub fn new(
        subscription_id: UserSubscriptionId,
        user_id: UserId,
        plan_id: SubscriptionPlanId,
        window_started_at: u64,
        window_ends_at: u64,
        bound_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        validate_time(window_started_at)?;
        validate_time(window_ends_at)?;
        validate_time(bound_at)?;
        if window_ends_at <= window_started_at || bound_at >= window_ends_at {
            return Err(SubscriptionInputError::InvalidWindow);
        }
        Ok(Self {
            subscription_id,
            user_id,
            plan_id,
            window_started_at,
            window_ends_at,
            bound_at,
        })
    }
}

impl fmt::Debug for UserSubscriptionBind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionBind(<redacted>)")
    }
}

/// 已持久化订阅计划的非敏感状态快照。
pub struct SubscriptionPlanRecord {
    pub(super) database_id: i64,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) status: SubscriptionPlanStatus,
    pub(super) quota_amount: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) version: u64,
    pub(super) disabled_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl SubscriptionPlanRecord {
    /// 返回稳定计划标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回计划名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回当前计划状态。
    #[must_use]
    pub const fn status(&self) -> SubscriptionPlanStatus {
        self.status
    }

    /// 返回每个周期可消费的整数额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回计划额度周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回可选停用时间的 Unix 秒数。
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }

    /// 返回保持单调的计划审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_write(&self, write: &SubscriptionPlanWrite) -> bool {
        self.plan_id == write.plan_id
            && self.name == write.name
            && self.created_by_user_id == write.created_by_user_id
            && self.quota_amount == write.quota_amount
            && self.cycle == write.cycle
            && self.created_at == write.created_at
    }
}

impl fmt::Debug for SubscriptionPlanRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanRecord(<redacted>)")
    }
}

/// 已持久化用户订阅的当前窗口快照。
pub struct UserSubscriptionRecord {
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) plan_version: u64,
    pub(super) status: UserSubscriptionStatus,
    pub(super) quota_amount: Quota,
    pub(super) quota_used: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) window_started_at: u64,
    pub(super) window_ends_at: u64,
    pub(super) version: u64,
    pub(super) bound_at: u64,
    pub(super) status_changed_at: u64,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl UserSubscriptionRecord {
    /// 返回稳定用户订阅标识。
    #[must_use]
    pub const fn subscription_id(&self) -> UserSubscriptionId {
        self.subscription_id
    }

    /// 返回订阅所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回订阅绑定的稳定计划标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回绑定时快照的计划版本。
    #[must_use]
    pub const fn plan_version(&self) -> u64 {
        self.plan_version
    }

    /// 返回当前订阅状态。
    #[must_use]
    pub const fn status(&self) -> UserSubscriptionStatus {
        self.status
    }

    /// 返回本周期固化的额度上限。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回本周期已消费额度。
    #[must_use]
    pub const fn quota_used(&self) -> Quota {
        self.quota_used
    }

    /// 返回绑定时快照的日历周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回当前额度窗口起点 Unix 秒数。
    #[must_use]
    pub const fn window_started_at(&self) -> u64 {
        self.window_started_at
    }

    /// 返回当前额度窗口终点 Unix 秒数。
    #[must_use]
    pub const fn window_ends_at(&self) -> u64 {
        self.window_ends_at
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回首次绑定时间 Unix 秒数。
    #[must_use]
    pub const fn bound_at(&self) -> u64 {
        self.bound_at
    }

    /// 返回最近一次生命周期状态变更时间 Unix 秒数。
    #[must_use]
    pub const fn status_changed_at(&self) -> u64 {
        self.status_changed_at
    }

    /// 返回首次持久化时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回保持单调的用户订阅审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_bind(&self, bind: &UserSubscriptionBind) -> bool {
        self.subscription_id == bind.subscription_id
            && self.user_id == bind.user_id
            && self.plan_id == bind.plan_id
            && self.window_started_at == bind.window_started_at
            && self.window_ends_at == bind.window_ends_at
            && self.bound_at == bind.bound_at
    }
}

impl fmt::Debug for UserSubscriptionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionRecord(<redacted>)")
    }
}

/// 幂等创建订阅计划后的闭合结果。
pub enum SubscriptionPlanCreateOutcome {
    /// 本次调用创建了新计划。
    Created(SubscriptionPlanRecord),
    /// 相同计划标识和完整不可变事实已经存在。
    Existing(SubscriptionPlanRecord),
    /// 计划创建者不存在或已经软删除。
    CreatorNotFound,
}

/// CAS 停用订阅计划后的闭合结果。
pub enum SubscriptionPlanDisableOutcome {
    /// 本次调用把 Active 计划推进为 Disabled。
    Applied(SubscriptionPlanRecord),
    /// 相同版本迁移事实已经提交。
    Existing(SubscriptionPlanRecord),
    /// 计划不存在。
    NotFound,
}

/// 用户订阅幂等绑定后的闭合结果。
pub enum UserSubscriptionBindOutcome {
    /// 本次调用首次创建用户订阅。
    Created(UserSubscriptionRecord),
    /// 相同绑定标识和完整事实已经存在。
    Existing(UserSubscriptionRecord),
    /// 目标用户不存在或已经软删除。
    UserNotFound,
    /// 目标计划不存在。
    PlanNotFound,
    /// 目标计划已停止新增绑定。
    PlanDisabled,
}

/// 订阅输入构造错误；不携带名称、标识或用户信息。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionInputError {
    /// 计划名称不符合长度、空白或控制字符边界。
    #[error("订阅计划名称无效")]
    InvalidName,
    /// 计划周期额度必须为正整数。
    #[error("订阅计划额度无效")]
    InvalidQuota,
    /// 用户订阅时间窗为空或绑定时已经结束。
    #[error("用户订阅时间窗无效")]
    InvalidWindow,
    /// 时间戳无法持久化。
    #[error("订阅时间边界无效")]
    InvalidTiming,
    /// CAS 版本不是可递增的正整数。
    #[error("订阅计划版本无效")]
    InvalidVersion,
}

/// 订阅仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("订阅仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 订阅仓储错误；不携带计划、用户或绑定标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionRepositoryError {
    /// 相同计划、绑定键或 CAS 版本绑定了不同事实。
    #[error("订阅计划或用户绑定冲突")]
    Conflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("订阅数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须复用原标识查询或重试。
    #[error("订阅操作结果未知")]
    OutcomeUnknown,
    /// 持久化计划、用户订阅或时间窗违反不变量。
    #[error("订阅持久化状态损坏")]
    Invariant,
}

impl fmt::Debug for SubscriptionPlanCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("SubscriptionPlanCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionPlanCreateOutcome::Existing(<redacted>)")
            }
            Self::CreatorNotFound => {
                formatter.write_str("SubscriptionPlanCreateOutcome::CreatorNotFound")
            }
        }
    }
}

impl fmt::Debug for SubscriptionPlanDisableOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("SubscriptionPlanDisableOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionPlanDisableOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("SubscriptionPlanDisableOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for UserSubscriptionBindOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("UserSubscriptionBindOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("UserSubscriptionBindOutcome::Existing(<redacted>)")
            }
            Self::UserNotFound => formatter.write_str("UserSubscriptionBindOutcome::UserNotFound"),
            Self::PlanNotFound => formatter.write_str("UserSubscriptionBindOutcome::PlanNotFound"),
            Self::PlanDisabled => formatter.write_str("UserSubscriptionBindOutcome::PlanDisabled"),
        }
    }
}

pub(super) fn validate_time(value: u64) -> Result<(), SubscriptionInputError> {
    if value > i64::MAX as u64 {
        return Err(SubscriptionInputError::InvalidTiming);
    }
    Ok(())
}

pub(super) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUBSCRIPTION_PLAN_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
