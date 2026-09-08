use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{
    Quota, SubscriptionCycle, SubscriptionOrderId, SubscriptionOrderRequestId,
    SubscriptionOrderStatus, SubscriptionPaymentEventType, SubscriptionPlanId,
    SubscriptionPlanStatus, SubscriptionWindow,
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
    entity::{
        SensitiveString, subscription_orders, subscription_payment_events,
        subscription_plan_prices, subscription_plans, user_subscriptions, users,
    },
    notification::UserNotificationWrite,
};

use super::types::{valid_currency, valid_name, valid_price_text};
use super::{
    MAX_SUBSCRIPTION_PAGE_SIZE, SubscriptionOrderCreate, SubscriptionOrderCreateOutcome,
    SubscriptionOrderRecord, SubscriptionOrderSubmission, SubscriptionOrderSubmitOutcome,
    SubscriptionPaymentEventOutcome, SubscriptionPaymentEventRejection,
    SubscriptionPaymentEventWrite, SubscriptionPlanCreateOutcome, SubscriptionPlanDisable,
    SubscriptionPlanDisableOutcome, SubscriptionPlanPageRecord, SubscriptionPlanPriceRecord,
    SubscriptionPlanRecord, SubscriptionPlanWrite, SubscriptionRepositoryConfigError,
    SubscriptionRepositoryError, SubscriptionResetDueCursor, UserSubscriptionBind,
    UserSubscriptionBindOutcome, UserSubscriptionPageRecord, UserSubscriptionRecord,
    UserSubscriptionResetDuePageRecord, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome, UserSubscriptionWindowAdvanceRecord,
    monotonic_updated_at,
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

    /// 按稳定订单标识读取订阅订单，供支付 webhook 路由先判定订单类型。
    pub async fn get_order(
        &self,
        order_id: SubscriptionOrderId,
    ) -> Result<Option<SubscriptionOrderRecord>, SubscriptionRepositoryError> {
        let operation = load_order_by_key(self.pool.connection(), order_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(SubscriptionRepositoryError::Timeout)),
        }
    }

    /// 按稳定业务标识读取单个用户订阅及其计划快照。
    pub async fn get_user_subscription(
        &self,
        subscription_id: UserSubscriptionId,
    ) -> Result<Option<UserSubscriptionRecord>, SubscriptionRepositoryError> {
        let operation = async {
            let Some((subscription, plan)) =
                load_subscription(self.pool.connection(), subscription_id).await?
            else {
                return Ok(None);
            };
            let plan = plan_record(plan)?;
            subscription_record(subscription, &plan).map(Some)
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

    /// 读取 Active 计划及其不可变价格快照，供用户目录过滤支付渠道。
    pub async fn list_active_catalog(
        &self,
    ) -> Result<
        Vec<(SubscriptionPlanRecord, SubscriptionPlanPriceRecord)>,
        SubscriptionRepositoryError,
    > {
        let operation = async {
            let rows = subscription_plans::Entity::find()
                .find_also_related(subscription_plan_prices::Entity)
                .filter(
                    subscription_plans::Column::Status.eq(SubscriptionPlanStatus::Active.code()),
                )
                .order_by_asc(subscription_plans::Column::Id)
                .limit((MAX_SUBSCRIPTION_PAGE_SIZE + 1) as u64)
                .all(self.pool.connection())
                .await
                .map_err(|_| SubscriptionRepositoryError::Query)?;
            rows.into_iter()
                .filter_map(|(plan, price)| {
                    // 历史管理员计划没有价格事实，绝不能借用默认值后对用户伪装为可售。
                    price.map(|price| Ok((plan_record(plan)?, plan_price_record(price)?)))
                })
                .collect::<Result<Vec<_>, _>>()
        }
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

    /// 在当前用户范围内幂等创建待支付订阅订单。
    pub async fn create_order(
        &self,
        create: &SubscriptionOrderCreate,
    ) -> Result<SubscriptionOrderCreateOutcome, SubscriptionRepositoryError> {
        let operation = self
            .create_order_inner(create)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, SubscriptionOrderCreateOutcome::Created(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    pub async fn submit_order(
        &self,
        write: SubscriptionOrderSubmission,
    ) -> Result<SubscriptionOrderSubmitOutcome, SubscriptionRepositoryError> {
        let operation = self
            .submit_order_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, SubscriptionOrderSubmitOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(SubscriptionRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
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

    /// 在单一事务内接收已验签事件、推进订单并绑定用户订阅。
    pub async fn accept_verified_event(
        &self,
        write: &SubscriptionPaymentEventWrite,
    ) -> Result<SubscriptionPaymentEventOutcome, SubscriptionRepositoryError> {
        let operation = self
            .accept_verified_event_inner(write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(SubscriptionRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, SubscriptionPaymentEventOutcome::Applied { .. })
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

    async fn create_order_inner(
        &self,
        create: &SubscriptionOrderCreate,
    ) -> Result<SubscriptionOrderCreateOutcome, SubscriptionRepositoryError> {
        if let Some(existing) =
            load_order_by_request(self.pool.connection(), create.user_id, create.request_id).await?
        {
            return classify_order(existing, create, None);
        }

        let transaction = begin(&self.pool).await?;
        if lock_user(&transaction, create.user_id).await?.is_none() {
            rollback(transaction).await?;
            return Ok(SubscriptionOrderCreateOutcome::UserNotFound);
        }
        let Some(plan_model) = lock_plan(&transaction, create.plan_id).await? else {
            rollback(transaction).await?;
            return Ok(SubscriptionOrderCreateOutcome::PlanNotFound);
        };
        let plan = plan_record(plan_model.clone())?;
        if !plan.status.is_active() {
            rollback(transaction).await?;
            return Ok(SubscriptionOrderCreateOutcome::PlanDisabled);
        }
        let Some(price_model) = load_plan_price(&transaction, plan.database_id()).await? else {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Invariant);
        };
        let price = plan_price_record(price_model)?;
        let expected_version = i64::try_from(create.plan_version)
            .map_err(|_| SubscriptionRepositoryError::Invariant)?;
        if plan.version() != create.plan_version
            || price.provider() != create.provider
            || price.currency() != create.currency
            || price.amount_minor() != create.amount_minor
        {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }

        if let Some(existing) =
            load_order_by_request(&transaction, create.user_id, create.request_id).await?
        {
            rollback(transaction).await?;
            return classify_order(existing, create, Some(plan.quota_amount()));
        }

        let created_at = to_database_time(create.created_at)?;
        let expires_at = create.expires_at.map(to_database_time).transpose()?;
        let inserted = subscription_orders::ActiveModel {
            order_key: Set(create.order_id.persistence_key()),
            user_id: Set(create.user_id.get()),
            plan_id: Set(plan.database_id()),
            plan_key: Set(create.plan_id.persistence_key()),
            plan_version: Set(expected_version),
            provider: Set(create.provider.clone()),
            currency: Set(create.currency.clone()),
            amount_minor: Set(create.amount_minor),
            quota_amount: Set(plan.quota_amount().units()),
            status: Set(SubscriptionOrderStatus::Created.code()),
            idempotency_key: Set(create.request_id.persistence_key()),
            version: Set(1),
            provider_order_id: Set(None),
            trade_no: Set(None),
            payment_method: Set(None),
            expires_at: Set(expires_at),
            paid_at: Set(None),
            closed_at: Set(None),
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
                if unique {
                    return recover_order_collision(self.pool.connection(), create).await;
                }
                return Err(SubscriptionRepositoryError::Query);
            }
        };
        let record = subscription_order_record(inserted)?;
        commit(transaction).await?;
        Ok(SubscriptionOrderCreateOutcome::Created(record))
    }

    async fn submit_order_inner(
        &self,
        write: &SubscriptionOrderSubmission,
    ) -> Result<SubscriptionOrderSubmitOutcome, SubscriptionRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let Some(model) = lock_order(&transaction, write.order_id).await? else {
            rollback(transaction).await?;
            return Ok(SubscriptionOrderSubmitOutcome::NotFound);
        };
        let current = subscription_order_record(model.clone())?;
        if current.matches_submission(write) {
            rollback(transaction).await?;
            return Ok(SubscriptionOrderSubmitOutcome::Existing(current));
        }
        if current.status() != SubscriptionOrderStatus::Created
            || model.version != write.expected_version
            || write.submitted_at < current.created_at()
        {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }
        let next_version = model
            .version
            .checked_add(1)
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        let submitted_at = to_database_time(write.submitted_at)?;
        let expires_at = to_database_time(write.expires_at)?;
        let update = subscription_orders::Entity::update_many()
            .filter(subscription_orders::Column::Id.eq(model.id))
            .filter(subscription_orders::Column::Status.eq(SubscriptionOrderStatus::Created.code()))
            .filter(subscription_orders::Column::Version.eq(write.expected_version))
            .col_expr(
                subscription_orders::Column::ProviderOrderId,
                Expr::value(Some(SensitiveString::from(write.provider_order_id.clone()))),
            )
            .col_expr(
                subscription_orders::Column::PaymentMethod,
                Expr::value(Some(write.payment_method.clone())),
            )
            .col_expr(
                subscription_orders::Column::Status,
                Expr::value(SubscriptionOrderStatus::Pending.code()),
            )
            .col_expr(
                subscription_orders::Column::ExpiresAt,
                Expr::value(expires_at),
            )
            .col_expr(
                subscription_orders::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                subscription_orders::Column::UpdatedAt,
                Expr::value(submitted_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| SubscriptionRepositoryError::Query)?;
        if update.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Conflict);
        }
        let updated = load_order_by_database_id(&transaction, model.id)
            .await?
            .ok_or(SubscriptionRepositoryError::Invariant)?;
        commit(transaction).await?;
        Ok(SubscriptionOrderSubmitOutcome::Applied(updated))
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
            return classify_plan_with_price(self.pool.connection(), model, write).await;
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
            let outcome = classify_plan_with_price(&transaction, model, write).await;
            rollback(transaction).await?;
            return outcome;
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

        let price = subscription_plan_prices::ActiveModel {
            plan_id: Set(inserted.id),
            provider: Set(write.price_provider.clone()),
            currency: Set(write.price_currency.clone()),
            amount_minor: Set(write.price_amount_minor),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await;
        if price.is_err() {
            rollback(transaction).await?;
            return Err(SubscriptionRepositoryError::Query);
        }

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

    async fn accept_verified_event_inner(
        &self,
        write: &SubscriptionPaymentEventWrite,
    ) -> Result<SubscriptionPaymentEventOutcome, SubscriptionRepositoryError> {
        let Some(preloaded) = load_order_by_key(self.pool.connection(), write.order_id).await?
        else {
            return Ok(SubscriptionPaymentEventOutcome::NotFound);
        };
        if let Some(existing) = load_event_collision(self.pool.connection(), write).await? {
            return classify_event_collision(self.pool.connection(), existing, &preloaded, write)
                .await;
        }

        let transaction = begin(&self.pool).await?;
        let result = accept_event_in_transaction(&transaction, preloaded.user_id, write).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                let order = load_order_by_key(self.pool.connection(), write.order_id)
                    .await?
                    .ok_or(SubscriptionRepositoryError::Invariant)?;
                let existing = load_event_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(SubscriptionRepositoryError::Invariant)?;
                classify_event_collision(self.pool.connection(), existing, &order, write).await
            }
            Err(TransactionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
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

enum TransactionWriteError {
    UniqueConflict,
    Repository(SubscriptionRepositoryError),
}

impl From<SubscriptionRepositoryError> for TransactionWriteError {
    fn from(error: SubscriptionRepositoryError) -> Self {
        Self::Repository(error)
    }
}

async fn accept_event_in_transaction(
    transaction: &DatabaseTransaction,
    preloaded_user_id: UserId,
    write: &SubscriptionPaymentEventWrite,
) -> Result<SubscriptionPaymentEventOutcome, TransactionWriteError> {
    let user = lock_user(transaction, preloaded_user_id)
        .await?
        .ok_or(SubscriptionRepositoryError::Invariant)?;
    let Some(order_model) = lock_order(transaction, write.order_id).await? else {
        return Ok(SubscriptionPaymentEventOutcome::NotFound);
    };
    let order = subscription_order_record(order_model.clone())?;
    if order.user_id != preloaded_user_id {
        return Err(SubscriptionRepositoryError::Invariant.into());
    }
    if let Some(existing) = load_event_collision(transaction, write).await? {
        return classify_event_collision(transaction, existing, &order, write)
            .await
            .map_err(Into::into);
    }

    let received_at = to_database_time(write.received_at)?;
    let inserted = subscription_payment_events::ActiveModel {
        event_key: Set(write.event_id.persistence_key()),
        order_id: Set(order.database_id),
        provider: Set(write.provider.clone()),
        provider_event_id: Set(SensitiveString::from(write.provider_event_id.clone())),
        trade_no: Set(write.trade_no.clone().map(SensitiveString::from)),
        amount_minor: Set(write.amount_minor),
        currency: Set(write.currency.clone()),
        payment_method: Set(write.payment_method.clone()),
        event_type: Set(write.event_type.code()),
        signature_key_fingerprint: Set(SensitiveString::from(hex_digest(
            write.signature_key_fingerprint,
        ))),
        payload_sha256: Set(SensitiveString::from(hex_digest(write.payload_sha256))),
        received_at: Set(received_at),
        processed_at: Set(None),
        created_at: Set(received_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) if is_event_unique_conflict(&error) => {
            return Err(TransactionWriteError::UniqueConflict);
        }
        Err(_) => return Err(SubscriptionRepositoryError::Query.into()),
    };

    if write.provider != order.provider {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::ProviderMismatch,
        ));
    }
    if write.amount_minor != Some(order.amount_minor) {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::AmountMismatch,
        ));
    }
    if write.currency.as_deref() != Some(order.currency.as_str()) {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::CurrencyMismatch,
        ));
    }
    if order
        .payment_method
        .as_deref()
        .is_some_and(|expected| write.payment_method.as_deref() != Some(expected))
    {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::PaymentMethodMismatch,
        ));
    }
    if write.received_at < order.created_at {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::TimingConflict,
        ));
    }
    if order
        .trade_no
        .as_deref()
        .zip(write.trade_no.as_deref())
        .is_some_and(|(existing, incoming)| existing != incoming)
    {
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::TradeNumberConflict,
        ));
    }

    if !order.status.is_open() {
        if terminal_event_matches(&order, write) {
            mark_event_processed_at(transaction, inserted.id, processing_time(received_at)).await?;
            let subscription = load_subscription_for_order(transaction, &order).await?;
            return Ok(SubscriptionPaymentEventOutcome::Acknowledged {
                order,
                subscription,
            });
        }
        return Ok(record_unprocessed(
            order,
            SubscriptionPaymentEventRejection::TerminalConflict,
        ));
    }

    let processed_at = processing_time(received_at);
    let subscription = if write.event_type == SubscriptionPaymentEventType::Succeeded {
        let Some(plan_model) = lock_plan(transaction, order.plan_id).await? else {
            return Err(SubscriptionRepositoryError::Invariant.into());
        };
        let plan = plan_record(plan_model)?;
        if plan.quota_amount() != order.quota_amount {
            return Err(SubscriptionRepositoryError::Conflict.into());
        }
        bind_paid_order(transaction, &order, &plan, write.received_at).await?
    } else {
        None
    };

    transition_order(transaction, &order_model, write, received_at, processed_at).await?;
    mark_event_processed_at(transaction, inserted.id, processed_at).await?;
    crate::notification::insert_queued(
        transaction,
        &UserNotificationWrite::subscription_purchase(
            user.id,
            order.order_id.persistence_key(),
            order_status_label(write.event_type),
            processed_at,
        ),
    )
    .await
    .map_err(|_| SubscriptionRepositoryError::Query)?;
    let updated = load_order_by_database_id(transaction, order.database_id)
        .await?
        .ok_or(SubscriptionRepositoryError::Invariant)?;
    Ok(SubscriptionPaymentEventOutcome::Applied {
        order: updated,
        subscription,
    })
}

