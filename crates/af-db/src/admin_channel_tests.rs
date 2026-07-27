use std::{error::Error, time::Duration};

use af_domain::{ChannelId, CredentialId, GroupId, Status};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder};
use sea_orm::{ActiveModelTrait, JsonValue, Set};

use super::{
    AdminChannelLookupOutcome, AdminChannelRepository, AdminChannelRepositoryConfigError,
    AdminChannelRepositoryError, AdminCredentialLookupOutcome, AdminCredentialPageOutcome,
    DatabaseOptions, MigrationOptions,
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
    assert_eq!(credential.quota_dimension(), "global");
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

#[test]
fn read_queries_never_select_sensitive_columns() {
    let channel_queries = [
        channel_base_query(sea_orm::DbBackend::Postgres).to_string(PostgresQueryBuilder),
        channel_base_query(sea_orm::DbBackend::MySql).to_string(MysqlQueryBuilder),
        channel_base_query(sea_orm::DbBackend::Sqlite).to_string(SqliteQueryBuilder),
    ];
    for sql in channel_queries {
        let sql = sql.to_ascii_lowercase();
        assert!(!sql.contains("header_override"));
        assert!(!sql.contains("settings"));
    }

    let credential_queries = [
        credential_base_query().to_string(PostgresQueryBuilder),
        credential_base_query().to_string(MysqlQueryBuilder),
        credential_base_query().to_string(SqliteQueryBuilder),
    ];
    for sql in credential_queries {
        let sql = sql.to_ascii_lowercase();
        assert!(!sql.contains("secret"));
        assert!(!sql.contains("temp_unschedulable_reason"));
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
