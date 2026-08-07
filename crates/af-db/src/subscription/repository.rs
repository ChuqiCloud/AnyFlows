use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{
    Quota, SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus, SubscriptionWindow,
    SubscriptionWindowAdvance as DomainSubscriptionWindowAdvance, UserId, UserSubscriptionId,
    UserSubscriptionStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Condition, Expr, LockType},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{SensitiveString, subscription_plans, user_subscriptions, users},
};

use super::types::valid_name;
use super::{
    MAX_SUBSCRIPTION_PAGE_SIZE, SubscriptionPlanCreateOutcome, SubscriptionPlanDisable,
    SubscriptionPlanDisableOutcome, SubscriptionPlanPageRecord, SubscriptionPlanRecord,
    SubscriptionPlanWrite, SubscriptionRepositoryConfigError, SubscriptionRepositoryError,
    SubscriptionResetDueCursor, UserSubscriptionBind, UserSubscriptionBindOutcome,
    UserSubscriptionPageRecord, UserSubscriptionRecord, UserSubscriptionResetDuePageRecord,
    UserSubscriptionWindowAdvance, UserSubscriptionWindowAdvanceOutcome,
    UserSubscriptionWindowAdvanceRecord,
};

/// 原子维护订阅计划启停和用户订阅幂等绑定的数据库仓储。
#[derive(Clone)]
pub struct SubscriptionRepository {
    pub(super) pool: DatabasePool,
    pub(super) operation_timeout: Duration,
    #[cfg(test)]
    pub(super) outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl SubscriptionRepository {
    /// 使用共享连接池和单次写入截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, SubscriptionRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(SubscriptionRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 按稳定业务标识读取单个订阅计划。
    pub async fn get_plan(
        &self,
        plan_id: SubscriptionPlanId,
    ) -> Result<Option<SubscriptionPlanRecord>, SubscriptionRepositoryError> {
        let operation = async {
            load_plan(self.pool.connection(), plan_id)
                .await?
                .map(plan_record)
                .transpose()
        }
        .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 按内部主键倒序读取一页订阅计划。
    pub async fn list_plans(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<SubscriptionPlanPageRecord, SubscriptionRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_SUBSCRIPTION_PAGE_SIZE).contains(&limit)
        {
            return Err(internal(SubscriptionRepositoryError::Invariant));
        }
        let operation = self
            .list_plans_inner(before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 按内部主键倒序读取指定用户的一页订阅记录。
    pub async fn list_user_subscriptions(
        &self,
        user_id: UserId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<UserSubscriptionPageRecord, SubscriptionRepositoryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_SUBSCRIPTION_PAGE_SIZE).contains(&limit)
        {
            return Err(internal(SubscriptionRepositoryError::Invariant));
        }
        let operation = self
            .list_user_subscriptions_inner(user_id, before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 按窗口结束时间和主键升序读取一页到期 Active 订阅。
    pub async fn list_reset_due_subscriptions(
        &self,
        now: u64,
        after: Option<SubscriptionResetDueCursor>,
        limit: usize,
    ) -> Result<UserSubscriptionResetDuePageRecord, SubscriptionRepositoryError> {
        if now > i64::MAX as u64 || !(1..=MAX_SUBSCRIPTION_PAGE_SIZE).contains(&limit) {
            return Err(internal(SubscriptionRepositoryError::Invariant));
        }
        let operation = self
            .list_reset_due_subscriptions_inner(now, after, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 按稳定标识和完整不可变事实幂等创建订阅计划。
    pub async fn create_plan(
        &self,
        write: &SubscriptionPlanWrite,
    ) -> Result<SubscriptionPlanCreateOutcome, SubscriptionRepositoryError> {
        let operation = self
            .create_plan_inner(write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, SubscriptionPlanCreateOutcome::Created(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 通过计划版本 CAS 停止新的用户订阅绑定。
    pub async fn disable_plan(
        &self,
        write: &SubscriptionPlanDisable,
    ) -> Result<SubscriptionPlanDisableOutcome, SubscriptionRepositoryError> {
        let operation = self
            .disable_plan_inner(write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, SubscriptionPlanDisableOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 将用户和有效计划绑定，并固化首个额度周期快照。
    pub async fn bind_user(
        &self,
        bind: &UserSubscriptionBind,
    ) -> Result<UserSubscriptionBindOutcome, SubscriptionRepositoryError> {
        let operation = self
            .bind_user_inner(bind)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, UserSubscriptionBindOutcome::Created(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 通过订阅版本和观测窗口 CAS 推进到包含扫描时刻的新窗口。
    pub async fn advance_user_subscription_window(
        &self,
        advance: &UserSubscriptionWindowAdvance,
    ) -> Result<UserSubscriptionWindowAdvanceOutcome, SubscriptionRepositoryError> {
        let operation = self
            .advance_user_subscription_window_inner(advance)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, UserSubscriptionWindowAdvanceOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    async fn list_plans_inner(
        &self,
        before: Option<i64>,
        limit: usize,
    ) -> Result<SubscriptionPlanPageRecord, SubscriptionRepositoryError> {
        let mut query = subscription_plans::Entity::find()
            .order_by_desc(subscription_plans::Column::Id)
            .limit(page_query_limit(limit)?);
        if let Some(before) = before {
            query = query.filter(subscription_plans::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        let has_more = models.len() > limit;
        if has_more {
            models.pop();
        }
        let next_cursor = if has_more {
            models.last().map(|model| model.id)
        } else {
            None
        };
        let plans = models
            .into_iter()
            .map(plan_record)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SubscriptionPlanPageRecord::new(plans, next_cursor))
    }

    async fn list_user_subscriptions_inner(
        &self,
        user_id: UserId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<UserSubscriptionPageRecord, SubscriptionRepositoryError> {
        let mut query = user_subscriptions::Entity::find()
            .find_also_related(subscription_plans::Entity)
            .filter(user_subscriptions::Column::UserId.eq(user_id.get()))
            .order_by_desc(user_subscriptions::Column::Id)
            .limit(page_query_limit(limit)?);
        if let Some(before) = before {
            query = query.filter(user_subscriptions::Column::Id.lt(before));
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
            rows.last().map(|(subscription, _)| subscription.id)
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
        Ok(UserSubscriptionPageRecord::new(subscriptions, next_cursor))
    }

    async fn list_reset_due_subscriptions_inner(
        &self,
        now: u64,
        after: Option<SubscriptionResetDueCursor>,
        limit: usize,
    ) -> Result<UserSubscriptionResetDuePageRecord, SubscriptionRepositoryError> {
        let now = to_database_time(now)?;
        let mut query = user_subscriptions::Entity::find()
            .find_also_related(subscription_plans::Entity)
            .filter(user_subscriptions::Column::Status.eq(UserSubscriptionStatus::Active.code()))
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
                SubscriptionResetDueCursor::new(
                    unix_seconds(subscription.window_ends_at)?,
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
        Ok(UserSubscriptionResetDuePageRecord::new(
            subscriptions,
            next_cursor,
        ))
    }

    async fn create_plan_inner(
        &self,
        write: &SubscriptionPlanWrite,
    ) -> Result<SubscriptionPlanCreateOutcome, SubscriptionRepositoryError> {
        if let Some(model) = load_plan(self.pool.connection(), write.plan_id).await? {
            return classify_plan(model, write);
        }

        let transaction = begin(&self.pool).await?;
        if lock_user(&transaction, write.created_by_user_id)
            .await?
            .is_none()
        {
            rollback(transaction).await?;
            return Ok(SubscriptionPlanCreateOutcome::CreatorNotFound);
        }
        if let Some(model) = load_plan(&transaction, write.plan_id).await? {
            rollback(transaction).await?;
            return classify_plan(model, write);
        }

        let created_at = to_database_time(write.created_at)?;
        let inserted = subscription_plans::ActiveModel {
            plan_key: Set(SensitiveString::from(write.plan_id.persistence_key())),
            name: Set(write.name.clone()),
            created_by_user_id: Set(write.created_by_user_id.get()),
            status: Set(SubscriptionPlanStatus::Active.code()),
            quota_amount: Set(write.quota_amount.units()),
            cycle: Set(write.cycle.code()),
            version: Set(1),
            disabled_at: Set(None),
            created_at: Set(created_at),
            updated_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await;
        let inserted = match inserted {
            Ok(model) => model,
            Err(error) => {
                let unique = is_unique_conflict(&error);
                rollback(transaction).await?;
                return if unique {
                    recover_plan_collision(self.pool.connection(), write).await
                } else {
                    Err(SubscriptionRepositoryError::Query)
                };
            }
        };

        let record = plan_record(inserted)?;
        commit(transaction).await?;
        Ok(SubscriptionPlanCreateOutcome::Created(record))
    }

    async fn disable_plan_inner(
        &self,
        write: &SubscriptionPlanDisable,
    ) -> Result<SubscriptionPlanDisableOutcome, SubscriptionRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let Some(model) = lock_plan(&transaction, write.plan_id).await? else {
            rollback(transaction).await?;
            return Ok(SubscriptionPlanDisableOutcome::NotFound);
        };
        let current = plan_record(model.clone())?;
        if current.status == SubscriptionPlanStatus::Disabled {
            rollback(transaction).await?;
            return if current.version == write.expected_version as u64 + 1
                && current.disabled_at == Some(write.disabled_at)
            {
                Ok(SubscriptionPlanDisableOutcome::Existing(current))
            } else {
                Err(SubscriptionRepositoryError::Conflict)
            };
        }
        if current.version != write.expected_version as u64
            || write.disabled_at < current.created_at
        {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }

        let disabled_at = to_database_time(write.disabled_at)?;
        let next_version = write
            .expected_version
            .checked_add(1)
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let updated_at = monotonic_updated_at(model.updated_at, disabled_at);
        let update = subscription_plans::Entity::update_many()
            .filter(subscription_plans::Column::Id.eq(model.id))
            .filter(subscription_plans::Column::Status.eq(SubscriptionPlanStatus::Active.code()))
            .filter(subscription_plans::Column::Version.eq(write.expected_version))
            .col_expr(
                subscription_plans::Column::Status,
                Expr::value(SubscriptionPlanStatus::Disabled.code()),
            )
            .col_expr(
                subscription_plans::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                subscription_plans::Column::DisabledAt,
                Expr::value(Some(disabled_at)),
            )
            .col_expr(
                subscription_plans::Column::UpdatedAt,
                Expr::value(updated_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        if update.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }
        let updated = subscription_plans::Entity::find_by_id(model.id)
            .one(&transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let record = plan_record(updated)?;
        commit(transaction).await?;
        Ok(SubscriptionPlanDisableOutcome::Applied(record))
    }

    async fn bind_user_inner(
        &self,
        bind: &UserSubscriptionBind,
    ) -> Result<UserSubscriptionBindOutcome, SubscriptionRepositoryError> {
        if let Some((subscription, plan)) =
            load_subscription(self.pool.connection(), bind.subscription_id).await?
        {
            return classify_subscription(subscription, plan, bind);
        }

        let transaction = begin(&self.pool).await?;
        let Some(plan_model) = lock_plan(&transaction, bind.plan_id).await? else {
            rollback(transaction).await?;
            return Ok(UserSubscriptionBindOutcome::PlanNotFound);
        };
        let plan = plan_record(plan_model)?;
        if !plan.status.is_active() {
            rollback(transaction).await?;
            return Ok(UserSubscriptionBindOutcome::PlanDisabled);
        }
        if lock_user(&transaction, bind.user_id).await?.is_none() {
            rollback(transaction).await?;
            return Ok(UserSubscriptionBindOutcome::UserNotFound);
        }
        if let Some((subscription, existing_plan)) =
            load_subscription(&transaction, bind.subscription_id).await?
        {
            rollback(transaction).await?;
            return classify_subscription(subscription, existing_plan, bind);
        }

        let window_started_at = to_database_time(bind.window_started_at)?;
        let window_ends_at = to_database_time(bind.window_ends_at)?;
        let bound_at = to_database_time(bind.bound_at)?;
        let inserted = user_subscriptions::ActiveModel {
            subscription_key: Set(SensitiveString::from(
                bind.subscription_id.persistence_key(),
            )),
            user_id: Set(bind.user_id.get()),
            plan_id: Set(plan.database_id),
            plan_version: Set(
                i64::try_from(plan.version).map_err(|_| SubscriptionRepositoryError::Invariant)?
            ),
            status: Set(UserSubscriptionStatus::Active.code()),
            quota_amount: Set(plan.quota_amount.units()),
            quota_used: Set(0),
            cycle: Set(plan.cycle.code()),
            window_started_at: Set(window_started_at),
            window_ends_at: Set(window_ends_at),
            version: Set(1),
            bound_at: Set(bound_at),
            status_changed_at: Set(bound_at),
            created_at: Set(bound_at),
            updated_at: Set(bound_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await;
        let inserted = match inserted {
            Ok(model) => model,
            Err(error) => {
                let unique = is_unique_conflict(&error);
                rollback(transaction).await?;
                return if unique {
                    recover_subscription_collision(self.pool.connection(), bind).await
                } else {
                    Err(SubscriptionRepositoryError::Query)
                };
            }
        };

        let record = subscription_record(inserted, &plan)?;
        commit(transaction).await?;
        Ok(UserSubscriptionBindOutcome::Created(record))
    }

    async fn advance_user_subscription_window_inner(
        &self,
        advance: &UserSubscriptionWindowAdvance,
    ) -> Result<UserSubscriptionWindowAdvanceOutcome, SubscriptionRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let Some((model, plan_model)) =
            lock_subscription(&transaction, advance.subscription_id).await?
        else {
            rollback(transaction).await?;
            return Ok(UserSubscriptionWindowAdvanceOutcome::NotFound);
        };
        let plan = plan_record(plan_model)?;
        let current = subscription_record(model.clone(), &plan)?;

        if current.status != UserSubscriptionStatus::Active {
            rollback(transaction).await?;
            return if matches_advance_snapshot(&current, advance) {
                Ok(UserSubscriptionWindowAdvanceOutcome::Inactive(current))
            } else {
                Err(SubscriptionRepositoryError::Conflict)
            };
        }

        let requested = requested_window_advance(current.cycle, advance)?;
        if current.version == advance.expected_version as u64 {
            if !matches_advance_snapshot(&current, advance) {
                rollback(transaction).await?;
                return Err(SubscriptionRepositoryError::Conflict);
            }
            let Some(requested) = requested else {
                rollback(transaction).await?;
                return Ok(UserSubscriptionWindowAdvanceOutcome::NotDue(current));
            };
            if crate::quota::subscription::has_in_flight_reservation(&transaction, model.id)
                .await
                .map_err(|_| SubscriptionRepositoryError::Query)?
            {
                rollback(transaction).await?;
                return Ok(UserSubscriptionWindowAdvanceOutcome::InUse(current));
            }

            let next_window = requested.window();
            let next_version = advance
                .expected_version
                .checked_add(1)
                .ok_or(SubscriptionRepositoryError::Invariant)?;
            let window_started_at = to_database_time(next_window.started_at())?;
            let window_ends_at = to_database_time(next_window.ends_at())?;
            let observed_window_started_at = to_database_time(advance.window_started_at)?;
            let observed_window_ends_at = to_database_time(advance.window_ends_at)?;
            let updated_at = monotonic_updated_at(model.updated_at, to_database_time(advance.now)?);
            let update = user_subscriptions::Entity::update_many()
                .filter(user_subscriptions::Column::Id.eq(model.id))
                .filter(
                    user_subscriptions::Column::Status.eq(UserSubscriptionStatus::Active.code()),
                )
                .filter(user_subscriptions::Column::Version.eq(advance.expected_version))
                .filter(user_subscriptions::Column::WindowStartedAt.eq(observed_window_started_at))
                .filter(user_subscriptions::Column::WindowEndsAt.eq(observed_window_ends_at))
                .col_expr(user_subscriptions::Column::QuotaUsed, Expr::value(0_i64))
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
                    user_subscriptions::Column::UpdatedAt,
                    Expr::value(updated_at),
                )
                .exec(&transaction)
                .await
                .map_err(|_| SubscriptionRepositoryError::Query)?;
            if update.rows_affected != 1 {
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
            return Ok(UserSubscriptionWindowAdvanceOutcome::Applied(
                UserSubscriptionWindowAdvanceRecord::new(record, requested.periods_elapsed()),
            ));
        }

        let replay_version = advance
            .expected_version
            .checked_add(1)
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let Some(requested) = requested else {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        };
        let replay_window = requested.window();
        if current.version == replay_version as u64
            && current.window_started_at == replay_window.started_at()
            && current.window_ends_at == replay_window.ends_at()
            && current.quota_used.is_zero()
        {
            rollback(transaction).await?;
            return Ok(UserSubscriptionWindowAdvanceOutcome::Existing(
                UserSubscriptionWindowAdvanceRecord::new(current, requested.periods_elapsed()),
            ));
        }

        rollback(transaction).await?;
        Err(SubscriptionRepositoryError::Conflict)
    }
}

async fn recover_plan_collision<C>(
    connection: &C,
    write: &SubscriptionPlanWrite,
) -> Result<SubscriptionPlanCreateOutcome, SubscriptionRepositoryError>
where
    C: ConnectionTrait,
{
    let model = load_plan(connection, write.plan_id)
        .await?
        .ok_or(SubscriptionRepositoryError::Conflict)?;
    classify_plan(model, write)
}

fn classify_plan(
    model: subscription_plans::Model,
    write: &SubscriptionPlanWrite,
) -> Result<SubscriptionPlanCreateOutcome, SubscriptionRepositoryError> {
    let record = plan_record(model)?;
    if record.matches_write(write) {
        Ok(SubscriptionPlanCreateOutcome::Existing(record))
    } else {
        Err(SubscriptionRepositoryError::Conflict)
    }
}

async fn recover_subscription_collision<C>(
    connection: &C,
    bind: &UserSubscriptionBind,
) -> Result<UserSubscriptionBindOutcome, SubscriptionRepositoryError>
where
    C: ConnectionTrait,
{
    let (subscription, plan) = load_subscription(connection, bind.subscription_id)
        .await?
        .ok_or(SubscriptionRepositoryError::Conflict)?;
    classify_subscription(subscription, plan, bind)
}

fn classify_subscription(
    model: user_subscriptions::Model,
    plan: subscription_plans::Model,
    bind: &UserSubscriptionBind,
) -> Result<UserSubscriptionBindOutcome, SubscriptionRepositoryError> {
    let plan = plan_record(plan)?;
    let record = subscription_record(model, &plan)?;
    if record.matches_bind(bind) {
        Ok(UserSubscriptionBindOutcome::Existing(record))
    } else {
        Err(SubscriptionRepositoryError::Conflict)
    }
}

fn requested_window_advance(
    cycle: SubscriptionCycle,
    advance: &UserSubscriptionWindowAdvance,
) -> Result<Option<DomainSubscriptionWindowAdvance>, SubscriptionRepositoryError> {
    advance_window_until(
        cycle,
        advance.window_started_at,
        advance.window_ends_at,
        advance.now,
    )
}

/// 校验窗口连续性，并推进到包含指定时刻的 UTC 日历周期。
pub(super) fn advance_window_until(
    cycle: SubscriptionCycle,
    window_started_at: u64,
    window_ends_at: u64,
    now: u64,
) -> Result<Option<DomainSubscriptionWindowAdvance>, SubscriptionRepositoryError> {
    let window = SubscriptionWindow::new(window_started_at, window_ends_at)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let next = SubscriptionWindow::initial(cycle, window.ends_at())
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    if next.started_at() != window.ends_at() {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    window
        .advance_until(cycle, now)
        .map_err(|_| SubscriptionRepositoryError::Invariant)
}

fn matches_advance_snapshot(
    current: &UserSubscriptionRecord,
    advance: &UserSubscriptionWindowAdvance,
) -> bool {
    current.subscription_id == advance.subscription_id
        && current.version == advance.expected_version as u64
        && current.window_started_at == advance.window_started_at
        && current.window_ends_at == advance.window_ends_at
}

async fn load_plan<C>(
    connection: &C,
    plan_id: SubscriptionPlanId,
) -> Result<Option<subscription_plans::Model>, SubscriptionRepositoryError>
where
    C: ConnectionTrait,
{
    subscription_plans::Entity::find()
        .filter(
            subscription_plans::Column::PlanKey
                .eq(SensitiveString::from(plan_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

async fn load_subscription<C>(
    connection: &C,
    subscription_id: UserSubscriptionId,
) -> Result<
    Option<(user_subscriptions::Model, subscription_plans::Model)>,
    SubscriptionRepositoryError,
>
where
    C: ConnectionTrait,
{
    let Some(subscription) = user_subscriptions::Entity::find()
        .filter(
            user_subscriptions::Column::SubscriptionKey
                .eq(SensitiveString::from(subscription_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
    else {
        return Ok(None);
    };
    let plan = subscription_plans::Entity::find_by_id(subscription.plan_id)
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
        .ok_or(SubscriptionRepositoryError::Invariant)?;
    Ok(Some((subscription, plan)))
}

pub(super) async fn lock_subscription(
    transaction: &DatabaseTransaction,
    subscription_id: UserSubscriptionId,
) -> Result<
    Option<(user_subscriptions::Model, subscription_plans::Model)>,
    SubscriptionRepositoryError,
> {
    let subscription_key = SensitiveString::from(subscription_id.persistence_key());
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，使用同值更新提前取得写锁并保持后续 CAS 原子性。
        user_subscriptions::Entity::update_many()
            .filter(user_subscriptions::Column::SubscriptionKey.eq(subscription_key.clone()))
            .col_expr(
                user_subscriptions::Column::Version,
                Expr::col(user_subscriptions::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
    }
    let mut query = user_subscriptions::Entity::find()
        .filter(user_subscriptions::Column::SubscriptionKey.eq(subscription_key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let Some(subscription) = query
        .one(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
    else {
        return Ok(None);
    };
    let plan = subscription_plans::Entity::find_by_id(subscription.plan_id)
        .one(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
        .ok_or(SubscriptionRepositoryError::Invariant)?;
    Ok(Some((subscription, plan)))
}

async fn lock_plan(
    transaction: &DatabaseTransaction,
    plan_id: SubscriptionPlanId,
) -> Result<Option<subscription_plans::Model>, SubscriptionRepositoryError> {
    let plan_key = SensitiveString::from(plan_id.persistence_key());
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，先通过恒等更新取得数据库写锁。
        subscription_plans::Entity::update_many()
            .filter(subscription_plans::Column::PlanKey.eq(plan_key.clone()))
            .col_expr(
                subscription_plans::Column::Version,
                Expr::col(subscription_plans::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
    }
    let mut query =
        subscription_plans::Entity::find().filter(subscription_plans::Column::PlanKey.eq(plan_key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, SubscriptionRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // 用户恒等更新与计划写锁使用同一事务，避免并发软删除或重复绑定穿透。
        let update = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        if update.rows_affected == 0 {
            return Ok(None);
        }
        if update.rows_affected != 1 {
            return Err(SubscriptionRepositoryError::Invariant);
        }
    }
    let mut query =
        users::Entity::find_by_id(user_id.get()).filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

pub(super) fn plan_record(
    model: subscription_plans::Model,
) -> Result<SubscriptionPlanRecord, SubscriptionRepositoryError> {
    let plan_id = SubscriptionPlanId::from_persistence_key(model.plan_key.as_str())
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let created_by_user_id = UserId::new(model.created_by_user_id)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let status = SubscriptionPlanStatus::try_from(model.status)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let cycle = SubscriptionCycle::try_from(model.cycle)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let version =
        u64::try_from(model.version).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let disabled_at = optional_unix_seconds(model.disabled_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let valid_state = match status {
        SubscriptionPlanStatus::Active => disabled_at.is_none(),
        SubscriptionPlanStatus::Disabled => disabled_at.is_some(),
    };
    if model.id <= 0
        || !valid_name(&model.name)
        || quota_amount.is_zero()
        || version == 0
        || disabled_at.is_some_and(|value| value < created_at || value > updated_at)
        || updated_at < created_at
        || !valid_state
    {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    Ok(SubscriptionPlanRecord {
        database_id: model.id,
        plan_id,
        name: model.name,
        created_by_user_id,
        status,
        quota_amount,
        cycle,
        version,
        disabled_at,
        created_at,
        updated_at,
    })
}

pub(super) fn subscription_record(
    model: user_subscriptions::Model,
    plan: &SubscriptionPlanRecord,
) -> Result<UserSubscriptionRecord, SubscriptionRepositoryError> {
    let subscription_id = UserSubscriptionId::from_persistence_key(model.subscription_key.as_str())
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let user_id = UserId::new(model.user_id).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let plan_version =
        u64::try_from(model.plan_version).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let status = UserSubscriptionStatus::try_from(model.status)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let quota_used =
        Quota::new(model.quota_used).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let cycle = SubscriptionCycle::try_from(model.cycle)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let window_started_at = unix_seconds(model.window_started_at)?;
    let window_ends_at = unix_seconds(model.window_ends_at)?;
    let version =
        u64::try_from(model.version).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let bound_at = unix_seconds(model.bound_at)?;
    let status_changed_at = unix_seconds(model.status_changed_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    if model.id <= 0
        || model.plan_id != plan.database_id
        || plan_version == 0
        || plan_version > plan.version
        || quota_amount.is_zero()
        || quota_amount != plan.quota_amount
        || quota_used.units() > quota_amount.units()
        || cycle != plan.cycle
        || window_ends_at <= window_started_at
        || bound_at >= window_ends_at
        || status_changed_at < bound_at
        || created_at != bound_at
        || updated_at < created_at
        || updated_at < status_changed_at
        || version == 0
    {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    Ok(UserSubscriptionRecord {
        subscription_id,
        user_id,
        plan_id: plan.plan_id,
        plan_name: plan.name.clone(),
        plan_version,
        status,
        quota_amount,
        quota_used,
        cycle,
        window_started_at,
        window_ends_at,
        version,
        bound_at,
        status_changed_at,
        created_at,
        updated_at,
    })
}

pub(super) fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, SubscriptionRepositoryError> {
    let value = i64::try_from(value).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| SubscriptionRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, SubscriptionRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| SubscriptionRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, SubscriptionRepositoryError> {
    value.map(unix_seconds).transpose()
}

pub(super) fn monotonic_updated_at(
    current: TimeDateTimeWithTimeZone,
    business_time: TimeDateTimeWithTimeZone,
) -> TimeDateTimeWithTimeZone {
    // 审计更新时间不能因历史停用时间或跨主机时钟偏差发生回拨。
    current
        .max(TimeDateTimeWithTimeZone::now_utc())
        .max(business_time)
}

pub(super) fn page_query_limit(limit: usize) -> Result<u64, SubscriptionRepositoryError> {
    u64::try_from(limit)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(SubscriptionRepositoryError::Invariant)
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_subscription_plans_plan_key")
        || rendered.contains("uq_user_subscriptions_subscription_key")
        || rendered.contains("subscription_plans.plan_key")
        || rendered.contains("user_subscriptions.subscription_key")
        || rendered.contains("Duplicate entry")
}

pub(super) async fn begin(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, SubscriptionRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

pub(super) async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), SubscriptionRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| SubscriptionRepositoryError::OutcomeUnknown)
}

pub(super) async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), SubscriptionRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| SubscriptionRepositoryError::OutcomeUnknown)
}

/// 仅记录闭合内部分类，避免计划、绑定键和用户标识进入日志。
pub(super) fn internal(error: SubscriptionRepositoryError) -> SubscriptionRepositoryError {
    let error_kind = match error {
        SubscriptionRepositoryError::Conflict => return error,
        SubscriptionRepositoryError::Query => "subscription_query",
        SubscriptionRepositoryError::Timeout => "subscription_timeout",
        SubscriptionRepositoryError::OutcomeUnknown => "subscription_outcome_unknown",
        SubscriptionRepositoryError::Invariant => "subscription_invariant",
    };
    tracing::error!(
        target: "af_db::subscription",
        error_kind,
        "订阅仓储发生内部错误"
    );
    error
}

impl fmt::Debug for SubscriptionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}
