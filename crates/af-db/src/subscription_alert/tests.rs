use std::{error::Error, time::Duration};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};

use super::*;
use crate::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions,
    balance_alert::{BalanceAlertCompletionOutcome, BalanceAlertDeliveryFailureKind},
    entity::{
        SensitiveString, subscription_balance_alert_events, subscription_plans,
        user_notification_events, user_subscriptions, users,
    },
};

use super::discovery::remaining_threshold_quota;

#[test]
fn percentage_threshold_uses_floor_without_overflow() {
    assert_eq!(remaining_threshold_quota(1_000, 20).unwrap(), 200);
    assert_eq!(remaining_threshold_quota(999, 20).unwrap(), 199);
    assert_eq!(remaining_threshold_quota(1, 20).unwrap(), 0);
    assert_eq!(
        remaining_threshold_quota(i64::MAX, 99).unwrap(),
        i64::MAX / 100 * 99 + (i64::MAX % 100) * 99 / 100
    );
}

#[tokio::test]
async fn queue_deduplicates_each_subscription_window_and_revalidates_opt_out()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let InitialSetupOutcome::Initialized { user_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "subscription-alert-owner".to_owned(),
                "a secure subscription alert password".to_owned(),
            ))
            .await?
    else {
        panic!("测试数据库必须完成首次安装");
    };
    let mut user = users::Entity::find_by_id(user_id.get())
        .one(pool.connection())
        .await?
        .expect("初始用户必须存在")
        .into_active_model();
    user.email = Set(Some("subscription-alert@example.com".to_owned()));
    let user = user.update(pool.connection()).await?;
    let now = crate::DatabaseTimestamp::now_utc();
    let plan = subscription_plans::ActiveModel {
        plan_key: Set(SensitiveString::from("11111111111111111111111111111111")),
        name: Set("专业订阅".to_owned()),
        created_by_user_id: Set(user.id),
        status: Set(1),
        quota_amount: Set(1_000),
        cycle: Set(1),
        version: Set(1),
        disabled_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let first = insert_subscription(
        pool.connection(),
        "22222222222222222222222222222222",
        user.id,
        plan.id,
        now,
    )
    .await?;
    let second = insert_subscription(
        pool.connection(),
        "33333333333333333333333333333333",
        user.id,
        plan.id,
        now,
    )
    .await?;
    set_subscription_usage(pool.connection(), first.id, 800).await?;
    set_subscription_usage(pool.connection(), second.id, 799).await?;

    let repository = SubscriptionBalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert_eq!(repository.enqueue_due(20, now, 1).await?.eligible(), 1);
    assert_eq!(repository.enqueue_due(20, now, 1).await?.eligible(), 0);
    set_subscription_usage(pool.connection(), second.id, 800).await?;
    assert_eq!(repository.enqueue_due(20, now, 1).await?.eligible(), 1);
    assert_eq!(
        subscription_balance_alert_events::Entity::find()
            .count(pool.connection())
            .await?,
        2
    );

    let SubscriptionBalanceAlertClaimOutcome::Claimed(first_lease) =
        repository.claim_next(now).await?
    else {
        panic!("首条订阅预警必须可领取");
    };
    assert_eq!(first_lease.user_id(), user_id);
    assert_eq!(first_lease.plan_name(), "专业订阅");
    assert_eq!(first_lease.quota_amount().units(), 1_000);
    assert_eq!(first_lease.quota_used().units(), 800);
    assert_eq!(first_lease.threshold_percent(), 20);
    let debug = format!("{first_lease:?}");
    assert!(!debug.contains("subscription-alert@example.com"));
    assert!(!debug.contains("subscription-alert-owner"));
    assert!(!debug.contains("专业订阅"));
    assert_eq!(
        repository.mark_sent(&first_lease, now).await?,
        BalanceAlertCompletionOutcome::Completed
    );
    let accepted_notification = user_notification_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("订阅通知事实必须存在");
    assert_eq!(
        accepted_notification.subscription_id.as_deref(),
        Some("22222222222222222222222222222222")
    );
    assert_eq!(accepted_notification.delivery_state, 2);

    let mut user = users::Entity::find_by_id(user.id)
        .one(pool.connection())
        .await?
        .expect("用户必须存在")
        .into_active_model();
    user.email_usage_alerts = Set(false);
    user.update(pool.connection()).await?;
    assert_eq!(
        repository.claim_next(now).await?,
        SubscriptionBalanceAlertClaimOutcome::Skipped
    );
    let canceled = subscription_balance_alert_events::Entity::find()
        .filter(subscription_balance_alert_events::Column::Status.eq(5))
        .count(pool.connection())
        .await?;
    assert_eq!(canceled, 1);
    let canceled_notification = user_notification_events::Entity::find()
        .filter(user_notification_events::Column::DeliveryState.eq(4))
        .count(pool.connection())
        .await?;
    assert_eq!(canceled_notification, 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn retry_uses_shared_backoff_and_completion_cas() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let InitialSetupOutcome::Initialized { user_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "subscription-retry-owner".to_owned(),
                "a secure subscription retry password".to_owned(),
            ))
            .await?
    else {
        panic!("测试数据库必须完成首次安装");
    };
    let mut user = users::Entity::find_by_id(user_id.get())
        .one(pool.connection())
        .await?
        .expect("初始用户必须存在")
        .into_active_model();
    user.email = Set(Some("subscription-retry@example.com".to_owned()));
    let user = user.update(pool.connection()).await?;
    let now = crate::DatabaseTimestamp::now_utc();
    let plan = subscription_plans::ActiveModel {
        plan_key: Set(SensitiveString::from("44444444444444444444444444444444")),
        name: Set("重试订阅".to_owned()),
        created_by_user_id: Set(user.id),
        status: Set(1),
        quota_amount: Set(1_000),
        cycle: Set(1),
        version: Set(1),
        disabled_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let subscription = insert_subscription(
        pool.connection(),
        "55555555555555555555555555555555",
        user.id,
        plan.id,
        now,
    )
    .await?;
    set_subscription_usage(pool.connection(), subscription.id, 900).await?;
    let repository = SubscriptionBalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?;
    repository.enqueue_due(20, now, 10).await?;
    let SubscriptionBalanceAlertClaimOutcome::Claimed(first) = repository.claim_next(now).await?
    else {
        panic!("订阅预警必须可领取");
    };
    assert_eq!(
        repository
            .record_failure(&first, BalanceAlertDeliveryFailureKind::Timeout, now)
            .await?,
        BalanceAlertCompletionOutcome::Completed
    );
    assert_eq!(
        repository.claim_next(now).await?,
        SubscriptionBalanceAlertClaimOutcome::Empty
    );
    let retry_at = now + Duration::from_secs(60);
    let SubscriptionBalanceAlertClaimOutcome::Claimed(second) =
        repository.claim_next(retry_at).await?
    else {
        panic!("退避结束后必须重新领取");
    };
    assert_eq!(second.attempt_count(), 2);
    assert_eq!(
        repository.mark_sent(&second, retry_at).await?,
        BalanceAlertCompletionOutcome::Completed
    );
    assert_eq!(
        repository.mark_sent(&second, retry_at).await?,
        BalanceAlertCompletionOutcome::Stale
    );
    pool.close().await?;
    Ok(())
}

async fn insert_subscription(
    connection: &sea_orm::DatabaseConnection,
    key: &str,
    user_id: i64,
    plan_id: i64,
    now: crate::DatabaseTimestamp,
) -> Result<user_subscriptions::Model, sea_orm::DbErr> {
    user_subscriptions::ActiveModel {
        subscription_key: Set(SensitiveString::from(key)),
        user_id: Set(user_id),
        plan_id: Set(plan_id),
        plan_version: Set(1),
        status: Set(1),
        quota_amount: Set(1_000),
        quota_used: Set(0),
        cycle: Set(1),
        window_started_at: Set(now),
        window_ends_at: Set(now + Duration::from_secs(86_400)),
        version: Set(1),
        bound_at: Set(now),
        status_changed_at: Set(now),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await
}

async fn set_subscription_usage(
    connection: &sea_orm::DatabaseConnection,
    subscription_id: i64,
    quota_used: i64,
) -> Result<(), sea_orm::DbErr> {
    let result = user_subscriptions::Entity::update_many()
        .col_expr(
            user_subscriptions::Column::QuotaUsed,
            sea_orm::sea_query::Expr::value(quota_used),
        )
        .filter(user_subscriptions::Column::Id.eq(subscription_id))
        .exec(connection)
        .await?;
    assert_eq!(result.rows_affected, 1);
    Ok(())
}
