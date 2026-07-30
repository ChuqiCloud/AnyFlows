mod support;

use std::{error::Error, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Status};
use sea_orm::{ActiveModelTrait, Set};

use crate::EncryptedCredentialEnvelope;

use super::{
    ChannelModelMappings, ChannelParameterOverrides, MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL,
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError,
    SchedulerRuntimeTargetRecord,
};

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
    let mut supported = runtime_channel(
        "supported",
        Status::Enabled,
        ChannelType::OpenAi.as_str(),
        Protocol::OpenAiChat.as_str(),
        "https://runtime-url-canary.example.com/prefix",
        [("x-runtime-header", "runtime-header-value-canary")],
    );
    supported.model_mapping = Set(serde_json::json!({
        "gpt-runtime-canary": "gpt-upstream-canary"
    }));
    supported.param_override = Set(serde_json::json!({
        "temperature": 0.25,
        "top_p": 0.8,
        "max_output_tokens": 4096,
        "stop_sequences": ["runtime-stop-canary"]
    }));
    let supported = supported.insert(pool.connection()).await?;
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
    assert_eq!(record.target().credentials().len(), 2);
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
    assert_eq!(record.target().credentials()[0].priority(), 30);
    assert_eq!(record.target().credentials()[1].priority(), 20);
    assert_eq!(
        record.target().mapped_model("gpt-runtime-canary"),
        Some("gpt-upstream-canary")
    );
    assert_eq!(record.target().mapped_model("gpt-unmapped"), None);
    let overrides = record.target().parameter_overrides();
    assert_eq!(overrides.temperature(), Some(0.25));
    assert_eq!(overrides.top_p(), Some(0.8));
    assert_eq!(overrides.max_output_tokens(), Some(4096));
    assert_eq!(overrides.stop_sequences().unwrap(), ["runtime-stop-canary"]);

    let rendered = format!("{record:?}");
    for secret in [
        "gpt-runtime-canary",
        "runtime-url-canary",
        "runtime-header-value-canary",
        "gpt-upstream-canary",
        "runtime-stop-canary",
        "selected-key-id-canary",
    ] {
        assert!(!rendered.contains(secret));
    }

    pool.close().await?;
    Ok(())
}

#[test]
fn runtime_target_rejects_empty_oversized_and_duplicate_credential_pools() {
    let channel_id = ChannelId::new(1).unwrap();
    let build = |credentials| {
        SchedulerRuntimeTargetRecord::new_pool(
            channel_id,
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            None,
            credentials,
            Vec::new(),
        )
    };

    assert!(build(Vec::new()).is_err());

    let oversized = (1..=MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL + 1)
        .map(|id| runtime_credential_record(i64::try_from(id).unwrap()))
        .collect();
    assert!(build(oversized).is_err());

    let duplicate = runtime_credential_record(7);
    assert!(build(vec![duplicate.clone(), duplicate]).is_err());
}

#[test]
fn responses_runtime_target_rejects_chat_only_stop_sequences() {
    let credential = runtime_credential_record(1);
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiResponses,
            None,
            vec![credential.clone()],
            Vec::new(),
        )
        .is_ok()
    );
    let parameters =
        ChannelParameterOverrides::parse(&serde_json::json!({"stop_sequences": ["private"]}))
            .unwrap();
    assert!(
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
            ChannelId::new(1).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiResponses,
            None,
            vec![credential],
            ChannelModelMappings::default(),
            parameters,
            Vec::new(),
        )
        .is_err()
    );
}

fn runtime_credential_record(credential_id: i64) -> SchedulerRuntimeCredentialRecord {
    SchedulerRuntimeCredentialRecord::with_scheduling(
        credential_id,
        CredentialKind::ApiKey,
        EncryptedCredentialEnvelope::new("runtime-test-key", [0x42; 24], vec![0x24; 16]).unwrap(),
        false,
        0,
        10,
    )
    .unwrap()
}
