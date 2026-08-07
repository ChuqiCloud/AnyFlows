use std::fmt;

#[cfg(test)]
use std::sync::atomic::Ordering;

use af_domain::{SubscriptionCycle, UserSubscriptionId, UserSubscriptionStatus};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::{Condition, Expr},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::entity::{subscription_plans, user_subscriptions};

use super::{
    MAX_SUBSCRIPTION_PAGE_SIZE, SubscriptionInputError, SubscriptionRepositoryError,
    UserSubscriptionRecord, monotonic_updated_at,
    repository::{
        SubscriptionRepository, advance_window_until, begin, commit, internal, lock_subscription,
        page_query_limit, plan_record, rollback, subscription_record, to_database_time,
    },
    types::validate_time,
};

/// 到期取消订阅扫描使用的复合游标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionExpirationDueCursor {
    window_ends_at: i64,
    database_id: i64,
}

impl SubscriptionExpirationDueCursor {
    /// 校验并构造一个按窗口终点和数据库主键排序的游标。
    pub fn new(window_ends_at: u64, database_id: i64) -> Result<Self, SubscriptionInputError> {
        let window_ends_at =
            i64::try_from(window_ends_at).map_err(|_| SubscriptionInputError::InvalidCursor)?;
        if window_ends_at < 0 || database_id <= 0 {
            return Err(SubscriptionInputError::InvalidCursor);
        }
        Ok(Self {
            window_ends_at,
            database_id,
        })
    }

    /// 返回游标保存的窗口终点 Unix 秒数。
    #[must_use]
    pub const fn window_ends_at(self) -> u64 {
        self.window_ends_at as u64
    }

    /// 返回游标保存的数据库主键。
    #[must_use]
    pub const fn database_id(self) -> i64 {
        self.database_id
    }
}

/// 使用版本、源状态和观测窗口固化的一次生命周期迁移命令。
pub struct UserSubscriptionLifecycleTransition {
    subscription_id: UserSubscriptionId,
    expected_version: i64,
    expected_status: UserSubscriptionStatus,
    target_status: UserSubscriptionStatus,
    window_started_at: u64,
    window_ends_at: u64,
    changed_at: u64,
}

impl UserSubscriptionLifecycleTransition {
    /// 校验状态图、版本、观测窗口与迁移时间并构造可重放命令。
    #[allow(clippy::too_many_arguments, reason = "字段与生命周期 CAS 事实一一对应")]
    pub fn new(
        subscription_id: UserSubscriptionId,
        expected_version: u64,
        expected_status: UserSubscriptionStatus,
        target_status: UserSubscriptionStatus,
        window_started_at: u64,
        window_ends_at: u64,
        changed_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionInputError::InvalidVersion);
        }
        validate_time(window_started_at)?;
        validate_time(window_ends_at)?;
        validate_time(changed_at)?;
        af_domain::SubscriptionWindow::new(window_started_at, window_ends_at)
            .map_err(|_| SubscriptionInputError::InvalidWindow)?;
        if !expected_status.can_transition_to(target_status) {
            return Err(SubscriptionInputError::InvalidLifecycleTransition);
        }
        Ok(Self {
            subscription_id,
            expected_version,
            expected_status,
            target_status,
            window_started_at,
            window_ends_at,
            changed_at,
        })
    }

    /// 从已读取的订阅快照构造迁移命令，避免调用方重新拼接 CAS 事实。
    pub fn from_record(
        record: &UserSubscriptionRecord,
        target_status: UserSubscriptionStatus,
        changed_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        Self::new(
            record.subscription_id,
            record.version,
            record.status,
            target_status,
            record.window_started_at,
            record.window_ends_at,
            changed_at,
        )
    }
}

impl fmt::Debug for UserSubscriptionLifecycleTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionLifecycleTransition(<redacted>)")
    }
}

/// 到期取消订阅的稳定分页结果。
pub struct UserSubscriptionExpirationDuePageRecord {
    subscriptions: Vec<UserSubscriptionRecord>,
    next_cursor: Option<SubscriptionExpirationDueCursor>,
}

impl UserSubscriptionExpirationDuePageRecord {
    fn new(
        subscriptions: Vec<UserSubscriptionRecord>,
        next_cursor: Option<SubscriptionExpirationDueCursor>,
    ) -> Self {
        Self {
            subscriptions,
            next_cursor,
        }
    }

