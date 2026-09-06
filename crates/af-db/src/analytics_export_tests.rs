use std::time::Duration;

use sea_orm::TransactionTrait;
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;

use crate::{
    AnalyticsExportCompletionOutcome, AnalyticsExportFactKind, AnalyticsExportRepository,
    DatabaseOptions, MigrationOptions, connect_and_migrate,
};

#[tokio::test]
async fn outbox_claim_retry_and_completion_are_versioned() -> Result<(), Box<dyn std::error::Error>>
{
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;
    let transaction = pool.connection().begin().await?;
    AnalyticsExportRepository::enqueue_in_transaction(
        &transaction,
        AnalyticsExportFactKind::UsageLog,
        1,
        TimeDateTimeWithTimeZone::now_utc(),
    )
    .await?;
    AnalyticsExportRepository::enqueue_in_transaction(
        &transaction,
        AnalyticsExportFactKind::UsageLog,
        1,
        TimeDateTimeWithTimeZone::now_utc(),
    )
    .await?;
    transaction.commit().await?;

    assert_eq!(repository.backlog_count().await?, 1);
    assert_eq!(
        repository.queue_counts().await?,
        crate::AnalyticsExportQueueCounts::new(1, 0, 0)
    );
    let lease = repository.claim_next_due().await?.expect("应领取一条事件");
    assert_eq!(lease.fact_kind(), AnalyticsExportFactKind::UsageLog);
    assert_eq!(lease.fact_id(), 1);
    assert_eq!(
        repository.record_failure_now(&lease).await?,
        AnalyticsExportCompletionOutcome::Completed
    );
    let retry = repository.claim_next_due().await?;
    assert!(retry.is_none(), "失败事件应进入退避窗口");
    assert_eq!(repository.backlog_count().await?, 1);
    assert_eq!(
        repository.queue_counts().await?,
        crate::AnalyticsExportQueueCounts::new(1, 0, 0)
    );
    Ok(())
}

#[tokio::test]
async fn replay_releases_only_pending_or_expired_leases() -> Result<(), Box<dyn std::error::Error>>
{
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;
    let transaction = pool.connection().begin().await?;
    for fact_id in [1, 2, 3] {
        AnalyticsExportRepository::enqueue_in_transaction(
            &transaction,
            AnalyticsExportFactKind::UsageLog,
            fact_id,
            TimeDateTimeWithTimeZone::now_utc(),
        )
        .await?;
    }
    transaction.commit().await?;

    let active = repository.claim_next_due().await?.expect("应领取活动租约");
    assert_eq!(repository.replay_now(10).await?, 2);
    assert_eq!(repository.queue_counts().await?.pending_count(), 2);
    assert_eq!(
        repository.mark_published_now(&active).await?,
        AnalyticsExportCompletionOutcome::Completed
    );
    assert_eq!(repository.queue_counts().await?.published_count(), 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn stale_lease_cannot_close_a_newer_attempt() -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;
    let transaction = pool.connection().begin().await?;
    AnalyticsExportRepository::enqueue_in_transaction(
        &transaction,
        AnalyticsExportFactKind::RequestOutcome,
        1,
        TimeDateTimeWithTimeZone::now_utc(),
    )
    .await?;
    transaction.commit().await?;

    let first = repository
        .claim_next_due()
        .await?
        .expect("应领取第一份租约");
    let second = repository
        .claim_next(TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(121))
        .await?
        .expect("过期租约应允许重新领取");
    assert_eq!(
        repository.mark_published_now(&first).await?,
        AnalyticsExportCompletionOutcome::Stale
    );
    assert_eq!(
        repository.mark_published_now(&second).await?,
        AnalyticsExportCompletionOutcome::Completed
    );
    assert_eq!(repository.backlog_count().await?, 0);

    pool.close().await?;
    Ok(())
}
