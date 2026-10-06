mod support;

use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, ClientSimulationBodyProfile, ClientSimulationProfile, CredentialKind,
    CredentialQuotaDimension, GroupId, Protocol, ResponsesCompactMode, ResponsesCompactProbeResult,
    Status,
};
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

use crate::EncryptedCredentialEnvelope;
use crate::entity::SensitiveJson;

use super::{
    ChannelModelMappings, ChannelParameterOverrides, MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL,
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError,
    SchedulerRuntimeTargetRecord,
};
use crate::SchedulerCatalogSubject;

use self::support::{ability, attach, group, runtime_channel, runtime_credential, runtime_proxy};

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
    supported.timeout_secs = Set(Some(60));
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
    let mut pending_oauth = runtime_credential(
        supported.id,
        CredentialKind::Oauth,
        45,
        true,
        None,
        "pending-oauth-key-id-canary",
        0x23,
        false,
    );
    pending_oauth.oauth_token_pending = Set(true);
    let pending_oauth = pending_oauth.insert(pool.connection()).await?;
    assert!(
        super::source::SchedulerRuntimeCredentialSource::from_entity(pending_oauth, None, None)?
            .is_none()
    );
    runtime_proxy(91).insert(pool.connection()).await?;
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

    let mut anthropic_channel = runtime_channel(
        "anthropic-channel",
        Status::Enabled,
        ChannelType::Anthropic.as_str(),
        Protocol::Anthropic.as_str(),
        "https://anthropic-channel-canary.example.com",
        [],
    );
    anthropic_channel.settings = Set(SensitiveJson::from(serde_json::json!({
        "client_simulation_profile": "anthropic_cli_headers_v1"
    })));
    let anthropic_channel = anthropic_channel.insert(pool.connection()).await?;
    attach(anthropic_channel.id, group.id, "gpt-runtime-canary")
        .insert(pool.connection())
        .await?;
    ability(
        group.id,
        "gpt-runtime-canary",
        anthropic_channel.id,
        true,
        7,
        0,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        anthropic_channel.id,
        CredentialKind::ApiKey,
        10,
        true,
        None,
        "anthropic-channel-key",
        0x55,
        false,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        anthropic_channel.id,
        CredentialKind::Oauth,
        20,
        true,
        None,
        "anthropic-oauth-key",
        0x56,
        false,
    )
    .insert(pool.connection())
    .await?;

    let mut responses_channel = runtime_channel(
        "responses-channel",
        Status::Enabled,
        ChannelType::OpenAi.as_str(),
        Protocol::OpenAiResponses.as_str(),
        "https://responses-channel-canary.example.com",
        [],
    );
    responses_channel.model_mapping = Set(serde_json::json!({
        "gpt-runtime-canary": "gpt-responses-upstream-canary"
    }));
    responses_channel.settings = Set(SensitiveJson::from(serde_json::json!({
        "responses_websocket_enabled": true,
        "responses_compact_mode": "force_on",
        "responses_compact_probe_result": "supported",
        "responses_compact_probe_checked_at": 1735000000000_i64,
        "responses_compact_probe_http_status": 200,
        "compact_model_mapping": {
            "gpt-responses-upstream-canary": "gpt-compact-upstream-canary"
        },
        "pool_mode": true,
        "auto_ban_rules": {
            "status_codes": [503],
            "keywords": ["workspace disabled"]
        },
        "private_extension": "private-settings-canary"
    })));
    let responses_channel = responses_channel.insert(pool.connection()).await?;
    attach(responses_channel.id, group.id, "gpt-runtime-canary")
        .insert(pool.connection())
        .await?;
    ability(
        group.id,
        "gpt-runtime-canary",
        responses_channel.id,
        true,
        6,
        0,
    )
    .insert(pool.connection())
    .await?;
    runtime_credential(
        responses_channel.id,
        CredentialKind::ApiKey,
        10,
        true,
        None,
        "responses-channel-key",
        0x66,
        false,
    )
    .insert(pool.connection())
    .await?;

    let records = SchedulerRuntimeRepository::new(pool.clone())
        .load_all()
        .await?;

    assert_eq!(records.len(), 3);
    let repository = SchedulerRuntimeRepository::new(pool.clone());
    let channel_projection = repository
        .load_subject(SchedulerCatalogSubject::Channel(ChannelId::new(
            supported.id,
        )?))
        .await?;
    assert_eq!(channel_projection.len(), 1);
    assert_eq!(
        channel_projection[0].target().channel_id().get(),
        supported.id
    );
    let group_projection = repository
        .load_subject(SchedulerCatalogSubject::Group(GroupId::new(group.id)?))
        .await?;
    assert_eq!(group_projection.len(), records.len());
    assert!(
        repository
            .load_subject(SchedulerCatalogSubject::Channel(ChannelId::new(i64::MAX)?))
            .await?
            .is_empty()
    );
    let record = records
        .iter()
        .find(|record| record.target().channel_id().get() == supported.id)
        .unwrap();
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
    assert_eq!(record.target().timeout().unwrap().seconds(), 60);
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
    assert_eq!(
        record.target().credentials()[0]
            .proxy()
            .unwrap()
            .proxy_id()
            .get(),
        91
    );
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

    let anthropic_record = records
        .iter()
        .find(|record| record.target().channel_id().get() == anthropic_channel.id)
        .unwrap();
    assert_eq!(anthropic_record.target().timeout(), None);
    assert_eq!(
        anthropic_record.target().channel_type(),
        ChannelType::Anthropic
    );
    assert_eq!(anthropic_record.target().protocol(), Protocol::Anthropic);
    assert_eq!(
        anthropic_record.target().credential_kind(),
        CredentialKind::Oauth
    );
    assert_eq!(anthropic_record.target().credentials().len(), 1);
    assert_eq!(
        anthropic_record.target().client_simulation_profile(),
        Some(ClientSimulationProfile::AnthropicCliHeadersV1)
    );

    let responses_record = records
        .iter()
        .find(|record| record.target().channel_id().get() == responses_channel.id)
        .unwrap();
    assert!(responses_record.target().responses_websocket_enabled());
    assert_eq!(
        responses_record.target().responses_compact_mode(),
        ResponsesCompactMode::ForceOn
    );
    assert_eq!(
        responses_record.target().responses_compact_probe_result(),
        ResponsesCompactProbeResult::Supported
    );
    assert!(responses_record.target().responses_compact_schedulable());
    assert_eq!(
        responses_record.target().mapped_model("gpt-runtime-canary"),
        Some("gpt-responses-upstream-canary")
    );
    assert_eq!(
        responses_record
            .target()
            .mapped_responses_compact_model("gpt-runtime-canary"),
        "gpt-compact-upstream-canary"
    );
    assert!(responses_record.target().pool_mode());
    assert_eq!(
        responses_record.target().auto_ban_rules().server_statuses()[0].get(),
        503
    );
    assert_eq!(
        responses_record.target().auto_ban_rules().keywords(),
        &["workspace disabled".to_owned()]
    );

    let rendered = format!("{records:?}");
    for secret in [
        "gpt-runtime-canary",
        "runtime-url-canary",
        "runtime-header-value-canary",
        "gpt-upstream-canary",
        "runtime-stop-canary",
        "private-settings-canary",
        "selected-key-id-canary",
        "anthropic-channel-canary",
        "anthropic-channel-key",
        "anthropic-oauth-key",
        "gpt-responses-upstream-canary",
        "gpt-compact-upstream-canary",
    ] {
        assert!(!rendered.contains(secret));
    }

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn spark_shadow_reads_parent_secret_proxy_and_concurrency_without_parent_quota_cooling()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &crate::DatabaseOptions::new("sqlite::memory:")?,
        crate::MigrationOptions::default(),
    )
    .await?;
    let group = group("spark-runtime").insert(pool.connection()).await?;
    let channel = runtime_channel(
        "spark-runtime",
        Status::Enabled,
        ChannelType::OpenAi.as_str(),
        Protocol::OpenAiResponses.as_str(),
        "https://spark-runtime.example.com",
        [],
    )
    .insert(pool.connection())
    .await?;
    attach(channel.id, group.id, "gpt-5.3-codex-spark")
        .insert(pool.connection())
        .await?;
    ability(group.id, "gpt-5.3-codex-spark", channel.id, true, 10, 10)
        .insert(pool.connection())
        .await?;
    runtime_proxy(92).insert(pool.connection()).await?;

    let mut parent = runtime_credential(
        channel.id,
        CredentialKind::Oauth,
        10,
        false,
        Some(92),
        "parent-secret-key",
        0x71,
        true,
    );
    parent.concurrency = Set(Some(3));
    parent.temp_unschedulable_until = Set(Some(
        sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(3_600),
    ));
    parent.temp_unschedulable_reason = Set(Some("quota_exhausted".to_owned()));
    let parent = parent.insert(pool.connection()).await?;

    let mut shadow = runtime_credential(
        channel.id,
        CredentialKind::Oauth,
        20,
        true,
        None,
        "shadow-reference",
        0,
        false,
    );
    shadow.parent_id = Set(Some(parent.id));
    shadow.quota_dimension = Set(CredentialQuotaDimension::Spark.as_str().to_owned());
    let shadow = shadow.insert(pool.connection()).await?;

    let records = SchedulerRuntimeRepository::new(pool.clone())
        .load_all()
        .await?;
    assert_eq!(records.len(), 1);
    let credential = &records[0].target().credentials()[0];
    assert_eq!(credential.routing_credential_id().get(), shadow.id);
    assert_eq!(credential.secret_owner_id().get(), parent.id);
    assert_eq!(credential.concurrency_owner_id().get(), parent.id);
    assert_eq!(credential.shared_health_id().get(), parent.id);
    assert_eq!(
        credential.quota_dimension(),
        CredentialQuotaDimension::Spark
    );
    assert_eq!(credential.envelope().key_id(), "parent-secret-key");
    assert_eq!(credential.proxy().unwrap().proxy_id().get(), 92);
    assert_eq!(credential.concurrency().unwrap().get(), 3);

    let mut auth_blocked = parent.clone().into_active_model();
    auth_blocked.temp_unschedulable_reason = Set(Some("auth_expired".to_owned()));
    let parent = auth_blocked.update(pool.connection()).await?;
    assert!(
        SchedulerRuntimeRepository::new(pool.clone())
            .load_all()
            .await?
            .is_empty()
    );

    let mut recovered = parent.into_active_model();
    recovered.temp_unschedulable_until = Set(None);
    recovered.temp_unschedulable_reason = Set(None);
    let parent = recovered.update(pool.connection()).await?;
    let mut broken_shadow = shadow.into_active_model();
    broken_shadow.secret = Set(parent.secret);
    broken_shadow.update(pool.connection()).await?;
    assert!(matches!(
        SchedulerRuntimeRepository::new(pool.clone())
            .load_all()
            .await,
        Err(SchedulerRuntimeRepositoryError::Invariant)
    ));

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

