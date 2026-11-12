use std::{error::Error, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
    sea_query::Expr,
};
use sea_orm_migration::MigratorTrait;

use crate::{
    ActiveChannelMonitorConfig, ActiveChannelMonitorError, ActiveChannelMonitorRepository,
    ActiveChannelMonitorTargetInput, ActiveChannelProbeOutcome, DatabaseOptions,
    InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MAX_ACTIVE_CHANNEL_MONITOR_EVENTS_PER_TARGET, MigrationOptions,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channel_models,
        channel_monitor_events, channels, credentials, user_notification_events, users,
    },
};

const NOW: i64 = 2_000_000_000;

#[tokio::test]
async fn selected_targets_are_disabled_by_default_and_retain_leases_when_disabled_or_deleted()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    assert!(!repository.get_config().await?.enabled);

    let (channel, credential) = target_fixture(
        &pool,
        "lease-target",
        "chat",
        Protocol::OpenAiChat,
        ChannelType::OpenAi,
    )
    .await?;
    let mut input = target_input(channel, credential, "chat");
    assert_eq!(
        repository
            .upsert_target(target_input(channel, credential, "other"))
            .await,
        Err(ActiveChannelMonitorError::NotEligible)
    );
    input.enabled = true;
    let target = repository.upsert_target(input.clone()).await?;
    repository
        .set_config(ActiveChannelMonitorConfig {
            enabled: true,
            global_daily_budget: 10,
        })
        .await?;

    let lease = repository
        .claim_due_targets("first-worker", 1, NOW)
        .await?
        .remove(0);
    input.enabled = false;
    repository.upsert_target(input).await?;
    assert!(
        repository
            .claim_due_targets("second-worker", 1, NOW + 1)
            .await?
            .is_empty()
    );

    assert!(repository.delete_target(target.id).await?);
    assert!(repository.list_targets(256).await?.is_empty());
    // A pending deletion still occupies the global slot until its probe completes.
    assert!(
        repository
            .claim_due_targets("second-worker", 1, NOW + 2)
            .await?
            .is_empty()
    );
    repository
        .complete_probe(
            lease,
            ActiveChannelProbeOutcome::Healthy { latency_ms: 4 },
            NOW + 3,
        )
        .await?;
    assert!(repository.list_targets(256).await?.is_empty());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn low_cost_capability_and_target_limits_are_enforced() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    let (image_channel, image_credential) = target_fixture(
        &pool,
        "image-target",
        "image",
        Protocol::OpenAiImages,
        ChannelType::OpenAi,
    )
    .await?;
    assert_eq!(
        repository
            .upsert_target(target_input(image_channel, image_credential, "image"))
            .await,
        Err(ActiveChannelMonitorError::NotEligible)
    );
    let (mismatched_channel, mismatched_credential) = target_fixture(
        &pool,
        "custom-target",
        "custom",
        Protocol::OpenAiChat,
        ChannelType::Custom,
    )
    .await?;
    assert_eq!(
        repository
            .upsert_target(target_input(
                mismatched_channel,
                mismatched_credential,
                "custom"
            ))
            .await,
        Err(ActiveChannelMonitorError::NotEligible)
    );
    let (opencode_channel, opencode_credential) = target_fixture(
        &pool,
        "opencode-target",
        "opencode",
        Protocol::OpenAiChat,
        ChannelType::OpenCode,
    )
    .await?;
    channels::Entity::update_many()
        .col_expr(
            channels::Column::Settings,
            Expr::value(SensitiveJson::from(Json::Object(
                [("opencode_full_compatibility".to_owned(), Json::Bool(true))]
                    .into_iter()
                    .collect(),
            ))),
        )
        .filter(channels::Column::Id.eq(opencode_channel.get()))
        .exec(pool.connection())
        .await?;
    assert_eq!(
        repository
            .upsert_target(target_input(
                opencode_channel,
                opencode_credential,
                "opencode"
            ))
            .await,
        Err(ActiveChannelMonitorError::NotEligible)
    );

    let (channel, credential) = target_fixture(
        &pool,
        "validation-target",
        "chat",
        Protocol::OpenAiChat,
        ChannelType::OpenAi,
    )
    .await?;
    let mut input = target_input(channel, credential, "chat");
    input.interval_secs = 299;
    assert_eq!(
        input.validate(),
        Err(crate::ActiveChannelMonitorConfigError::Interval)
    );
    input.interval_secs = 86_401;
    assert_eq!(
        input.validate(),
        Err(crate::ActiveChannelMonitorConfigError::Interval)
    );
    input.interval_secs = 300;
    input.timeout_secs = 16;
    assert_eq!(
        input.validate(),
        Err(crate::ActiveChannelMonitorConfigError::Timeout)
    );
    input.timeout_secs = 15;
    input.alert_after_secs = 59;
    assert_eq!(
        input.validate(),
        Err(crate::ActiveChannelMonitorConfigError::AlertWindow)
    );
    input.alert_after_secs = 3_601;
    assert_eq!(
        input.validate(),
        Err(crate::ActiveChannelMonitorConfigError::AlertWindow)
    );
    input.alert_after_secs = 60;
    assert!(input.validate().is_ok());
    assert_eq!(
        repository
            .set_config(ActiveChannelMonitorConfig {
                enabled: true,
                global_daily_budget: 289
            })
            .await,
        Err(ActiveChannelMonitorError::InvalidConfiguration)
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn claims_enforce_two_in_flight_global_budget_and_rolling_24_hour_reset()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    repository
        .set_config(ActiveChannelMonitorConfig {
            enabled: true,
            global_daily_budget: 3,
        })
        .await?;
    let mut inputs = Vec::new();
    for index in 0..3 {
        let model = format!("model-{index}");
        let (channel, credential) = target_fixture(
            &pool,
            &format!("budget-{index}"),
            &model,
            Protocol::OpenAiChat,
            ChannelType::OpenAi,
        )
        .await?;
        let input = target_input(channel, credential, &model);
        repository.upsert_target(input.clone()).await?;
        inputs.push(input);
    }

    let leases = repository.claim_due_targets("worker-a", 2, NOW).await?;
    assert_eq!(leases.len(), 2);
    // Disabling and requesting deletion cannot release the first live lease early.
    let first_id = leases[0].target.id;
    inputs[0].enabled = false;
    repository.upsert_target(inputs.remove(0)).await?;
    assert!(repository.delete_target(first_id).await?);
    assert!(
        repository
            .claim_due_targets("worker-b", 2, NOW + 1)
            .await?
            .is_empty()
    );

    repository
        .complete_probe(
            leases[0].clone(),
            ActiveChannelProbeOutcome::Healthy { latency_ms: 2 },
            NOW + 2,
        )
        .await?;
    assert!(
        repository
            .list_targets(256)
            .await?
            .iter()
            .all(|target| target.id != first_id)
    );
    let third = repository.claim_due_targets("worker-b", 2, NOW + 3).await?;
    assert_eq!(third.len(), 1);
    repository
        .complete_probe(
            leases[1].clone(),
            ActiveChannelProbeOutcome::Healthy { latency_ms: 2 },
            NOW + 4,
        )
        .await?;
    repository
        .complete_probe(
            third[0].clone(),
            ActiveChannelProbeOutcome::Healthy { latency_ms: 2 },
            NOW + 5,
        )
        .await?;
    assert!(
        repository
            .claim_due_targets("worker-b", 2, NOW + 6)
            .await?
            .is_empty()
    );

    let next_window = repository
        .claim_due_targets("worker-b", 2, NOW + 86_400)
        .await?;
    assert_eq!(next_window.len(), 2);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn target_interval_and_per_target_rolling_budget_are_enforced() -> Result<(), Box<dyn Error>>
{
    let pool = test_pool().await?;
    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    repository
        .set_config(ActiveChannelMonitorConfig {
            enabled: true,
            global_daily_budget: 10,
        })
        .await?;
    let (channel, credential) = target_fixture(
        &pool,
        "interval-target",
        "chat",
        Protocol::OpenAiChat,
        ChannelType::OpenAi,
    )
    .await?;
    let mut input = target_input(channel, credential, "chat");
    input.daily_budget = 2;
    repository.upsert_target(input).await?;

    let first = repository
        .claim_due_targets("worker", 1, NOW)
        .await?
        .remove(0);
    repository
        .complete_probe(
            first,
            ActiveChannelProbeOutcome::Healthy { latency_ms: 1 },
            NOW + 1,
        )
        .await?;
    assert!(
        repository
            .claim_due_targets("worker", 1, NOW + 299)
            .await?
            .is_empty()
    );
    let second = repository
        .claim_due_targets("worker", 1, NOW + 300)
        .await?
        .remove(0);
    repository
        .complete_probe(
            second,
            ActiveChannelProbeOutcome::Healthy { latency_ms: 1 },
            NOW + 301,
        )
        .await?;
    assert!(
        repository
            .claim_due_targets("worker", 1, NOW + 600)
            .await?
            .is_empty()
    );
    assert_eq!(
        repository
            .claim_due_targets("worker", 1, NOW + 86_400)
            .await?
            .len(),
        1
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn down_is_deduplicated_muted_down_is_not_notified_and_recovery_is_notified_to_enabled_admins()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let InitialSetupOutcome::Initialized { user_id: admin_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "monitor-admin".to_owned(),
                "a secure monitor password".to_owned(),
            ))
            .await?
    else {
        panic!("initial admin must be created")
    };
    let admin = users::Entity::find_by_id(admin_id.get())
        .one(pool.connection())
        .await?
        .expect("admin exists");
    let mut regular_user: users::ActiveModel = admin.clone().into_active_model();
    regular_user.id = sea_orm::ActiveValue::NotSet;
    regular_user.username = Set("monitor-user".to_owned());
    regular_user.email = Set(None);
    regular_user.role = Set(0);
    regular_user.aff_code = Set("monitor-user-aff".to_owned());
    regular_user.insert(pool.connection()).await?;

    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    repository
        .set_config(ActiveChannelMonitorConfig {
            enabled: true,
            global_daily_budget: 10,
        })
        .await?;
    let (channel, credential) = target_fixture(
        &pool,
        "alert-target",
        "chat",
        Protocol::OpenAiChat,
        ChannelType::OpenAi,
    )
    .await?;
    let mut input = target_input(channel, credential, "chat");
    input.muted_until_epoch_seconds = Some(NOW + 10_000);
    input.alert_after_secs = 60;
    let target = repository.upsert_target(input).await?;

    let first = repository
        .claim_due_targets("worker", 1, NOW)
        .await?
        .remove(0);
    repository
        .complete_probe(first, ActiveChannelProbeOutcome::Unhealthy, NOW + 1)
        .await?;
    let second = repository
        .claim_due_targets("worker", 1, NOW + 300)
        .await?
        .remove(0);
    repository
        .complete_probe(second, ActiveChannelProbeOutcome::Unhealthy, NOW + 300)
        .await?;
    let third = repository
        .claim_due_targets("worker", 1, NOW + 600)
        .await?
        .remove(0);
    repository
        .complete_probe(third, ActiveChannelProbeOutcome::Unhealthy, NOW + 600)
        .await?;
    let fourth = repository
        .claim_due_targets("worker", 1, NOW + 900)
        .await?
        .remove(0);
    repository
        .complete_probe(
            fourth,
            ActiveChannelProbeOutcome::Healthy { latency_ms: 7 },
            NOW + 900,
        )
        .await?;

    let events = repository.events(target.id, 100).await?;
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == crate::ActiveChannelMonitorEventKind::Down)
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == crate::ActiveChannelMonitorEventKind::Recovered)
            .count(),
        1
    );
    let notifications = user_notification_events::Entity::find()
        .all(pool.connection())
        .await?;
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].user_id, admin_id.get());
    assert_eq!(notifications[0].channel, 2);
    assert_eq!(notifications[0].delivery_state, 5);
    assert!(notifications[0].source_key.ends_with(":recovered"));
    // Target the monitor migration even when later migrations are appended.
    let migrations = crate::migration::Migrator::migrations();
    let protected_migration_index = migrations
        .iter()
        .position(|migration| migration.name() == "m20261110_000179_active_channel_monitor")
        .expect("active channel monitor migration must remain registered");
    let rollback_steps = u32::try_from(migrations.len() - protected_migration_index)?;
    let rollback = crate::migration::Migrator::down(pool.connection(), Some(rollback_steps)).await;
    assert!(matches!(
        rollback,
        Err(sea_orm_migration::DbErr::Custom(message))
            if message == "存在渠道拨测通知事实，无法回退主动拨测迁移"
    ));
    assert!(
        repository.get_config().await.is_ok(),
        "rollback must preserve monitor storage when notification facts exist"
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn monitor_event_history_keeps_only_the_newest_hundred_per_target()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let repository = ActiveChannelMonitorRepository::new(pool.clone());
    let (channel, credential) = target_fixture(
        &pool,
        "history-target",
        "chat",
        Protocol::OpenAiChat,
        ChannelType::OpenAi,
    )
    .await?;
    let target = repository
        .upsert_target(target_input(channel, credential, "chat"))
        .await?;
    for offset in 0..(MAX_ACTIVE_CHANNEL_MONITOR_EVENTS_PER_TARGET + 7) {
        crate::channel_monitor::insert_event(
            pool.connection(),
            target.id,
            "down",
            None,
            timestamp(NOW + offset as i64),
        )
        .await?;
    }
    let count = channel_monitor_events::Entity::find()
        .filter(channel_monitor_events::Column::TargetId.eq(target.id))
        .count(pool.connection())
        .await?;
    assert_eq!(count as usize, MAX_ACTIVE_CHANNEL_MONITOR_EVENTS_PER_TARGET);
    pool.close().await?;
    Ok(())
}

