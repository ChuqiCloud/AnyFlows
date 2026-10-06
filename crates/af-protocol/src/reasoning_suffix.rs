use std::{error::Error, fmt};

use crate::{CanonicalRequestEnvelope, ReasoningConfig, ReasoningConfigError, ReasoningEffort};

const REASONING_SUFFIXES: [(&str, ReasoningSuffix); 9] = [
    (
        "-nothinking",
        ReasoningSuffix::Effort(ReasoningEffort::None),
    ),
    ("-thinking", ReasoningSuffix::Thinking),
    (
        "-minimal",
        ReasoningSuffix::Effort(ReasoningEffort::Minimal),
    ),
    ("-medium", ReasoningSuffix::Effort(ReasoningEffort::Medium)),
    (
        "-xhigh",
        ReasoningSuffix::Effort(ReasoningEffort::ExtraHigh),
    ),
    ("-high", ReasoningSuffix::Effort(ReasoningEffort::High)),
    ("-none", ReasoningSuffix::Effort(ReasoningEffort::None)),
    ("-low", ReasoningSuffix::Effort(ReasoningEffort::Low)),
    ("-max", ReasoningSuffix::Effort(ReasoningEffort::Max)),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReasoningSuffix {
    Thinking,
    Effort(ReasoningEffort),
}

/// 模型推理后缀归一错误；不保留客户端模型或显式参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningModelSuffixError {
    /// 后缀缺少基础模型，或同时叠加多个保留后缀。
    InvalidModelSuffix,
    /// 后缀语义与客户端显式推理参数冲突。
    ConflictingReasoning,
}

impl fmt::Display for ReasoningModelSuffixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModelSuffix => formatter.write_str("模型推理后缀无效"),
            Self::ConflictingReasoning => formatter.write_str("模型推理后缀与显式参数冲突"),
        }
    }
}

impl Error for ReasoningModelSuffixError {}

/// 将客户端模型末尾的闭合推理后缀归一为基础模型与 `ReasoningConfig`。
///
/// 未命中保留后缀时原样返回信封并保留同协议直通资格；命中后会保存原始客户端
/// 模型作为脱敏审计上下文，并永久丢弃原始正文。后缀匹配区分大小写，且只允许一层。
pub fn apply_reasoning_model_suffix(
    request: CanonicalRequestEnvelope,
) -> Result<CanonicalRequestEnvelope, ReasoningModelSuffixError> {
    let requested_model = request.canonical().model.clone();
    let Some((base_model, suffix)) = split_reasoning_suffix(&requested_model) else {
        return Ok(request);
    };
    if base_model.is_empty() || split_reasoning_suffix(base_model).is_some() {
        return Err(ReasoningModelSuffixError::InvalidModelSuffix);
    }

    let reasoning = merge_reasoning(request.canonical().reasoning, suffix)?;
    let mut canonical = request.into_canonical();
    canonical.model = base_model.to_owned();
    canonical.reasoning = Some(reasoning);
    Ok(CanonicalRequestEnvelope::from_normalized_model(
        canonical,
        requested_model,
    ))
}

fn split_reasoning_suffix(model: &str) -> Option<(&str, ReasoningSuffix)> {
    REASONING_SUFFIXES
        .iter()
        .find_map(|(suffix, reasoning)| model.strip_suffix(suffix).map(|base| (base, *reasoning)))
}

fn merge_reasoning(
    current: Option<ReasoningConfig>,
    suffix: ReasoningSuffix,
) -> Result<ReasoningConfig, ReasoningModelSuffixError> {
    let effort = current.and_then(|reasoning| reasoning.effort());
    let budget = current.and_then(|reasoning| reasoning.budget_tokens());
    let include_thinking = current.is_some_and(|reasoning| reasoning.include_thinking());

    match suffix {
        ReasoningSuffix::Thinking => {
            if effort == Some(ReasoningEffort::None) {
                return Err(ReasoningModelSuffixError::ConflictingReasoning);
            }
            ReasoningConfig::new(effort, budget, true)
        }
        ReasoningSuffix::Effort(suffix_effort) => {
            if effort.is_some_and(|current_effort| current_effort != suffix_effort) {
                return Err(ReasoningModelSuffixError::ConflictingReasoning);
            }
            ReasoningConfig::new(Some(suffix_effort), budget, include_thinking)
        }
    }
    .map_err(map_reasoning_config_error)
}

