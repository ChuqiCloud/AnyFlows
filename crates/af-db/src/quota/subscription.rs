use af_domain::{Quota, UserSubscriptionStatus};
use rust_decimal::{Decimal, prelude::ToPrimitive as _};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, EntityTrait,
    JoinType, QueryFilter, QueryOrder, QueryResult, QuerySelect, QueryTrait, RelationTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Func, LockType},
};

use crate::{
    entity::{
        BillingReservationKey, billing_reservations, billing_subscription_reservations,
        user_subscriptions, users,
    },
    subscription::monotonic_updated_at,
};

use super::{
    quota_from_db,
    state::{
        QuotaFundingSource, QuotaRepositoryError, QuotaReservationKind, QuotaReservationStatus,
        ReservationState, UserQuotaState,
    },
};

const MAX_SUBSCRIPTION_CANDIDATES: u64 = 100;
const COMMITTED_QUOTA_ALIAS: &str = "reserved_quota";

/// 单次请求绑定的订阅窗口预留快照。
#[derive(Clone)]
pub(super) struct SubscriptionAllocation {
    key: BillingReservationKey,
    subscription_database_id: i64,
    window_started_at: TimeDateTimeWithTimeZone,
    window_ends_at: TimeDateTimeWithTimeZone,
    reserved_quota: i64,
    subscription_actual_quota: Option<i64>,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

/// 结算时固化的订阅与钱包分摊；重放不得重新选择资金来源。
#[derive(Clone, Copy)]
pub(super) struct SubscriptionSettlement {
    subscription_database_id: i64,
    window_started_at: TimeDateTimeWithTimeZone,
    window_ends_at: TimeDateTimeWithTimeZone,
    subscription_updated_at: TimeDateTimeWithTimeZone,
    subscription_actual_quota: i64,
    wallet_actual_quota: i64,
}

/// 按“最早结束、最小主键”顺序为请求选择一项能完整承接预估额度的订阅。
pub(super) async fn try_reserve(
    transaction: &DatabaseTransaction,
    reservation_key: &str,
    parent: &ReservationState,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<bool, QuotaRepositoryError> {
    let mut query = user_subscriptions::Entity::find()
        .filter(user_subscriptions::Column::UserId.eq(parent.user_id))
        .filter(user_subscriptions::Column::Status.eq(UserSubscriptionStatus::Active.code()))
        .filter(user_subscriptions::Column::WindowStartedAt.lte(now))
        .filter(user_subscriptions::Column::WindowEndsAt.gt(now))
        .order_by_asc(user_subscriptions::Column::WindowEndsAt)
        .order_by_asc(user_subscriptions::Column::Id)
        .limit(MAX_SUBSCRIPTION_CANDIDATES + 1);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let candidates = query
        .all(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if candidates.len() > MAX_SUBSCRIPTION_CANDIDATES as usize {
        return Err(QuotaRepositoryError::Invariant);
    }

    for candidate in candidates {
        validate_subscription(&candidate, parent.user_id, None)?;
        let committed = active_commitment(transaction, &candidate).await?;
        let available = candidate
            .quota_amount
            .checked_sub(candidate.quota_used)
            .and_then(|value| value.checked_sub(committed))
            .ok_or(QuotaRepositoryError::Invariant)?;
        if available < amount.units() {
            continue;
        }
        persist_allocation(transaction, reservation_key, &candidate, amount, now).await?;
        return Ok(true);
    }
    Ok(false)
}

/// 读取并校验父预留的资金来源快照。
pub(super) async fn load_allocation<C>(
    connection: &C,
    reservation_key: &str,
    parent: &ReservationState,
) -> Result<Option<SubscriptionAllocation>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    let key = parse_key(reservation_key)?;
    let model = billing_subscription_reservations::Entity::find_by_id(key.clone())
        .one(connection)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    match (parent.funding_source, model) {
        (QuotaFundingSource::Wallet, None) => Ok(None),
        (QuotaFundingSource::Subscription, Some(model)) => {
            let allocation = SubscriptionAllocation {
                key,
                subscription_database_id: model.user_subscription_id,
                window_started_at: model.window_started_at,
                window_ends_at: model.window_ends_at,
                reserved_quota: model.reserved_quota,
                subscription_actual_quota: model.subscription_actual_quota,
                created_at: model.created_at,
                updated_at: model.updated_at,
            };
            validate_allocation(&allocation, parent)?;
            Ok(Some(allocation))
        }
        (QuotaFundingSource::Wallet, Some(_)) | (QuotaFundingSource::Subscription, None) => {
            Err(QuotaRepositoryError::Invariant)
        }
    }
}

/// 在父预留进入待结算前冻结实际订阅分摊，超出订阅容量的部分落到钱包。
pub(super) async fn prepare_settlement(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    allocation: &SubscriptionAllocation,
    actual: Quota,
) -> Result<SubscriptionSettlement, QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::Reserved {
        return Err(QuotaRepositoryError::Invariant);
    }
    let subscription = lock_subscription(transaction, allocation, parent.user_id).await?;
    let committed = active_commitment(transaction, &subscription).await?;
    let other_commitment = committed
        .checked_sub(allocation.reserved_quota)
        .ok_or(QuotaRepositoryError::Invariant)?;
    let capacity = subscription
        .quota_amount
        .checked_sub(subscription.quota_used)
        .and_then(|value| value.checked_sub(other_commitment))
        .ok_or(QuotaRepositoryError::Invariant)?;
    if capacity < allocation.reserved_quota {
        return Err(QuotaRepositoryError::Invariant);
    }
    let subscription_actual_quota = actual.units().min(capacity);
    let wallet_actual_quota = actual
        .units()
        .checked_sub(subscription_actual_quota)
        .ok_or(QuotaRepositoryError::Invariant)?;
    Ok(SubscriptionSettlement {
        subscription_database_id: allocation.subscription_database_id,
        window_started_at: allocation.window_started_at,
        window_ends_at: allocation.window_ends_at,
        subscription_updated_at: subscription.updated_at,
        subscription_actual_quota,
        wallet_actual_quota,
    })
}

/// 固化首次结算计算出的订阅实际分摊；父预留和子快照在同一事务提交。
pub(super) async fn persist_settlement(
    transaction: &DatabaseTransaction,
    allocation: &SubscriptionAllocation,
    settlement: SubscriptionSettlement,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let result = billing_subscription_reservations::Entity::update_many()
        .filter(
            billing_subscription_reservations::Column::IdempotencyKey.eq(allocation.key.clone()),
        )
        .filter(
            billing_subscription_reservations::Column::UserSubscriptionId
                .eq(allocation.subscription_database_id),
        )
        .filter(
            billing_subscription_reservations::Column::ReservedQuota.eq(allocation.reserved_quota),
        )
        .filter(billing_subscription_reservations::Column::SubscriptionActualQuota.is_null())
        .col_expr(
            billing_subscription_reservations::Column::SubscriptionActualQuota,
            Expr::value(settlement.subscription_actual_quota),
        )
        .col_expr(
            billing_subscription_reservations::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

/// 从已经固化的待结算快照恢复同一分摊，不因重试时余额变化而改写来源。
pub(super) async fn replay_settlement(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    allocation: &SubscriptionAllocation,
    actual: Quota,
) -> Result<SubscriptionSettlement, QuotaRepositoryError> {
    if parent.status != QuotaReservationStatus::SettlementPending {
        return Err(QuotaRepositoryError::Invariant);
    }
    let subscription = lock_subscription(transaction, allocation, parent.user_id).await?;
    let committed = active_commitment(transaction, &subscription).await?;
    let available_shape = subscription
        .quota_used
        .checked_add(committed)
        .is_some_and(|value| value <= subscription.quota_amount);
    let subscription_actual_quota = allocation
        .subscription_actual_quota
        .ok_or(QuotaRepositoryError::Invariant)?;
    if !available_shape || subscription_actual_quota > actual.units() {
        return Err(QuotaRepositoryError::Invariant);
    }
    let wallet_actual_quota = actual
        .units()
        .checked_sub(subscription_actual_quota)
        .ok_or(QuotaRepositoryError::Invariant)?;
    Ok(SubscriptionSettlement {
        subscription_database_id: allocation.subscription_database_id,
        window_started_at: allocation.window_started_at,
        window_ends_at: allocation.window_ends_at,
        subscription_updated_at: subscription.updated_at,
        subscription_actual_quota,
        wallet_actual_quota,
    })
}

/// 退款前锁定原订阅窗口，确保释放虚拟预留与周期推进互斥。
pub(super) async fn lock_for_refund(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    allocation: &SubscriptionAllocation,
) -> Result<(), QuotaRepositoryError> {
    let subscription = lock_subscription(transaction, allocation, parent.user_id).await?;
    let committed = active_commitment(transaction, &subscription).await?;
    if committed < allocation.reserved_quota {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

/// 在同一保存点内累计订阅实际用量、钱包溢出和用户总用量。
pub(super) async fn apply_settlement(
    transaction: &DatabaseTransaction,
    parent: &ReservationState,
    user: &UserQuotaState,
    actual: Quota,
    settlement: SubscriptionSettlement,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    if user.used_quota > i64::MAX - actual.units() || user.request_count == i64::MAX {
        return Err(QuotaRepositoryError::Invariant);
    }
    if user.quota < settlement.wallet_actual_quota {
        return Err(QuotaRepositoryError::UserQuotaInsufficient);
    }

    let subscription_updated_at = monotonic_updated_at(settlement.subscription_updated_at, now);
    let user_result = users::Entity::update_many()
        .filter(users::Column::Id.eq(parent.user_id))
        .filter(users::Column::Quota.gte(settlement.wallet_actual_quota))
        .filter(users::Column::FrozenQuota.gte(0_i64))
        .filter(users::Column::UsedQuota.gte(0_i64))
        .filter(users::Column::UsedQuota.lte(i64::MAX - actual.units()))
        .filter(users::Column::RequestCount.gte(0_i64))
        .filter(users::Column::RequestCount.lt(i64::MAX))
        .col_expr(
            users::Column::Quota,
            Expr::col(users::Column::Quota).sub(settlement.wallet_actual_quota),
        )
        .col_expr(
            users::Column::UsedQuota,
            Expr::col(users::Column::UsedQuota).add(actual.units()),
        )
        .col_expr(
            users::Column::RequestCount,
            Expr::col(users::Column::RequestCount).add(1_i64),
        )
        .col_expr(users::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if user_result.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }

    let subscription_result = user_subscriptions::Entity::update_many()
        .filter(user_subscriptions::Column::Id.eq(settlement.subscription_database_id))
        .filter(user_subscriptions::Column::UserId.eq(parent.user_id))
        .filter(user_subscriptions::Column::Status.is_in([
            UserSubscriptionStatus::Active.code(),
            UserSubscriptionStatus::Suspended.code(),
            UserSubscriptionStatus::Canceled.code(),
        ]))
        .filter(user_subscriptions::Column::WindowStartedAt.eq(settlement.window_started_at))
        .filter(user_subscriptions::Column::WindowEndsAt.eq(settlement.window_ends_at))
        .filter(user_subscriptions::Column::QuotaUsed.gte(0_i64))
        .filter(
            Expr::col(user_subscriptions::Column::QuotaUsed).lte(
                Expr::col(user_subscriptions::Column::QuotaAmount)
                    .sub(settlement.subscription_actual_quota),
            ),
        )
        .col_expr(
            user_subscriptions::Column::QuotaUsed,
            Expr::col(user_subscriptions::Column::QuotaUsed)
                .add(settlement.subscription_actual_quota),
        )
        .col_expr(
            user_subscriptions::Column::UpdatedAt,
            Expr::value(subscription_updated_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if subscription_result.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

/// 判断订阅是否仍有 Reserved 或 SettlementPending 请求；周期变更必须据此失败关闭。
pub(crate) async fn has_in_flight_reservation<C>(
    connection: &C,
    subscription_database_id: i64,
) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    let query = billing_subscription_reservations::Entity::find()
        .select_only()
        .column(billing_subscription_reservations::Column::IdempotencyKey)
        .join(
            JoinType::InnerJoin,
            billing_subscription_reservations::Relation::BillingReservation.def(),
        )
        .filter(
            billing_subscription_reservations::Column::UserSubscriptionId
                .eq(subscription_database_id),
        )
        .filter(billing_reservations::Column::Status.is_in([
            QuotaReservationStatus::Reserved.code(),
            QuotaReservationStatus::SettlementPending.code(),
        ]))
        .limit(1)
        .into_query();
    Ok(connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .is_some())
}

async fn persist_allocation(
    transaction: &DatabaseTransaction,
    reservation_key: &str,
    subscription: &user_subscriptions::Model,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), QuotaRepositoryError> {
    let key = parse_key(reservation_key)?;
    let source = billing_reservations::Entity::update_many()
        .filter(billing_reservations::Column::IdempotencyKey.eq(key.clone()))
        .filter(billing_reservations::Column::Status.eq(QuotaReservationStatus::Reserved.code()))
        .filter(
            billing_reservations::Column::ReservationKind.eq(QuotaReservationKind::Request.code()),
        )
        .filter(billing_reservations::Column::FundingSource.eq(QuotaFundingSource::Wallet.code()))
        .filter(billing_reservations::Column::ReservedQuota.eq(amount.units()))
        .col_expr(
            billing_reservations::Column::FundingSource,
            Expr::value(QuotaFundingSource::Subscription.code()),
        )
        .exec(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if source.rows_affected != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    billing_subscription_reservations::ActiveModel {
        idempotency_key: Set(key),
        user_subscription_id: Set(subscription.id),
        window_started_at: Set(subscription.window_started_at),
        window_ends_at: Set(subscription.window_ends_at),
        reserved_quota: Set(amount.units()),
        subscription_actual_quota: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(transaction)
    .await
    .map_err(|_| QuotaRepositoryError::Query)?;
    Ok(())
}

async fn lock_subscription(
    transaction: &DatabaseTransaction,
    allocation: &SubscriptionAllocation,
    user_id: i64,
) -> Result<user_subscriptions::Model, QuotaRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = user_subscriptions::Entity::update_many()
            .filter(user_subscriptions::Column::Id.eq(allocation.subscription_database_id))
            .col_expr(
                user_subscriptions::Column::Version,
                Expr::col(user_subscriptions::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(QuotaRepositoryError::Invariant);
        }
    }
    let mut query = user_subscriptions::Entity::find_by_id(allocation.subscription_database_id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let subscription = query
        .one(transaction)
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .ok_or(QuotaRepositoryError::Invariant)?;
    validate_subscription(&subscription, user_id, Some(allocation))?;
    Ok(subscription)
}

fn validate_subscription(
    subscription: &user_subscriptions::Model,
    user_id: i64,
    allocation: Option<&SubscriptionAllocation>,
) -> Result<(), QuotaRepositoryError> {
    let status = UserSubscriptionStatus::try_from(subscription.status)
        .map_err(|_| QuotaRepositoryError::Invariant)?;
    if subscription.id <= 0
        || subscription.user_id != user_id
        || status == UserSubscriptionStatus::Expired
        || subscription.quota_amount <= 0
        || subscription.quota_used < 0
        || subscription.quota_used > subscription.quota_amount
        || subscription.window_ends_at <= subscription.window_started_at
        || subscription.version <= 0
        || allocation.is_some_and(|snapshot| {
            subscription.id != snapshot.subscription_database_id
                || subscription.window_started_at != snapshot.window_started_at
                || subscription.window_ends_at != snapshot.window_ends_at
        })
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

async fn active_commitment(
    transaction: &DatabaseTransaction,
    subscription: &user_subscriptions::Model,
) -> Result<i64, QuotaRepositoryError> {
    if has_invalid_active_allocation(transaction, subscription).await? {
        return Err(QuotaRepositoryError::Invariant);
    }
    let status = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::Status,
    ));
    let committed = Expr::case(
        status.eq(QuotaReservationStatus::Reserved.code()),
        Expr::col((
            billing_subscription_reservations::Entity,
            billing_subscription_reservations::Column::ReservedQuota,
        )),
    )
    .finally(Expr::col((
        billing_subscription_reservations::Entity,
        billing_subscription_reservations::Column::SubscriptionActualQuota,
    )));
    let query = billing_subscription_reservations::Entity::find()
        .select_only()
        .expr_as(
            Func::sum(committed),
            billing_subscription_reservations::Column::ReservedQuota,
        )
        .join(
            JoinType::InnerJoin,
            billing_subscription_reservations::Relation::BillingReservation.def(),
        )
        .filter(billing_subscription_reservations::Column::UserSubscriptionId.eq(subscription.id))
        .filter(billing_reservations::Column::Status.is_in([
            QuotaReservationStatus::Reserved.code(),
            QuotaReservationStatus::SettlementPending.code(),
        ]))
        .into_query();
    let backend = transaction.get_database_backend();
    let result = transaction
        .query_one(backend.build(&query))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .ok_or(QuotaRepositoryError::Invariant)?;
    read_sum(&result, COMMITTED_QUOTA_ALIAS, backend)
}

async fn has_invalid_active_allocation(
    transaction: &DatabaseTransaction,
    subscription: &user_subscriptions::Model,
) -> Result<bool, QuotaRepositoryError> {
    let parent_status = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::Status,
    ));
    let child_actual = Expr::col((
        billing_subscription_reservations::Entity,
        billing_subscription_reservations::Column::SubscriptionActualQuota,
    ));
    let parent_actual = Expr::col((
        billing_reservations::Entity,
        billing_reservations::Column::ActualQuota,
    ));
    let invalid = Condition::any()
        .add(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::ReservationKind,
            ))
            .ne(QuotaReservationKind::Request.code()),
        )
        .add(
            Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::FundingSource,
            ))
            .ne(QuotaFundingSource::Subscription.code()),
        )
        .add(
            Expr::col((
                billing_subscription_reservations::Entity,
                billing_subscription_reservations::Column::ReservedQuota,
            ))
            .ne(Expr::col((
                billing_reservations::Entity,
                billing_reservations::Column::ReservedQuota,
            ))),
        )
        .add(
            Expr::col((
                billing_subscription_reservations::Entity,
                billing_subscription_reservations::Column::WindowStartedAt,
            ))
            .ne(subscription.window_started_at),
        )
        .add(
            Expr::col((
                billing_subscription_reservations::Entity,
                billing_subscription_reservations::Column::WindowEndsAt,
            ))
            .ne(subscription.window_ends_at),
        )
        .add(
            parent_status
                .clone()
                .eq(QuotaReservationStatus::Reserved.code())
                .and(child_actual.clone().is_not_null()),
        )
        .add(
            parent_status
                .eq(QuotaReservationStatus::SettlementPending.code())
                .and(
                    child_actual
                        .clone()
                        .is_null()
                        .or(parent_actual.clone().is_null())
                        .or(child_actual.gt(parent_actual)),
                ),
        );
    let query = billing_subscription_reservations::Entity::find()
        .select_only()
        .column(billing_subscription_reservations::Column::IdempotencyKey)
        .join(
            JoinType::InnerJoin,
            billing_subscription_reservations::Relation::BillingReservation.def(),
        )
        .filter(billing_subscription_reservations::Column::UserSubscriptionId.eq(subscription.id))
        .filter(billing_reservations::Column::Status.is_in([
            QuotaReservationStatus::Reserved.code(),
            QuotaReservationStatus::SettlementPending.code(),
        ]))
        .filter(invalid)
        .limit(1)
        .into_query();
    Ok(transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?
        .is_some())
}

fn validate_allocation(
    allocation: &SubscriptionAllocation,
    parent: &ReservationState,
) -> Result<(), QuotaRepositoryError> {
    let actual_shape = match parent.status {
        QuotaReservationStatus::Reserved | QuotaReservationStatus::Refunded => {
            allocation.subscription_actual_quota.is_none()
        }
        QuotaReservationStatus::SettlementPending | QuotaReservationStatus::Settled => allocation
            .subscription_actual_quota
            .zip(parent.actual_quota)
            .is_some_and(|(subscription, total)| subscription >= 0 && subscription <= total),
    };
    if allocation.subscription_database_id <= 0
        || allocation.reserved_quota != parent.reserved_quota
        || allocation.window_started_at.unix_timestamp() < 0
        || allocation.window_ends_at <= allocation.window_started_at
        || allocation.created_at < allocation.window_started_at
        || allocation.created_at >= allocation.window_ends_at
        || allocation.updated_at < allocation.created_at
        || !actual_shape
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

fn parse_key(value: &str) -> Result<BillingReservationKey, QuotaRepositoryError> {
    BillingReservationKey::parse(value).map_err(|_| QuotaRepositoryError::Invariant)
}

pub(super) fn read_sum(
    result: &QueryResult,
    column: &str,
    backend: DbBackend,
) -> Result<i64, QuotaRepositoryError> {
    let value = match backend {
        DbBackend::Sqlite => result
            .try_get::<Option<i64>>("", column)
            .map(|value| value.unwrap_or(0))
            .ok(),
        DbBackend::Postgres | DbBackend::MySql => result
            .try_get::<Option<Decimal>>("", column)
            .ok()
            .and_then(|value| match value {
                None => Some(0),
                Some(value) if value.fract().is_zero() => value.to_i64(),
                Some(_) => None,
            }),
    }
    .ok_or(QuotaRepositoryError::Invariant)?;
    quota_from_db(value).map(Quota::units)
}
