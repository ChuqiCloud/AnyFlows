use std::{error::Error as StdError, time::Duration};

use af_domain::{
    ChannelId, ChannelType, ClientProfileDefinition, Protocol, Status, UserId,
    built_in_client_profiles,
};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter};
use sea_orm_migration::MigratorTrait;

use super::client_profiles::*;
use crate::{
    AdminChannelRepository, AdminChannelWriteRecord, DatabaseOptions, DatabasePool,
    MigrationOptions,
    entity::{channels, scheduler_outbox_events},
    migration::Migrator,
};

type TestResult = Result<(), Box<dyn StdError>>;

async fn fixture() -> Result<(DatabasePool, ClientProfileRepository, ChannelId), Box<dyn StdError>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let channel = AdminChannelRepository::new(pool.clone(), Duration::from_secs(5))?
        .create_channel(channel_write(serde_json::json!({})))
        .await?
        .channel_id();
    Ok((pool.clone(), ClientProfileRepository::new(pool), channel))
}
fn channel_write(headers: serde_json::Value) -> AdminChannelWriteRecord {
    AdminChannelWriteRecord::new(
        "profile-test".to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some("https://example.com/v1".to_owned()),
        None,
        Status::Enabled,
        1,
        1,
        false,
        Vec::new(),
        Vec::new(),
        serde_json::json!({}),
        serde_json::json!({}),
        headers,
        serde_json::json!({}),
        None,
    )
}
fn custom() -> ClientProfileDefinition {
    let mut definition = built_in_client_profiles().remove(0).definition;
    definition.name = "Example client".to_owned();
    definition
        .identity_headers
        .insert("user-agent".to_owned(), "example/1".to_owned());
    definition
}

