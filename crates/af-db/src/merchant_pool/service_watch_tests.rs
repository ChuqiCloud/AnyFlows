use super::*;
use af_domain::MerchantMaintenanceWrite;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use std::error::Error;

async fn enable_merchant_event_plugin(fixture: &Fixture) -> Result<(), Box<dyn Error>> {
    let operator = UserId::new(3)?;
    let plugin_id = "org.example.merchant-events";
    let capability_id = "events.merchant-service.changed.consume";
    let repository = crate::PluginCatalogRepository::new(fixture.pool.clone());
    let digest = "cd".repeat(32);
    let artifact = repository
        .register_artifact(crate::PluginArtifactCreate {
            plugin_id: plugin_id.to_owned(),
            plugin_version: "1.0.0".to_owned(),
            digest: digest.clone(),
            manifest: serde_json::json!({
                "pluginId": plugin_id,
                "pluginVersion": "1.0.0",
                "configSchema": {"type":"object", "additionalProperties":false},
                "contractRange": {"minimum":{"major":1,"minor":0},"maximum":{"major":1,"minor":2}},
                "authenticationModes": ["none"],
                "limits": {"maxConcurrency":1,"requestTimeoutMs":1000,"maxResponseBytes":4096,"maxPageSize":20},
                "capabilities": [{"id":capability_id,"version":1,"access":"read","sideEffect":"none"}]
            }),
            source_kind: crate::PluginArtifactSourceKind::LocalProcess,
            source_reference: "/opt/anyflows/plugins/merchant-events".to_owned(),
            signer_key_id: String::new(),
            signature: String::new(),
            created_at: 1_800_000_000,
        })
        .await?;
    repository
        .approve_artifact(crate::PluginArtifactApproval {
            artifact_id: artifact.id,
            approved_by: operator,
            approved_at: 1_800_000_001,
            request_id: "merchant-event-approval".to_owned(),
        })
        .await?;
    repository
        .deploy(crate::PluginDeploymentWrite {
            plugin_id: plugin_id.to_owned(),
            artifact_digest: digest,
            expected_generation: None,
            enabled: true,
            granted_capabilities: vec![crate::PluginCapabilityGrant {
                id: capability_id.to_owned(),
                version: 1,
            }],
            config: serde_json::json!({}),
            updated_by: operator,
            updated_at: 1_800_000_002,
            request_id: "merchant-event-deploy".to_owned(),
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn public_service_lifecycle_events_are_queued_without_watchers() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    enable_merchant_event_plugin(&fixture).await?;

    let service = publish(&fixture).await?;
    let deliveries = crate::entity::plugin_event_deliveries::Entity::find()
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(
        deliveries.len(),
        1,
        "publishing must queue without watchers"
    );
    assert_eq!(deliveries[0].event_kind, "merchant.service.changed");
    let publication: serde_json::Value = serde_json::from_str(&deliveries[0].payload_json)?;
    assert_eq!(publication["eventKey"], deliveries[0].event_key);
    assert_eq!(publication["serviceId"], service.id);
    assert_eq!(publication["revision"], 1);
    assert_eq!(publication["status"], "active");
    assert!(publication["observedAt"].as_i64().is_some());
    assert_eq!(publication.as_object().unwrap().len(), 5);

    let observed_at = publication["observedAt"].as_i64().unwrap();
    crate::plugin_event_delivery::enqueue_merchant_service_changed(
        fixture.pool.connection(),
        service.id,
        1,
        &deliveries[0].event_key,
        "active",
        observed_at,
    )
    .await?;
    assert_eq!(
        crate::entity::plugin_event_deliveries::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1,
        "replaying the same transition must be idempotent"
    );

    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, true)
        .await?;
    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, true)
        .await?;
    let deliveries = crate::entity::plugin_event_deliveries::Entity::find()
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(deliveries.len(), 2);
    let pause = deliveries
        .iter()
        .find(|delivery| {
            serde_json::from_str::<serde_json::Value>(&delivery.payload_json)
                .is_ok_and(|payload| payload["status"] == "paused")
        })
        .expect("pause transition delivery");
    let payload: serde_json::Value = serde_json::from_str(&pause.payload_json)?;
    assert_eq!(payload["serviceId"], service.id);
    assert_eq!(payload["revision"], 1);
    assert_eq!(payload["status"], "paused");
    assert_eq!(payload["eventKey"], pause.event_key);
    assert_eq!(payload.as_object().unwrap().len(), 5);

    fixture.pool.close().await?;
    Ok(())
}

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
