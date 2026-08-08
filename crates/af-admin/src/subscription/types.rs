use std::{fmt, future::Future, pin::Pin};

use af_db::{
    MAX_SUBSCRIPTION_PAGE_SIZE, MAX_SUBSCRIPTION_PLAN_NAME_BYTES, SubscriptionPlanRecord,
    UserSubscriptionRecord,
};
use af_domain::{
    Quota, SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus, UserId,
    UserSubscriptionId, UserSubscriptionStatus,
};
use thiserror::Error;

use crate::SessionPrincipal;

use super::lifecycle::{
    AdminUserSubscriptionLifecycleCommand, AdminUserSubscriptionLifecycleResult,
};

/// 管理端订阅目录和用户订阅列表的默认页大小。
pub const DEFAULT_ADMIN_SUBSCRIPTION_PAGE_SIZE: usize = 25;

/// 管理员可读取的订阅计划快照。
pub struct AdminSubscriptionPlan {
    plan_id: SubscriptionPlanId,
    name: String,
    created_by_user_id: UserId,
    status: SubscriptionPlanStatus,
    quota_amount: Quota,
    cycle: SubscriptionCycle,
    version: u64,
    disabled_at: Option<u64>,
    created_at: u64,
    updated_at: u64,
}

impl AdminSubscriptionPlan {
    pub(super) fn from_record(record: &SubscriptionPlanRecord) -> Self {
        Self {
            plan_id: record.plan_id(),
            name: record.name().to_owned(),
            created_by_user_id: record.created_by_user_id(),
            status: record.status(),
            quota_amount: record.quota_amount(),
            cycle: record.cycle(),
            version: record.version(),
            disabled_at: record.disabled_at(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    /// 返回计划业务标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回计划显示名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回创建计划的用户标识。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回计划当前状态。
    #[must_use]
    pub const fn status(&self) -> SubscriptionPlanStatus {
        self.status
    }

    /// 返回每个周期的额度快照。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回计划周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回计划乐观锁版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回停用时间；有效计划为空。
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }

    /// 返回计划创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回计划最近更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminSubscriptionPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminSubscriptionPlan(<redacted>)")
    }
}

/// 管理员或当前用户可读取的订阅窗口快照。
pub struct AdminUserSubscription {
    subscription_id: UserSubscriptionId,
    user_id: UserId,
    plan_id: SubscriptionPlanId,
    plan_name: String,
    plan_version: u64,
    status: UserSubscriptionStatus,
    quota_amount: Quota,
    quota_used: Quota,
    cycle: SubscriptionCycle,
    window_started_at: u64,
    window_ends_at: u64,
    version: u64,
    bound_at: u64,
    status_changed_at: u64,
    updated_at: u64,
}

impl AdminUserSubscription {
    pub(super) fn from_record(record: &UserSubscriptionRecord) -> Self {
        Self {
            subscription_id: record.subscription_id(),
            user_id: record.user_id(),
            plan_id: record.plan_id(),
            plan_name: record.plan_name().to_owned(),
            plan_version: record.plan_version(),
            status: record.status(),
            quota_amount: record.quota_amount(),
            quota_used: record.quota_used(),
            cycle: record.cycle(),
            window_started_at: record.window_started_at(),
            window_ends_at: record.window_ends_at(),
            version: record.version(),
            bound_at: record.bound_at(),
            status_changed_at: record.status_changed_at(),
            updated_at: record.updated_at(),
        }
    }

    /// 返回用户订阅业务标识。
    #[must_use]
    pub const fn subscription_id(&self) -> UserSubscriptionId {
        self.subscription_id
    }

    /// 返回订阅所有者。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回绑定的计划标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回绑定计划的显示名称。
    #[must_use]
    pub fn plan_name(&self) -> &str {
        &self.plan_name
    }

    /// 返回绑定时固化的计划版本。
    #[must_use]
    pub const fn plan_version(&self) -> u64 {
        self.plan_version
    }

    /// 返回用户订阅生命周期状态。
    #[must_use]
    pub const fn status(&self) -> UserSubscriptionStatus {
        self.status
    }

    /// 返回绑定时固化的周期额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回当前周期已经确认的用量。
    #[must_use]
    pub const fn quota_used(&self) -> Quota {
        self.quota_used
    }

    /// 返回绑定时固化的周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回当前 UTC 窗口起点 Unix 秒数。
    #[must_use]
    pub const fn window_started_at(&self) -> u64 {
        self.window_started_at
    }