#[test]
fn anthropic_runtime_target_accepts_messages_pair_and_rejects_mismatch() {
    let credential = runtime_credential_record(1);
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Anthropic,
            Protocol::Anthropic,
            None,
            vec![credential.clone()],
            Vec::new(),
        )
        .is_ok()
    );
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Anthropic,
            Protocol::OpenAiChat,
            None,
            vec![credential],
            Vec::new(),
        )
        .is_err()
    );
    let oauth = SchedulerRuntimeCredentialRecord::with_scheduling(
        2,
        CredentialKind::Oauth,
        EncryptedCredentialEnvelope::new("runtime-test-key", [0x43; 24], vec![0x25; 16]).unwrap(),
        false,
        0,
        10,
    )
    .unwrap();
    let target = SchedulerRuntimeTargetRecord::new_pool(
        ChannelId::new(2).unwrap(),
        ChannelType::Anthropic,
        Protocol::Anthropic,
        None,
        vec![oauth],
        Vec::new(),
    )
    .unwrap()
    .with_client_simulation_profile(Some(ClientSimulationProfile::AnthropicCliHeadersV1))
    .unwrap()
    .with_client_simulation_body_profile(Some(
        ClientSimulationBodyProfile::AnthropicCliSystemDateV1,
    ))
    .unwrap();
    assert_eq!(
        target.client_simulation_profile(),
        Some(ClientSimulationProfile::AnthropicCliHeadersV1)
    );
    assert_eq!(
        target.client_simulation_body_profile(),
        Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1)
    );
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(4).unwrap(),
            ChannelType::Anthropic,
            Protocol::Anthropic,
            None,
            vec![
                SchedulerRuntimeCredentialRecord::with_scheduling(
                    4,
                    CredentialKind::Oauth,
                    EncryptedCredentialEnvelope::new(
                        "runtime-test-key",
                        [0x43; 24],
                        vec![0x25; 16]
                    )
                    .unwrap(),
                    false,
                    0,
                    10,
                )
                .unwrap()
            ],
            Vec::new(),
        )
        .unwrap()
        .with_client_simulation_body_profile(Some(
            ClientSimulationBodyProfile::AnthropicCliSystemDateV1,
        ))
        .is_err()
    );
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(3).unwrap(),
            ChannelType::Anthropic,
            Protocol::Anthropic,
            None,
            vec![runtime_credential_record(3)],
            Vec::new(),
        )
        .unwrap()
        .with_client_simulation_profile(Some(ClientSimulationProfile::AnthropicCliHeadersV1,))
        .is_err()
    );
}