async fn bind_paid_order(
    transaction: &DatabaseTransaction,
    order: &SubscriptionOrderRecord,
    plan: &SubscriptionPlanRecord,
    bound_at: u64,
) -> Result<Option<UserSubscriptionRecord>, SubscriptionRepositoryError> {
    let subscription_id =
        UserSubscriptionId::from_persistence_key(&order.order_id.persistence_key())
            .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let window = SubscriptionWindow::initial(plan.cycle(), bound_at)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let bind = UserSubscriptionBind::new(
        subscription_id,
        order.user_id,
        order.plan_id,
        window.started_at(),
        window.ends_at(),
        bound_at,
    )
    .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    if let Some((model, existing_plan)) = lock_subscription(transaction, subscription_id).await? {
        let existing_plan = plan_record(existing_plan)?;
        let current = subscription_record(model, &existing_plan)?;
        if current.matches_bind(&bind) {
            return Ok(Some(current));
        }
        return Err(SubscriptionRepositoryError::BindingConflict);
    }

    let bound_at = to_database_time(bind.bound_at)?;
    let inserted = user_subscriptions::ActiveModel {
        subscription_key: Set(SensitiveString::from(
            bind.subscription_id.persistence_key(),
        )),
        user_id: Set(bind.user_id.get()),
        plan_id: Set(plan.database_id()),
        plan_version: Set(
            i64::try_from(plan.version()).map_err(|_| SubscriptionRepositoryError::Invariant)?
        ),
        status: Set(UserSubscriptionStatus::Active.code()),
        quota_amount: Set(plan.quota_amount().units()),
        quota_used: Set(0),
        cycle: Set(plan.cycle().code()),
        window_started_at: Set(to_database_time(bind.window_started_at)?),
        window_ends_at: Set(to_database_time(bind.window_ends_at)?),
        version: Set(1),
        bound_at: Set(bound_at),
        status_changed_at: Set(bound_at),
        created_at: Set(bound_at),
        updated_at: Set(bound_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) if is_subscription_unique_conflict(&error) => {
            return Err(SubscriptionRepositoryError::BindingConflict);
        }
        Err(_) => return Err(SubscriptionRepositoryError::Query),
    };
    subscription_record(inserted, plan).map(Some)
}

