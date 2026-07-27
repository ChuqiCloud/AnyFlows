mod support;

use std::{error::Error, time::Duration};

use af_domain::{ChannelType, CredentialKind, Protocol, Status};
use sea_orm::ActiveModelTrait;

use super::{SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError};

use self::support::{ability, attach, group, runtime_channel, runtime_credential};

#[tokio::test]
async fn repository_selects_supported_target_and_filters_incomplete_channels()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &crate::DatabaseOptions::new("sqlite::memory:")?,
        crate::MigrationOptions::default(),
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
