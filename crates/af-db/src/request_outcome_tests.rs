use std::{error::Error, time::Duration};

use af_domain::{GroupId, Operation, OrganizationId, Protocol, TokenId, UserId};
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, PaginatorTrait, Set, Statement};

use super::{
    AnalyticsExportRepository, DatabaseOptions, MigrationOptions, RequestFailureKind,
    RequestOutcomeRepository, RequestOutcomeRepositoryError, RequestOutcomeSubject,
    RequestOutcomeWrite, RequestOutcomeWriteOutcome,
};
use crate::entity::{analytics_export_outbox_events, groups, request_outcome_logs, users};

#[tokio::test]
async fn failed_call_pages_enforce_user_and_organization_scope() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("failed-call-tests".to_owned()),
        display_name: Set("Failed calls".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for name in ["failure-owner", "failure-other"] {
        let user = users::ActiveModel {
            username: Set(name.to_owned()),
            email: Set(None),
            role: Set(1),
            status: Set(1),
            default_group_id: Set(group.id),
            quota: Set(10_000),
            aff_code: Set(name.to_owned()),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    let owner = user_ids[0];
    let other = user_ids[1];
    let organization = OrganizationId::new(11)?;
    let other_organization = OrganizationId::new(12)?;
    let repository = RequestOutcomeRepository::new(pool.clone());
    for (request_id, user_id, organization_id) in [
        ("own-first", owner, organization),
        ("other-user", other, organization),
        ("own-second", owner, organization),
        ("other-organization", owner, other_organization),
    ] {
        let subject = RequestOutcomeSubject::new(
            user_id,
            TokenId::new(user_id.get())?,
            GroupId::new(group.id)?,
            Some(organization_id),
            None,
        );
        repository
            .record(&RequestOutcomeWrite::failed_for_subject(
                request_id,
                Protocol::OpenAiChat,
                Operation::Chat,
                "test-model",
                subject,
                RequestFailureKind::UpstreamNetwork,
                None,
                Duration::from_millis(17),
            )?)
            .await?;
    }
    repository
        .record(&RequestOutcomeWrite::failed(
            "legacy-failure",
            Protocol::OpenAiChat,
            Operation::Chat,
            "test-model",
            RequestFailureKind::UpstreamServer,
            None,
            Duration::from_millis(9),
        )?)
        .await?;
    repository
        .record(&RequestOutcomeWrite::succeeded(
            "successful-call",
            Protocol::OpenAiChat,
            Operation::Chat,
            "test-model",
            None,
            Duration::from_millis(8),
        )?)
        .await?;

    let (logs, cursor) = repository
        .list_failed_for_user(owner, None, 2)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].request_id(), "other-organization");
    assert_eq!(logs[1].request_id(), "own-second");
    assert_eq!(cursor, Some(logs[1].id()));
    let (logs, cursor) = repository
        .list_failed_for_user(owner, cursor, 2)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].request_id(), "own-first");
    assert_eq!(logs[0].username(), Some("failure-owner"));
    assert_eq!(cursor, None);

    let (logs, _) = repository
        .list_failed_for_organization(organization, None, None, 10)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 3);
    assert!(
        logs.iter()
            .all(|log| log.organization_id() == Some(organization))
    );
    let (logs, _) = repository
        .list_failed_for_organization(organization, Some(owner), None, 10)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|log| log.user_id() == Some(owner)));

    let (logs, _) = repository.list_failed(None, 10).await?.into_parts();
    assert_eq!(logs.len(), 5);
    assert_eq!(logs[0].request_id(), "legacy-failure");
    assert_eq!(logs[0].user_id(), None);
    assert_eq!(
        logs[0].public_error_code(),
        RequestFailureKind::UpstreamServer.public_error_code()
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_is_idempotent_and_rejects_conflicting_terminal_facts()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = RequestOutcomeRepository::new(pool.clone());
    let succeeded = RequestOutcomeWrite::succeeded(
        "request-outcome-idempotent",
        Protocol::OpenAiChat,
        Operation::Chat,
        "gpt-test",
        None,
        Duration::from_millis(17),
    )?;
    assert_eq!(
        repository.record(&succeeded).await?,
        RequestOutcomeWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&succeeded).await?,
        RequestOutcomeWriteOutcome::Existing
    );

    let conflicting = RequestOutcomeWrite::failed(
        "request-outcome-idempotent",
        Protocol::OpenAiChat,
        Operation::Chat,
        "gpt-test",
        RequestFailureKind::UpstreamNetwork,
        None,
        Duration::from_millis(17),
    )?;
    assert_eq!(
        repository.record(&conflicting).await,
        Err(RequestOutcomeRepositoryError::Conflict)
    );
    assert_eq!(
        request_outcome_logs::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn request_outcome_and_export_pointer_commit_together_and_replay_repairs_pointer()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let exporter = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;
    let repository =
        RequestOutcomeRepository::new(pool.clone()).with_analytics_export(exporter.clone());
    let write = RequestOutcomeWrite::succeeded(
        "request-outcome-export",
        Protocol::OpenAiChat,
        Operation::Chat,
        "gpt-test",
        None,
        Duration::from_millis(17),
    )?;

    assert_eq!(
        repository.record(&write).await?,
        RequestOutcomeWriteOutcome::Applied
    );
    assert_eq!(
        analytics_export_outbox_events::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    let lease = exporter
        .claim_next_due()
        .await?
        .expect("应领取请求终态事实");
    let fact = exporter.load_fact(&lease).await?;
    let payload = fact.payload().to_string();
    assert!(!payload.contains("request_id"));
    assert!(!payload.contains("user_id"));
    assert!(!payload.contains("token_id"));

    analytics_export_outbox_events::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    assert_eq!(
        repository.record(&write).await?,
        RequestOutcomeWriteOutcome::Existing
    );
    assert_eq!(
        analytics_export_outbox_events::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn database_guard_rejects_updates_to_persisted_outcomes() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = RequestOutcomeRepository::new(pool.clone());
    repository
        .record(&RequestOutcomeWrite::failed(
            "request-outcome-append-only",
            Protocol::OpenAiResponses,
            Operation::Responses,
            "gpt-test",
            RequestFailureKind::UpstreamServer,
            None,
            Duration::from_millis(31),
        )?)
        .await?;

    let backend = pool.connection().get_database_backend();
    let result = pool
        .connection()
        .execute(Statement::from_string(
            backend,
            "UPDATE request_outcome_logs SET duration_ms = 32 WHERE request_id = 'request-outcome-append-only'",
        ))
        .await;
    assert!(result.is_err());
    pool.close().await?;
    Ok(())
}

#[test]
fn failure_kinds_round_trip_only_stable_low_cardinality_values() {
    for kind in [
        RequestFailureKind::InvalidRequest,
        RequestFailureKind::InsufficientQuota,
        RequestFailureKind::UpstreamAuthentication,
        RequestFailureKind::UpstreamProtocol,
        RequestFailureKind::Internal,
    ] {
        assert_eq!(kind.as_str().parse(), Ok(kind));
    }
    assert!(
        "private upstream message"
            .parse::<RequestFailureKind>()
            .is_err()
    );
}