#[test]
fn gemini_runtime_target_accepts_native_pair_and_rejects_mismatch() {
    let credential = runtime_credential_record(1);
    let oauth = SchedulerRuntimeCredentialRecord::with_scheduling(
        2,
        CredentialKind::Oauth,
        EncryptedCredentialEnvelope::new("runtime-test-key", [0x43; 24], vec![0x25; 16]).unwrap(),
        false,
        0,
        10,
    )
    .unwrap();
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Gemini,
            Protocol::Gemini,
            None,
            vec![credential.clone(), oauth],
            Vec::new(),
        )
        .is_ok()
    );
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Gemini,
            Protocol::OpenAiChat,
            None,
            vec![credential],
            Vec::new(),
        )
        .is_err()
    );
}

#[test]
fn xai_runtime_target_accepts_video_pair_and_rejects_mismatch() {
    let credential = runtime_credential_record(1);
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Xai,
            Protocol::XaiVideo,
            None,
            vec![credential.clone()],
            Vec::new(),
        )
        .is_ok()
    );
    assert!(
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(1).unwrap(),
            ChannelType::Xai,
            Protocol::OpenAiImages,
            None,
            vec![credential],
            Vec::new(),
        )
        .is_err()
    );
}

#[test]
fn credential_revision_changes_with_every_envelope_component() {
    let baseline = runtime_credential_record(1);
    let changed_nonce = SchedulerRuntimeCredentialRecord::new(
        1,
        CredentialKind::ApiKey,
        EncryptedCredentialEnvelope::new("runtime-test-key", [0x43; 24], vec![0x24; 16]).unwrap(),
        false,
    )
    .unwrap();
    let changed_ciphertext = SchedulerRuntimeCredentialRecord::new(
        1,
        CredentialKind::ApiKey,
        EncryptedCredentialEnvelope::new("runtime-test-key", [0x42; 24], vec![0x25; 16]).unwrap(),
        false,
    )
    .unwrap();

    assert_ne!(
        baseline.credential_revision(),
        changed_nonce.credential_revision()
    );
    assert_ne!(
        baseline.credential_revision(),
        changed_ciphertext.credential_revision()
    );
    let debug = format!("{baseline:?}");
    assert!(!debug.contains(&baseline.credential_revision().to_string()));
}