async fn load_subscription_for_order(
    transaction: &DatabaseTransaction,
    order: &SubscriptionOrderRecord,
) -> Result<Option<UserSubscriptionRecord>, SubscriptionRepositoryError> {
    let subscription_id =
        UserSubscriptionId::from_persistence_key(&order.order_id.persistence_key())
            .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let Some((model, plan)) = lock_subscription(transaction, subscription_id).await? else {
        return Ok(None);
    };
    subscription_record(model, &plan_record(plan)?).map(Some)
}

fn record_unprocessed(
    order: SubscriptionOrderRecord,
    reason: SubscriptionPaymentEventRejection,
) -> SubscriptionPaymentEventOutcome {
    SubscriptionPaymentEventOutcome::RecordedUnprocessed { order, reason }
}

fn terminal_event_matches(
    order: &SubscriptionOrderRecord,
    write: &SubscriptionPaymentEventWrite,
) -> bool {
    order.status == write.event_type.target_status()
        && (order.status != SubscriptionOrderStatus::Paid
            || order.trade_no.as_deref() == write.trade_no.as_deref())
}

fn order_status_label(event_type: SubscriptionPaymentEventType) -> String {
    match event_type {
        SubscriptionPaymentEventType::Succeeded => "paid",
        SubscriptionPaymentEventType::Failed => "failed",
        SubscriptionPaymentEventType::Expired => "expired",
    }
    .to_owned()
}

