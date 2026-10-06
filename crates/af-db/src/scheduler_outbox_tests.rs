use std::{error::Error, time::Duration};

use af_domain::{ChannelId, GroupId};
use sea_orm::{
    ActiveModelTrait, EntityTrait, QueryOrder, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    DatabaseOptions, MigrationOptions, SchedulerCatalogSubject, SchedulerOutboxClaimOutcome,
    SchedulerOutboxCompletionOutcome, SchedulerOutboxRepository,
    SchedulerOutboxRepositoryConfigError, entity::scheduler_outbox_events,
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

#[tokio::test]
async fn outbox_insert_is_transactional_and_uses_a_closed_pending_shape()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let transaction = pool.connection().begin().await?;
    enqueue_scheduler_catalog_change(
        &transaction,
        SchedulerCatalogSubject::Channel(ChannelId::new(7)?),
        now,
    )
    .await?;
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .all(&transaction)
            .await?
            .len(),
        1
    );
    transaction.rollback().await?;
    assert!(
        scheduler_outbox_events::Entity::find()
            .all(pool.connection())
            .await?
            .is_empty(),
        "业务事务回滚时 outbox 不得遗留孤立事件"
    );

    enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Channel(ChannelId::new(7)?),
        now,
    )
    .await?;
    enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Group(GroupId::new(11)?),
        now,
    )
    .await?;
    let rows = scheduler_outbox_events::Entity::find()
        .order_by_asc(scheduler_outbox_events::Column::Id)
        .all(pool.connection())
        .await?;
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].subject_kind, rows[0].subject_id), (1, 7));
    assert_eq!((rows[1].subject_kind, rows[1].subject_id), (2, 11));
    assert_eq!(
        SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?
            .latest_event_id()
            .await?,
        u64::try_from(rows[1].id)?
    );
    assert!(rows.iter().all(|row| {
        row.status == 1
            && row.attempt_count == 0
            && row.lease_expires_at.is_none()
            && row.published_at.is_none()
            && row.version == 1
    }));

    let invalid = scheduler_outbox_events::ActiveModel {
        subject_kind: Set(3),
        subject_id: Set(1),
        status: Set(1),
        attempt_count: Set(0),
        next_attempt_at: Set(now),
        lease_expires_at: Set(None),
        published_at: Set(None),
        version: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    assert!(invalid.insert(pool.connection()).await.is_err());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn outbox_repository_rejects_a_zero_database_deadline() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    assert_eq!(
        SchedulerOutboxRepository::new(pool.clone(), Duration::ZERO).unwrap_err(),
        SchedulerOutboxRepositoryConfigError::ZeroOperationTimeout
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_claims_publish_one_event_once_by_version_cas() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Channel(ChannelId::new(999)?),
        now,
    )
    .await?;
    let first = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;
    let second = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;

    let (first_result, second_result) = tokio::join!(first.claim_next(now), second.claim_next(now));
    let mut lease = None;
    let mut empty = 0;
    for outcome in [first_result?, second_result?] {
        match outcome {
            SchedulerOutboxClaimOutcome::Claimed(claimed) => {
                assert!(lease.replace(claimed).is_none());
            }
            SchedulerOutboxClaimOutcome::Empty => empty += 1,
        }
    }
    assert_eq!(empty, 1);
    let lease = lease.expect("并发竞争必须产生一个租约 owner");
    assert_eq!(lease.attempt_count(), 1);
    assert_eq!(
        lease.subject(),
        SchedulerCatalogSubject::Channel(ChannelId::new(999)?)
    );
    assert!(!format!("{lease:?}").contains("999"));
    assert_eq!(
        first.mark_published(&lease, now).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );
    assert_eq!(
        first.mark_published(&lease, now).await?,
        SchedulerOutboxCompletionOutcome::Stale
    );
    let row = scheduler_outbox_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("事件必须存在");
    assert_eq!(row.status, 3);
    assert_eq!(row.attempt_count, 1);
    assert!(row.lease_expires_at.is_none());
    assert!(row.published_at.is_some());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn failed_and_expired_leases_retry_without_overwriting_new_owner()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Group(GroupId::new(7)?),
        now,
    )
    .await?;
    let first = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;
    let second = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;

    let SchedulerOutboxClaimOutcome::Claimed(initial) = first.claim_next(now).await? else {
        panic!("初始事件必须可领取");
    };
    assert_eq!(
        first.record_failure(&initial, now).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );
    assert_eq!(
        first.claim_next(now).await?,
        SchedulerOutboxClaimOutcome::Empty
    );

    let retry_at = now + Duration::from_secs(1);
    let SchedulerOutboxClaimOutcome::Claimed(retried) = first.claim_next(retry_at).await? else {
        panic!("退避结束后必须重新领取");
    };
    assert_eq!(retried.attempt_count(), 2);
    let takeover_at = retry_at + Duration::from_secs(120);
    let SchedulerOutboxClaimOutcome::Claimed(takeover) = second.claim_next(takeover_at).await?
    else {
        panic!("租约到期后必须允许其他实例接管");
    };
    assert_eq!(takeover.attempt_count(), 3);
    assert_eq!(
        first.mark_published(&retried, takeover_at).await?,
        SchedulerOutboxCompletionOutcome::Stale
    );
    assert_eq!(
        second.mark_published(&takeover, takeover_at).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );
    let row = scheduler_outbox_events::Entity::find()
        .one(pool.connection())
        .await?
        .expect("事件必须存在");
    assert_eq!((row.status, row.attempt_count), (3, 3));
    pool.close().await?;
    Ok(())
}
