use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, CredentialKind, Protocol, ResponsesCompactProbeResult, Status,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    ChannelProbeTargetRepository, CompactProbeStateRepository, CompactProbeStateRepositoryError,
    CompactProbeStateWriteOutcome, DatabaseOptions, MigrationOptions,
    channel_settings::responses_compact_probe_record,
    entity::{
        EncryptedJson, HeaderOverrides, SensitiveJson, channel_models, channels, credentials,
        scheduler_outbox_events,
    },
};

#[tokio::test]
async fn repository_records_fact_without_invalidating_recovery_lease() -> Result<(), Box<dyn Error>>
{
    let pool = test_pool().await?;
    let channel = responses_channel(Status::AutoDisabled)
        .insert(pool.connection())
        .await?;
    attach_target(&pool, channel.id).await?;
    let target = ChannelProbeTargetRepository::new(pool.clone())
        .load(ChannelId::new(channel.id)?)
        .await?
        .expect("自动禁用的完整渠道必须可探活");

    let outcome = CompactProbeStateRepository::new(pool.clone())
        .record(
            target.channel_id(),
            &target.revision(),
            ResponsesCompactProbeResult::Supported,
            1_735_000_000_123,
            Some(200),
        )
        .await?;
    assert_eq!(outcome, CompactProbeStateWriteOutcome::Recorded);

    let saved = channels::Entity::find_by_id(channel.id)
        .one(pool.connection())
        .await?
        .expect("渠道必须仍然存在");
    assert_eq!(saved.updated_at, channel.updated_at);
    let settings = saved.settings.into_inner();
    let record = responses_compact_probe_record(&settings)
        .expect("仓储写入必须保持合法结构")
        .expect("确定性结论必须存在");
    assert_eq!(record.result(), ResponsesCompactProbeResult::Supported);
    assert_eq!(record.checked_at(), 1_735_000_000_123);
    assert_eq!(record.http_status(), Some(200));
    assert_eq!(settings["private_extension"], true);
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .filter(scheduler_outbox_events::Column::SubjectId.eq(channel.id))
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_stale_config_and_older_probe() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let channel = responses_channel(Status::AutoDisabled)
        .insert(pool.connection())
        .await?;
    attach_target(&pool, channel.id).await?;
    let target = ChannelProbeTargetRepository::new(pool.clone())
        .load(ChannelId::new(channel.id)?)
        .await?
        .expect("自动禁用的完整渠道必须可探活");
    let repository = CompactProbeStateRepository::new(pool.clone());
    assert_eq!(
        repository
            .record(
                target.channel_id(),
                &target.revision(),
                ResponsesCompactProbeResult::Unsupported,
                1_735_000_000_200,
                Some(404),
            )
            .await?,
        CompactProbeStateWriteOutcome::Recorded
    );
    assert_eq!(
        repository
            .record(
                target.channel_id(),
                &target.revision(),
                ResponsesCompactProbeResult::Supported,
                1_735_000_000_100,
                Some(200),
            )
            .await?,
        CompactProbeStateWriteOutcome::Stale
    );

    let mut current: channels::ActiveModel = channels::Entity::find_by_id(channel.id)
        .one(pool.connection())
        .await?
        .expect("渠道必须仍然存在")
        .into();
    current.updated_at = Set(TimeDateTimeWithTimeZone::from_unix_timestamp(
        channel.updated_at.unix_timestamp() + 1,
    )?);
    current.update(pool.connection()).await?;
    assert_eq!(
        repository
            .record(
                target.channel_id(),
                &target.revision(),
                ResponsesCompactProbeResult::Supported,
                1_735_000_000_300,
                Some(200),
            )
            .await?,
        CompactProbeStateWriteOutcome::Stale
    );

    let saved = channels::Entity::find_by_id(channel.id)
        .one(pool.connection())
        .await?
        .expect("渠道必须仍然存在");
    let record = responses_compact_probe_record(&saved.settings.into_inner())
        .expect("已保存的探测事实必须保持合法结构")
        .expect("较新的确定性结论必须保留");
    assert_eq!(record.result(), ResponsesCompactProbeResult::Unsupported);
    assert_eq!(record.checked_at(), 1_735_000_000_200);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_unknown_and_zero_timeout() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    assert_eq!(
        CompactProbeStateRepository::with_write_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        CompactProbeStateRepositoryError::InvalidConfiguration
    );
    assert!(!format!("{:?}", CompactProbeStateRepository::new(pool.clone())).contains("sqlite"));

    let channel = responses_channel(Status::AutoDisabled)
        .insert(pool.connection())
        .await?;
    attach_target(&pool, channel.id).await?;
    let target = ChannelProbeTargetRepository::new(pool.clone())
        .load(ChannelId::new(channel.id)?)
        .await?
        .expect("自动禁用的完整渠道必须可探活");
    assert_eq!(
        CompactProbeStateRepository::new(pool.clone())
            .record(
                target.channel_id(),
                &target.revision(),
                ResponsesCompactProbeResult::Unknown,
                1,
                None,
            )
            .await
            .unwrap_err(),
        CompactProbeStateRepositoryError::InvalidInput
    );

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

async fn attach_target(pool: &crate::DatabasePool, channel_id: i64) -> Result<(), sea_orm::DbErr> {
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set("gpt-probe".to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(CredentialKind::ApiKey.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(serde_json::json!({
            "version": 1,
            "algorithm": "xchacha20poly1305",
            "key_id": "compact-probe-key",
            "nonce": URL_SAFE_NO_PAD.encode([0x11; 24]),
            "ciphertext": URL_SAFE_NO_PAD.encode([0x22; 32])
        }))
        .expect("测试密文封套必须有效")),
        status: Set(Status::Enabled.code()),
        schedulable: Set(true),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(())
}

fn responses_channel(status: Status) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set("compact-probe".to_owned()),
        r#type: Set(ChannelType::OpenAi.as_str().to_owned()),
        protocol: Set(Protocol::OpenAiResponses.as_str().to_owned()),
        status: Set(status.code()),
        auto_ban: Set(true),
        model_mapping: Set(serde_json::json!({})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({})).unwrap()),
        settings: Set(SensitiveJson::from(
            serde_json::json!({"private_extension": true}),
        )),
        ..Default::default()
    }
}
