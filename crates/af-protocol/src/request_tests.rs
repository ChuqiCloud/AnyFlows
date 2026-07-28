use std::error::Error as _;

use af_domain::Operation;
use serde_json::json;

use crate::{
    Attachment, CanonicalRequest, MediaSource, ReasoningConfig, ReasoningConfigError,
    ReasoningEffort, RequestContinuation, RequestMetadata, Sampling, SamplingError, TokenCount,
    ToolChoice, ToolDef,
};

#[test]
fn sampling_validates_common_numeric_boundaries() {
    let sampling = Sampling::new(
        Some(2.0),
        Some(1.0),
        Some(TokenCount::new(1024).unwrap()),
        vec!["stop".to_owned()],
    )
    .unwrap();
    assert_eq!(sampling.temperature(), Some(2.0));
    assert_eq!(sampling.top_p(), Some(1.0));
    assert_eq!(sampling.max_output_tokens().unwrap().get(), 1024);
    assert_eq!(sampling.stop_sequences(), &["stop"]);

    for invalid in [f64::NAN, f64::INFINITY, -0.1] {
        assert_eq!(
            Sampling::new(Some(invalid), None, None, vec![]),
            Err(SamplingError::InvalidTemperature)
        );
    }
    for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert_eq!(
            Sampling::new(None, Some(invalid), None, vec![]),
            Err(SamplingError::InvalidTopP)
        );
    }
}

#[test]
fn reasoning_and_metadata_preserve_explicit_protocol_semantics() {
    let reasoning = ReasoningConfig::new(
        Some(ReasoningEffort::ExtraHigh),
        Some(TokenCount::new(4096).unwrap()),
        true,
    )
    .unwrap();
    assert_eq!(reasoning.effort(), Some(ReasoningEffort::ExtraHigh));
    assert_eq!(reasoning.budget_tokens().unwrap().get(), 4096);
    assert!(reasoning.include_thinking());

    let metadata = RequestMetadata::new(Some("user-id".to_owned()), Some("session-id".to_owned()));
    assert_eq!(metadata.user_id(), Some("user-id"));
    assert_eq!(metadata.session_id(), Some("session-id"));
    assert_eq!(RequestMetadata::EMPTY.user_id(), None);
}

#[test]
fn continuation_preserves_protocol_state_without_exposing_values_in_debug() {
    let continuation = RequestContinuation::new(
        Some("response-secret-canary".to_owned()),
        None,
        Some("cache-secret-canary".to_owned()),
    );

    assert_eq!(
        continuation.previous_response_id(),
        Some("response-secret-canary")
    );
    assert_eq!(continuation.conversation_id(), None);
    assert_eq!(continuation.prompt_cache_key(), Some("cache-secret-canary"));
    assert!(!continuation.is_empty());
    let rendered = format!("{continuation:?}");
    assert!(!rendered.contains("response-secret-canary"));
    assert!(!rendered.contains("cache-secret-canary"));
}

#[test]
fn reasoning_rejects_disabled_configs_with_active_options() {
    let budget = Some(TokenCount::new(1).unwrap());
    assert_eq!(
        ReasoningConfig::new(Some(ReasoningEffort::None), budget, false),
        Err(ReasoningConfigError::DisabledWithActiveOptions)
    );
    assert_eq!(
        ReasoningConfig::new(Some(ReasoningEffort::None), None, true),
        Err(ReasoningConfigError::DisabledWithActiveOptions)
    );
    assert!(ReasoningConfig::new(Some(ReasoningEffort::None), None, false).is_ok());

    let error = ReasoningConfigError::DisabledWithActiveOptions;
    assert_eq!(error.to_string(), "关闭推理时不得设置预算或输出思考内容");
    assert!(error.source().is_none());
}

#[test]
fn request_parameter_debug_redacts_external_content() {
    let canaries = [
        "tool-name-secret-canary",
        "tool-description-secret-canary",
        "schema-secret-canary",
        "choice-secret-canary",
        "stop-secret-canary",
        "attachment-data-secret-canary",
        "mime-secret-canary",
        "filename-secret-canary",
        "user-secret-canary",
        "session-secret-canary",
        "aggregate-model-secret-canary",
    ];
    let tool = ToolDef {
        name: canaries[0].to_owned(),
        description: Some(canaries[1].to_owned()),
        input_schema: json!({"description": canaries[2]}),
        strict: None,
    };
    let choice = ToolChoice::Named {
        name: canaries[3].to_owned(),
    };
    let sampling = Sampling::new(None, None, None, vec![canaries[4].to_owned()]).unwrap();
    let attachment = Attachment {
        source: MediaSource::Base64(canaries[5].to_owned()),
        mime_type: Some(canaries[6].to_owned()),
        filename: Some(canaries[7].to_owned()),
    };
    let metadata = RequestMetadata::new(Some(canaries[8].to_owned()), Some(canaries[9].to_owned()));
    let mut request =
        CanonicalRequest::new(Operation::Chat, canaries[10].to_owned(), Vec::new(), false);
    request.tools = vec![tool.clone()];
    request.tool_choice = choice.clone();
    request.sampling = sampling.clone();
    request.attachments = vec![attachment.clone()];
    request.metadata = metadata.clone();

    let rendered = format!(
        "{tool:?}\n{choice:?}\n{sampling:?}\n{attachment:?}\n{metadata:?}\n{:?}\n{request:?}",
        MediaSource::Base64(canaries[5].to_owned())
    );
    for canary in canaries {
        assert!(!rendered.contains(canary), "Debug 泄露请求参数：{canary}");
    }
}

#[test]
fn sampling_errors_have_fixed_chinese_diagnostics_without_sources() {
    for (error, message) in [
        (
            SamplingError::InvalidTemperature,
            "temperature 必须是有限非负数",
        ),
        (SamplingError::InvalidTopP, "top_p 必须在 0 到 1 之间"),
    ] {
        assert_eq!(error.to_string(), message);
        assert!(error.source().is_none());
    }
}
