use std::error::Error;

use af_domain::{ChannelId, GroupId};
use sea_orm::{
    ActiveModelTrait, EntityTrait, QueryOrder, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    DatabaseOptions, MigrationOptions, SchedulerCatalogSubject, entity::scheduler_outbox_events,
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
