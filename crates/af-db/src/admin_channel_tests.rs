use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, ClientSimulationProfile, CredentialId, GroupId, Protocol,
    ResponsesCompactMode, ResponsesCompactProbeResult, Status,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder};
use sea_orm::{ActiveModelTrait, EntityTrait, JsonValue, Set};

use super::{
    AdminChannelLookupOutcome, AdminChannelRepository, AdminChannelRepositoryConfigError,
    AdminChannelRepositoryError, AdminChannelWriteRecord, AdminCredentialLookupOutcome,
    AdminCredentialPageOutcome, DatabaseOptions, MigrationOptions,
};
use crate::{
    admin_channel::channel_base_query,
    admin_credential::credential_base_query,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channel_groups,
        channel_models, channels, credentials, groups,
    },
};

#[tokio::test]
async fn channel_and_credential_lists_use_stable_scoped_cursors() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture.repository.list(None, 1).await?;
    let (channels, next_cursor) = first.into_parts();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name(), "primary");
    assert_eq!(next_cursor, Some(channels[0].channel_id()));

    let second = fixture.repository.list(next_cursor, 2).await?;
    let (channels, next_cursor) = second.into_parts();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name(), "secondary");
    assert_eq!(next_cursor, None);

    let AdminCredentialPageOutcome::Found(first) = fixture
        .repository
        .list_credentials(fixture.primary_channel_id, None, 1)
        .await?
    else {
        panic!("有效渠道必须返回凭据页");
    };
    let (credentials, next_cursor) = first.into_parts();
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].kind().as_str(), "api_key");
    assert_eq!(credentials[0].channel_id(), fixture.primary_channel_id);
    assert_eq!(next_cursor, Some(credentials[0].credential_id()));

    let AdminCredentialPageOutcome::Found(second) = fixture
        .repository
        .list_credentials(fixture.primary_channel_id, next_cursor, 2)
        .await?
    else {
        panic!("有效渠道必须返回第二页凭据");
    };
    let (credentials, next_cursor) = second.into_parts();
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].kind().as_str(), "oauth");
    assert_eq!(next_cursor, None);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn detail_exposes_only_validated_non_sensitive_metadata() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let AdminChannelLookupOutcome::Found(channel) =
        fixture.repository.get(fixture.primary_channel_id).await?
    else {
        panic!("有效渠道详情必须存在");
    };
    assert_eq!(channel.name(), "primary");
    assert_eq!(channel.channel_type().as_str(), "openai");
    assert_eq!(channel.protocol().as_str(), "openai_chat");
    assert_eq!(channel.base_url(), Some("https://api.example.com/v1"));
    assert_eq!(channel.status(), Status::Enabled);
    assert_eq!(channel.models(), &["public-model"]);
    assert_eq!(channel.group_ids(), &[fixture.primary_group_id]);
    assert_eq!(channel.model_mapping()["public-model"], "upstream-model");
    assert_eq!(channel.param_override()["temperature"], 0);
    assert!(channel.auto_ban_rules().is_empty());
    assert!(!channel.pool_mode());
    assert_eq!(channel.responses_compact_mode(), ResponsesCompactMode::Auto);
    assert_eq!(
        channel.responses_compact_model_mapping(),
        &serde_json::json!({})
    );
    assert_eq!(
        channel.responses_compact_probe_result(),
        ResponsesCompactProbeResult::Unknown
    );
    assert_eq!(channel.responses_compact_probe_checked_at(), None);
    assert_eq!(channel.responses_compact_probe_http_status(), None);
    assert_eq!(format!("{channel:?}"), "AdminChannelRecord(<redacted>)");
    assert!(matches!(
        fixture.repository.get(fixture.deleted_channel_id).await?,
        AdminChannelLookupOutcome::NotFound
    ));

    let AdminCredentialLookupOutcome::Found(credential) = fixture
        .repository
        .get_credential(fixture.primary_channel_id, fixture.primary_credential_id)
        .await?
    else {
        panic!("有效凭据详情必须存在");
    };
    assert_eq!(credential.kind().as_str(), "api_key");
    assert_eq!(credential.status(), Status::Enabled);
    assert_eq!(credential.multi_key_mode(), Some(1));
    assert_eq!(credential.load_factor_micros(), Some(1_100_000));
    assert_eq!(credential.rate_multiplier_micros(), Some(900_000));
    assert_eq!(
        credential.quota_dimension(),
        af_domain::CredentialQuotaDimension::Global
    );
    assert_eq!(credential.oauth_provider(), Some("example-oauth"));
    assert_eq!(
        format!("{credential:?}"),
        "AdminCredentialRecord(<redacted>)"
    );

    assert!(matches!(
        fixture
            .repository
            .get_credential(fixture.secondary_channel_id, fixture.primary_credential_id)
            .await?,
        AdminCredentialLookupOutcome::CredentialNotFound
    ));
    assert!(matches!(
        fixture
            .repository
            .list_credentials(fixture.deleted_channel_id, None, 10)
            .await?,
        AdminCredentialPageOutcome::ChannelNotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn non_object_sensitive_settings_fail_closed_in_database_projection()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let model = channels::Entity::find_by_id(fixture.primary_channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(serde_json::json!("invalid-settings")));
    active.update(fixture.pool.connection()).await?;

    assert_eq!(
        fixture
            .repository
            .get(fixture.primary_channel_id)
            .await
            .unwrap_err(),
        AdminChannelRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn auto_ban_rule_projection_distinguishes_missing_valid_and_null_values()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let model = channels::Entity::find_by_id(fixture.primary_channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(serde_json::json!({
        "auto_ban_rules": {
            "status_codes": [503],
            "keywords": ["workspace disabled"]
        }
    })));
    let model = active.update(fixture.pool.connection()).await?;

    let AdminChannelLookupOutcome::Found(channel) =
        fixture.repository.get(fixture.primary_channel_id).await?
    else {
        panic!("有效渠道详情必须存在");
    };
    assert_eq!(channel.auto_ban_rules().server_statuses()[0].get(), 503);
    assert_eq!(
        channel.auto_ban_rules().keywords(),
        &["workspace disabled".to_owned()]
    );

    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(serde_json::json!({
        "auto_ban_rules": null
    })));
    active.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .repository
            .get(fixture.primary_channel_id)
            .await
            .unwrap_err(),
        AdminChannelRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn pool_mode_projection_accepts_boolean_and_rejects_other_values()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let model = channels::Entity::find_by_id(fixture.primary_channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(serde_json::json!({"pool_mode": true})));
    let model = active.update(fixture.pool.connection()).await?;

    let AdminChannelLookupOutcome::Found(channel) =
        fixture.repository.get(fixture.primary_channel_id).await?
    else {
        panic!("有效渠道详情必须存在");
    };
    assert!(channel.pool_mode());

    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(
        serde_json::json!({"pool_mode": "true"}),
    ));
    active.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .repository
            .get(fixture.primary_channel_id)
            .await
            .unwrap_err(),
        AdminChannelRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn client_simulation_projection_exposes_only_validated_profile() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let model = channels::Entity::find_by_id(fixture.primary_channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    let mut active = channels::ActiveModel::from(model);
    active.r#type = Set("anthropic".to_owned());
    active.protocol = Set("anthropic".to_owned());
    active.settings = Set(SensitiveJson::from(serde_json::json!({
        "client_simulation_profile": "anthropic_cli_headers_v1",
        "private_setting": "must-not-be-returned"
    })));
    let model = active.update(fixture.pool.connection()).await?;

    let AdminChannelLookupOutcome::Found(channel) =
        fixture.repository.get(fixture.primary_channel_id).await?
    else {
        panic!("有效渠道详情必须存在");
    };
    assert_eq!(
        channel.client_simulation_profile(),
        Some(ClientSimulationProfile::AnthropicCliHeadersV1)
    );

    let mut active = channels::ActiveModel::from(model);
    active.settings = Set(SensitiveJson::from(serde_json::json!({
        "client_simulation_profile": "unknown"
    })));
    active.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .repository
            .get(fixture.primary_channel_id)
            .await
            .unwrap_err(),
        AdminChannelRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn compact_projection_exposes_only_validated_capability_fields() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let model = channels::Entity::find_by_id(fixture.primary_channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    let mut active = channels::ActiveModel::from(model);
    active.protocol = Set("openai_responses".to_owned());
    active.settings = Set(SensitiveJson::from(serde_json::json!({
        "responses_compact_mode": "force_on",
        "compact_model_mapping": {"public-model": "compact-model"},
        "responses_compact_probe_result": "supported",
        "responses_compact_probe_checked_at": 1_700_000_000_000_i64,
        "responses_compact_probe_http_status": 200,
        "private_setting": "must-not-be-returned"
    })));
    active.update(fixture.pool.connection()).await?;

    let AdminChannelLookupOutcome::Found(channel) =
        fixture.repository.get(fixture.primary_channel_id).await?
    else {
        panic!("有效渠道详情必须存在");
    };
    assert_eq!(
        channel.responses_compact_mode(),
        ResponsesCompactMode::ForceOn
    );
    assert_eq!(
        channel.responses_compact_model_mapping()["public-model"],
        "compact-model"
    );
    assert_eq!(
        channel.responses_compact_probe_result(),
        ResponsesCompactProbeResult::Supported
    );
    assert_eq!(
        channel.responses_compact_probe_checked_at(),
        Some(1_700_000_000_000)
    );
    assert_eq!(channel.responses_compact_probe_http_status(), Some(200));

    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn read_queries_only_project_non_sensitive_channel_capabilities() {
    let channel_queries = [
        channel_base_query(sea_orm::DbBackend::Postgres).to_string(PostgresQueryBuilder),
        channel_base_query(sea_orm::DbBackend::MySql).to_string(MysqlQueryBuilder),
        channel_base_query(sea_orm::DbBackend::Sqlite).to_string(SqliteQueryBuilder),
    ];
    for sql in channel_queries {
        let sql = sql.to_ascii_lowercase();
        assert!(!sql.contains("header_override"));
        assert!(sql.contains("responses_websocket_enabled"));
        assert!(sql.contains("responses_compact_mode"));
        assert!(sql.contains("responses_compact_model_mapping"));
        assert!(sql.contains("responses_compact_probe_result"));
        assert!(sql.contains("auto_ban_rules"));
        assert!(sql.contains("pool_mode"));
        assert!(sql.contains("client_simulation_profile"));
        assert!(!sql.contains("as \"settings\""));
        assert!(!sql.contains("as `settings`"));
    }

    let credential_queries = [
        credential_base_query().to_string(PostgresQueryBuilder),
        credential_base_query().to_string(MysqlQueryBuilder),
        credential_base_query().to_string(SqliteQueryBuilder),
    ];
    for sql in credential_queries {
        let sql = sql.to_ascii_lowercase();
        assert!(!sql.contains("secret"));
        // 内部原因只用于派生共享认证状态，不会进入管理 API 响应。
        assert!(sql.contains("temp_unschedulable_reason"));
    }
}

#[tokio::test]
async fn invalid_limits_and_closed_pool_fail_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for limit in [0, 101] {
        assert_eq!(
            fixture.repository.list(None, limit).await.unwrap_err(),
            AdminChannelRepositoryError::Invariant
        );
        assert_eq!(
            fixture
                .repository
                .list_credentials(fixture.primary_channel_id, None, limit)
                .await
                .unwrap_err(),
            AdminChannelRepositoryError::Invariant
        );
    }
    fixture.pool.clone().close().await?;
    assert_eq!(
        fixture.repository.list(None, 1).await.unwrap_err(),
        AdminChannelRepositoryError::Query
    );
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        AdminChannelRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminChannelRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminChannelRepository,
    primary_channel_id: ChannelId,
    primary_group_id: GroupId,
    secondary_channel_id: ChannelId,
    deleted_channel_id: ChannelId,
    primary_credential_id: CredentialId,
}

#[tokio::test]
async fn newly_created_channels_remain_readable() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    exercise_newly_created_channels(&pool).await?;
    pool.close().await?;
    Ok(())
}

/// 同时由 SQLite 单测和三方言迁移冒烟调用，验证写入后真实查询的类型解码。
pub(crate) async fn exercise_newly_created_channels(
    pool: &super::DatabasePool,
) -> Result<(), Box<dyn Error>> {
    let repository = AdminChannelRepository::new(pool.clone(), Duration::from_secs(5))?;
    let group = groups::ActiveModel {
        name: Set("channel-read-regression".to_owned()),
        display_name: Set("渠道读取回归".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    for (protocol, enabled) in [
        (Protocol::OpenAiChat, false),
        (Protocol::OpenAiResponses, true),
    ] {
        let models = if enabled {
            vec!["public-model".to_owned()]
        } else {
            Vec::new()
        };
        let group_ids = if enabled {
            vec![GroupId::new(group.id)?]
        } else {
            Vec::new()
        };
        eprintln!("渠道回归阶段：创建 {} 渠道", protocol.as_str());
        let created = repository
            .create_channel(
                AdminChannelWriteRecord::new(
                    format!("channel-read-{}", protocol.as_str()),
                    ChannelType::OpenAi,
                    protocol,
                    Some("https://api.example.com".to_owned()),
                    None,
                    Status::Enabled,
                    1,
                    0,
                    true,
                    models.clone(),
                    group_ids.clone(),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    None,
                )
                .with_pool_mode(enabled)
                .with_responses_websocket_enabled(enabled),
            )
            .await?;
        let channel_id = created.channel_id();
        eprintln!("渠道回归阶段：读取渠道详情");
        let AdminChannelLookupOutcome::Found(detail) = repository.get(channel_id).await? else {
            panic!("刚创建的渠道必须能读取详情");
        };
        assert_eq!(detail.models(), models);
        assert_eq!(detail.group_ids(), group_ids);
        assert_eq!(detail.model_mapping(), &serde_json::json!({}));
        assert_eq!(detail.param_override(), &serde_json::json!({}));
        assert_eq!(detail.pool_mode(), enabled);
        assert_eq!(detail.responses_websocket_enabled(), enabled);
        eprintln!("渠道回归阶段：读取渠道列表");
        let (page, cursor) = repository
            .list(ChannelId::new(channel_id.get() - 1).ok(), 1)
            .await?
            .into_parts();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].channel_id(), channel_id);
        assert_eq!(page[0].pool_mode(), enabled);
        assert_eq!(page[0].responses_websocket_enabled(), enabled);
        assert_eq!(cursor, None);
        eprintln!("渠道回归阶段：读取空凭据列表");
        let AdminCredentialPageOutcome::Found(empty) =
            repository.list_credentials(channel_id, None, 20).await?
        else {
            panic!("无凭据的新渠道必须返回空凭据页");
        };
        assert!(empty.into_parts().0.is_empty());
        eprintln!("渠道回归阶段：添加凭据并读取列表与详情");
        let credential = insert_credential(pool, channel_id.get(), "api_key", None).await?;
        let AdminCredentialPageOutcome::Found(page) =
            repository.list_credentials(channel_id, None, 20).await?
        else {
            panic!("添加凭据后必须能读取凭据页");
        };
        assert_eq!(page.into_parts().0.len(), 1);
        assert!(matches!(
            repository
                .get_credential(channel_id, CredentialId::new(credential.id)?)
                .await?,
            AdminCredentialLookupOutcome::Found(_)
        ));

        // 类型修复不能把非法 JSON 状态静默当成关闭。
        eprintln!("渠道回归阶段：拒绝非法 JSON 配置");
        let model = channels::Entity::find_by_id(channel_id.get())
            .one(pool.connection())
            .await?
            .unwrap();
        for settings in [
            serde_json::json!({"pool_mode": "true"}),
            serde_json::json!({"responses_websocket_enabled": "true"}),
        ] {
            let mut active = channels::ActiveModel::from(model.clone());
            active.settings = Set(SensitiveJson::from(settings));
            active.update(pool.connection()).await?;
            assert_eq!(
                repository.get(channel_id).await.unwrap_err(),
                AdminChannelRepositoryError::Invariant
            );
        }
        let mut active = channels::ActiveModel::from(model.clone());
        active.settings = Set(model.settings);
        active.update(pool.connection()).await?;
    }
    Ok(())
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let primary = insert_channel(&pool, "primary").await?;
    let secondary = insert_channel(&pool, "secondary").await?;
    let deleted = insert_channel(&pool, "deleted").await?;
    let mut deleted_model = channels::ActiveModel::from(deleted.clone());
    deleted_model.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    deleted_model.update(pool.connection()).await?;
    let group = groups::ActiveModel {
        name: Set("primary-group".to_owned()),
        display_name: Set("Primary Group".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_models::ActiveModel {
        channel_id: Set(primary.id),
        model: Set("public-model".to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(primary.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let first = insert_credential(&pool, primary.id, "api_key", Some(1)).await?;
    insert_credential(&pool, primary.id, "oauth", Some(2)).await?;
    insert_credential(&pool, secondary.id, "api_key", None).await?;
    let removed = insert_credential(&pool, primary.id, "api_key", None).await?;
    let mut removed_model = credentials::ActiveModel::from(removed);
    removed_model.deleted_at = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
    ));
    removed_model.update(pool.connection()).await?;

    Ok(Fixture {
        repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        primary_channel_id: ChannelId::new(primary.id)?,
        primary_group_id: GroupId::new(group.id)?,
        secondary_channel_id: ChannelId::new(secondary.id)?,
        deleted_channel_id: ChannelId::new(deleted.id)?,
        primary_credential_id: CredentialId::new(first.id)?,
    })
}

async fn insert_channel(
    pool: &super::DatabasePool,
    name: &str,
) -> Result<channels::Model, sea_orm::DbErr> {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com/v1").expect("测试地址必须有效"),
        )),
        status: Set(1),
        weight: Set(10),
        priority: Set(20),
        auto_ban: Set(true),
        model_mapping: Set(serde_json::json!({"public-model": "upstream-model"})),
        param_override: Set(serde_json::json!({"temperature": 0})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({
            "x-private-header": "header-value-canary"
        }))
        .expect("测试 Header 覆盖必须有效")),
        balance: Set(Some(1_000)),
        used_quota: Set(25),
        settings: Set(SensitiveJson::from(serde_json::json!({
            "private_setting": "settings-value-canary"
        }))),
        tag: Set(Some("primary".to_owned())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn insert_credential(
    pool: &super::DatabasePool,
    channel_id: i64,
    kind: &str,
    multi_key_mode: Option<i16>,
) -> Result<credentials::Model, sea_orm::DbErr> {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(kind.to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope()).expect("测试密文必须有效")),
        status: Set(1),
        multi_key_mode: Set(multi_key_mode),
        priority: Set(30),
        weight: Set(40),
        concurrency: Set(Some(2)),
        load_factor_micros: Set(Some(1_100_000)),
        rate_multiplier_micros: Set(Some(900_000)),
        schedulable: Set(true),
        quota_dimension: Set("global".to_owned()),
        oauth_provider: Set(Some("example-oauth".to_owned())),
        oauth_account_key: Set(Some("account-canary".to_owned())),
        oauth_project_id: Set(Some("project-canary".to_owned())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

fn encrypted_envelope() -> JsonValue {
    serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "admin-read-test",
        "nonce": URL_SAFE_NO_PAD.encode([0x42; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode(b"credential-secret-canary-with-tag")
    })
}