#[tokio::test]
async fn revision_is_frozen_and_channel_switch_is_explicit() -> TestResult {
    let (pool, repo, channel) = fixture().await?;
    let actor = UserId::new(1)?;
    let first = repo
        .save(
            "example",
            ClientProfileWrite {
                expected_version: 0,
                definition: custom(),
            },
            actor,
        )
        .await?;
    repo.bind(channel, first.binding.clone(), 0).await?;
    let mut next = custom();
    next.identity_headers
        .insert("user-agent".to_owned(), "example/2".to_owned());
    let second = repo
        .save(
            "example",
            ClientProfileWrite {
                expected_version: 1,
                definition: next,
            },
            actor,
        )
        .await?;
    assert_eq!(second.binding.revision, 2);
    assert_eq!(repo.revision("example", 1).await?, first);
    assert_eq!(
        repo.channel_binding(channel).await?.unwrap().binding,
        first.binding
    );
    assert_eq!(
        runtime_profiles(pool.connection(), &[channel.get()]).await?[&channel.get()],
        Some(first.clone())
    );
    assert_eq!(
        repo.delete_revision("example", 1, 2).await,
        Err(ClientProfileRepositoryError::Conflict)
    );
    assert_eq!(
        repo.bind(channel, second.binding.clone(), 0)
            .await
            .unwrap_err(),
        ClientProfileRepositoryError::Conflict
    );
    repo.bind(channel, second.binding.clone(), 1).await?;
    assert_eq!(
        runtime_profiles(pool.connection(), &[channel.get()]).await?[&channel.get()],
        Some(second)
    );
    repo.delete_revision("example", 1, 2).await?;
    assert_eq!(
        repo.revision("example", 1).await.unwrap_err(),
        ClientProfileRepositoryError::NotFound
    );
    assert_eq!(first.definition.identity_headers["user-agent"], "example/1");
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disable_publishes_invalidation_and_existing_snapshot_survives() -> TestResult {
    let (pool, repo, channel) = fixture().await?;
    let first = repo
        .save(
            "example",
            ClientProfileWrite {
                expected_version: 0,
                definition: custom(),
            },
            UserId::new(1)?,
        )
        .await?;
    repo.bind(channel, first.binding.clone(), 0).await?;
    let before = scheduler_outbox_events::Entity::find()
        .count(pool.connection())
        .await?;
    repo.set_enabled("example", 1, false).await?;
    assert!(repo.bind(channel, first.binding.clone(), 1).await.is_err());
    assert_eq!(
        runtime_profiles(pool.connection(), &[channel.get()]).await?[&channel.get()],
        None
    );
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .count(pool.connection())
            .await?,
        before + 1
    );
    assert_eq!(first.definition.identity_headers["user-agent"], "example/1");
    repo.set_enabled("example", 2, true).await?;
    assert_eq!(
        runtime_profiles(pool.connection(), &[channel.get()]).await?[&channel.get()],
        Some(first)
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn protected_built_ins_and_headers_cannot_be_overwritten() -> TestResult {
    let (pool, repo, channel) = fixture().await?;
    for builtin in built_in_client_profiles() {
        assert_eq!(repo.revision(&builtin.binding.key, 1).await?, builtin);
        assert_eq!(
            repo.save(
                &builtin.binding.key,
                ClientProfileWrite {
                    expected_version: 1,
                    definition: custom()
                },
                UserId::new(1)?
            )
            .await
            .unwrap_err(),
            ClientProfileRepositoryError::Conflict
        );
    }
    let writer = AdminChannelRepository::new(pool.clone(), Duration::from_secs(5))?;
    let conflicting = writer
        .create_channel(channel_write(serde_json::json!({"User-Agent":"override"})))
        .await?
        .channel_id();
    let profile = repo
        .save(
            "example",
            ClientProfileWrite {
                expected_version: 0,
                definition: custom(),
            },
            UserId::new(1)?,
        )
        .await?;
    assert_eq!(
        repo.bind(conflicting, profile.binding.clone(), 0)
            .await
            .unwrap_err(),
        ClientProfileRepositoryError::Conflict
    );
    assert!(repo.channel_binding(conflicting).await?.is_none());
    repo.bind(channel, profile.binding, 0).await?;
    assert_eq!(
        repo.save(
            "example",
            ClientProfileWrite {
                expected_version: 0,
                definition: custom()
            },
            UserId::new(1)?
        )
        .await
        .unwrap_err(),
        ClientProfileRepositoryError::Conflict
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn pagination_filters_and_limits_are_applied_by_database() -> TestResult {
    let (pool, repo, _) = fixture().await?;
    let first = repo
        .list(ClientProfileQuery {
            limit: 2,
            ..Default::default()
        })
        .await?;
    assert_eq!(first.items.len(), 2);
    let second = repo
        .list(ClientProfileQuery {
            after: first.next_cursor,
            limit: 2,
            ..Default::default()
        })
        .await?;
    assert!(
        second
            .items
            .iter()
            .all(|r| first.items.iter().all(|p| p.key != r.key))
    );
    let filtered = repo
        .list(ClientProfileQuery {
            protocol: Some(Protocol::Gemini),
            enabled: Some(true),
            ..Default::default()
        })
        .await?;
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].key, "standard");
    let compatible = repo
        .list(ClientProfileQuery {
            channel_type: Some(ChannelType::OpenCode),
            protocol: Some(Protocol::OpenAiChat),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        compatible
            .items
            .iter()
            .map(|p| p.key.as_str())
            .collect::<Vec<_>>(),
        vec!["opencode", "opencode-public", "standard"]
    );
    assert_eq!(
        repo.list(ClientProfileQuery {
            limit: 101,
            ..Default::default()
        })
        .await
        .unwrap_err(),
        ClientProfileRepositoryError::InvalidInput
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn opencode_entry_updates_preserve_private_settings_and_reject_stale_versions() -> TestResult
{
    let (pool, repo, other_channel) = fixture().await?;
    pool.connection().execute(sea_orm::Statement::from_string(sea_orm::DbBackend::Sqlite,
        "INSERT INTO channels (id, name, type, protocol, model_mapping, param_override, header_override, settings) VALUES (903, 'opencode', 'opencode', 'openai_chat', '{}', '{}', '{}', '{\"opencode_entry\":\"inference_v1\",\"private_setting\":\"retained\"}')".to_owned())).await?;
    let channel = ChannelId::new(903)?;
    let original = repo.opencode_settings(channel).await?;
    let before = scheduler_outbox_events::Entity::find()
        .count(pool.connection())
        .await?;
    let next = repo
        .set_opencode_settings(
            channel,
            &original.version,
            af_domain::OpenCodeEntry::ZenV1,
            Some(true),
        )
        .await?;
    assert!(!original.full_compatibility);
    assert!(next.full_compatibility);
    assert_eq!(next.entry, af_domain::OpenCodeEntry::ZenV1);
    assert_ne!(next.version, original.version);
    assert_eq!(
        repo.set_opencode_entry(
            channel,
            &original.version,
            af_domain::OpenCodeEntry::InferenceV1
        )
        .await
        .unwrap_err(),
        ClientProfileRepositoryError::Conflict
    );
    let rollback = repo
        .set_opencode_entry(
            channel,
            &next.version,
            af_domain::OpenCodeEntry::InferenceV1,
        )
        .await?;
    assert!(
        rollback.full_compatibility,
        "旧入口更新接口必须保留兼容开关"
    );
    assert_ne!(rollback.version, original.version);
    assert_ne!(rollback.version, next.version);
    let row = channels::Entity::find_by_id(channel.get())
        .one(pool.connection())
        .await?
        .unwrap();
    assert_eq!(row.settings.into_inner()["private_setting"], "retained");
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .count(pool.connection())
            .await?,
        before + 2
    );
    assert_eq!(
        repo.opencode_settings(other_channel).await.unwrap_err(),
        ClientProfileRepositoryError::InvalidInput
    );
    let public = serde_json::to_value(rollback)?;
    assert!(public.get("settings").is_none());
    assert!(public.get("private_setting").is_none());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn public_profile_upgrade_preserves_frozen_revisions_and_blocks_used_rollback() -> TestResult
{
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    let boundary = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20261018_000152_add_opencode_public_profile")
        .expect("公开访问档案迁移必须保留");
    // 回滚至目标迁移之前，后续新增迁移不改变此测试的验证范围。
    let rollback_steps = u32::try_from(Migrator::migrations().len() - boundary)?;
    Migrator::up(pool.connection(), Some(u32::try_from(boundary)?)).await?;
    let repo = ClientProfileRepository::new(pool.clone());
    let original = repo.revision("opencode", 1).await?;
    let standard = repo.revision("standard", 1).await?;
    Migrator::up(pool.connection(), None).await?;
    assert_eq!(repo.revision("opencode", 1).await?, original);
    assert_eq!(repo.revision("standard", 1).await?, standard);
    let public = repo.revision("opencode-public", 1).await?;
    assert_eq!(
        public.definition.credential_kinds,
        vec![af_domain::CredentialKind::Public]
    );
    Migrator::down(pool.connection(), Some(rollback_steps)).await?;
    assert!(repo.get("opencode-public").await.is_err());
    Migrator::up(pool.connection(), None).await?;
    pool.connection().execute_unprepared("INSERT INTO channels (id, name, type, protocol, model_mapping, param_override, header_override, settings) VALUES (903, 'public-rollback', 'opencode', 'openai_chat', '{}', '{}', '{}', '{}')").await?;
    repo.bind(ChannelId::new(903)?, public.binding, 0).await?;
    assert!(
        Migrator::down(pool.connection(), Some(rollback_steps))
            .await
            .is_err()
    );
    assert!(repo.get("opencode-public").await?.built_in);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn migration_backfills_legacy_claude_binding_without_changing_settings() -> TestResult {
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    let boundary = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20261018_000151_create_client_profiles")
        .expect("旧 Claude 档案迁移必须保留");
    Migrator::up(pool.connection(), Some(u32::try_from(boundary)?)).await?;
    for (id, body) in [
        (901_i64, ""),
        (
            902,
            ",\"client_simulation_body_profile\":\"anthropic_cli_system_date_v1\"",
        ),
    ] {
        let settings =
            format!("{{\"client_simulation_profile\":\"anthropic_cli_headers_v1\"{body}}}");
        // 使用固定 SQL 创建迁移前实体，避免仓储读取尚未存在的新表。
        pool.connection().execute(sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Sqlite,
            "INSERT INTO channels (id, name, type, protocol, model_mapping, param_override, header_override, settings) VALUES (?, ?, 'anthropic', 'anthropic', '{}', '{}', '{}', ?)",
            [id.into(), format!("legacy-{id}").into(), settings.into()])).await?;
    }
    Migrator::up(pool.connection(), None).await?;
    let repo = ClientProfileRepository::new(pool.clone());
    assert_eq!(
        repo.channel_binding(ChannelId::new(901)?)
            .await?
            .unwrap()
            .binding
            .key,
        "claude-code"
    );
    assert_eq!(
        repo.channel_binding(ChannelId::new(902)?)
            .await?
            .unwrap()
            .binding
            .key,
        "claude-code-date"
    );
    let legacy = channels::Entity::find()
        .filter(channels::Column::Id.is_in([901, 902]))
        .all(pool.connection())
        .await?;
    assert!(legacy.iter().all(
        |c| c.settings.clone().into_inner()["client_simulation_profile"]
            == "anthropic_cli_headers_v1"
    ));
    pool.close().await?;
    Ok(())
}