    /// 返回当前 UTC 窗口终点 Unix 秒数（不包含）。
    #[must_use]
    pub const fn window_ends_at(&self) -> u64 {
        self.window_ends_at
    }

    /// 返回用户订阅乐观锁版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回首次绑定时间的 Unix 秒数。
    #[must_use]
    pub const fn bound_at(&self) -> u64 {
        self.bound_at
    }

    /// 返回生命周期状态最近变化时间的 Unix 秒数。
    #[must_use]
    pub const fn status_changed_at(&self) -> u64 {
        self.status_changed_at
    }

    /// 返回订阅最近更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminUserSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUserSubscription(<redacted>)")
    }
}

/// 订阅计划分页结果。
pub struct AdminSubscriptionPlanPage {
    plans: Vec<AdminSubscriptionPlan>,
    next_cursor: Option<i64>,
}

impl AdminSubscriptionPlanPage {
    pub(super) fn new(plans: Vec<AdminSubscriptionPlan>, next_cursor: Option<i64>) -> Self {
        Self { plans, next_cursor }
    }

    /// 返回当前页计划。
    #[must_use]
    pub fn plans(&self) -> &[AdminSubscriptionPlan] {
        &self.plans
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminSubscriptionPlanPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminSubscriptionPlanPage")
            .field("plan_count", &self.plans.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 用户订阅分页结果。
pub struct AdminUserSubscriptionPage {
    subscriptions: Vec<AdminUserSubscription>,
    next_cursor: Option<i64>,
}

impl AdminUserSubscriptionPage {
    pub(super) fn new(subscriptions: Vec<AdminUserSubscription>, next_cursor: Option<i64>) -> Self {
        Self {
            subscriptions,
            next_cursor,
        }
    }

    /// 返回当前页用户订阅。
    #[must_use]
    pub fn subscriptions(&self) -> &[AdminUserSubscription] {
        &self.subscriptions
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminUserSubscriptionPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminUserSubscriptionPage")
            .field("subscription_count", &self.subscriptions.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 计划目录与用户订阅共用的稳定游标分页参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminSubscriptionPageQuery {
    pub(super) before: Option<i64>,
    pub(super) limit: usize,
}

impl AdminSubscriptionPageQuery {
    /// 校验正游标和有界页大小后构造查询。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, SubscriptionServiceError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_SUBSCRIPTION_PAGE_SIZE).contains(&limit)
        {
            return Err(SubscriptionServiceError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

impl Default for AdminSubscriptionPageQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_SUBSCRIPTION_PAGE_SIZE,
        }
    }
}

/// 管理员创建不可变订阅计划的结构化命令。
pub struct AdminSubscriptionPlanCreateCommand {
    pub(super) name: String,
    pub(super) quota_amount: Quota,
    pub(super) cycle: SubscriptionCycle,
}

impl AdminSubscriptionPlanCreateCommand {
    /// 校验名称、正整数额度和闭合周期后构造创建命令。
    pub fn new(
        name: String,
        quota_amount: i64,
        cycle: SubscriptionCycle,
    ) -> Result<Self, SubscriptionServiceError> {
        if name.is_empty()
            || name.len() > MAX_SUBSCRIPTION_PLAN_NAME_BYTES
            || name.trim() != name
            || name.chars().any(char::is_control)
        {
            return Err(SubscriptionServiceError::InvalidInput);
        }
        let quota_amount =
            Quota::new(quota_amount).map_err(|_| SubscriptionServiceError::InvalidInput)?;
        if quota_amount.is_zero() {
            return Err(SubscriptionServiceError::InvalidInput);
        }
        Ok(Self {
            name,
            quota_amount,
            cycle,
        })
    }
}

impl fmt::Debug for AdminSubscriptionPlanCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminSubscriptionPlanCreateCommand(<redacted>)")
    }
}

/// 管理员以当前版本停用计划的结构化命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminSubscriptionPlanDisableCommand {
    pub(super) expected_version: u64,
}

impl AdminSubscriptionPlanDisableCommand {
    /// 校验正版本后构造计划停用命令。
    pub fn new(expected_version: i64) -> Result<Self, SubscriptionServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionServiceError::InvalidInput);
        }
        Ok(Self {
            expected_version: u64::try_from(expected_version)
                .map_err(|_| SubscriptionServiceError::InvalidInput)?,
        })
    }
}

