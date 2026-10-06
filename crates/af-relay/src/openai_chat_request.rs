use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, SameProtocolDecision, Sampling, StreamOptions,
    TokenCount,
    openai_chat::{MAX_OUTPUT_TOKENS, MAX_STOP_BYTES, MAX_STOP_SEQUENCES, build_request},
};
use thiserror::Error;

use crate::{RelayCandidateRequest, RelayError};

/// 目标 OpenAI Chat 渠道对单次请求施加的强类型语义覆盖。
#[derive(Clone, PartialEq)]
pub struct OpenAiChatRequestOverrides {
    model: String,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Option<Vec<String>>,
}

impl OpenAiChatRequestOverrides {
    /// 校验映射后模型与当前支持的 Canonical 采样覆盖。
    pub fn new(
        model: impl Into<String>,
        temperature: Option<f64>,
        top_p: Option<f64>,
        max_output_tokens: Option<i64>,
        stop_sequences: Option<Vec<String>>,
    ) -> Result<Self, OpenAiChatRequestOverrideError> {
        let model = model.into();
        if !valid_model(&model)
            || temperature.is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
            || top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || stop_sequences.as_ref().is_some_and(|values| {
                values.is_empty()
                    || values.len() > MAX_STOP_SEQUENCES
                    || values
                        .iter()
                        .any(|value| value.is_empty() || value.len() > MAX_STOP_BYTES)
            })
        {
            return Err(OpenAiChatRequestOverrideError);
        }
        let max_output_tokens = max_output_tokens
            .map(|tokens| {
                if !(1..=MAX_OUTPUT_TOKENS).contains(&tokens) {
                    return Err(OpenAiChatRequestOverrideError);
                }
                TokenCount::new(tokens).map_err(|_| OpenAiChatRequestOverrideError)
            })
            .transpose()?;
        Ok(Self {
            model,
            temperature,
            top_p,
            max_output_tokens,
            stop_sequences,
        })
    }

    /// 消费请求信封、永久丢弃直通资格，并构造候选级 OpenAI Chat 正文。
    pub fn prepare(
        &self,
        request: CanonicalRequestEnvelope,
    ) -> Result<RelayCandidateRequest, OpenAiChatRequestOverrideError> {
        let mut canonical = request.into_canonical();
        canonical.model.clone_from(&self.model);
        let current = &canonical.sampling;
        let max_output_tokens = match (current.max_output_tokens(), self.max_output_tokens) {
            (Some(requested), Some(limit)) => Some(requested.min(limit)),
            (requested, None) => requested,
            (None, limit) => limit,
        };
        canonical.sampling = Sampling::new(
            self.temperature.or(current.temperature()),
            self.top_p.or(current.top_p()),
            max_output_tokens,
            self.stop_sequences
                .clone()
                .unwrap_or_else(|| current.stop_sequences().to_vec()),
        )
        .map_err(|_| OpenAiChatRequestOverrideError)?;
        if canonical.stream {
            // 候选级重构仍必须保留内部结算所需的流末 usage。
            canonical.stream_options = StreamOptions::new(true);
        }
        let value = build_request(&canonical).map_err(|_| OpenAiChatRequestOverrideError)?;
        let body = serde_json::to_vec(&value)
            .map(Bytes::from)
            .map_err(|_| OpenAiChatRequestOverrideError)?;
        RelayCandidateRequest::new(self.model.clone(), Some(body))
            .map_err(map_candidate_request_error)
    }
}

impl fmt::Debug for OpenAiChatRequestOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiChatRequestOverrides")
            .field("model", &"<已脱敏>")
            .field("has_temperature", &self.temperature.is_some())
            .field("has_top_p", &self.top_p.is_some())
            .field("has_max_output_tokens", &self.max_output_tokens.is_some())
            .field("has_stop_sequences", &self.stop_sequences.is_some())
            .finish()
    }
}

/// OpenAI Chat 请求覆盖在验证或重构时失败。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("OpenAI Chat 请求覆盖无效")]
pub struct OpenAiChatRequestOverrideError;

/// 已决定正文来源的 OpenAI Chat 上游请求。
pub(crate) struct PreparedOpenAiChatRequest {
    pub(crate) canonical: CanonicalRequest,
    pub(crate) body: Bytes,
    pub(crate) include_usage: bool,
    pub(crate) client_model: String,
}

/// 在不削弱协议校验的前提下选择原字节直通或 Canonical 重构。
pub(crate) fn prepare_openai_chat_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedOpenAiChatRequest, AfError> {
    let include_usage = request.canonical().stream_options.include_usage();
    let requires_usage_injection = request.canonical().stream && !include_usage;
    let client_model = request.requested_model().to_owned();

    if !requires_usage_injection {
        match request.into_same_protocol(Protocol::OpenAiChat) {
            SameProtocolDecision::Passthrough(passthrough) => {
                let (canonical, body) = passthrough.into_parts();
                return Ok(PreparedOpenAiChatRequest {
                    canonical,
                    body,
                    include_usage,
                    client_model,
                });
            }
            SameProtocolDecision::Rebuild(request) => {
                return rebuild_request(request, include_usage, client_model);
            }
        }
    }

    rebuild_request(request, include_usage, client_model)
}

