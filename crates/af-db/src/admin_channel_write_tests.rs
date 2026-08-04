use std::{error::Error, sync::Mutex, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement};

use super::{
    AdminChannelDeleteOutcome, AdminChannelMutationOutcome, AdminChannelRepository,
    AdminChannelWriteRecord, AdminChannelWriteRepositoryError, AdminCredentialCreateOutcome,
    AdminCredentialDeleteOutcome, AdminCredentialMutationOutcome, AdminCredentialWriteRecord,
    DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions,
};

#[tokio::test]
async fn channel_create_and_update_preserve_runtime_totals() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let created = fixture
        .repository
        .create_channel(channel_record("primary", "https://api.example.com/v1"))
        .await?;
    let channel_id = created.channel_id();
    assert_eq!(created.timeout().unwrap().seconds(), 60);
    assert_eq!(created.used_quota(), 0);
    assert_eq!(created.balance(), None);

    execute(
        &fixture,
        "UPDATE channels SET balance = ?, used_quota = ? WHERE id = ?",
        [5_000_i64.into(), 125_i64.into(), channel_id.get().into()],
    )
    .await?;
    let AdminChannelMutationOutcome::Mutated(updated) = fixture
        .repository
        .update_channel(
            channel_id,
            channel_update_record("renamed", "https://gateway.example.com/openai"),
        )
        .await?
    else {
        panic!("有效渠道必须完成更新");
    };
    assert_eq!(updated.name(), "renamed");
    assert_eq!(
        updated.base_url(),
        Some("https://gateway.example.com/openai")
    );
    assert_eq!(updated.balance(), Some(5_000));
    assert_eq!(updated.used_quota(), 125);
    assert_eq!(updated.timeout().unwrap().seconds(), 90);
    let (header_override, settings) = channel_sensitive_fields(&fixture, channel_id).await?;
    assert_eq!(header_override, r#"{"x-runtime":"private-header-value"}"#);
    assert_eq!(settings, r#"{"private_setting":"private-setting-value"}"#);

    assert_eq!(
        fixture
            .repository
            .create_channel(AdminChannelWriteRecord::new(
                "mismatched".to_owned(),
                ChannelType::Anthropic,
                Protocol::OpenAiChat,
                None,
                None,
                Status::Disabled,
                0,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            ))
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidInput
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn responses_websocket_toggle_preserves_unknown_sensitive_settings()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let create = AdminChannelWriteRecord::new(
        "responses".to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiResponses,
        Some("https://api.example.com/v1".to_owned()),
        Some(af_domain::ChannelTimeout::new(60).unwrap()),
        Status::Enabled,
        10,
        20,
        true,
        Vec::new(),
        Vec::new(),
        serde_json::json!({}),
        serde_json::json!({}),
        serde_json::json!({}),
        serde_json::json!({"private_setting": "private-setting-value"}),
        None,
    )
    .with_responses_websocket_enabled(true);
    let created = fixture.repository.create_channel(create).await?;
    assert!(created.responses_websocket_enabled());

    // 显式结构化开关必须能够修复旧版本留下的非法受控值，同时保留未知设置。
    execute(
        &fixture,
        "UPDATE channels SET settings = ? WHERE id = ?",
        [
            serde_json::json!({
                "responses_websocket_enabled": "invalid",
                "private_setting": "private-setting-value"
            })
            .into(),
            created.channel_id().get().into(),
        ],
    )
    .await?;

    let update = AdminChannelWriteRecord::new_update(
        "responses".to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiResponses,
        Some("https://api.example.com/v1".to_owned()),
        Some(af_domain::ChannelTimeout::new(60).unwrap()),
        Status::Enabled,
        10,
        20,
        true,
        Vec::new(),
        Vec::new(),
        serde_json::json!({}),
        serde_json::json!({}),
        None,
        None,
        None,
    )
    .with_responses_websocket_enabled(false);
    let AdminChannelMutationOutcome::Mutated(updated) = fixture
        .repository
        .update_channel(created.channel_id(), update)
        .await?
    else {
        panic!("有效 Responses 渠道必须完成更新");
    };
    assert!(!updated.responses_websocket_enabled());
    let (_, settings) = channel_sensitive_fields(&fixture, created.channel_id()).await?;
    let settings: serde_json::Value = serde_json::from_str(&settings)?;
    assert_eq!(settings["private_setting"], "private-setting-value");
    assert!(settings.get("responses_websocket_enabled").is_none());

    let invalid =
        channel_record("chat", "https://api.example.com/v1").with_responses_websocket_enabled(true);
    assert_eq!(
        fixture
            .repository
            .create_channel(invalid)
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidInput
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_create_uses_real_id_and_failure_rolls_back() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let observed_id = Mutex::new(None);
    let AdminCredentialCreateOutcome::Created(created) = fixture
        .repository
        .create_credential(
            channel_id,
            credential_record(CredentialKind::ApiKey, None),
            |id| {
                *observed_id.lock().expect("测试锁不得中毒") = Some(id);
                Ok(envelope(0x31))
            },
        )
        .await?
    else {
        panic!("有效渠道必须创建凭据");
    };
    assert_eq!(
        *observed_id.lock().expect("测试锁不得中毒"),
        Some(created.credential_id())
    );
    let stored = credential_secret(&fixture, created.credential_id()).await?;
    assert!(stored.contains("write-test-key"));
    assert!(!stored.contains("transaction-pending"));

    let before = credential_count(&fixture).await?;
    assert_eq!(
        fixture
            .repository
            .create_credential(
                channel_id,
                credential_record(CredentialKind::ApiKey, None),
                |_| Err(()),
            )
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::SecretPreparation
    );
    assert_eq!(credential_count(&fixture).await?, before);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_update_preserves_secret_and_runtime_state() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let credential_id = create_credential(&fixture, channel_id, None, 0x41).await?;
    let original_secret = credential_secret(&fixture, credential_id).await?;
    execute(
        &fixture,
        "UPDATE credentials SET rate_limited_at = ?, last_used_at = ? WHERE id = ?",
        [
            "2026-01-01T00:00:00Z".into(),
            "2026-01-02T00:00:00Z".into(),
            credential_id.get().into(),
        ],
    )
    .await?;

    let AdminCredentialMutationOutcome::Mutated(updated) = fixture
        .repository
        .update_credential(
            channel_id,
            credential_id,
            AdminCredentialWriteRecord::new(
                CredentialKind::ApiKey,
                Status::Disabled,
                Some(2),
                9,
                8,
                Some(7),
                Some(600_000),
                Some(700_000),
                false,
                None,
                "spark",
                None,
                None,
                None,
                None,
            ),
            None,
        )
        .await?
    else {
        panic!("有效凭据必须完成更新");
    };
    assert_eq!(updated.weight(), 8);
    assert_eq!(updated.rate_limited_at(), Some(1_767_225_600));
    assert_eq!(updated.last_used_at(), Some(1_767_312_000));
    assert_eq!(
        credential_secret(&fixture, credential_id).await?,
        original_secret
    );

    fixture
        .repository
        .update_credential(
            channel_id,
            credential_id,
            credential_record(CredentialKind::ApiKey, None),
            Some(envelope(0x52)),
        )
        .await?;
    assert_ne!(
        credential_secret(&fixture, credential_id).await?,
        original_secret
    );
    assert_eq!(
        fixture
            .repository
            .update_credential(
                channel_id,
                credential_id,
                credential_record(CredentialKind::Oauth, None),
                Some(envelope(0x63)),
            )
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidInput
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn oauth_credential_update_invalidates_concurrent_refresh_guards()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let AdminCredentialCreateOutcome::Created(created) = fixture
        .repository
        .create_credential(channel_id, oauth_credential_record("codex", None), |_| {
            Ok(envelope(0x64))
        })
        .await?
    else {
        panic!("有效渠道必须创建 OAuth 凭据");
    };
    let credential_id = created.credential_id();
    execute(
        &fixture,
        "UPDATE credentials SET oauth_revision = ?, oauth_expires_at_epoch_seconds = ? WHERE id = ?",
        [
            4_i64.into(),
            1_800_000_000_i64.into(),
            credential_id.get().into(),
        ],
    )
    .await?;

    let AdminCredentialMutationOutcome::Mutated(_) = fixture
        .repository
        .update_credential(
            channel_id,
            credential_id,
            oauth_credential_record("codex", None),
            Some(envelope(0x65)),
        )
        .await?
    else {
        panic!("手工换密钥必须完成更新");
    };
    let (revision, expiration, provider, replaced_secret) =
        oauth_credential_state(&fixture, credential_id).await?;
    assert_eq!(revision, 5);
    assert_eq!(expiration, None);
    assert_eq!(provider.as_deref(), Some("codex"));

    execute(
        &fixture,
        "UPDATE credentials SET oauth_expires_at_epoch_seconds = ? WHERE id = ?",
        [1_900_000_000_i64.into(), credential_id.get().into()],
    )
    .await?;
    let AdminCredentialMutationOutcome::Mutated(_) = fixture
        .repository
        .update_credential(
            channel_id,
            credential_id,
            oauth_credential_record("gemini", None),
            None,
        )
        .await?
    else {
        panic!("切换 Provider 必须完成更新");
    };
    let (revision, expiration, provider, current_secret) =
        oauth_credential_state(&fixture, credential_id).await?;
    assert_eq!(revision, 6);
    assert_eq!(expiration, None);
    assert_eq!(provider.as_deref(), Some("gemini"));
    assert_eq!(current_secret, replaced_secret);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn deletes_cascade_shadow_credentials_and_clear_channel_relations()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let parent_id = create_credential(&fixture, channel_id, None, 0x71).await?;
    let child_id = create_credential(&fixture, channel_id, Some(parent_id), 0x72).await?;
    seed_relations(&fixture, channel_id).await?;

    assert_eq!(
        fixture
            .repository
            .delete_credential(channel_id, parent_id)
            .await?,
        AdminCredentialDeleteOutcome::Deleted
    );
    for id in [parent_id, child_id] {
        assert!(credential_deleted(&fixture, id).await?);
    }

    let second_id = create_credential(&fixture, channel_id, None, 0x73).await?;
    assert_eq!(
        fixture.repository.delete_channel(channel_id).await?,
        AdminChannelDeleteOutcome::Deleted
    );
    assert!(credential_deleted(&fixture, second_id).await?);
    for table in ["abilities", "channel_models", "channel_groups"] {
        assert_eq!(
            relation_count(&fixture, table, channel_id).await?,
            0,
            "{table}"
        );
    }
    assert_eq!(
        fixture.repository.delete_channel(channel_id).await?,
        AdminChannelDeleteOutcome::NotFound
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminChannelRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
    })
}

async fn create_channel(fixture: &Fixture) -> Result<ChannelId, Box<dyn Error>> {
    Ok(fixture
        .repository
        .create_channel(channel_record("primary", "https://api.example.com/v1"))
        .await?
        .channel_id())
}

async fn create_credential(
    fixture: &Fixture,
    channel_id: ChannelId,
    parent_id: Option<CredentialId>,
    marker: u8,
) -> Result<CredentialId, Box<dyn Error>> {
    let AdminCredentialCreateOutcome::Created(credential) = fixture
        .repository
        .create_credential(
            channel_id,
            credential_record(CredentialKind::ApiKey, parent_id),
            |_| Ok(envelope(marker)),
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    Ok(credential.credential_id())
}

fn channel_record(name: &str, base_url: &str) -> AdminChannelWriteRecord {
    AdminChannelWriteRecord::new(
        name.to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some(base_url.to_owned()),
        Some(af_domain::ChannelTimeout::new(60).unwrap()),
        Status::Enabled,
        10,
        20,
        true,
        Vec::new(),
        Vec::new(),
        serde_json::json!({"public-model": "upstream-model"}),
        serde_json::json!({"temperature": 0}),
        serde_json::json!({"x-runtime": "private-header-value"}),
        serde_json::json!({"private_setting": "private-setting-value"}),
        Some("test".to_owned()),
    )
}

fn channel_update_record(name: &str, base_url: &str) -> AdminChannelWriteRecord {
    AdminChannelWriteRecord::new_update(
        name.to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some(base_url.to_owned()),
        Some(af_domain::ChannelTimeout::new(90).unwrap()),
        Status::Enabled,
        10,
        20,
        true,
        Vec::new(),
        Vec::new(),
        serde_json::json!({"public-model": "upstream-model"}),
        serde_json::json!({"temperature": 0}),
        None,
        None,
        Some("test".to_owned()),
    )
}

fn credential_record(
    kind: CredentialKind,
    parent_id: Option<CredentialId>,
) -> AdminCredentialWriteRecord {
    if kind == CredentialKind::Oauth {
        return oauth_credential_record("example", parent_id);
    }
    AdminCredentialWriteRecord::new(
        kind,
        Status::Enabled,
        Some(1),
        10,
        20,
        Some(2),
        Some(1_000_000),
        Some(900_000),
        true,
        parent_id,
        "global",
        None,
        None,
        None,
        None,
    )
}

fn oauth_credential_record(
    provider: &str,
    parent_id: Option<CredentialId>,
) -> AdminCredentialWriteRecord {
    AdminCredentialWriteRecord::new(
        CredentialKind::Oauth,
        Status::Enabled,
        Some(1),
        10,
        20,
        Some(2),
        Some(1_000_000),
        Some(900_000),
        true,
        parent_id,
        "global",
        None,
        Some(provider.to_owned()),
        Some("account".to_owned()),
        Some("project".to_owned()),
    )
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("write-test-key", [marker; 24], vec![marker; 32]).unwrap()
}

async fn execute<const N: usize>(
    fixture: &Fixture,
    sql: &str,
    values: [sea_orm::Value; N],
) -> Result<(), sea_orm::DbErr> {
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await?;
    Ok(())
}

async fn credential_secret(
    fixture: &Fixture,
    credential_id: CredentialId,
) -> Result<String, sea_orm::DbErr> {
    let row = query_one(
        fixture,
        "SELECT secret FROM credentials WHERE id = ?",
        [credential_id.get().into()],
    )
    .await?;
    row.try_get("", "secret")
}

async fn oauth_credential_state(
    fixture: &Fixture,
    credential_id: CredentialId,
) -> Result<(i64, Option<i64>, Option<String>, String), sea_orm::DbErr> {
    let row = query_one(
        fixture,
        "SELECT oauth_revision, oauth_expires_at_epoch_seconds, oauth_provider, secret FROM credentials WHERE id = ?",
        [credential_id.get().into()],
    )
    .await?;
    Ok((
        row.try_get("", "oauth_revision")?,
        row.try_get("", "oauth_expires_at_epoch_seconds")?,
        row.try_get("", "oauth_provider")?,
        row.try_get("", "secret")?,
    ))
}

async fn channel_sensitive_fields(
    fixture: &Fixture,
    channel_id: ChannelId,
) -> Result<(String, String), sea_orm::DbErr> {
    let row = query_one(
        fixture,
        "SELECT header_override, settings FROM channels WHERE id = ?",
        [channel_id.get().into()],
    )
    .await?;
    Ok((
        row.try_get("", "header_override")?,
        row.try_get("", "settings")?,
    ))
}

async fn credential_count(fixture: &Fixture) -> Result<i64, sea_orm::DbErr> {
    query_one(fixture, "SELECT COUNT(*) AS count FROM credentials", [])
        .await?
        .try_get("", "count")
}

async fn credential_deleted(
    fixture: &Fixture,
    credential_id: CredentialId,
) -> Result<bool, sea_orm::DbErr> {
    let deleted: Option<String> = query_one(
        fixture,
        "SELECT deleted_at FROM credentials WHERE id = ?",
        [credential_id.get().into()],
    )
    .await?
    .try_get("", "deleted_at")?;
    Ok(deleted.is_some())
}

async fn relation_count(
    fixture: &Fixture,
    table: &str,
    channel_id: ChannelId,
) -> Result<i64, sea_orm::DbErr> {
    let sql = match table {
        "abilities" => "SELECT COUNT(*) AS count FROM abilities WHERE channel_id = ?",
        "channel_models" => "SELECT COUNT(*) AS count FROM channel_models WHERE channel_id = ?",
        "channel_groups" => "SELECT COUNT(*) AS count FROM channel_groups WHERE channel_id = ?",
        _ => unreachable!("测试只允许固定关系表"),
    };
    query_one(fixture, sql, [channel_id.get().into()])
        .await?
        .try_get("", "count")
}

async fn query_one<const N: usize>(
    fixture: &Fixture,
    sql: &str,
    values: [sea_orm::Value; N],
) -> Result<QueryResult, sea_orm::DbErr> {
    fixture
        .pool
        .connection()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            sql,
            values,
        ))
        .await?
        .ok_or_else(|| sea_orm::DbErr::Custom("测试查询缺少结果".to_owned()))
}

async fn seed_relations(fixture: &Fixture, channel_id: ChannelId) -> Result<(), sea_orm::DbErr> {
    execute(
        fixture,
        "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
        [
            1_i64.into(),
            "default".into(),
            "Default".into(),
            "{}".into(),
        ],
    )
    .await?;
    execute(
        fixture,
        "INSERT INTO channel_models (channel_id, model) VALUES (?, ?)",
        [channel_id.get().into(), "gpt-test".into()],
    )
    .await?;
    execute(
        fixture,
        "INSERT INTO channel_groups (channel_id, group_id) VALUES (?, ?)",
        [channel_id.get().into(), 1_i64.into()],
    )
    .await?;
    execute(
        fixture,
        "INSERT INTO abilities (group_id, model, channel_id, enabled, priority, weight) VALUES (?, ?, ?, ?, ?, ?)",
        [
            1_i64.into(),
            "gpt-test".into(),
            channel_id.get().into(),
            true.into(),
            20_i32.into(),
            10_i32.into(),
        ],
    )
    .await
}