async fn test_pool() -> Result<crate::DatabasePool, crate::DatabaseError> {
    crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await
}

async fn target_fixture(
    pool: &crate::DatabasePool,
    name: &str,
    model: &str,
    protocol: Protocol,
    channel_type: ChannelType,
) -> Result<(ChannelId, CredentialId), Box<dyn Error>> {
    let channel = channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set(channel_type.as_str().to_owned()),
        protocol: Set(protocol.as_str().to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://monitor.example.com").unwrap(),
        )),
        status: Set(Status::Enabled.code()),
        auto_ban: Set(false),
        model_mapping: Set(Json::Object(Default::default())),
        param_override: Set(Json::Object(Default::default())),
        header_override: Set(HeaderOverrides::validate(Json::Object(Default::default())).unwrap()),
        settings: Set(SensitiveJson::from(Json::Object(Default::default()))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let credential = credentials::ActiveModel {
        channel_id: Set(channel.id),
        kind: Set(CredentialKind::ApiKey.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope()).unwrap()),
        status: Set(Status::Enabled.code()),
        schedulable: Set(true),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok((
        ChannelId::new(channel.id)?,
        CredentialId::new(credential.id)?,
    ))
}

fn target_input(
    channel_id: ChannelId,
    credential_id: CredentialId,
    model: &str,
) -> ActiveChannelMonitorTargetInput {
    ActiveChannelMonitorTargetInput {
        channel_id,
        credential_id,
        model: model.to_owned(),
        enabled: true,
        interval_secs: 300,
        daily_budget: 12,
        timeout_secs: 15,
        alert_after_secs: 900,
        muted_until_epoch_seconds: None,
    }
}

fn encrypted_envelope() -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            (
                "key_id".to_owned(),
                Json::String("monitor-test-key".to_owned()),
            ),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn timestamp(value: i64) -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value).unwrap()
}
