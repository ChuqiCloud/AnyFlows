use std::collections::HashSet;

use crate::{
    ChannelType, ClientSimulationBodyPatchResult, ClientSimulationBodyProfile,
    ClientSimulationProfile, ClientSimulationResult, CredentialKind, CredentialQuotaDimension,
    Operation, Protocol, PublicErrorCode, ResponsesCompactMode, ResponsesCompactProbeResult, Role,
    Status,
};

macro_rules! assert_string_enum_contract {
    ($type:ty, $all:expr, [$(($value:path, $wire:literal)),+ $(,)?]) => {{
        let expected = [$($value),+];
        assert_eq!($all, expected.as_slice());

        let mut wire_values = HashSet::new();
        $(
            let value: $type = $value;
            assert_eq!(value.as_str(), $wire);
            assert_eq!(value.to_string(), $wire);
            assert_eq!($wire.parse::<$type>(), Ok(value));
            assert_eq!(serde_json::to_string(&value).unwrap(), concat!("\"", $wire, "\""));
            assert_eq!(serde_json::from_str::<$type>(concat!("\"", $wire, "\"")).unwrap(), value);
            assert!(wire_values.insert($wire));
        )+
    }};
}

#[test]
fn string_enums_have_unique_stable_parse_and_json_contracts() {
    assert_string_enum_contract!(
        Protocol,
        Protocol::ALL,
        [
            (Protocol::OpenAiChat, "openai_chat"),
            (Protocol::OpenAiResponses, "openai_responses"),
            (Protocol::OpenAiEmbeddings, "openai_embeddings"),
            (Protocol::OpenAiImages, "openai_images"),
            (Protocol::OpenAiAudio, "openai_audio"),
            (Protocol::OpenAiSpeech, "openai_speech"),
            (Protocol::JinaRerank, "jina_rerank"),
            (Protocol::CohereRerank, "cohere_rerank"),
            (Protocol::XaiVideo, "xai_video"),
            (Protocol::Anthropic, "anthropic"),
            (Protocol::Gemini, "gemini"),
        ]
    );
    assert_string_enum_contract!(
        Operation,
        Operation::ALL,
        [
            (Operation::Chat, "chat"),
            (Operation::Responses, "responses"),
            (Operation::ResponsesCompact, "responses_compact"),
            (Operation::Embedding, "embedding"),
            (Operation::Image, "image"),
            (Operation::Audio, "audio"),
            (Operation::Rerank, "rerank"),
            (Operation::Video, "video"),
            (Operation::CountTokens, "count_tokens"),
        ]
    );
    assert_string_enum_contract!(
        ResponsesCompactMode,
        ResponsesCompactMode::ALL,
        [
            (ResponsesCompactMode::Auto, "auto"),
            (ResponsesCompactMode::ForceOn, "force_on"),
            (ResponsesCompactMode::ForceOff, "force_off"),
        ]
    );
    assert_string_enum_contract!(
        ResponsesCompactProbeResult,
        ResponsesCompactProbeResult::ALL,
        [
            (ResponsesCompactProbeResult::Unknown, "unknown"),
            (ResponsesCompactProbeResult::Supported, "supported"),
            (ResponsesCompactProbeResult::Unsupported, "unsupported"),
        ]
    );
    assert_string_enum_contract!(
        ClientSimulationProfile,
        ClientSimulationProfile::ALL,
        [(
            ClientSimulationProfile::AnthropicCliHeadersV1,
            "anthropic_cli_headers_v1"
        )]
    );
    assert_string_enum_contract!(
        ClientSimulationBodyProfile,
        ClientSimulationBodyProfile::ALL,
        [(
            ClientSimulationBodyProfile::AnthropicCliSystemDateV1,
            "anthropic_cli_system_date_v1"
        )]
    );
    assert_string_enum_contract!(
        ClientSimulationResult,
        ClientSimulationResult::ALL,
        [
            (ClientSimulationResult::NotApplied, "not_applied"),
            (ClientSimulationResult::Applied, "applied"),
            (ClientSimulationResult::Failed, "failed"),
        ]
    );
    assert_string_enum_contract!(
        ClientSimulationBodyPatchResult,
        ClientSimulationBodyPatchResult::ALL,
        [
            (ClientSimulationBodyPatchResult::Applied, "applied"),
            (ClientSimulationBodyPatchResult::Rejected, "rejected"),
        ]
    );
    assert_string_enum_contract!(
        ChannelType,
        ChannelType::ALL,
        [
            (ChannelType::OpenAi, "openai"),
            (ChannelType::Anthropic, "anthropic"),
            (ChannelType::Gemini, "gemini"),
            (ChannelType::Bedrock, "bedrock"),
            (ChannelType::Vertex, "vertex"),
            (ChannelType::Jina, "jina"),
            (ChannelType::Cohere, "cohere"),
            (ChannelType::Xai, "xai"),
            (ChannelType::Custom, "custom"),
        ]
    );
    assert_string_enum_contract!(
        CredentialKind,
        CredentialKind::ALL,
        [
            (CredentialKind::ApiKey, "api_key"),
            (CredentialKind::Oauth, "oauth"),
            (CredentialKind::SetupToken, "setup_token"),
            (CredentialKind::Bedrock, "bedrock"),
            (CredentialKind::ServiceAccount, "service_account"),
            (CredentialKind::Upstream, "upstream"),
        ]
    );
    assert_string_enum_contract!(
        CredentialQuotaDimension,
        CredentialQuotaDimension::ALL,
        [
            (CredentialQuotaDimension::Global, "global"),
            (CredentialQuotaDimension::Spark, "spark"),
        ]
    );
    assert_string_enum_contract!(
        Role,
        Role::ALL,
        [
            (Role::System, "system"),
            (Role::Developer, "developer"),
            (Role::User, "user"),
            (Role::Assistant, "assistant"),
            (Role::Tool, "tool"),
        ]
    );
    assert_string_enum_contract!(
        PublicErrorCode,
        PublicErrorCode::ALL,
        [
            (PublicErrorCode::InvalidRequest, "invalid_request"),
            (PublicErrorCode::InvalidApiKey, "invalid_api_key"),
            (PublicErrorCode::InsufficientQuota, "insufficient_quota"),
            (PublicErrorCode::ModelNotFound, "model_not_found"),
            (PublicErrorCode::TaskNotFound, "task_not_found"),
            (PublicErrorCode::IdempotencyConflict, "idempotency_conflict"),
            (
                PublicErrorCode::RequestOutcomeUnknown,
                "request_outcome_unknown"
            ),
            (PublicErrorCode::UpstreamUnavailable, "upstream_unavailable"),
            (PublicErrorCode::RateLimited, "rate_limited"),
            (PublicErrorCode::InternalError, "internal_error"),
        ]
    );
}

