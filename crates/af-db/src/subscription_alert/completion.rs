use std::time::Duration;

use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, MAX_BALANCE_ALERT_ATTEMPTS,
    balance_alert::{BalanceAlertCompletionOutcome, BalanceAlertDeliveryFailureKind},
    entity::subscription_balance_alert_events,
    notification::{NotificationDeliveryState, NotificationKind, update_delivery_state},
};

use super::{
    STATUS_FAILED, STATUS_PENDING, STATUS_SENDING, STATUS_SENT,
    storage::{begin, commit, rollback},
    types::{SubscriptionBalanceAlertDeliveryLease, SubscriptionBalanceAlertRepositoryError},
};

/// 使用领取版本完成一次成功投递。
pub(super) async fn mark_sent(
    pool: &DatabasePool,
    lease: &SubscriptionBalanceAlertDeliveryLease,
    sent_at: TimeDateTimeWithTimeZone,
) -> Result<BalanceAlertCompletionOutcome, SubscriptionBalanceAlertRepositoryError> {
    let next_version = lease
        .version
        .checked_add(1)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    let transaction = begin(pool).await?;
    let result = subscription_balance_alert_events::Entity::update_many()
        .col_expr(
            subscription_balance_alert_events::Column::Status,
            Expr::value(STATUS_SENT),
        )
        .col_expr(
            subscription_balance_alert_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            subscription_balance_alert_events::Column::SentAt,
            Expr::value(Some(sent_at)),
        )
        .col_expr(
            subscription_balance_alert_events::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            subscription_balance_alert_events::Column::UpdatedAt,
            Expr::value(sent_at),
        )
        .filter(subscription_balance_alert_events::Column::Id.eq(lease.event_id))
        .filter(subscription_balance_alert_events::Column::Status.eq(STATUS_SENDING))
        .filter(subscription_balance_alert_events::Column::Version.eq(lease.version))
        .exec(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    match completion_outcome(result.rows_affected)? {
        BalanceAlertCompletionOutcome::Stale => {
            rollback(transaction).await?;
            Ok(BalanceAlertCompletionOutcome::Stale)
        }
        BalanceAlertCompletionOutcome::Completed => {
            sync_notification_state(
                &transaction,
                &lease.notification_source_key,
                NotificationDeliveryState::Accepted,
                lease.attempt_count(),
                sent_at,
            )
            .await?;
            commit(transaction).await?;
            Ok(BalanceAlertCompletionOutcome::Completed)
        }
    }
}

/// 使用领取版本记录一次失败，并推进到退避重试或终态失败。
pub(super) async fn record_failure(
    pool: &DatabasePool,
    lease: &SubscriptionBalanceAlertDeliveryLease,
    failure: BalanceAlertDeliveryFailureKind,
    failed_at: TimeDateTimeWithTimeZone,
) -> Result<BalanceAlertCompletionOutcome, SubscriptionBalanceAlertRepositoryError> {
    let terminal = matches!(failure, BalanceAlertDeliveryFailureKind::Configuration)
        || lease.attempt_count >= MAX_BALANCE_ALERT_ATTEMPTS;
    let status = if terminal {
        STATUS_FAILED
    } else {
        STATUS_PENDING
    };
    let next_attempt_at = if terminal {
        failed_at
    } else {
        failed_at + retry_delay(lease.attempt_count)?
    };
    let next_version = lease
        .version
        .checked_add(1)
        .ok_or(SubscriptionBalanceAlertRepositoryError::Invariant)?;
    let transaction = begin(pool).await?;
    let result = subscription_balance_alert_events::Entity::update_many()
        .col_expr(
            subscription_balance_alert_events::Column::Status,
            Expr::value(status),
        )
        .col_expr(
            subscription_balance_alert_events::Column::NextAttemptAt,
            Expr::value(next_attempt_at),
        )
        .col_expr(
            subscription_balance_alert_events::Column::LeaseExpiresAt,
            Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
        )
        .col_expr(
            subscription_balance_alert_events::Column::LastErrorKind,
            Expr::value(Some(failure_code(failure))),
        )
        .col_expr(
            subscription_balance_alert_events::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            subscription_balance_alert_events::Column::UpdatedAt,
            Expr::value(failed_at),
        )
        .filter(subscription_balance_alert_events::Column::Id.eq(lease.event_id))
        .filter(subscription_balance_alert_events::Column::Status.eq(STATUS_SENDING))
        .filter(subscription_balance_alert_events::Column::Version.eq(lease.version))
        .exec(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| SubscriptionBalanceAlertRepositoryError::Query)?;
    match completion_outcome(result.rows_affected)? {
        BalanceAlertCompletionOutcome::Stale => {
            rollback(transaction).await?;
            Ok(BalanceAlertCompletionOutcome::Stale)
        }
        BalanceAlertCompletionOutcome::Completed => {
            sync_notification_state(
                &transaction,
                &lease.notification_source_key,
                if terminal {
                    NotificationDeliveryState::Failed
                } else {
                    NotificationDeliveryState::Queued
                },
                lease.attempt_count(),
                failed_at,
            )
            .await?;
            commit(transaction).await?;
            Ok(BalanceAlertCompletionOutcome::Completed)
        }
    }
}

fn retry_delay(attempt_count: i16) -> Result<Duration, SubscriptionBalanceAlertRepositoryError> {
    let seconds = match attempt_count {
        1 => 60,
        2 => 300,
        3 => 1_800,
        4 => 7_200,
        _ => return Err(SubscriptionBalanceAlertRepositoryError::Invariant),
    };
    Ok(Duration::from_secs(seconds))
}

fn failure_code(failure: BalanceAlertDeliveryFailureKind) -> i16 {
    match failure {
        BalanceAlertDeliveryFailureKind::Timeout => 1,
        BalanceAlertDeliveryFailureKind::Transport => 2,
        BalanceAlertDeliveryFailureKind::Configuration => 3,
    }
}

fn completion_outcome(
    rows_affected: u64,
) -> Result<BalanceAlertCompletionOutcome, SubscriptionBalanceAlertRepositoryError> {
    match rows_affected {
        0 => Ok(BalanceAlertCompletionOutcome::Stale),
        1 => Ok(BalanceAlertCompletionOutcome::Completed),
        _ => Err(SubscriptionBalanceAlertRepositoryError::Invariant),
    }
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
