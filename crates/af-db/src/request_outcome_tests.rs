use std::{error::Error, time::Duration};

use af_domain::{Operation, Protocol};
use sea_orm::{ConnectionTrait, EntityTrait, PaginatorTrait, Statement};

use super::{
    AnalyticsExportRepository, DatabaseOptions, MigrationOptions, RequestFailureKind,
    RequestOutcomeRepository, RequestOutcomeRepositoryError, RequestOutcomeWrite,
    RequestOutcomeWriteOutcome,
};
use crate::entity::{analytics_export_outbox_events, request_outcome_logs};

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