    /// 返回当前页已经到达窗口终点的取消订阅。
    #[must_use]
    pub fn subscriptions(&self) -> &[UserSubscriptionRecord] {
        &self.subscriptions
    }

    /// 返回下一页复合游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<SubscriptionExpirationDueCursor> {
        self.next_cursor
    }
}

impl fmt::Debug for UserSubscriptionExpirationDuePageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSubscriptionExpirationDuePageRecord")
            .field("subscription_count", &self.subscriptions.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 生命周期迁移结果及恢复时跨越的完整周期数。
pub struct UserSubscriptionLifecycleTransitionRecord {
    subscription: UserSubscriptionRecord,
    periods_elapsed: u32,
}

impl UserSubscriptionLifecycleTransitionRecord {
    const fn new(subscription: UserSubscriptionRecord, periods_elapsed: u32) -> Self {
        Self {
            subscription,
            periods_elapsed,
        }
    }

    /// 返回迁移后的订阅快照。
    #[must_use]
    pub const fn subscription(&self) -> &UserSubscriptionRecord {
        &self.subscription
    }

    /// 返回恢复订阅时跨越的完整周期数；非恢复迁移固定为零。
    #[must_use]
    pub const fn periods_elapsed(&self) -> u32 {
        self.periods_elapsed
    }
}

impl fmt::Debug for UserSubscriptionLifecycleTransitionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSubscriptionLifecycleTransitionRecord")
            .field("periods_elapsed", &self.periods_elapsed)
            .finish_non_exhaustive()
    }
}

/// 单订阅生命周期迁移的闭合结果。
pub enum UserSubscriptionLifecycleTransitionOutcome {
    /// 本次调用提交了状态迁移。
    Applied(UserSubscriptionLifecycleTransitionRecord),
    /// 相同命令已经提交，返回当前等价快照。
    Existing(UserSubscriptionLifecycleTransitionRecord),
    /// 订阅标识不存在。
    NotFound,
    /// 取消订阅尚未到达当前窗口终点。
    NotDue(UserSubscriptionRecord),
    /// 窗口仍绑定在途计费预留，不能推进或关闭该窗口。
    InUse(UserSubscriptionRecord),
}

impl fmt::Debug for UserSubscriptionLifecycleTransitionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => formatter
                .write_str("UserSubscriptionLifecycleTransitionOutcome::Applied(<redacted>)"),
            Self::Existing(_) => formatter
                .write_str("UserSubscriptionLifecycleTransitionOutcome::Existing(<redacted>)"),
            Self::NotFound => {
                formatter.write_str("UserSubscriptionLifecycleTransitionOutcome::NotFound")
            }
            Self::NotDue(_) => formatter
                .write_str("UserSubscriptionLifecycleTransitionOutcome::NotDue(<redacted>)"),
            Self::InUse(_) => {
                formatter.write_str("UserSubscriptionLifecycleTransitionOutcome::InUse(<redacted>)")
            }
        }
    }
}

