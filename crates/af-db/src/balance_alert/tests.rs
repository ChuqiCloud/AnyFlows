use std::{error::Error, time::Duration};

use af_domain::Quota;
use sea_orm::{
    ActiveModelTrait, EntityTrait, IntoActiveModel, NotSet, PaginatorTrait, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::*;
use crate::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions,
    entity::{balance_alert_events, user_notification_events, users},
};

async fn fixture() -> Result<(crate::DatabasePool, i64), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let InitialSetupOutcome::Initialized { user_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "alert-owner".to_owned(),
                "a secure alert password".to_owned(),
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
    user.email = Set(Some("owner@example.com".to_owned()));
    user.quota = Set(500);
    user.update(pool.connection()).await?;
    Ok((pool, user_id.get()))
}

#[tokio::test]
async fn settings_and_event_queue_close_dedup_retry_and_success() -> Result<(), Box<dyn Error>> {
    let (pool, user_id) = fixture().await?;
    let settings = BalanceAlertSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let settings = settings
        .update(BalanceAlertSettingsWriteRecord::new(
            true,
            Quota::new(1_000)?,
            3_600,
        ))
        .await?;
    let queue = BalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let window = settings.window_started_at_epoch(now)?;

    assert_eq!(queue.enqueue_due(settings, now, 10).await?.eligible(), 1);
    assert_eq!(queue.enqueue_due(settings, now, 10).await?.eligible(), 0);
    assert_eq!(
        balance_alert_events::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );

    let BalanceAlertClaimOutcome::Claimed(first) = queue.claim_next(settings, window, now).await?
    else {
        panic!("低余额事件必须可领取");
    };
    assert_eq!(first.user_id().get(), user_id);
    assert_eq!(first.attempt_count(), 1);
    assert_eq!(first.current_quota().units(), 500);
    let debug = format!("{first:?}");
    assert!(!debug.contains("owner@example.com"));
    assert!(!debug.contains("alert-owner"));
    assert!(!debug.contains("500"));
    assert!(!debug.contains("1000"));
    assert_eq!(
        queue
            .record_failure(&first, BalanceAlertDeliveryFailureKind::Timeout, now)
            .await?,
        BalanceAlertCompletionOutcome::Completed
    );
    let retry_notification = user_notification_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("余额通知事实必须存在");
    assert_eq!(retry_notification.delivery_state, 1);
    assert_eq!(retry_notification.delivery_attempts, 1);
    assert_eq!(
        queue.claim_next(settings, window, now).await?,
        BalanceAlertClaimOutcome::Empty
    );

    let retry_at = now + Duration::from_secs(60);
    let BalanceAlertClaimOutcome::Claimed(second) =
        queue.claim_next(settings, window, retry_at).await?
    else {
        panic!("退避结束后必须重新领取");
    };
    assert_eq!(second.attempt_count(), 2);
    assert_eq!(
        queue.mark_sent(&second, retry_at).await?,
        BalanceAlertCompletionOutcome::Completed
    );
    assert_eq!(
        queue.mark_sent(&second, retry_at).await?,
        BalanceAlertCompletionOutcome::Stale
    );
    let event = balance_alert_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("投递事件必须存在");
    assert_eq!(event.status, 3);
    assert_eq!(event.attempt_count, 2);
    assert!(event.sent_at.is_some());
    let accepted_notification = user_notification_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("余额通知事实必须存在");
    assert_eq!(accepted_notification.delivery_state, 2);
    assert_eq!(accepted_notification.delivery_attempts, 2);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn enqueue_limit_advances_past_users_already_queued() -> Result<(), Box<dyn Error>> {
    let (pool, first_user_id) = fixture().await?;
    let first_user = users::Entity::find_by_id(first_user_id)
        .one(pool.connection())
        .await?
        .expect("首个用户必须存在");
    let mut second_user = first_user.into_active_model();
    second_user.id = NotSet;
    second_user.username = Set("alert-second".to_owned());
    second_user.email = Set(Some("second@example.com".to_owned()));
    second_user.aff_code = Set("BALANCEALERTSECOND".to_owned());
    second_user.insert(pool.connection()).await?;

    let settings = BalanceAlertSettingsRepository::new(pool.clone(), Duration::from_secs(5))?
        .update(BalanceAlertSettingsWriteRecord::new(
            true,
            Quota::new(1_000)?,
            3_600,
        ))
        .await?;
    let queue = BalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?;
    let now = TimeDateTimeWithTimeZone::now_utc();

    assert_eq!(queue.enqueue_due(settings, now, 1).await?.eligible(), 1);
    assert_eq!(queue.enqueue_due(settings, now, 1).await?.eligible(), 1);
    assert_eq!(queue.enqueue_due(settings, now, 1).await?.eligible(), 0);
    assert_eq!(
        balance_alert_events::Entity::find()
            .count(pool.connection())
            .await?,
        2
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn opt_out_cancels_claim_without_exposing_recipient() -> Result<(), Box<dyn Error>> {
    let (pool, user_id) = fixture().await?;
    let settings_repo = BalanceAlertSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let settings = settings_repo
        .update(BalanceAlertSettingsWriteRecord::new(
            true,
            Quota::new(1_000)?,
            3_600,
        ))
        .await?;
    let queue = BalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let window = settings.window_started_at_epoch(now)?;
    queue.enqueue_due(settings, now, 10).await?;

    let mut user = users::Entity::find_by_id(user_id)
        .one(pool.connection())
        .await?
        .expect("用户必须存在")
        .into_active_model();
    user.email_usage_alerts = Set(false);
    user.update(pool.connection()).await?;

    assert_eq!(
        queue.claim_next(settings, window, now).await?,
        BalanceAlertClaimOutcome::Skipped
    );
    let event = balance_alert_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("投递事件必须存在");
    assert_eq!(event.status, 5);
    pool.close().await?;
    Ok(())
}
