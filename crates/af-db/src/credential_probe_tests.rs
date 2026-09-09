use std::{error::Error, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, ResponsesCompactMode, Status};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, Set, entity::prelude::Json};

use crate::{
    ChannelProbeTargetRepository, ChannelProbeTargetRepositoryError, DatabaseOptions,
    MigrationOptions,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, SensitiveString,
        channel_models, channels, credentials, proxies,
    },
};

#[tokio::test]
async fn load_selects_stable_model_and_highest_priority_credential() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let mut channel = channel(
        "probe-target",
        Status::AutoDisabled,
        true,
        "https://probe-url-canary.example.com/prefix",
        [("x-probe-header", "probe-header-value-canary")],
    );
    channel.timeout_secs = Set(Some(60));
    let channel = channel.insert(pool.connection()).await?;
    channel_model(channel.id, "z-model-canary")
        .insert(pool.connection())
        .await?;
    channel_model(channel.id, "a-model-canary")
        .insert(pool.connection())
        .await?;
    credential(
        channel.id,
        CredentialKind::ApiKey,
        10,
        true,
        Status::Enabled,
        None,
        "low-key-id-canary",
        0x11,
    )
    .insert(pool.connection())
    .await?;
    proxy(91).insert(pool.connection()).await?;
    let selected = credential(
        channel.id,
        CredentialKind::Oauth,
        20,
        true,
        Status::Enabled,
        Some(91),
        "selected-key-id-canary",
        0x22,
    )
    .insert(pool.connection())
    .await?;
    credential(
        channel.id,
        CredentialKind::ApiKey,
        30,
        false,
        Status::Enabled,
        None,
        "unschedulable-key-id-canary",
        0x33,
    )
    .insert(pool.connection())
    .await?;
    let mut pending = credential(
        channel.id,
        CredentialKind::Oauth,
        40,
        true,
        Status::Enabled,
        None,
        "pending-key-id-canary",
        0x44,
    );
    pending.oauth_token_pending = Set(true);
    pending.insert(pool.connection()).await?;

    let target = ChannelProbeTargetRepository::new(pool.clone())
        .load(channel_id(channel.id))
        .await?
        .expect("满足条件的渠道必须返回真实探活目标");

    assert_eq!(target.channel_id(), channel_id(channel.id));
    assert_eq!(target.credential_id(), selected.id);
    assert_eq!(target.channel_type(), ChannelType::OpenAi);
    assert_eq!(target.protocol(), Protocol::OpenAiChat);
    assert_eq!(
        target.base_url(),
        Some("https://probe-url-canary.example.com/prefix")
    );
    assert_eq!(target.timeout().unwrap().seconds(), 60);
    assert_eq!(target.model(), "a-model-canary");
    assert_eq!(target.credential_kind(), CredentialKind::Oauth);
    assert_eq!(target.envelope().key_id(), "selected-key-id-canary");
    assert_eq!(target.headers().len(), 1);
    assert_eq!(target.headers()[0].name(), "x-probe-header");
    assert_eq!(target.headers()[0].value(), "probe-header-value-canary");
    assert!(target.proxy_required());
    assert_eq!(target.responses_compact_mode(), ResponsesCompactMode::Auto);
    assert_eq!(target.responses_compact_model(), None);

    let rendered = format!("{target:?}");
    for secret in [
        "probe-url-canary",
        "a-model-canary",
        "probe-header-value-canary",
        "selected-key-id-canary",
        &URL_SAFE_NO_PAD.encode([0x22; 32]),
    ] {
        assert!(!rendered.contains(secret));
    }

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn load_resolves_compact_probe_model_through_both_mapping_layers()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let mut channel = channel(
        "responses-probe-target",
        Status::AutoDisabled,
        true,
        "https://responses-probe.example.com/v1",
        [],
    );
    channel.protocol = Set(Protocol::OpenAiResponses.as_str().to_owned());
    channel.model_mapping = Set(serde_json::json!({
        "public-model": "responses-upstream-model"
    }));
    channel.settings = Set(SensitiveJson::from(serde_json::json!({
        "responses_compact_mode": "force_on",
        "compact_model_mapping": {
            "responses-upstream-model": "compact-upstream-model"
        }
    })));
    let channel = channel.insert(pool.connection()).await?;
    channel_model(channel.id, "public-model")
        .insert(pool.connection())
        .await?;
    credential(
        channel.id,
        CredentialKind::ApiKey,
        0,
        true,
        Status::Enabled,
        None,
        "responses-key",
        0x66,
    )
    .insert(pool.connection())
    .await?;

    let target = ChannelProbeTargetRepository::new(pool.clone())
        .load(channel_id(channel.id))
        .await?
        .expect("原生 Responses 渠道必须返回 Compact 探测目标");
    assert_eq!(
        target.responses_compact_mode(),
        ResponsesCompactMode::ForceOn
    );
    assert_eq!(target.model(), "responses-upstream-model");
    assert_eq!(
        target.responses_compact_model(),
        Some("compact-upstream-model")
    );
    assert!(!format!("{target:?}").contains("compact-upstream-model"));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn load_accepts_active_statuses_but_hides_incomplete_targets() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let enabled = channel(
        "enabled",
        Status::Enabled,
        true,
        "https://enabled.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    attach_complete_target(&pool, enabled.id).await?;

    let missing_model = channel(
        "missing-model",
        Status::AutoDisabled,
        true,
        "https://missing-model.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    credential(
        missing_model.id,
        CredentialKind::ApiKey,
        0,
        true,
        Status::Enabled,
        None,
        "missing-model-key",
        0x44,
    )
    .insert(pool.connection())
    .await?;

    let missing_credential = channel(
        "missing-credential",
        Status::AutoDisabled,
        true,
        "https://missing-credential.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    channel_model(missing_credential.id, "probe-model")
        .insert(pool.connection())
        .await?;

    let repository = ChannelProbeTargetRepository::new(pool.clone());
    assert!(repository.load(channel_id(enabled.id)).await?.is_some());
    for id in [missing_model.id, missing_credential.id] {
        assert_eq!(repository.load(channel_id(id)).await?, None);
    }

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_zero_timeout_and_redacts_connection() -> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;

    assert_eq!(
        ChannelProbeTargetRepository::with_load_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        ChannelProbeTargetRepositoryError::InvalidConfiguration
    );
    assert!(!format!("{:?}", ChannelProbeTargetRepository::new(pool.clone())).contains("sqlite"));

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

async fn attach_complete_target(
    pool: &crate::DatabasePool,
    channel_id: i64,
) -> Result<(), sea_orm::DbErr> {
    channel_model(channel_id, "probe-model")
        .insert(pool.connection())
        .await?;
    credential(
        channel_id,
        CredentialKind::ApiKey,
        0,
        true,
        Status::Enabled,
        None,
        "complete-key",
        0x55,
    )
    .insert(pool.connection())
    .await?;
    Ok(())
}

fn channel<const N: usize>(
    name: &str,
    status: Status,
    auto_ban: bool,
    base_url: &str,
    headers: [(&str, &str); N],
) -> channels::ActiveModel {
    let headers = Json::Object(
        headers
            .into_iter()
            .map(|(name, value)| (name.to_owned(), Json::String(value.to_owned())))
            .collect(),
    );
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(ChannelBaseUrl::parse(base_url).unwrap())),
        status: Set(status.code()),
        auto_ban: Set(auto_ban),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(headers).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

fn channel_model(channel_id: i64, model: &str) -> channel_models::ActiveModel {
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
}

fn proxy(id: i64) -> proxies::ActiveModel {
    let name = format!("probe-proxy-{id}");
    proxies::ActiveModel {
        id: Set(id),
        active_name: Set(Some(name.clone())),
        name: Set(name),
        scheme: Set("http".to_owned()),
        host: Set(SensitiveString::from("proxy.example".to_owned())),
        port: Set(8080),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn credential(
    channel_id: i64,
    kind: CredentialKind,
    priority: i32,
    schedulable: bool,
    status: Status,
    proxy_id: Option<i64>,
    key_id: &str,
    marker: u8,
) -> credentials::ActiveModel {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(kind.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(key_id, marker)).unwrap()),
        status: Set(status.code()),
        priority: Set(priority),
        schedulable: Set(schedulable),
        proxy_id: Set(proxy_id),
        ..Default::default()
    }
}

fn encrypted_envelope(key_id: &str, marker: u8) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(key_id.to_owned())),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn channel_id(value: i64) -> ChannelId {
    ChannelId::new(value).unwrap()
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}