#[test]
fn responses_specific_capabilities_only_accept_native_responses_target() {
    let responses = SchedulerRuntimeTargetRecord::new_pool(
        ChannelId::new(1).unwrap(),
        ChannelType::OpenAi,
        Protocol::OpenAiResponses,
        None,
        vec![runtime_credential_record(1)],
        Vec::new(),
    )
    .unwrap()
    .with_responses_websocket_enabled(true)
    .unwrap();
    assert!(responses.responses_websocket_enabled());

    let chat = SchedulerRuntimeTargetRecord::new_pool(
        ChannelId::new(1).unwrap(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        None,
        vec![runtime_credential_record(1)],
        Vec::new(),
    )
    .unwrap();
    assert!(chat.clone().with_responses_websocket_enabled(true).is_err());

    let native_compact = SchedulerRuntimeTargetRecord::new_pool(
        ChannelId::new(2).unwrap(),
        ChannelType::OpenAi,
        Protocol::OpenAiResponses,
        None,
        vec![runtime_credential_record(2)],
        Vec::new(),
    )
    .unwrap()
    .with_responses_compact_mode(ResponsesCompactMode::ForceOn)
    .unwrap();
    assert_eq!(
        native_compact.responses_compact_mode(),
        ResponsesCompactMode::ForceOn
    );
    let normal_mapping = ChannelModelMappings::parse(&serde_json::json!({
        "gpt-public": "gpt-upstream"
    }))
    .unwrap();
    let compact_mapping = ChannelModelMappings::parse(&serde_json::json!({
        "gpt-upstream": "gpt-compact-upstream"
    }))
    .unwrap();
    let mapped_compact =
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy_and_compact_mapping(
            ChannelId::new(3).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiResponses,
            None,
            vec![runtime_credential_record(3)],
            normal_mapping,
            compact_mapping.clone(),
            ChannelParameterOverrides::default(),
            Vec::new(),
        )
        .unwrap();
    assert_eq!(
        mapped_compact.mapped_model("gpt-public"),
        Some("gpt-upstream")
    );
    assert_eq!(
        mapped_compact.mapped_responses_compact_model("gpt-public"),
        "gpt-compact-upstream"
    );
    assert_eq!(
        mapped_compact.mapped_responses_compact_model("gpt-unmapped"),
        "gpt-unmapped"
    );
    assert!(!mapped_compact.responses_compact_schedulable());
    assert_eq!(
        mapped_compact.schedulable_responses_compact_model("gpt-public"),
        None
    );
    let auto_supported = mapped_compact
        .clone()
        .with_responses_compact_probe_result(ResponsesCompactProbeResult::Supported)
        .unwrap();
    assert!(auto_supported.responses_compact_schedulable());
    assert_eq!(
        auto_supported.schedulable_responses_compact_model("gpt-public"),
        Some("gpt-compact-upstream")
    );
    let auto_unsupported = mapped_compact
        .clone()
        .with_responses_compact_probe_result(ResponsesCompactProbeResult::Unsupported)
        .unwrap();
    assert!(!auto_unsupported.responses_compact_schedulable());
    let forced_on = auto_unsupported
        .with_responses_compact_mode(ResponsesCompactMode::ForceOn)
        .unwrap();
    assert!(forced_on.responses_compact_schedulable());
    let forced_off = auto_supported
        .with_responses_compact_mode(ResponsesCompactMode::ForceOff)
        .unwrap();
    assert!(!forced_off.responses_compact_schedulable());
    assert!(
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy_and_compact_mapping(
            ChannelId::new(4).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            None,
            vec![runtime_credential_record(4)],
            ChannelModelMappings::default(),
            compact_mapping,
            ChannelParameterOverrides::default(),
            Vec::new(),
        )
        .is_err()
    );
    assert!(
        chat.clone()
            .with_responses_compact_mode(ResponsesCompactMode::ForceOn)
            .is_err()
    );
    assert!(
        chat.with_responses_compact_probe_result(ResponsesCompactProbeResult::Supported)
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