fn map_reasoning_config_error(_error: ReasoningConfigError) -> ReasoningModelSuffixError {
    ReasoningModelSuffixError::ConflictingReasoning
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Protocol};
    use bytes::Bytes;

    use crate::{CanonicalRequest, SameProtocolDecision, TokenCount, openai_chat};

    use super::*;

    fn request(model: &str, explicit_effort: Option<&str>) -> CanonicalRequestEnvelope {
        let reasoning = explicit_effort
            .map(|effort| format!(",\"reasoning_effort\":\"{effort}\""))
            .unwrap_or_default();
        openai_chat::parse_request_envelope(Bytes::from(format!(
            r#"{{"model":"{model}","messages":[{{"role":"user","content":"hello"}}]{reasoning}}}"#
        )))
        .unwrap()
    }

    #[test]
    fn maps_closed_suffixes_to_base_model_and_reasoning() {
        let cases = [
            ("model-thinking", "model", None, true),
            (
                "model-nothinking",
                "model",
                Some(ReasoningEffort::None),
                false,
            ),
            ("model-none", "model", Some(ReasoningEffort::None), false),
            (
                "model-minimal",
                "model",
                Some(ReasoningEffort::Minimal),
                false,
            ),
            ("model-low", "model", Some(ReasoningEffort::Low), false),
            (
                "model-medium",
                "model",
                Some(ReasoningEffort::Medium),
                false,
            ),
            ("model-high", "model", Some(ReasoningEffort::High), false),
            (
                "model-xhigh",
                "model",
                Some(ReasoningEffort::ExtraHigh),
                false,
            ),
            ("model-max", "model", Some(ReasoningEffort::Max), false),
        ];

        for (requested, base, effort, include_thinking) in cases {
            let normalized = apply_reasoning_model_suffix(request(requested, None)).unwrap();
            let reasoning = normalized.canonical().reasoning.unwrap();
            assert_eq!(normalized.canonical().model, base);
            assert_eq!(normalized.requested_model(), requested);
            assert_eq!(normalized.source_protocol(), None);
            assert_eq!(reasoning.effort(), effort);
            assert_eq!(reasoning.budget_tokens(), None);
            assert_eq!(reasoning.include_thinking(), include_thinking);
            assert!(matches!(
                normalized.into_same_protocol(Protocol::OpenAiChat),
                SameProtocolDecision::Rebuild(_)
            ));
        }
    }

    #[test]
    fn preserves_unrecognized_or_case_mismatched_models_and_source_body() {
        for model in [
            "model-highway",
            "model-High",
            "model-thinking-preview",
            "model-thinking-1024",
        ] {
            let original = request(model, None);
            let normalized = apply_reasoning_model_suffix(original.clone()).unwrap();
            assert_eq!(normalized, original);
            assert_eq!(normalized.requested_model(), model);
            assert_eq!(normalized.source_protocol(), Some(Protocol::OpenAiChat));
        }
    }

    #[test]
    fn rejects_empty_or_stacked_reserved_suffixes() {
        for model in ["-high", "model-high-low", "model-thinking-max"] {
            assert_eq!(
                apply_reasoning_model_suffix(request(model, None)),
                Err(ReasoningModelSuffixError::InvalidModelSuffix)
            );
        }
    }

    #[test]
    fn accepts_equal_effort_and_rejects_conflicting_effort() {
        let equal = apply_reasoning_model_suffix(request("gpt-5-high", Some("high"))).unwrap();
        assert_eq!(
            equal.canonical().reasoning.unwrap().effort(),
            Some(ReasoningEffort::High)
        );
        assert_eq!(
            apply_reasoning_model_suffix(request("gpt-5-high", Some("low"))),
            Err(ReasoningModelSuffixError::ConflictingReasoning)
        );
    }

    #[test]
    fn thinking_merges_with_active_effort_but_rejects_explicit_disable() {
        let active =
            apply_reasoning_model_suffix(request("claude-sonnet-thinking", Some("high"))).unwrap();
        let reasoning = active.canonical().reasoning.unwrap();
        assert_eq!(reasoning.effort(), Some(ReasoningEffort::High));
        assert!(reasoning.include_thinking());

        assert_eq!(
            apply_reasoning_model_suffix(request("claude-sonnet-thinking", Some("none"))),
            Err(ReasoningModelSuffixError::ConflictingReasoning)
        );
    }

    #[test]
    fn effort_suffix_preserves_explicit_budget_and_output_preference() {
        let mut canonical =
            CanonicalRequest::new(Operation::Chat, "model-high".to_owned(), Vec::new(), false);
        canonical.reasoning =
            Some(ReasoningConfig::new(None, Some(TokenCount::new(2_048).unwrap()), true).unwrap());

        let normalized = apply_reasoning_model_suffix(canonical.into()).unwrap();
        let reasoning = normalized.canonical().reasoning.unwrap();
        assert_eq!(reasoning.effort(), Some(ReasoningEffort::High));
        assert_eq!(reasoning.budget_tokens().unwrap().get(), 2_048);
        assert!(reasoning.include_thinking());

        let mut disabled = normalized.into_canonical();
        disabled.model = "model-none".to_owned();
        assert_eq!(
            apply_reasoning_model_suffix(disabled.into()),
            Err(ReasoningModelSuffixError::ConflictingReasoning)
        );
    }

    #[test]
    fn debug_output_does_not_expose_requested_or_base_model() {
        let normalized =
            apply_reasoning_model_suffix(request("private-reasoning-model-canary-high", None))
                .unwrap();
        let rendered = format!("{normalized:?}");
        assert!(!rendered.contains("private-reasoning-model-canary"));
        assert!(rendered.contains("has_requested_model_alias: true"));
    }
}
