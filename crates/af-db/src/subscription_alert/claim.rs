use std::time::Duration;

use af_domain::{Quota, UserId, UserSubscriptionId, UserSubscriptionStatus};
use sea_orm::{
    ColumnTrait, Condition, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, MAX_BALANCE_ALERT_ATTEMPTS,
    entity::{subscription_balance_alert_events, subscription_plans, user_subscriptions, users},
    notification::{NotificationDeliveryState, NotificationKind, update_delivery_state},
};

use super::{
    CLAIM_CANDIDATE_LIMIT, DELIVERY_LEASE_SECONDS, ENABLED_USER_STATUS, STATUS_CANCELED,
    STATUS_FAILED, STATUS_PENDING, STATUS_SENDING,
    discovery::remaining_threshold_quota,
    storage::{begin, commit, rollback},
    types::{
        SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertDeliveryLease,
        SubscriptionBalanceAlertRepositoryError,
    },
};

/// 使用版本条件领取最早到期的一条订阅预警事件。
pub(super) async fn claim_next(
    pool: &DatabasePool,
    now: TimeDateTimeWithTimeZone,
) -> Result<SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertRepositoryError> {
    let candidates = subscription_balance_alert_events::Entity::find()
        .filter(Condition::all().add(due_condition(now)).add(
            subscription_balance_alert_events::Column::AttemptCount.lt(MAX_BALANCE_ALERT_ATTEMPTS),
        ))
        .filter(subscription_balance_alert_events::Column::WindowEndsAt.gt(now))
        .order_by_asc(subscription_balance_alert_events::Column::NextAttemptAt)
        .order_by_asc(subscription_balance_alert_events::Column::Id)
        .limit(CLAIM_CANDIDATE_LIMIT)
        .all(pool.connection())
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;

    for candidate in candidates {
        let transaction = begin(pool).await?;
        let next_attempt_count = candidate
            .attempt_count
            .checked_add(1)
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
        let next_version = candidate
            .version
            .checked_add(1)
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
        let result = subscription_balance_alert_events::Entity::update_many()
            .col_expr(
                subscription_balance_alert_events::Column::Status,
                Expr::value(STATUS_SENDING),
            )
            .col_expr(
                subscription_balance_alert_events::Column::AttemptCount,
                Expr::value(next_attempt_count),
            )
            .col_expr(
                subscription_balance_alert_events::Column::LeaseExpiresAt,
                Expr::value(Some(now + Duration::from_secs(DELIVERY_LEASE_SECONDS))),
            )
            .col_expr(
                subscription_balance_alert_events::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                subscription_balance_alert_events::Column::UpdatedAt,
                Expr::value(now),
            )
            .filter(subscription_balance_alert_events::Column::Id.eq(candidate.id))
            .filter(subscription_balance_alert_events::Column::Version.eq(candidate.version))
            .filter(due_condition(now))
            .filter(
                subscription_balance_alert_events::Column::AttemptCount
                    .lt(MAX_BALANCE_ALERT_ATTEMPTS),
            )
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        if result.rows_affected == 0 {
            rollback(transaction).await?;
            continue;
        }
        if result.rows_affected != 1 {
            return Err(SubscriptionBalanceAlertRepositoryError::Invariant);
        }
        let event = subscription_balance_alert_events::Entity::find_by_id(candidate.id)
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
        let subscription = user_subscriptions::Entity::find_by_id(event.user_subscription_id)
            .find_also_related(subscription_plans::Entity)
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        let user = users::Entity::find_by_id(event.user_id)
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        let (Some((subscription, Some(plan))), Some(user)) = (subscription, user) else {
            cancel_claimed(&transaction, &event, now).await?;
            commit(transaction).await?;
            return Ok(SubscriptionBalanceAlertClaimOutcome::Skipped);
        };
        if !valid_snapshot(&event, &subscription, now) {
            cancel_claimed(&transaction, &event, now).await?;
            commit(transaction).await?;
            return Ok(SubscriptionBalanceAlertClaimOutcome::Skipped);
        }
        let threshold_quota =
            remaining_threshold_quota(subscription.quota_amount, event.threshold_percent)?;
        let remaining_quota = subscription
            .quota_amount
            .checked_sub(subscription.quota_used)
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
        let eligible = remaining_quota <= threshold_quota
            && user.status == ENABLED_USER_STATUS
            && user.deleted_at.is_none()
            && user.email_usage_alerts
            && user.email.as_deref().is_some_and(valid_recipient);
        if !eligible {
            cancel_claimed(&transaction, &event, now).await?;
            commit(transaction).await?;
            return Ok(SubscriptionBalanceAlertClaimOutcome::Skipped);
        }
        let lease = SubscriptionBalanceAlertDeliveryLease {
            event_id: event.id,
            subscription_id: UserSubscriptionId::from_persistence_key(
                subscription.subscription_key.as_str(),
            )
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Invariant)?,
            user_id: UserId::new(event.user_id)
                .map_err(|_| SubscriptionBalanceAlertRepositoryError::Invariant)?,
            recipient: user
                .email
                .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?,
            username: user.username,
            plan_name: plan.name,
            quota_amount: Quota::new(subscription.quota_amount)
                .map_err(|_| SubscriptionBalanceAlertRepositoryError::Invariant)?,
            quota_used: Quota::new(subscription.quota_used)
                .map_err(|_| SubscriptionBalanceAlertRepositoryError::Invariant)?,
            window_ends_at: subscription.window_ends_at,
            threshold_percent: event.threshold_percent,
            attempt_count: event.attempt_count,
            version: event.version,
            notification_source_key: format!(
                "subscription:{}:{}",
                event.user_subscription_id,
                event.window_started_at.unix_timestamp()
            ),
        };
        commit(transaction).await?;
        return Ok(SubscriptionBalanceAlertClaimOutcome::Claimed(lease));
    }
    Ok(SubscriptionBalanceAlertClaimOutcome::Empty)
}