#[test]
fn credential_quota_dimension_only_selects_spark_for_the_closed_model() {
    assert_eq!(
        CredentialQuotaDimension::for_canonical_model("gpt-5.3-codex-spark"),
        CredentialQuotaDimension::Spark
    );
    for model in [
        "gpt-5.3-codex",
        "gpt-5.3-codex-spark-high",
        "GPT-5.3-CODEX-SPARK",
        "",
    ] {
        assert_eq!(
            CredentialQuotaDimension::for_canonical_model(model),
            CredentialQuotaDimension::Global
        );
    }
}

#[test]
fn string_enums_reject_unknown_case_aliases_and_non_strings() {
    for invalid in [
        "",
        "OpenAiChat",
        "open_ai_chat",
        "openai-chat",
        " openai_chat",
        "domain-enum-secret-canary",
    ] {
        let error = invalid.parse::<Protocol>().unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error.enum_name(), "Protocol");
        if !invalid.is_empty() {
            assert!(!rendered.contains(invalid));
        }
        let serde_error = serde_json::from_str::<Protocol>(&format!("\"{invalid}\""))
            .unwrap_err()
            .to_string();
        if !invalid.is_empty() {
            assert!(!serde_error.contains(invalid));
        }
    }
    let type_canary = "4242424242424242";
    let serde_error = serde_json::from_str::<Protocol>(type_canary)
        .unwrap_err()
        .to_string();
    assert!(!serde_error.contains(type_canary));
}

#[test]
fn public_error_codes_reject_unknown_case_aliases_and_non_strings() {
    for invalid in [
        "",
        "InvalidRequest",
        "invalid-request",
        "invalid request",
        " invalid_request",
        "public-error-secret-canary",
    ] {
        let error = invalid.parse::<PublicErrorCode>().unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error.enum_name(), "PublicErrorCode");
        if !invalid.is_empty() {
            assert!(!rendered.contains(invalid));
        }
        let serde_error = serde_json::from_str::<PublicErrorCode>(&format!("\"{invalid}\""))
            .unwrap_err()
            .to_string();
        if !invalid.is_empty() {
            assert!(!serde_error.contains(invalid));
        }
    }
    let type_canary = "3131313131313131";
    let serde_error = serde_json::from_str::<PublicErrorCode>(type_canary)
        .unwrap_err()
        .to_string();
    assert!(!serde_error.contains(type_canary));
}

#[test]
fn status_uses_fail_closed_database_and_json_codes() {
    assert_eq!(
        Status::ALL,
        &[Status::Enabled, Status::Disabled, Status::AutoDisabled]
    );
    assert_eq!(Status::default(), Status::Disabled);
    assert!(Status::Enabled.is_enabled());
    assert!(!Status::Disabled.is_enabled());
    assert!(!Status::AutoDisabled.is_enabled());

    for (status, code, display) in [
        (Status::Enabled, 1_i16, "启用"),
        (Status::Disabled, 2_i16, "禁用"),
        (Status::AutoDisabled, 3_i16, "自动禁用"),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(i16::from(status), code);
        assert_eq!(Status::try_from(code), Ok(status));
        assert_eq!(status.to_string(), display);
        assert_eq!(serde_json::to_string(&status).unwrap(), code.to_string());
        assert_eq!(
            serde_json::from_str::<Status>(&code.to_string()).unwrap(),
            status
        );
    }

    for invalid in [i16::MIN, -1, 0, 4, i16::MAX] {
        let error = Status::try_from(invalid).unwrap_err();
        assert_eq!(error.enum_name(), "Status");
        assert!(serde_json::from_str::<Status>(&invalid.to_string()).is_err());
    }
    let type_canary = "status-secret-canary";
    let serde_error = serde_json::from_str::<Status>(&format!("\"{type_canary}\""))
        .unwrap_err()
        .to_string();
    assert!(!serde_error.contains(type_canary));
}
