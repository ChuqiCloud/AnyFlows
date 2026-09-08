use std::{error::Error, time::Duration};

use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set, entity::prelude::Json};

use crate::{
    AnalyticsExportRepository, DatabaseOptions, MigrationOptions, UsageLogBillingMode,
    UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics, UsageLogSource, UsageLogUsage,
    UsageLogVideoResolution, UsageLogWrite, UsageLogWriteError, UsageLogWriteOutcome,
    entity::{TokenHash, analytics_export_outbox_events, groups, tokens, usage_logs, users},
};

#[tokio::test]
async fn identical_usage_log_replay_is_idempotent_and_conflict_is_rejected()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let principal = create_principal(&pool, 1).await?;
    let repository = UsageLogRepository::new(pool.clone());
    let write = usage_write(principal, 1, 7);

    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Existing
    );
    assert_eq!(
        usage_logs::Entity::find().count(pool.connection()).await?,
        1
    );
    assert_eq!(
        repository.record(&usage_write(principal, 1, 8)).await,
        Err(UsageLogRepositoryError::Conflict)
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn usage_fact_and_export_pointer_commit_together_and_replay_repairs_pointer()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let principal = create_principal(&pool, 13).await?;
    let exporter = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;
    let repository = UsageLogRepository::new(pool.clone()).with_analytics_export(exporter.clone());
    let write = usage_write(principal, 13, 7);

    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        analytics_export_outbox_events::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    let lease = exporter.claim_next_due().await?.expect("应领取用量事实");
    let fact = exporter.load_fact(&lease).await?;
    let payload = fact.payload().to_string();
    assert!(!payload.contains("user_id"));
    assert!(!payload.contains("token_id"));
    assert!(!payload.contains("request_id"));

    analytics_export_outbox_events::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Existing
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
async fn historical_usage_fact_backfill_only_fills_missing_pointer() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let principal = create_principal(&pool, 14).await?;
    let repository = UsageLogRepository::new(pool.clone());
    repository.record(&usage_write(principal, 14, 7)).await?;
    let exporter = AnalyticsExportRepository::new(pool.clone(), Duration::from_secs(2))?;

    assert_eq!(exporter.backfill_missing(1).await?, 1);
    assert_eq!(exporter.backfill_missing(1).await?, 0);
    assert_eq!(exporter.backlog_count().await?, 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn outcome_unknown_replays_the_same_persisted_usage_fact() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let principal = create_principal(&pool, 2).await?;
    let repository = UsageLogRepository::new(pool.clone());
    let write = usage_write(principal, 2, 9);

    repository.inject_outcome_unknown_after_insert();
    assert_eq!(
        repository.record(&write).await,
        Err(UsageLogRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Existing
    );
    assert_eq!(
        usage_logs::Entity::find().count(pool.connection()).await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn call_observation_participates_in_idempotency_comparison() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let principal = create_principal(&pool, 3).await?;
    let repository = UsageLogRepository::new(pool.clone());
    let base = usage_write(principal, 3, 9);
    let first = base.clone().with_call_observation(
        "request-3",
        "gpt-test",
        af_domain::Protocol::OpenAiResponses,
        af_domain::Operation::Responses,
        true,
        Some(5),
        Some(4_096),
        Some(80),
        320,
    )?;
    let changed = base.with_call_observation(
        "request-3",
        "gpt-test",
        af_domain::Protocol::OpenAiResponses,
        af_domain::Operation::Responses,
        true,
        Some(5),
        Some(4_096),
        Some(81),
        320,
    )?;

    assert_eq!(
        repository.record(&first).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&first).await?,
        UsageLogWriteOutcome::Existing
    );
    assert_eq!(
        repository.record(&changed).await,
        Err(UsageLogRepositoryError::Conflict)
    );

    pool.close().await?;
    Ok(())
}

#[test]
fn usage_log_usage_rejects_negative_token_dimension() {
    assert_eq!(
        UsageLogUsage::new(-1, 0, 0, 0, 0, 0, 0, 0),
        Err(UsageLogWriteError::NegativeTokenCount)
    );
}

#[test]
fn usage_log_audio_duration_keeps_missing_distinct_and_rejects_invalid_values() {
    let principal = GatewayPrincipal::new(
        TokenId::new(1).unwrap(),
        UserId::new(2).unwrap(),
        GroupId::new(3).unwrap(),
    );
    let write = usage_write(principal, 9, 1);
    assert!(
        write
            .clone()
            .with_audio_duration_nanoseconds(Some(1_500_000_000))
            .is_ok()
    );
    assert_eq!(
        write.with_audio_duration_nanoseconds(Some(-1)),
        Err(UsageLogWriteError::InvalidAudioDuration)
    );
}

#[test]
fn usage_log_video_dimensions_keep_missing_distinct_and_reject_invalid_duration() {
    let principal = GatewayPrincipal::new(
        TokenId::new(1).unwrap(),
        UserId::new(2).unwrap(),
        GroupId::new(3).unwrap(),
    );
    let write = usage_write(principal, 10, 1)
        .with_video_dimensions(Some(8), Some(UsageLogVideoResolution::P720))
        .unwrap();
    assert_eq!(write.video_duration_seconds(), Some(8));
    assert_eq!(
        write.video_resolution(),
        Some(UsageLogVideoResolution::P720)
    );
    assert_eq!(
        usage_write(principal, 11, 1).with_video_dimensions(Some(0), None),
        Err(UsageLogWriteError::InvalidVideoDuration)
    );
    let missing = usage_write(principal, 12, 1)
        .with_video_dimensions(None, None)
        .unwrap();
    assert_eq!(missing.video_duration_seconds(), None);
    assert_eq!(missing.video_resolution(), None);
}

async fn create_principal(
    pool: &crate::DatabasePool,
    marker: u8,
) -> Result<GatewayPrincipal, Box<dyn Error>> {
    let group = groups::ActiveModel {
        name: Set(format!("usage-group-{marker}")),
        display_name: Set(format!("用量测试分组 {marker}")),
        flags: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set(format!("usage-user-{marker}")),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set(format!("usage-aff-{marker}")),
        settings: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{marker:064x}"))?),
        key_prefix: Set(format!("sk-af-{marker}")),
        name: Set(format!("usage-token-{marker}")),
        status: Set(1),
        group_id: Set(Some(group.id)),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    ))
}

fn usage_write(principal: GatewayPrincipal, marker: u8, quota: i64) -> UsageLogWrite {
    UsageLogWrite::new(
        BillingReservationId::new([marker; 16]).unwrap(),
        principal,
        UsageLogBillingMode::PerToken,
        UsageLogUsage::new(10, 2, 3, 4, 5, 1, 0, 0).unwrap(),
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(quota).unwrap(),
    )
}