async fn transition_order(
    transaction: &DatabaseTransaction,
    order: &subscription_orders::Model,
    write: &SubscriptionPaymentEventWrite,
    received_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), SubscriptionRepositoryError> {
    let next_version = order
        .version
        .checked_add(1)
        .ok_or(SubscriptionRepositoryError::Invariant)?;
    let target = write.event_type.target_status();
    let paid_at = (target == SubscriptionOrderStatus::Paid).then_some(received_at);
    let closed_at = (target != SubscriptionOrderStatus::Paid).then_some(received_at);
    let update = subscription_orders::Entity::update_many()
        .filter(subscription_orders::Column::Id.eq(order.id))
        .filter(subscription_orders::Column::Version.eq(order.version))
        .filter(subscription_orders::Column::Status.is_in([
            SubscriptionOrderStatus::Created.code(),
            SubscriptionOrderStatus::Pending.code(),
        ]))
        .col_expr(
            subscription_orders::Column::Status,
            Expr::value(target.code()),
        )
        .col_expr(
            subscription_orders::Column::TradeNo,
            Expr::value(write.trade_no.clone().map(SensitiveString::from)),
        )
        .col_expr(
            subscription_orders::Column::PaymentMethod,
            Expr::value(write.payment_method.clone()),
        )
        .col_expr(
            subscription_orders::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(subscription_orders::Column::PaidAt, Expr::value(paid_at))
        .col_expr(
            subscription_orders::Column::ClosedAt,
            Expr::value(closed_at),
        )
        .col_expr(
            subscription_orders::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(SubscriptionRepositoryError::Conflict);
    }
    Ok(())
}

async fn mark_event_processed_at(
    transaction: &DatabaseTransaction,
    event_id: i64,
    processed_at: TimeDateTimeWithTimeZone,
) -> Result<(), SubscriptionRepositoryError> {
    let update = subscription_payment_events::Entity::update_many()
        .filter(subscription_payment_events::Column::Id.eq(event_id))
        .filter(subscription_payment_events::Column::ProcessedAt.is_null())
        .col_expr(
            subscription_payment_events::Column::ProcessedAt,
            Expr::value(processed_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    Ok(())
}

fn processing_time(received_at: TimeDateTimeWithTimeZone) -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::now_utc().max(received_at)
}

struct StoredPaymentEvent {
    order_database_id: i64,
    provider: String,
    provider_event_id: String,
    trade_no: Option<String>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    payment_method: Option<String>,
    event_type: SubscriptionPaymentEventType,
    signature_key_fingerprint: String,
    payload_sha256: String,
}

impl StoredPaymentEvent {
    fn try_from_model(
        model: subscription_payment_events::Model,
    ) -> Result<Self, SubscriptionRepositoryError> {
        if model.id <= 0 || model.order_id <= 0 {
            return Err(SubscriptionRepositoryError::Invariant);
        }
        Ok(Self {
            order_database_id: model.order_id,
            provider: model.provider,
            provider_event_id: model.provider_event_id.as_str().to_owned(),
            trade_no: model.trade_no.map(|value| value.as_str().to_owned()),
            amount_minor: model.amount_minor,
            currency: model.currency,
            payment_method: model.payment_method,
            event_type: SubscriptionPaymentEventType::try_from(model.event_type)
                .map_err(|_| SubscriptionRepositoryError::Invariant)?,
            signature_key_fingerprint: model.signature_key_fingerprint.as_str().to_owned(),
            payload_sha256: model.payload_sha256.as_str().to_owned(),
        })
    }

    fn matches(
        &self,
        order: &SubscriptionOrderRecord,
        write: &SubscriptionPaymentEventWrite,
    ) -> bool {
        self.order_database_id == order.database_id
            && self.provider == write.provider
            && self.provider_event_id == write.provider_event_id
            && self.trade_no == write.trade_no
            && self.amount_minor == write.amount_minor
            && self.currency == write.currency
            && self.payment_method == write.payment_method
            && self.event_type == write.event_type
            && self.signature_key_fingerprint == hex_digest(write.signature_key_fingerprint)
            && self.payload_sha256 == hex_digest(write.payload_sha256)
    }
}

async fn load_event_collision<C: ConnectionTrait>(
    connection: &C,
    write: &SubscriptionPaymentEventWrite,
) -> Result<Option<StoredPaymentEvent>, SubscriptionRepositoryError> {
    let models = subscription_payment_events::Entity::find()
        .filter(
            Condition::any()
                .add(
                    subscription_payment_events::Column::EventKey
                        .eq(write.event_id.persistence_key()),
                )
                .add(
                    Condition::all()
                        .add(
                            subscription_payment_events::Column::Provider
                                .eq(write.provider.clone()),
                        )
                        .add(
                            subscription_payment_events::Column::ProviderEventId
                                .eq(SensitiveString::from(write.provider_event_id.clone())),
                        ),
                ),
        )
        .all(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?;
    if models.len() > 1 {
        return Err(SubscriptionRepositoryError::Conflict);
    }
    models
        .into_iter()
        .next()
        .map(StoredPaymentEvent::try_from_model)
        .transpose()
}

async fn classify_event_collision<C: ConnectionTrait>(
    connection: &C,
    existing: StoredPaymentEvent,
    order: &SubscriptionOrderRecord,
    write: &SubscriptionPaymentEventWrite,
) -> Result<SubscriptionPaymentEventOutcome, SubscriptionRepositoryError> {
    if !existing.matches(order, write) {
        return Err(SubscriptionRepositoryError::Conflict);
    }
    let subscription = if order.status == SubscriptionOrderStatus::Paid {
        load_subscription_for_order_connection(connection, order).await?
    } else {
        None
    };
    Ok(SubscriptionPaymentEventOutcome::Existing {
        order: subscription_order_snapshot(order),
        subscription,
    })
}

async fn load_subscription_for_order_connection<C: ConnectionTrait>(
    connection: &C,
    order: &SubscriptionOrderRecord,
) -> Result<Option<UserSubscriptionRecord>, SubscriptionRepositoryError> {
    let subscription_id =
        UserSubscriptionId::from_persistence_key(&order.order_id.persistence_key())
            .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let Some((model, plan)) = load_subscription(connection, subscription_id).await? else {
        return Ok(None);
    };
    subscription_record(model, &plan_record(plan)?).map(Some)
}

fn subscription_order_snapshot(order: &SubscriptionOrderRecord) -> SubscriptionOrderRecord {
    SubscriptionOrderRecord {
        database_id: order.database_id,
        order_id: order.order_id,
        request_id: order.request_id,
        user_id: order.user_id,
        plan_id: order.plan_id,
        plan_version: order.plan_version,
        provider: order.provider.clone(),
        currency: order.currency.clone(),
        amount_minor: order.amount_minor,
        quota_amount: order.quota_amount,
        status: order.status,
        version: order.version,
        provider_order_id: order.provider_order_id.clone(),
        trade_no: order.trade_no.clone(),
        payment_method: order.payment_method.clone(),
        expires_at: order.expires_at,
        paid_at: order.paid_at,
        closed_at: order.closed_at,
        created_at: order.created_at,
        updated_at: order.updated_at,
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
    classify_plan_with_price(connection, model, write).await
}

async fn classify_plan_with_price<C: ConnectionTrait>(
    connection: &C,
    model: subscription_plans::Model,
    write: &SubscriptionPlanWrite,
) -> Result<SubscriptionPlanCreateOutcome, SubscriptionRepositoryError> {
    let record = plan_record(model)?;
    let Some(price) = load_plan_price(connection, record.database_id()).await? else {
        return Err(SubscriptionRepositoryError::Invariant);
    };
    if record.matches_write(write)
        && price.provider == write.price_provider
        && price.currency == write.price_currency
        && price.amount_minor == write.price_amount_minor
    {
        Ok(SubscriptionPlanCreateOutcome::Existing(record))
    } else {
        Err(SubscriptionRepositoryError::Conflict)
    }
}

async fn load_plan_price<C: ConnectionTrait>(
    connection: &C,
    plan_database_id: i64,
) -> Result<Option<subscription_plan_prices::Model>, SubscriptionRepositoryError> {
    subscription_plan_prices::Entity::find()
        .filter(subscription_plan_prices::Column::PlanId.eq(plan_database_id))
        .order_by_desc(subscription_plan_prices::Column::Id)
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

async fn load_order_by_request<C: ConnectionTrait>(
    connection: &C,
    user_id: UserId,
    request_id: SubscriptionOrderRequestId,
) -> Result<Option<subscription_orders::Model>, SubscriptionRepositoryError> {
    subscription_orders::Entity::find()
        .filter(subscription_orders::Column::UserId.eq(user_id.get()))
        .filter(subscription_orders::Column::IdempotencyKey.eq(request_id.persistence_key()))
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)
}

async fn load_order_by_key<C: ConnectionTrait>(
    connection: &C,
    order_id: SubscriptionOrderId,
) -> Result<Option<SubscriptionOrderRecord>, SubscriptionRepositoryError> {
    subscription_orders::Entity::find()
        .filter(subscription_orders::Column::OrderKey.eq(order_id.persistence_key()))
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
        .map(subscription_order_record)
        .transpose()
}

async fn load_order_by_database_id<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<Option<SubscriptionOrderRecord>, SubscriptionRepositoryError> {
    subscription_orders::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(|_| SubscriptionRepositoryError::Query)?
        .map(subscription_order_record)
        .transpose()
}

async fn recover_order_collision<C: ConnectionTrait>(
    connection: &C,
    create: &SubscriptionOrderCreate,
) -> Result<SubscriptionOrderCreateOutcome, SubscriptionRepositoryError> {
    let Some(model) = load_order_by_request(connection, create.user_id, create.request_id).await?
    else {
        return Err(SubscriptionRepositoryError::Conflict);
    };
    classify_order(model, create, None)
}

fn classify_order(
    model: subscription_orders::Model,
    create: &SubscriptionOrderCreate,
    quota: Option<Quota>,
) -> Result<SubscriptionOrderCreateOutcome, SubscriptionRepositoryError> {
    let record = subscription_order_record(model)?;
    if record.matches_create(create, quota) {
        Ok(SubscriptionOrderCreateOutcome::Existing(record))
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

async fn lock_order(
    transaction: &DatabaseTransaction,
    order_id: SubscriptionOrderId,
) -> Result<Option<subscription_orders::Model>, SubscriptionRepositoryError> {
    let key = order_id.persistence_key();
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，先执行同值更新取得写锁，再读取订单快照。
        let update = subscription_orders::Entity::update_many()
            .filter(subscription_orders::Column::OrderKey.eq(key.clone()))
            .col_expr(
                subscription_orders::Column::Version,
                Expr::col(subscription_orders::Column::Version).into(),
            )
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
        subscription_orders::Entity::find().filter(subscription_orders::Column::OrderKey.eq(key));
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

fn subscription_order_record(
    model: subscription_orders::Model,
) -> Result<SubscriptionOrderRecord, SubscriptionRepositoryError> {
    let order_id = SubscriptionOrderId::from_persistence_key(&model.order_key)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let request_id = SubscriptionOrderRequestId::from_persistence_key(&model.idempotency_key)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let user_id = UserId::new(model.user_id).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let plan_id = SubscriptionPlanId::from_persistence_key(model.plan_key.as_str())
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let plan_version =
        u64::try_from(model.plan_version).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let status = SubscriptionOrderStatus::try_from(model.status)
        .map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let version =
        u64::try_from(model.version).map_err(|_| SubscriptionRepositoryError::Invariant)?;
    let expires_at = optional_unix_seconds(model.expires_at)?;
    let paid_at = optional_unix_seconds(model.paid_at)?;
    let closed_at = optional_unix_seconds(model.closed_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    if model.id <= 0
        || model.plan_id <= 0
        || plan_version == 0
        || !valid_price_text(&model.provider, super::MAX_SUBSCRIPTION_PROVIDER_BYTES)
        || !valid_currency(&model.currency)
        || model.amount_minor <= 0
        || model.payment_method.as_deref().is_some_and(|value| {
            !valid_price_text(value, super::MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES)
        })
        || model.trade_no.as_ref().is_some_and(|value| {
            !valid_price_text(value.as_str(), super::MAX_SUBSCRIPTION_TRADE_NO_BYTES)
        })
        || quota_amount.is_zero()
        || version == 0
        || expires_at.is_some_and(|value| value <= created_at)
        || updated_at < created_at
    {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    Ok(SubscriptionOrderRecord {
        database_id: model.id,
        order_id,
        request_id,
        user_id,
        plan_id,
        plan_version,
        provider: model.provider,
        currency: model.currency,
        amount_minor: model.amount_minor,
        quota_amount,
        status,
        version,
        provider_order_id: model
            .provider_order_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        trade_no: model
            .trade_no
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        payment_method: model.payment_method,
        expires_at,
        paid_at,
        closed_at,
        created_at,
        updated_at,
    })
}

fn plan_price_record(
    model: subscription_plan_prices::Model,
) -> Result<SubscriptionPlanPriceRecord, SubscriptionRepositoryError> {
    if model.id <= 0
        || model.plan_id <= 0
        || model.amount_minor <= 0
        || !valid_price_text(&model.provider, super::MAX_SUBSCRIPTION_PROVIDER_BYTES)
        || !valid_currency(&model.currency)
    {
        return Err(SubscriptionRepositoryError::Invariant);
    }
    Ok(SubscriptionPlanPriceRecord {
        database_id: model.id,
        plan_database_id: model.plan_id,
        provider: model.provider,
        currency: model.currency,
        amount_minor: model.amount_minor,
        created_at: unix_seconds(model.created_at)?,
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

pub(super) fn page_query_limit(limit: usize) -> Result<u64, SubscriptionRepositoryError> {
    u64::try_from(limit)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(SubscriptionRepositoryError::Invariant)
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

fn hex_digest(value: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in value {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn is_subscription_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_user_subscriptions_subscription_key")
        || rendered.contains("user_subscriptions.subscription_key")
        || rendered.contains("Duplicate entry")
}

fn is_event_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_subscription_payment_events_event_key")
        || rendered.contains("uq_subscription_payment_events_provider_event")
        || rendered.contains("subscription_payment_events.event_key")
        || rendered.contains("subscription_payment_events.provider")
        || rendered.contains("Duplicate entry")
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_subscription_orders_order_key")
        || rendered.contains("uq_subscription_orders_user_idempotency_key")
        || rendered.contains("subscription_orders.order_key")
        || rendered.contains("subscription_orders.user_id")
        || rendered.contains("uq_subscription_plans_plan_key")
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
        SubscriptionRepositoryError::BindingConflict => "subscription_binding_conflict",
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
