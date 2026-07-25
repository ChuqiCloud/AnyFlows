use std::{error::Error, time::Duration};

use af_domain::{ChannelType, CredentialKind, Protocol, Status};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
};

use crate::{
    DatabaseOptions, MigrationOptions, SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, abilities, channel_groups,
        channel_models, channels, credentials, groups,
    },
};

#[tokio::test]
async fn repository_selects_supported_target_and_filters_incomplete_channels()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    assert_eq!(
        SchedulerRuntimeRepository::with_load_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        SchedulerRuntimeRepositoryError::InvalidConfiguration
    );

    let group = group("default").insert(pool.connection()).await?;
    let supported = runtime_channel(
        "supported",
        Status::Enabled,
        ChannelType::OpenAi.as_str(),
        Protocol::OpenAiChat.as_str(),
        "https://runtime-url-canary.example.com/prefix",
        [("x-runtime-header", "runtime-header-value-canary")],
    )
    .insert(pool.connection())
    .await?;
    attach(supported.id, group.id, "gpt-runtime-canary")
        .insert(pool.connection())
        .await?;
    ability(group.id, "gpt-runtime-canary", supported.id, true, 9, 7)
        .insert(pool.connection())
        .await?;

    runtime_credential(
        supported.id,
        CredentialKind::ApiKey,
        50,
        true,
        None,
        "cooled-key-id-canary",
        0x11,
        true,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        supported.id,
        CredentialKind::SetupToken,
        40,
        true,
        None,
        "unsupported-key-id-canary",
        0x22,
        false,
    )
    .insert(pool.connection())
    .await?;
    let selected = runtime_credential(
        supported.id,
        CredentialKind::Oauth,
        30,
        true,
        Some(91),
        "selected-key-id-canary",
        0x33,
        false,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        supported.id,
        CredentialKind::ApiKey,
        20,
        true,
        None,
        "lower-key-id-canary",
        0x44,
        false,
    )
    .insert(pool.connection())
    .await?;

    let missing_credential = runtime_channel(
        "missing-credential",
        Status::Enabled,
        ChannelType::OpenAi.as_str(),
        Protocol::OpenAiChat.as_str(),
        "https://missing-credential.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    attach(missing_credential.id, group.id, "gpt-runtime-canary")
        .insert(pool.connection())
        .await?;
    ability(
        group.id,
        "gpt-runtime-canary",
        missing_credential.id,
        true,
        8,
        0,
    )
    .insert(pool.connection())
    .await?;

    let unsupported_channel = runtime_channel(
        "unsupported-channel",
        Status::Enabled,
        ChannelType::Anthropic.as_str(),
        Protocol::Anthropic.as_str(),
        "https://unsupported-channel-canary.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    attach(unsupported_channel.id, group.id, "gpt-runtime-canary")
        .insert(pool.connection())
        .await?;
    ability(
        group.id,
        "gpt-runtime-canary",
        unsupported_channel.id,
        true,
        7,
        0,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        unsupported_channel.id,
        CredentialKind::ApiKey,
        10,
        true,
        None,
        "unsupported-channel-key",
        0x55,
        false,
    )
    .insert(pool.connection())
    .await?;

    let records = SchedulerRuntimeRepository::new(pool.clone())
        .load_all()
        .await?;

    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.ability().group_id().get(), group.id);
    assert_eq!(record.ability().model(), "gpt-runtime-canary");
    assert_eq!(record.ability().priority(), 9);
    assert_eq!(record.ability().weight(), 7);
    assert_eq!(record.target().channel_id().get(), supported.id);
    assert_eq!(record.target().credential_id(), selected.id);
    assert_eq!(record.target().channel_type(), ChannelType::OpenAi);
    assert_eq!(record.target().protocol(), Protocol::OpenAiChat);
    assert_eq!(record.target().credential_kind(), CredentialKind::Oauth);
    assert_eq!(
        record.target().base_url(),
        Some("https://runtime-url-canary.example.com/prefix")
    );
    assert_eq!(record.target().headers().len(), 1);
    assert_eq!(record.target().headers()[0].name(), "x-runtime-header");
    assert_eq!(
        record.target().headers()[0].value(),
        "runtime-header-value-canary"
    );
    assert_eq!(
        record.target().envelope().key_id(),
        "selected-key-id-canary"
    );
    assert!(record.target().proxy_required());

    let rendered = format!("{record:?}");
    for secret in [
        "gpt-runtime-canary",
        "runtime-url-canary",
        "runtime-header-value-canary",
        "selected-key-id-canary",
    ] {
        assert!(!rendered.contains(secret));
    }

    pool.close().await?;
    Ok(())
}

fn group(name: &str) -> groups::ActiveModel {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
}

fn runtime_channel<const N: usize>(
    name: &str,
    status: Status,
    channel_type: &str,
    protocol: &str,
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
        r#type: Set(channel_type.to_owned()),
        protocol: Set(protocol.to_owned()),
        base_url: Set(Some(ChannelBaseUrl::parse(base_url).unwrap())),
        status: Set(status.code()),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(headers).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn runtime_credential(
    channel_id: i64,
    kind: CredentialKind,
    priority: i32,
    schedulable: bool,
    proxy_id: Option<i64>,
    key_id: &str,
    marker: u8,
    rate_limited: bool,
) -> credentials::ActiveModel {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(kind.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(key_id, marker)).unwrap()),
        status: Set(Status::Enabled.code()),
        priority: Set(priority),
        schedulable: Set(schedulable),
        proxy_id: Set(proxy_id),
        rate_limit_reset_at: Set(
            rate_limited.then(|| TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(3_600))
        ),
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

fn attach(channel_id: i64, group_id: i64, model: &str) -> AttachmentModels {
    AttachmentModels {
        channel_model: channel_model(channel_id, model),
        channel_group: channel_group(channel_id, group_id),
    }
}

fn channel_model(channel_id: i64, model: &str) -> channel_models::ActiveModel {
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
}

fn channel_group(channel_id: i64, group_id: i64) -> channel_groups::ActiveModel {
    channel_groups::ActiveModel {
        channel_id: Set(channel_id),
        group_id: Set(group_id),
        ..Default::default()
    }
}

fn ability(
    group_id: i64,
    model: &str,
    channel_id: i64,
    enabled: bool,
    priority: i32,
    weight: i32,
) -> abilities::ActiveModel {
    abilities::ActiveModel {
        group_id: Set(group_id),
        model: Set(model.to_owned()),
        channel_id: Set(channel_id),
        enabled: Set(enabled),
        priority: Set(priority),
        weight: Set(weight),
        ..Default::default()
    }
}

struct AttachmentModels {
    channel_model: channel_models::ActiveModel,
    channel_group: channel_groups::ActiveModel,
}

impl AttachmentModels {
    async fn insert(self, connection: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
        self.channel_model.insert(connection).await?;
        self.channel_group.insert(connection).await?;
        Ok(())
    }
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}
