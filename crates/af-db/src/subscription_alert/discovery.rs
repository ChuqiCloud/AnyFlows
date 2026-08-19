use af_domain::UserSubscriptionStatus;
use sea_orm::{
    ColumnTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, ExprTrait, OnConflict, Query, SimpleExpr},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{SensitiveString, subscription_balance_alert_events, user_subscriptions, users},
    notification::UserNotificationWrite,
};

use super::{
    ENABLED_USER_STATUS, STATUS_PENDING,
    storage::{begin, commit},
    types::{SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepositoryError},
};

/// 扫描当前 UTC 窗口内达到剩余额度边界的订阅。
pub(super) async fn enqueue_due(
    pool: &DatabasePool,
    threshold_percent: i16,
    now: TimeDateTimeWithTimeZone,
    limit: usize,
) -> Result<SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepositoryError> {
    let limit =
        u64::try_from(limit).map_err(|_| SubscriptionBalanceAlertRepositoryError::Invariant)?;
    let transaction = begin(pool).await?;
    let mut already_queued = Query::select();
    already_queued
        .expr(Expr::value(1_i16))
        .from(subscription_balance_alert_events::Entity)
        .and_where(
            Expr::col((
                subscription_balance_alert_events::Entity,
                subscription_balance_alert_events::Column::UserSubscriptionId,
            ))
            .equals((user_subscriptions::Entity, user_subscriptions::Column::Id)),
        )
        .and_where(
            Expr::col((
                subscription_balance_alert_events::Entity,
                subscription_balance_alert_events::Column::WindowStartedAt,
            ))
            .equals((
                user_subscriptions::Entity,
                user_subscriptions::Column::WindowStartedAt,
            )),
        );
    let quota_amount: SimpleExpr = Expr::col((
        user_subscriptions::Entity,
        user_subscriptions::Column::QuotaAmount,
    ))
    .into();
    let quota_used: SimpleExpr = Expr::col((
        user_subscriptions::Entity,
        user_subscriptions::Column::QuotaUsed,
    ))
    .into();
    let remaining = quota_amount.clone().sub(quota_used);
    let threshold = remaining_threshold_expression(quota_amount, threshold_percent);
    let candidates = user_subscriptions::Entity::find()
        .select_only()
        .column(user_subscriptions::Column::Id)
        .column(user_subscriptions::Column::SubscriptionKey)
        .column(user_subscriptions::Column::UserId)
        .column(user_subscriptions::Column::WindowStartedAt)
        .column(user_subscriptions::Column::WindowEndsAt)
        .column(user_subscriptions::Column::QuotaAmount)
        .column(user_subscriptions::Column::QuotaUsed)
        .join(JoinType::InnerJoin, user_subscriptions::Relation::User.def())
        .filter(user_subscriptions::Column::Status.eq(UserSubscriptionStatus::Active.code()))
        .filter(user_subscriptions::Column::WindowStartedAt.lte(now))
        .filter(user_subscriptions::Column::WindowEndsAt.gt(now))
        .filter(remaining.lte(threshold))
        // 在 LIMIT 前排除本窗口已有事件，避免首批订阅长期占满候选位。
        .filter(Expr::exists(already_queued.to_owned()).not())
        .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
        .filter(users::Column::DeletedAt.is_null())
        .filter(users::Column::Email.is_not_null())
        .filter(users::Column::Email.ne(""))
        .filter(users::Column::EmailUsageAlerts.eq(true))
        .order_by_asc(user_subscriptions::Column::WindowEndsAt)
        .order_by_asc(user_subscriptions::Column::Id)
        .limit(limit)
        .into_tuple::<(
            i64,
            SensitiveString,
            i64,
            TimeDateTimeWithTimeZone,
            TimeDateTimeWithTimeZone,
            i64,
            i64,
        )>()
        .all(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    for (
        user_subscription_id,
        subscription_key,
        user_id,
        window_started_at,
        window_ends_at,
        quota_amount,
        observed_quota_used,
    ) in &candidates
    {
        subscription_balance_alert_events::Entity::insert(
            subscription_balance_alert_events::ActiveModel {
                id: sea_orm::NotSet,
                user_subscription_id: Set(*user_subscription_id),
                user_id: Set(*user_id),
                window_started_at: Set(*window_started_at),
                window_ends_at: Set(*window_ends_at),
                threshold_percent: Set(threshold_percent),
                quota_amount: Set(*quota_amount),
                observed_quota_used: Set(*observed_quota_used),
                status: Set(STATUS_PENDING),
                attempt_count: Set(0),
                next_attempt_at: Set(now),
                lease_expires_at: Set(None),
                last_error_kind: Set(None),
                version: Set(1),
                sent_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            },
        )
        .on_conflict(subscription_alert_on_conflict())
        .exec_without_returning(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
        crate::notification::insert_queued(
            &transaction,
            &UserNotificationWrite::subscription_balance_alert(
                *user_id,
                *user_subscription_id,
                subscription_key.as_str().to_owned(),
                *window_started_at,
                *window_ends_at,
                *quota_amount,
                *observed_quota_used,
                threshold_percent,
                now,
            ),
        )
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    }
    commit(transaction).await?;
    Ok(SubscriptionBalanceAlertEnqueueReport {
        eligible: candidates.len(),
    })
}

pub(super) fn remaining_threshold_expression(
    quota_amount: SimpleExpr,
    threshold_percent: i16,
) -> SimpleExpr {
    let percent = i64::from(threshold_percent);
    let remainder = quota_amount.clone().modulo(100_i64);
    let whole_hundreds = quota_amount
        .sub(remainder.clone())
        .div(100_i64)
        .mul(percent);
    let scaled_remainder = remainder.mul(percent);
    let remainder_part = scaled_remainder
        .clone()
        .sub(scaled_remainder.modulo(100_i64))
        .div(100_i64);
    whole_hundreds.add(remainder_part)
}

pub(super) fn remaining_threshold_quota(
    quota_amount: i64,
    threshold_percent: i16,
) -> Result<i64, SubscriptionBalanceAlertRepositoryError> {
    if quota_amount <= 0 || !(1..=99).contains(&threshold_percent) {
        return Err(SubscriptionBalanceAlertRepositoryError::Invariant);
    }
    let percent = i64::from(threshold_percent);
    let whole = quota_amount
        .div_euclid(100)
        .checked_mul(percent)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    let remainder = quota_amount
        .rem_euclid(100)
        .checked_mul(percent)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?
        .div_euclid(100);
    whole
        .checked_add(remainder)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)
}

fn subscription_alert_on_conflict() -> OnConflict {
    OnConflict::columns([
        subscription_balance_alert_events::Column::UserSubscriptionId,
        subscription_balance_alert_events::Column::WindowStartedAt,
    ])
    // MySQL 需要显式冲突列生成合法的无变化更新语句。
    .do_nothing_on([
        subscription_balance_alert_events::Column::UserSubscriptionId,
        subscription_balance_alert_events::Column::WindowStartedAt,
    ])
    .to_owned()
}
