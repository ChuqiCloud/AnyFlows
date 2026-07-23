use std::error::Error;

use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set, entity::prelude::Json};

use crate::{
    DatabaseOptions, MigrationOptions, UsageLogBillingMode, UsageLogRepository,
    UsageLogRepositoryError, UsageLogSemantics, UsageLogSource, UsageLogUsage, UsageLogWrite,
    UsageLogWriteError, UsageLogWriteOutcome,
    entity::{TokenHash, groups, tokens, usage_logs, users},
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

#[test]
fn usage_log_usage_rejects_negative_token_dimension() {
    assert_eq!(
        UsageLogUsage::new(-1, 0, 0, 0, 0, 0, 0, 0),
        Err(UsageLogWriteError::NegativeTokenCount)
    );
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