/// 取消窗口已经结束且当前不存在有效租约的事件。
pub(super) async fn cancel_expired(
    pool: &DatabasePool,
    now: TimeDateTimeWithTimeZone,
) -> Result<u64, SubscriptionBalanceAlertRepositoryError> {
    let transaction = begin(pool).await?;
    let candidates = subscription_balance_alert_events::Entity::find()
        .filter(subscription_balance_alert_events::Column::WindowEndsAt.lte(now))
        .filter(cancelable_condition(now))
        .all(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    let mut canceled = 0_u64;
    for event in candidates {
        let result = subscription_balance_alert_events::Entity::update_many()
            .col_expr(
                subscription_balance_alert_events::Column::Status,
                Expr::value(STATUS_CANCELED),
            )
            .col_expr(
                subscription_balance_alert_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                subscription_balance_alert_events::Column::Version,
                Expr::value(
                    event
                        .version
                        .checked_add(1)
                        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?,
                ),
            )
            .col_expr(
                subscription_balance_alert_events::Column::UpdatedAt,
                Expr::value(now),
            )
            .filter(subscription_balance_alert_events::Column::Id.eq(event.id))
            .filter(subscription_balance_alert_events::Column::Version.eq(event.version))
            .filter(cancelable_condition(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        if result.rows_affected == 0 {
            continue;
        }
        if result.rows_affected != 1 {
            return Err(SubscriptionBalanceAlertRepositoryError::Invariant);
        }
        sync_notification_state(
            &transaction,
            &subscription_notification_source_key(&event),
            NotificationDeliveryState::Canceled,
            event.attempt_count,
            now,
        )
        .await?;
        canceled = canceled
            .checked_add(1)
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    }
    commit(transaction).await?;
    Ok(canceled)
}

/// 将达到最大投递次数且没有有效租约的事件推进到失败终态。
pub(super) async fn fail_exhausted(
    pool: &DatabasePool,
    now: TimeDateTimeWithTimeZone,
) -> Result<u64, SubscriptionBalanceAlertRepositoryError> {
    let transaction = begin(pool).await?;
    let candidates = subscription_balance_alert_events::Entity::find()
        .filter(
            subscription_balance_alert_events::Column::AttemptCount.gte(MAX_BALANCE_ALERT_ATTEMPTS),
        )
        .filter(cancelable_condition(now))
        .all(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    let mut failed = 0_u64;
    for event in candidates {
        let result = subscription_balance_alert_events::Entity::update_many()
            .col_expr(
                subscription_balance_alert_events::Column::Status,
                Expr::value(STATUS_FAILED),
            )
            .col_expr(
                subscription_balance_alert_events::Column::LeaseExpiresAt,
                Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
            )
            .col_expr(
                subscription_balance_alert_events::Column::Version,
                Expr::value(
                    event
                        .version
                        .checked_add(1)
                        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?,
                ),
            )
            .col_expr(
                subscription_balance_alert_events::Column::UpdatedAt,
                Expr::value(now),
            )
            .filter(subscription_balance_alert_events::Column::Id.eq(event.id))
            .filter(subscription_balance_alert_events::Column::Version.eq(event.version))
            .filter(cancelable_condition(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        if result.rows_affected == 0 {
            continue;
        }
        if result.rows_affected != 1 {
            return Err(SubscriptionBalanceAlertRepositoryError::Invariant);
        }
        sync_notification_state(
            &transaction,
            &subscription_notification_source_key(&event),
            NotificationDeliveryState::Failed,
            event.attempt_count,
            now,
        )
        .await?;
        failed = failed
            .checked_add(1)
            .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    }
    commit(transaction).await?;
    Ok(failed)
}

fn valid_snapshot(
    event: &subscription_balance_alert_events::Model,
    subscription: &user_subscriptions::Model,
    now: TimeDateTimeWithTimeZone,
) -> bool {
    subscription.user_id == event.user_id
        && subscription.status == UserSubscriptionStatus::Active.code()
        && subscription.window_started_at == event.window_started_at
        && subscription.window_ends_at == event.window_ends_at
        && subscription.window_started_at <= now
        && subscription.window_ends_at > now
        && subscription.quota_amount == event.quota_amount
        && subscription.quota_used >= event.observed_quota_used
        && (0..=subscription.quota_amount).contains(&subscription.quota_used)
        && (1..=99).contains(&event.threshold_percent)
}

fn due_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(subscription_balance_alert_events::Column::Status.eq(STATUS_PENDING))
                .add(subscription_balance_alert_events::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(subscription_balance_alert_events::Column::Status.eq(STATUS_SENDING))
                .add(subscription_balance_alert_events::Column::LeaseExpiresAt.lte(now)),
        )
}

fn cancelable_condition(now: TimeDateTimeWithTimeZone) -> Condition {
    Condition::any()
        .add(subscription_balance_alert_events::Column::Status.eq(STATUS_PENDING))
        .add(
            Condition::all()
                .add(subscription_balance_alert_events::Column::Status.eq(STATUS_SENDING))
                .add(subscription_balance_alert_events::Column::LeaseExpiresAt.lte(now)),
        )
}

async fn cancel_claimed(
    transaction: &DatabaseTransaction,
    event: &subscription_balance_alert_events::Model,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), SubscriptionBalanceAlertRepositoryError> {
    let next_version = event
        .version
        .checked_add(1)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    let result = subscription_balance_alert_events::Entity::update_many()
        .col_expr(
            subscription_balance_alert_events::Column::Status,
            Expr::value(STATUS_CANCELED),
        )
        .col_expr(
            subscription_balance_alert_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            subscription_balance_alert_events::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            subscription_balance_alert_events::Column::UpdatedAt,
            Expr::value(now),
        )
        .filter(subscription_balance_alert_events::Column::Id.eq(event.id))
        .filter(subscription_balance_alert_events::Column::Status.eq(STATUS_SENDING))
        .filter(subscription_balance_alert_events::Column::Version.eq(event.version))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    if result.rows_affected == 1 {
        sync_notification_state(
            transaction,
            &subscription_notification_source_key(event),
            NotificationDeliveryState::Canceled,
            event.attempt_count,
            now,
        )
        .await?;
        Ok(())
    } else {
        Err(SubscriptionBalanceAlertRepositoryError::Invariant)
    }
}

fn subscription_notification_source_key(
    event: &subscription_balance_alert_events::Model,
) -> String {
    format!(
        "subscription:{}:{}",
        event.user_subscription_id,
        event.window_started_at.unix_timestamp()
    )
}

async fn sync_notification_state(
    transaction: &DatabaseTransaction,
    source_key: &str,
    state: NotificationDeliveryState,
    attempts: i16,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), SubscriptionBalanceAlertRepositoryError> {
    update_delivery_state(
        transaction,
        NotificationKind::SubscriptionBalanceAlert,
        source_key,
        state,
        attempts,
        updated_at,
    )
    .await
    .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)
}

fn valid_recipient(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 320
        && value.trim() == value
        && value.contains('@')
        && !value.chars().any(char::is_control)
}