fn rebuild_request(
    request: CanonicalRequestEnvelope,
    include_usage: bool,
    client_model: String,
) -> Result<PreparedOpenAiChatRequest, AfError> {
    let mut canonical = request.into_canonical();
    if canonical.stream {
        // 内部结算始终需要上游 usage；客户端可见性仍使用原始 include_usage。
        canonical.stream_options = StreamOptions::new(true);
    }
    let value = build_request(&canonical).map_err(|_| AfError::InvalidRequest)?;
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)?;
    Ok(PreparedOpenAiChatRequest {
        canonical,
        body,
        include_usage,
        client_model,
    })
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn map_candidate_request_error(_error: RelayError) -> OpenAiChatRequestOverrideError {
    OpenAiChatRequestOverrideError
}

#[cfg(test)]
mod tests {
    use af_protocol::{anthropic, apply_reasoning_model_suffix, openai_chat};
    use serde_json::json;

    use super::*;

    #[test]
    fn validated_same_protocol_body_is_preserved_exactly() {
        let body = Bytes::from_static(
            b"{\n  \"model\": \"gpt-test\",\n  \"messages\": [{\"role\":\"user\",\"content\":\"hello\"}]\n}",
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert!(!prepared.canonical.stream);
    }

    #[test]
    fn cross_protocol_and_canonical_only_requests_are_rebuilt() {
        let anthropic_body = Bytes::from_static(
            br#"{"model":"gpt-test","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        );
        let cross_protocol = anthropic::parse_request_envelope(anthropic_body.clone()).unwrap();
        let prepared = prepare_openai_chat_request(cross_protocol).unwrap();
        assert_ne!(prepared.body, anthropic_body);
        assert!(
            serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap()["messages"]
                .is_array()
        );

        let canonical_only = CanonicalRequestEnvelope::from_canonical(prepared.canonical);
        let rebuilt = prepare_openai_chat_request(canonical_only).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&rebuilt.body).is_ok());
    }

    #[test]
    fn stream_without_client_usage_is_rebuilt_for_internal_settlement() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true}"#,
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();

        assert_ne!(prepared.body, body);
        assert!(!prepared.include_usage);
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn stream_with_explicit_usage_keeps_the_validated_body() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#,
        );
        let request = openai_chat::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert!(prepared.include_usage);
    }

    #[test]
    fn candidate_overrides_map_model_and_apply_closed_sampling_fields() {
        let request = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","messages":[{"role":"user","content":"hello"}],"temperature":0.9,"top_p":0.7,"max_completion_tokens":64,"stop":["old"]}"#,
        ))
        .unwrap();
        let overrides = OpenAiChatRequestOverrides::new(
            "upstream-model",
            Some(0.25),
            Some(0.8),
            Some(32),
            Some(vec!["private-stop-canary".to_owned()]),
        )
        .unwrap();

        let candidate = overrides.prepare(request).unwrap();
        let (model, body) = candidate.into_parts();
        let value: serde_json::Value = serde_json::from_slice(&body.unwrap()).unwrap();

        assert_eq!(model, "upstream-model");
        assert_eq!(value["model"], "upstream-model");
        assert_eq!(value["temperature"], 0.25);
        assert_eq!(value["top_p"], 0.8);
        assert_eq!(value["max_completion_tokens"], 32);
        assert_eq!(value["stop"], json!(["private-stop-canary"]));
        let rendered = format!("{overrides:?}");
        assert!(!rendered.contains("upstream-model"));
        assert!(!rendered.contains("private-stop-canary"));
    }

    #[test]
    fn maximum_output_override_only_tightens_client_limit() {
        let limited = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","messages":[{"role":"user","content":"hello"}],"max_completion_tokens":32}"#,
        ))
        .unwrap();
        let absent = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
        let overrides =
            OpenAiChatRequestOverrides::new("public-model", None, None, Some(64), None).unwrap();

        let (_, limited_body) = overrides.prepare(limited).unwrap().into_parts();
        let limited_value: serde_json::Value =
            serde_json::from_slice(&limited_body.unwrap()).unwrap();
        assert_eq!(limited_value["max_completion_tokens"], 32);

        let (_, absent_body) = overrides.prepare(absent).unwrap().into_parts();
        let absent_value: serde_json::Value =
            serde_json::from_slice(&absent_body.unwrap()).unwrap();
        assert_eq!(absent_value["max_completion_tokens"], 64);
    }

    #[test]
    fn candidate_stream_rebuild_keeps_internal_usage_requirement() {
        let request = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","messages":[{"role":"user","content":"hello"}],"stream":true}"#,
        ))
        .unwrap();
        let overrides =
            OpenAiChatRequestOverrides::new("upstream-model", None, None, None, None).unwrap();

        let (_, body) = overrides.prepare(request).unwrap().into_parts();
        let value: serde_json::Value = serde_json::from_slice(&body.unwrap()).unwrap();
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn reasoning_suffix_uses_base_upstream_model_and_keeps_client_alias() {
        let request = openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"gpt-5-high","messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
        let request = apply_reasoning_model_suffix(request).unwrap();

        let prepared = prepare_openai_chat_request(request).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(prepared.canonical.model, "gpt-5");
        assert_eq!(prepared.client_model, "gpt-5-high");
        assert_eq!(value["model"], "gpt-5");
        assert_eq!(value["reasoning_effort"], "high");
    }
}