impl SubscriptionRepository {
    /// 按窗口终点和主键升序读取一页到期的 Canceled 订阅。
    pub async fn list_expiration_due_subscriptions(
        &self,
        now: u64,
        after: Option<SubscriptionExpirationDueCursor>,
        limit: usize,
    ) -> Result<UserSubscriptionExpirationDuePageRecord, SubscriptionRepositoryError> {
        if now > i64::MAX as u64 || !(1..=MAX_SUBSCRIPTION_PAGE_SIZE).contains(&limit) {
            return Err(internal(SubscriptionRepositoryError::Invariant));
        }
        let operation = self
            .list_expiration_due_subscriptions_inner(now, after, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 通过版本、源状态和观测窗口联合 CAS 迁移单个订阅生命周期。
    pub async fn transition_user_subscription_lifecycle(
        &self,
        transition: &UserSubscriptionLifecycleTransition,
    ) -> Result<UserSubscriptionLifecycleTransitionOutcome, SubscriptionRepositoryError> {
        let operation = self
            .transition_user_subscription_lifecycle_inner(transition)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(
            &outcome,
            UserSubscriptionLifecycleTransitionOutcome::Applied(_)
        ) && self
            .outcome_unknown_after_commit
            .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    async fn list_expiration_due_subscriptions_inner(
        &self,
        now: u64,
        after: Option<SubscriptionExpirationDueCursor>,
        limit: usize,
    ) -> Result<UserSubscriptionExpirationDuePageRecord, SubscriptionRepositoryError> {
        let now = to_database_time(now)?;
        let mut query = user_subscriptions::Entity::find()
            .find_also_related(subscription_plans::Entity)
            .filter(user_subscriptions::Column::Status.eq(UserSubscriptionStatus::Canceled.code()))
            .filter(user_subscriptions::Column::WindowEndsAt.lte(now))
            .order_by_asc(user_subscriptions::Column::WindowEndsAt)
            .order_by_asc(user_subscriptions::Column::Id)
            .limit(page_query_limit(limit)?);
        if let Some(after) = after {
            let window_ends_at = to_database_time(after.window_ends_at())?;
            query = query.filter(
                Condition::any()
                    .add(user_subscriptions::Column::WindowEndsAt.gt(window_ends_at))
                    .add(
                        Condition::all()
                            .add(user_subscriptions::Column::WindowEndsAt.eq(window_ends_at))
                            .add(user_subscriptions::Column::Id.gt(after.database_id())),
                    ),
            );
        }
        let mut rows = query
            .all(self.pool.connection())
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        let has_more = rows.len() > limit;
        if has_more {
            rows.pop();
        }
        let next_cursor = if has_more {
            let subscription = rows
                .last()
                .map(|(subscription, _)| subscription)
                .ok_or(SubscriptionRepositoryError::Invariant)?;
            Some(
                SubscriptionExpirationDueCursor::new(
                    u64::try_from(subscription.window_ends_at.unix_timestamp())
                        .map_err(|_| SubscriptionRepositoryError::Invariant)?,
                    subscription.id,
                )
                .map_err(|_| SubscriptionRepositoryError::Invariant)?,
            )
        } else {
            None
        };
        let subscriptions = rows
            .into_iter()
            .map(|(subscription, plan)| {
                let plan = plan.ok_or(SubscriptionRepositoryError::Invariant)?;
                let plan = plan_record(plan)?;
                subscription_record(subscription, &plan)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UserSubscriptionExpirationDuePageRecord::new(
            subscriptions,
            next_cursor,
        ))
    }

    async fn transition_user_subscription_lifecycle_inner(
        &self,
        transition: &UserSubscriptionLifecycleTransition,
    ) -> Result<UserSubscriptionLifecycleTransitionOutcome, SubscriptionRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let Some((model, plan_model)) =
            lock_subscription(&transaction, transition.subscription_id).await?
        else {
            rollback(transaction).await?;
            return Ok(UserSubscriptionLifecycleTransitionOutcome::NotFound);
        };
        let plan = plan_record(plan_model)?;
        let current = subscription_record(model.clone(), &plan)?;

        if current.version != transition.expected_version as u64 {
            let replay_version = transition
                .expected_version
                .checked_add(1)
                .ok_or(SubscriptionRepositoryError::Invariant)?;
            if current.version != replay_version as u64
                || current.status != transition.target_status
                || current.status_changed_at != transition.changed_at
            {
                rollback(transaction).await?;
                return Err(SubscriptionRepositoryError::Conflict);
            }
            let effect = match lifecycle_effect(current.cycle, transition) {
                Ok(effect) => effect,
                Err(_) => {
                    rollback(transaction).await?;
                    return Err(SubscriptionRepositoryError::Conflict);
                }
            };
            let is_replay = current.window_started_at == effect.window_started_at
                && current.window_ends_at == effect.window_ends_at;
            rollback(transaction).await?;
            return if is_replay {
                Ok(UserSubscriptionLifecycleTransitionOutcome::Existing(
                    UserSubscriptionLifecycleTransitionRecord::new(current, effect.periods_elapsed),
                ))
            } else {
                Err(SubscriptionRepositoryError::Conflict)
            };
        }

        if current.status != transition.expected_status
            || current.window_started_at != transition.window_started_at
            || current.window_ends_at != transition.window_ends_at
            || transition.changed_at < current.status_changed_at
        {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }
        if transition.expected_status == UserSubscriptionStatus::Canceled
            && transition.target_status == UserSubscriptionStatus::Expired
            && transition.changed_at < transition.window_ends_at
        {
            rollback(transaction).await?;
            return Ok(UserSubscriptionLifecycleTransitionOutcome::NotDue(current));
        }
        let effect = lifecycle_effect(current.cycle, transition)?;
        if (effect.reset_quota || transition.target_status == UserSubscriptionStatus::Expired)
            && crate::quota::subscription::has_in_flight_reservation(&transaction, model.id)
                .await
                .map_err(|_| SubscriptionRepositoryError::Query)?
        {
            rollback(transaction).await?;
            return Ok(UserSubscriptionLifecycleTransitionOutcome::InUse(current));
        }

        let next_version = transition
            .expected_version
            .checked_add(1)
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let observed_window_started_at = to_database_time(transition.window_started_at)?;
        let observed_window_ends_at = to_database_time(transition.window_ends_at)?;
        let window_started_at = to_database_time(effect.window_started_at)?;
        let window_ends_at = to_database_time(effect.window_ends_at)?;
        let changed_at = to_database_time(transition.changed_at)?;
        let updated_at = monotonic_updated_at(model.updated_at, changed_at);
        let mut update = user_subscriptions::Entity::update_many()
            .filter(user_subscriptions::Column::Id.eq(model.id))
            .filter(user_subscriptions::Column::Status.eq(transition.expected_status.code()))
            .filter(user_subscriptions::Column::Version.eq(transition.expected_version))
            .filter(user_subscriptions::Column::WindowStartedAt.eq(observed_window_started_at))
            .filter(user_subscriptions::Column::WindowEndsAt.eq(observed_window_ends_at))
            .col_expr(
                user_subscriptions::Column::Status,
                Expr::value(transition.target_status.code()),
            )
            .col_expr(
                user_subscriptions::Column::WindowStartedAt,
                Expr::value(window_started_at),
            )
            .col_expr(
                user_subscriptions::Column::WindowEndsAt,
                Expr::value(window_ends_at),
            )
            .col_expr(
                user_subscriptions::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                user_subscriptions::Column::StatusChangedAt,
                Expr::value(changed_at),
            )
            .col_expr(
                user_subscriptions::Column::UpdatedAt,
                Expr::value(updated_at),
            );
        if effect.reset_quota {
            // 陈旧暂停订阅恢复时必须在同一事务推进窗口并清零旧窗口用量。
            update = update.col_expr(user_subscriptions::Column::QuotaUsed, Expr::value(0_i64));
        }
        let result = update
            .exec(&transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }

        let updated = user_subscriptions::Entity::find_by_id(model.id)
            .one(&transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let record = subscription_record(updated, &plan)?;
        commit(transaction).await?;
        Ok(UserSubscriptionLifecycleTransitionOutcome::Applied(
            UserSubscriptionLifecycleTransitionRecord::new(record, effect.periods_elapsed),
        ))
    }
}

/// 单次迁移预先计算出的目标窗口和额度归零要求。
struct LifecycleEffect {
    window_started_at: u64,
    window_ends_at: u64,
    periods_elapsed: u32,
    reset_quota: bool,
}

/// 仅在恢复陈旧暂停订阅时推进窗口，其余迁移保留原窗口事实。
fn lifecycle_effect(
    cycle: SubscriptionCycle,
    transition: &UserSubscriptionLifecycleTransition,
) -> Result<LifecycleEffect, SubscriptionRepositoryError> {
    if transition.expected_status == UserSubscriptionStatus::Suspended
        && transition.target_status == UserSubscriptionStatus::Active
        && let Some(advance) = advance_window_until(
            cycle,
            transition.window_started_at,
            transition.window_ends_at,
            transition.changed_at,
        )?
    {
        let window = advance.window();
        return Ok(LifecycleEffect {
            window_started_at: window.started_at(),
            window_ends_at: window.ends_at(),
            periods_elapsed: advance.periods_elapsed(),
            reset_quota: true,
        });
    }
    Ok(LifecycleEffect {
        window_started_at: transition.window_started_at,
        window_ends_at: transition.window_ends_at,
        periods_elapsed: 0,
        reset_quota: false,
    })
}
