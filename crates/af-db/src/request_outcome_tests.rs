use std::{error::Error, time::Duration};

use af_domain::{ChannelId, GroupId, Operation, OrganizationId, Protocol, TokenId, UserId};
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, PaginatorTrait, Set, Statement};

use super::{
    AnalyticsExportRepository, DatabaseOptions, MigrationOptions, RequestFailureKind,
    RequestOutcomeRepository, RequestOutcomeRepositoryError, RequestOutcomeSubject,
    RequestOutcomeWrite, RequestOutcomeWriteOutcome,
};
use crate::entity::{
    HeaderOverrides, SensitiveJson, analytics_export_outbox_events, channels, groups, models,
    request_outcome_logs, users,
};

#[tokio::test]
async fn failed_call_pages_enforce_user_and_organization_scope() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    models::ActiveModel {
        model: Set("test-model".to_owned()),
        display_name: Set("Test model".to_owned()),
        provider: Set("test".to_owned()),
        tags: Set(serde_json::json!([])),
        supports_text_input: Set(true),
        supports_text_output: Set(true),
        visibility: Set(1),
        lifecycle: Set(2),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    models::ActiveModel {
        model: Set("disabled-model".to_owned()),
        display_name: Set("Disabled model".to_owned()),
        provider: Set("test".to_owned()),
        tags: Set(serde_json::json!([])),
        supports_text_input: Set(true),
        supports_text_output: Set(true),
        visibility: Set(1),
        lifecycle: Set(4),
        ..Default::default()
    }
    .insert(pool.connection())
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

    for (request_id, model) in [
        ("unknown-model-failure", "unknown-model"),
        ("disabled-model-failure", "disabled-model"),
    ] {
        repository
            .record(&RequestOutcomeWrite::failed(
                request_id,
                Protocol::OpenAiChat,
                Operation::Chat,
                model,
                RequestFailureKind::UpstreamModel,
                None,
                Duration::from_millis(9),
            )?)
            .await?;
    }
    repository
        .record(&RequestOutcomeWrite::failed_for_subject(
            "denied-model-failure",
            Protocol::OpenAiChat,
            Operation::Chat,
            "test-model",
            RequestOutcomeSubject::new(
                owner,
                TokenId::new(owner.get())?,
                GroupId::new(group.id)?,
                None,
                None,
            ),
            RequestFailureKind::ModelNotAllowed,
            None,
            Duration::from_millis(9),
        )?)
        .await?;

    let (logs, cursor) = repository
        .list_failed_for_user(owner, None, 2)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].request_id(), "denied-model-failure");
    assert_eq!(logs[1].request_id(), "other-organization");
    assert_eq!(cursor, Some(logs[1].id()));
    let (logs, cursor) = repository
        .list_failed_for_user(owner, cursor, 2)
        .await?
        .into_parts();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].request_id(), "own-second");
    assert_eq!(logs[1].request_id(), "own-first");
    assert_eq!(logs[1].username(), Some("failure-owner"));
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
    assert_eq!(logs.len(), 6);
    assert_eq!(logs[0].request_id(), "denied-model-failure");
    assert_eq!(logs[1].request_id(), "legacy-failure");
    assert_eq!(logs[1].user_id(), None);
    assert_eq!(
        logs[1].public_error_code(),
        RequestFailureKind::UpstreamServer.public_error_code()
    );
    assert!(
        logs.iter()
            .any(|log| log.request_id() == "denied-model-failure")
    );
    assert!(logs.iter().all(|log| {
        !matches!(
            log.request_id(),
            "unknown-model-failure" | "disabled-model-failure"
        )
    }));
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn failed_call_pages_preserve_real_upstream_failures_without_catalog_metadata()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let channel = channels::ActiveModel {
        name: Set("historical-failure".to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        status: Set(1),
        weight: Set(1),
        priority: Set(0),
        auto_ban: Set(false),
        model_mapping: Set(serde_json::json!({})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({}))?),
        used_quota: Set(0),
        settings: Set(SensitiveJson::from(serde_json::json!({}))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let repository = RequestOutcomeRepository::new(pool.clone());
    for (request_id, channel_id) in [
        ("unknown-before-routing", None),
        (
            "historical-upstream-failure",
            Some(ChannelId::new(channel.id)?),
        ),
    ] {
        repository
            .record(&RequestOutcomeWrite::failed(
                request_id,
                Protocol::OpenAiChat,
                Operation::Chat,
                "not-in-product-catalog",
                RequestFailureKind::UpstreamModel,
                channel_id,
                Duration::from_millis(9),
            )?)
            .await?;
    }
    let (logs, cursor) = repository.list_failed(None, 10).await?.into_parts();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].request_id(), "historical-upstream-failure");
    assert_eq!(cursor, None);
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
