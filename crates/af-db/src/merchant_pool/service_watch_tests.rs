use super::*;
use af_domain::MerchantMaintenanceWrite;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::error::Error;

#[tokio::test]
async fn published_service_updates_notify_once_and_draft_edits_stay_private()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let owner = UserId::new(2)?;
    fixture
        .repository
        .watch_service(owner, fixture.shared_pool, service.id)
        .await?;

    let mut draft = service_write(fixture.shared_pool);
    draft.name = "Updated Reviewed Service".to_owned();
    fixture
        .repository
        .write_service(UserId::new(1)?, Some(service.id), draft, envelope)
        .await?;
    assert_eq!(
        merchant_service_notification_count(&fixture, owner.get()).await?,
        0
    );

    let submitted = fixture
        .repository
        .submit_service(UserId::new(1)?, service.id)
        .await?;
    assert_eq!(
        merchant_service_notification_count(&fixture, owner.get()).await?,
        0
    );

    let expected_revision = submitted
        .draft
        .as_ref()
        .map(|revision| revision.revision)
        .expect("submitted revision");
    fixture
        .repository
        .review_service(
            UserId::new(3)?,
            service.id,
            review("approve", Some(expected_revision)),
        )
        .await?;
    assert_eq!(
        merchant_service_notification_count(&fixture, owner.get()).await?,
        1
    );

    let txn = fixture.pool.connection().begin().await?;
    let event = crate::entity::merchant_service_watch_events::Entity::find()
        .filter(crate::entity::merchant_service_watch_events::Column::ServiceId.eq(service.id))
        .filter(
            crate::entity::merchant_service_watch_events::Column::EventKind.eq("service_updated"),
        )
        .one(&txn)
        .await?
        .expect("published update event");
    let prefix = format!("service:{}:service_updated:", service.id);
    let transition_key = event
        .source_key
        .strip_prefix(&prefix)
        .expect("event source key prefix");
    let service_row = crate::entity::merchant_services::Entity::find_by_id(service.id)
        .one(&txn)
        .await?
        .expect("published service row");
    crate::merchant_pool::service_watch::record_service_event(
        &txn,
        &service_row,
        "service_updated",
        transition_key,
        event.occurred_at,
    )
    .await?;
    txn.commit().await?;

    assert_eq!(
        fixture
            .pool
            .connection()
            .query_one(sea_orm::Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM merchant_service_watch_events WHERE service_id=? AND event_kind='service_updated'",
                [service.id.into()],
            ))
            .await?
            .expect("service event count")
            .try_get::<i64>("", "count")?,
        1
    );
    assert_eq!(
        merchant_service_notification_count(&fixture, owner.get()).await?,
        1
    );

    fixture.pool.close().await?;
    Ok(())
}

async fn merchant_service_notification_count(
    fixture: &Fixture,
    owner: i64,
) -> Result<i64, Box<dyn Error>> {
    let row = fixture
        .pool
        .connection()
        .query_one(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM user_notification_events WHERE user_id=? AND source_key LIKE 'merchant-service:%'",
            [owner.into()],
        ))
        .await?
        .expect("merchant service notification count");
    Ok(row.try_get::<i64>("", "count")?)
}

#[tokio::test]
async fn followed_service_updates_are_deduplicated_and_redacted_after_access_revocation()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let owner = UserId::new(2)?;

    let watch = fixture
        .repository
        .watch_service(owner, fixture.shared_pool, service.id)
        .await?;
    assert_eq!(watch.service_id, service.id);
    assert!(watch.active);
    let same_watch = fixture
        .repository
        .watch_service(owner, fixture.shared_pool, service.id)
        .await?;
    assert_eq!(same_watch.service_id, service.id);
    assert_eq!(
        fixture
            .repository
            .watched_services(owner)
            .await?
            .items
            .len(),
        1
    );

    let now = DatabaseTimestamp::now_utc().unix_timestamp();
    fixture
        .repository
        .write_service_maintenance(
            None,
            UserId::new(1)?,
            service.id,
            None,
            MerchantMaintenanceWrite {
                starts_at: now + 3600,
                ends_at: now + 7200,
                message: "Scheduled maintenance".to_owned(),
                blocks_requests: true,
            },
        )
        .await?;
    let notification_repo =
        crate::UserNotificationRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let page = match notification_repo
        .list(owner.get(), crate::UserNotificationListQuery::default())
        .await?
    {
        crate::UserNotificationListOutcome::Found(page) => page,
        crate::UserNotificationListOutcome::NotFound => unreachable!(),
    };
    assert_eq!(page.entries().len(), 1);
    assert!(page.entries()[0].is_merchant_service_update());
    assert_eq!(
        page.entries()[0].merchant_service_event_kind(),
        Some("maintenance_announced")
    );
    assert_eq!(
        page.entries()[0].merchant_service_name(),
        Some("Reviewed Service")
    );

    fixture
        .pool
        .connection()
        .execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "INSERT INTO merchant_maintenance (service_id,starts_at,ends_at,message,blocks_requests,cancelled,created_at,updated_at) VALUES (?,?,?,?,0,0,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
            [service.id.into(), (now - 7200).into(), (now - 3600).into(), "Completed maintenance".to_owned().into()],
        ))
        .await?;
    assert_eq!(fixture.repository.process_due_pricing(100).await?, 0);
    assert_eq!(fixture.repository.process_due_pricing(100).await?, 0);
    let page = match notification_repo
        .list(owner.get(), crate::UserNotificationListQuery::default())
        .await?
    {
        crate::UserNotificationListOutcome::Found(page) => page,
        crate::UserNotificationListOutcome::NotFound => unreachable!(),
    };
    assert_eq!(page.entries().len(), 2);
    assert!(
        page.entries()
            .iter()
            .any(|entry| { entry.merchant_service_event_kind() == Some("maintenance_ended") })
    );

    assert!(
        fixture
            .repository
            .unwatch_service(owner, fixture.shared_pool, service.id)
            .await?
    );
    assert!(
        !fixture
            .repository
            .unwatch_service(owner, fixture.shared_pool, service.id)
            .await?
    );
    let notification_count = fixture
        .pool
        .connection()
        .query_one(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM user_notification_events WHERE user_id=? AND source_key LIKE 'merchant-service:%'",
            [owner.get().into()],
        ))
        .await?
        .expect("notification count");
    assert_eq!(notification_count.try_get::<i64>("", "count")?, 2);

    fixture
        .pool
        .connection()
        .execute_unprepared("INSERT INTO groups (id,name,display_name,flags) VALUES (2,'other-group','Other Group','{}')")
        .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE users SET default_group_id=2 WHERE id=2")
        .await?;
    let page = match notification_repo
        .list(owner.get(), crate::UserNotificationListQuery::default())
        .await?
    {
        crate::UserNotificationListOutcome::Found(page) => page,
        crate::UserNotificationListOutcome::NotFound => unreachable!(),
    };
    assert_eq!(page.entries().len(), 2, "event history remains user-owned");
    for entry in page.entries() {
        assert!(entry.is_merchant_service_update());
        assert_eq!(entry.merchant_service_name(), None);
        assert_eq!(entry.merchant_service_pool_id(), None);
        assert_eq!(entry.merchant_service_id(), None);
        assert_eq!(entry.merchant_service_merchant_id(), None);
        assert_eq!(entry.merchant_service_event_kind(), None);
        assert!(entry.merchant_service_event_id().is_some());
    }

    assert!(matches!(
        fixture
            .repository
            .watch_service(owner, fixture.shared_pool, service.id)
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    fixture.pool.close().await?;
    Ok(())
}
