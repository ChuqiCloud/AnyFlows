use std::{error::Error, time::Duration};

use af_domain::{ChannelId, Status};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
    sea_query::Expr,
};

use crate::{
    ChannelAutoDisableOutcome, ChannelProbeRecoveryOutcome, ChannelStateRepository,
    ChannelStateRepositoryError, DatabaseOptions, MAX_CHANNEL_PROBE_BATCH, MigrationOptions,
    entity::{
        ChannelBaseUrl, HeaderOverrides, SensitiveJson, abilities, channel_groups, channel_models,
        channels, groups, scheduler_outbox_events,
    },
};

#[tokio::test]
async fn auto_disable_obeys_flag_status_and_soft_delete_boundaries() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let eligible = channel("eligible", Status::Enabled, true)
        .insert(pool.connection())
        .await?;
    let flag_off = channel("flag-off", Status::Enabled, false)
        .insert(pool.connection())
        .await?;
    let manual = channel("manual", Status::Disabled, true)
        .insert(pool.connection())
        .await?;
    let deleted = channel("deleted", Status::Enabled, true)
        .insert(pool.connection())
        .await?;
    insert_ability(&pool, eligible.id, true, "disable-model").await?;
    soft_delete(&pool, &deleted).await?;
    let repository = ChannelStateRepository::new(pool.clone());

    assert_eq!(
        repository.auto_disable(channel_id(eligible.id)).await?,
        ChannelAutoDisableOutcome::Disabled
    );
    assert_eq!(
        repository.auto_disable(channel_id(eligible.id)).await?,
        ChannelAutoDisableOutcome::AlreadyDisabled
    );
    assert_eq!(
        repository.auto_disable(channel_id(flag_off.id)).await?,
        ChannelAutoDisableOutcome::NotEligible
    );
    assert_eq!(
        repository.auto_disable(channel_id(manual.id)).await?,
        ChannelAutoDisableOutcome::NotEligible
    );
    assert_eq!(
        repository.auto_disable(channel_id(deleted.id)).await?,
        ChannelAutoDisableOutcome::NotEligible
    );

    assert_status(&pool, eligible.id, Status::AutoDisabled).await?;
    assert_status(&pool, flag_off.id, Status::Enabled).await?;
    assert_status(&pool, manual.id, Status::Disabled).await?;
    assert_status(&pool, deleted.id, Status::Enabled).await?;
    assert_ability_enabled(&pool, eligible.id, false).await?;
    assert_eq!(channel_outbox_count(&pool, eligible.id).await?, 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn probe_recovery_only_reenables_auto_disabled_live_channels() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let automatic = channel("automatic", Status::AutoDisabled, true)
        .insert(pool.connection())
        .await?;
    let enabled = channel("enabled", Status::Enabled, true)
        .insert(pool.connection())
        .await?;
    let manual = channel("manual", Status::Disabled, true)
        .insert(pool.connection())
        .await?;
    let deleted = channel("deleted", Status::AutoDisabled, true)
        .insert(pool.connection())
        .await?;
    insert_ability(&pool, automatic.id, false, "recovery-model").await?;
    soft_delete(&pool, &deleted).await?;
    let repository = ChannelStateRepository::new(pool.clone());
    let leases = repository
        .load_probe_candidates(None, MAX_CHANNEL_PROBE_BATCH)
        .await?;
    let automatic_lease = lease_for(&leases, automatic.id);
    let deleted_is_hidden = leases
        .iter()
        .all(|lease| lease.channel_id().get() != deleted.id);
    assert!(deleted_is_hidden);

    assert_eq!(
        repository
            .recover_after_probe(automatic_lease.clone())
            .await?,
        ChannelProbeRecoveryOutcome::Recovered
    );
    assert_eq!(
        repository.recover_after_probe(automatic_lease).await?,
        ChannelProbeRecoveryOutcome::AlreadyEnabled
    );

    assert_status(&pool, automatic.id, Status::Enabled).await?;
    assert_status(&pool, enabled.id, Status::Enabled).await?;
    assert_status(&pool, manual.id, Status::Disabled).await?;
    assert_status(&pool, deleted.id, Status::AutoDisabled).await?;
    assert_ability_enabled(&pool, automatic.id, true).await?;
    assert_eq!(channel_outbox_count(&pool, automatic.id).await?, 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn stale_probe_cannot_overwrite_a_newer_auto_disable_state() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let automatic = channel("automatic", Status::AutoDisabled, true)
        .insert(pool.connection())
        .await?;
    let repository = ChannelStateRepository::new(pool.clone());
    let lease = repository
        .load_probe_candidates(None, 1)
        .await?
        .into_iter()
        .next()
        .expect("自动禁用渠道必须产生探活租约");
    let newer_time = TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(1);
    channels::Entity::update_many()
        .col_expr(channels::Column::UpdatedAt, Expr::value(newer_time))
        .filter(channels::Column::Id.eq(automatic.id))
        .exec(pool.connection())
        .await?;

    assert_eq!(
        repository.recover_after_probe(lease).await?,
        ChannelProbeRecoveryOutcome::StaleProbe
    );
    assert_status(&pool, automatic.id, Status::AutoDisabled).await?;

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_zero_timeout_and_redacts_channel_identity() -> Result<(), Box<dyn Error>>
{
    let pool = test_pool().await?;

    assert_eq!(
        ChannelStateRepository::with_mutation_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        ChannelStateRepositoryError::InvalidConfiguration
    );
    assert_eq!(
        ChannelStateRepository::new(pool.clone())
            .load_probe_candidates(None, 0)
            .await,
        Err(ChannelStateRepositoryError::InvalidBatchSize)
    );
    assert_eq!(
        ChannelStateRepository::new(pool.clone())
            .load_probe_candidates(None, MAX_CHANNEL_PROBE_BATCH + 1)
            .await,
        Err(ChannelStateRepositoryError::InvalidBatchSize)
    );
    assert!(!format!("{:?}", ChannelStateRepository::new(pool.clone())).contains("sqlite"));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn probe_cursor_scans_past_the_batch_limit_without_starvation() -> Result<(), Box<dyn Error>>
{
    let pool = test_pool().await?;
    let first = channel("first", Status::AutoDisabled, true)
        .insert(pool.connection())
        .await?;
    let second = channel("second", Status::AutoDisabled, true)
        .insert(pool.connection())
        .await?;
    let repository = ChannelStateRepository::new(pool.clone());

    let first_batch = repository.load_probe_candidates(None, 1).await?;
    assert_eq!(first_batch[0].channel_id().get(), first.id);
    let second_batch = repository
        .load_probe_candidates(Some(first_batch[0].channel_id()), 1)
        .await?;
    assert_eq!(second_batch[0].channel_id().get(), second.id);
    assert!(
        repository
            .load_probe_candidates(Some(second_batch[0].channel_id()), 1)
            .await?
            .is_empty()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库渠道状态流水线执行"]
async fn live_database_channel_state_round_trip() -> Result<(), Box<dyn Error>> {
    let database_url = std::env::var("AF_TEST_DATABASE_URL")
        .expect("隔离数据库渠道状态测试缺少 AF_TEST_DATABASE_URL");
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new(database_url)?,
        MigrationOptions::default(),
    )
    .await?;
    let channel = channel("live-channel-state", Status::Enabled, true)
        .insert(pool.connection())
        .await?;
    let repository = ChannelStateRepository::new(pool.clone());

    assert_eq!(
        repository.auto_disable(channel_id(channel.id)).await?,
        ChannelAutoDisableOutcome::Disabled
    );
    let lease = repository
        .load_probe_candidates(None, 1)
        .await?
        .into_iter()
        .next()
        .expect("自动禁用渠道必须产生探活租约");
    assert_eq!(
        repository.recover_after_probe(lease).await?,
        ChannelProbeRecoveryOutcome::Recovered
    );
    assert_status(&pool, channel.id, Status::Enabled).await?;

    pool.close().await?;
    Ok(())
}

async fn test_pool() -> Result<crate::DatabasePool, crate::DatabaseError> {
    crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:").expect("测试数据库 URL 必须有效"),
        MigrationOptions::default(),
    )
    .await
}

fn channel(name: &str, status: Status, auto_ban: bool) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com").unwrap(),
        )),
        status: Set(status.code()),
        auto_ban: Set(auto_ban),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(empty_json_object()).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

async fn soft_delete(
    pool: &crate::DatabasePool,
    channel: &channels::Model,
) -> Result<(), sea_orm::DbErr> {
    let mut update = channel.clone().into_active_model();
    update.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    update.update(pool.connection()).await?;
    Ok(())
}

async fn assert_status(
    pool: &crate::DatabasePool,
    id: i64,
    expected: Status,
) -> Result<(), Box<dyn Error>> {
    let channel = channels::Entity::find_by_id(id)
        .one(pool.connection())
        .await?
        .expect("测试渠道必须存在");
    assert_eq!(Status::try_from(channel.status)?, expected);
    Ok(())
}

async fn insert_ability(
    pool: &crate::DatabasePool,
    channel_id: i64,
    enabled: bool,
    model: &str,
) -> Result<(), sea_orm::DbErr> {
    let group = groups::ActiveModel {
        name: Set(format!("state-group-{channel_id}")),
        display_name: Set(format!("State Group {channel_id}")),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(channel_id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set(model.to_owned()),
        channel_id: Set(channel_id),
        enabled: Set(enabled),
        priority: Set(0),
        weight: Set(0),
        tag: Set(None),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(())
}

async fn assert_ability_enabled(
    pool: &crate::DatabasePool,
    channel_id: i64,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    let rows = abilities::Entity::find()
        .filter(abilities::Column::ChannelId.eq(channel_id))
        .all(pool.connection())
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].enabled, expected);
    Ok(())
}

async fn channel_outbox_count(
    pool: &crate::DatabasePool,
    channel_id: i64,
) -> Result<u64, sea_orm::DbErr> {
    scheduler_outbox_events::Entity::find()
        .filter(scheduler_outbox_events::Column::SubjectKind.eq(1_i16))
        .filter(scheduler_outbox_events::Column::SubjectId.eq(channel_id))
        .count(pool.connection())
        .await
}

fn channel_id(value: i64) -> ChannelId {
    ChannelId::new(value).unwrap()
}

fn lease_for(leases: &[crate::ChannelProbeLease], channel_id: i64) -> crate::ChannelProbeLease {
    leases
        .iter()
        .find(|lease| lease.channel_id().get() == channel_id)
        .expect("目标渠道必须出现在探活租约中")
        .clone()
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}