/// 管理员为路径指定用户绑定一个有效计划的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUserSubscriptionBindCommand {
    pub(super) plan_id: SubscriptionPlanId,
}

impl AdminUserSubscriptionBindCommand {
    /// 使用路径之外单独校验的计划标识构造绑定命令。
    #[must_use]
    pub const fn new(plan_id: SubscriptionPlanId) -> Self {
        Self { plan_id }
    }
}

/// 订阅管理与当前用户只读入口的应用错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionServiceError {
    /// 请求字段、标识或分页参数无效。
    #[error("订阅请求参数无效")]
    InvalidInput,
    /// 当前会话没有管理员权限。
    #[error("订阅管理权限不足")]
    Forbidden,
    /// 当前会话主体已失效。
    #[error("订阅会话无效")]
    InvalidSession,
    /// 计划不存在。
    #[error("订阅计划不存在")]
    PlanNotFound,
    /// 计划已停用，不能新增绑定。
    #[error("订阅计划已停用")]
    PlanDisabled,
    /// 目标用户不存在。
    #[error("订阅用户不存在")]
    UserNotFound,
    /// 订阅不存在或不属于路径指定用户。
    #[error("用户订阅不存在")]
    SubscriptionNotFound,
    /// 当前状态不允许执行请求的生命周期动作。
    #[error("订阅生命周期动作无效")]
    InvalidTransition,
    /// 陈旧窗口仍绑定在途计费预留，暂时不能恢复。
    #[error("订阅仍有在途计费预留")]
    InUse,
    /// 版本或业务事实发生冲突。
    #[error("订阅状态发生冲突")]
    Conflict,
    /// 写入结果无法确认，调用方应刷新事实。
    #[error("订阅操作结果未知")]
    OutcomeUnknown,
    /// 内部依赖或持久化不变量失败。
    #[error("订阅内部失败")]
    Internal,
}

/// 管理员读取计划目录的异步结果。
pub type SubscriptionListPlansFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminSubscriptionPlanPage, SubscriptionServiceError>>
            + Send
            + 'a,
    >,
>;
/// 管理员创建计划的异步结果。
pub type SubscriptionCreatePlanFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminSubscriptionPlan, SubscriptionServiceError>> + Send + 'a>,
>;
/// 管理员停用计划的异步结果。
pub type SubscriptionDisablePlanFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminSubscriptionPlan, SubscriptionServiceError>> + Send + 'a>,
>;
/// 管理员或当前用户读取用户订阅的异步结果。
pub type SubscriptionListUserFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminUserSubscriptionPage, SubscriptionServiceError>>
            + Send
            + 'a,
    >,
>;
/// 管理员绑定用户计划的异步结果。
pub type SubscriptionBindFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminUserSubscription, SubscriptionServiceError>> + Send + 'a>,
>;
/// 管理员迁移用户订阅生命周期的异步结果。
pub type SubscriptionLifecycleFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminUserSubscriptionLifecycleResult, SubscriptionServiceError>>
            + Send
            + 'a,
    >,
>;

/// 管理员订阅管理与当前用户只读应用端口。
pub trait SubscriptionService: Send + Sync {
    /// 管理员读取计划目录。
    fn list_plans<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListPlansFuture<'a>;

    /// 管理员创建不可变计划。
    fn create_plan<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminSubscriptionPlanCreateCommand,
    ) -> SubscriptionCreatePlanFuture<'a>;

    /// 管理员按预期版本停用计划。
    fn disable_plan<'a>(
        &'a self,
        principal: SessionPrincipal,
        plan_id: SubscriptionPlanId,
        command: AdminSubscriptionPlanDisableCommand,
    ) -> SubscriptionDisablePlanFuture<'a>;

    /// 管理员读取路径指定用户的订阅。
    fn list_user_subscriptions<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListUserFuture<'a>;

    /// 当前登录用户读取自己的订阅。
    fn list_current_subscriptions<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminSubscriptionPageQuery,
    ) -> SubscriptionListUserFuture<'a>;

    /// 管理员给路径指定用户绑定有效计划。
    fn bind_user<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminUserSubscriptionBindCommand,
    ) -> SubscriptionBindFuture<'a>;

    /// 管理员迁移路径指定用户的单个订阅生命周期。
    fn transition_user_lifecycle<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        subscription_id: UserSubscriptionId,
        command: AdminUserSubscriptionLifecycleCommand,
    ) -> SubscriptionLifecycleFuture<'a>;
}
