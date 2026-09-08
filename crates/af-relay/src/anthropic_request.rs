use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, SameProtocolDecision, Sampling, TokenCount,
    anthropic::{MAX_OUTPUT_TOKENS, MAX_STOP_BYTES, MAX_STOP_SEQUENCES, build_request},
};
use thiserror::Error;

use crate::{RelayCandidateRequest, RelayError};

/// 目标 Anthropic 渠道对单次请求施加的强类型语义覆盖。
#[derive(Clone, PartialEq)]
pub struct AnthropicRequestOverrides {
    model: String,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Option<Vec<String>>,
}

impl AnthropicRequestOverrides {
    /// 校验映射后模型和 Anthropic Messages 支持的采样覆盖。
    pub fn new(
        model: impl Into<String>,
        temperature: Option<f64>,
        top_p: Option<f64>,
        max_output_tokens: Option<i64>,
        stop_sequences: Option<Vec<String>>,
    ) -> Result<Self, AnthropicRequestOverrideError> {
        let model = model.into();
        if !valid_model(&model)
            || temperature.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || stop_sequences.as_ref().is_some_and(|values| {
                values.is_empty()
                    || values.len() > MAX_STOP_SEQUENCES
                    || values
                        .iter()
                        .any(|value| value.is_empty() || value.len() > MAX_STOP_BYTES)
            })
        {
            return Err(AnthropicRequestOverrideError);
        }
        let max_output_tokens = max_output_tokens
            .map(|tokens| {
                if !(1..=MAX_OUTPUT_TOKENS).contains(&tokens) {
                    return Err(AnthropicRequestOverrideError);
                }
                TokenCount::new(tokens).map_err(|_| AnthropicRequestOverrideError)
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

    /// 消费请求信封、丢弃直通资格，并构造候选级 Anthropic 正文。
    pub fn prepare(
        &self,
        request: CanonicalRequestEnvelope,
    ) -> Result<RelayCandidateRequest, AnthropicRequestOverrideError> {
        let mut canonical = request.into_canonical();
        canonical.model.clone_from(&self.model);
        apply_sampling(
            &mut canonical,
            self.temperature,
            self.top_p,
            self.max_output_tokens,
            self.stop_sequences.as_deref(),
        )?;
        let body = build_body(&canonical).map_err(|_| AnthropicRequestOverrideError)?;
        RelayCandidateRequest::new(self.model.clone(), Some(body))
            .map_err(map_candidate_request_error)
    }
}

impl fmt::Debug for AnthropicRequestOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicRequestOverrides")
            .field("model", &"<已脱敏>")
            .field("has_temperature", &self.temperature.is_some())
            .field("has_top_p", &self.top_p.is_some())
            .field("has_max_output_tokens", &self.max_output_tokens.is_some())
            .field("has_stop_sequences", &self.stop_sequences.is_some())
            .finish()
    }
}

/// Anthropic 请求覆盖在验证或重构时失败。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("Anthropic 请求覆盖无效")]
pub struct AnthropicRequestOverrideError;

/// 已决定正文来源的原生 Anthropic 上游请求。
pub(crate) struct PreparedAnthropicRequest {
    pub(crate) canonical: CanonicalRequest,
    pub(crate) body: Bytes,
    pub(crate) client_model: String,
}

/// 在不削弱协议校验的前提下选择原字节直通或 Canonical 重构。
pub(crate) fn prepare_anthropic_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedAnthropicRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    match request.into_same_protocol(Protocol::Anthropic) {
        SameProtocolDecision::Passthrough(passthrough) => {
            let (canonical, body) = passthrough.into_parts();
            Ok(PreparedAnthropicRequest {
                canonical,
                body,
                client_model,
            })
        }
        SameProtocolDecision::Rebuild(request) => {
            let canonical = request.into_canonical();
            let body = build_body(&canonical).map_err(|_| AfError::InvalidRequest)?;
            Ok(PreparedAnthropicRequest {
                canonical,
                body,
                client_model,
            })
        }
    }
}

fn apply_sampling(
    request: &mut CanonicalRequest,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Option<&[String]>,
) -> Result<(), AnthropicRequestOverrideError> {
    let current = &request.sampling;
    let max_output_tokens = match (current.max_output_tokens(), max_output_tokens) {
        (Some(requested), Some(limit)) => Some(requested.min(limit)),
        (requested, None) => requested,
        (None, limit) => limit,
    };
    request.sampling = Sampling::new(
        temperature.or(current.temperature()),
        top_p.or(current.top_p()),
        max_output_tokens,
        stop_sequences
            .map(<[String]>::to_vec)
            .unwrap_or_else(|| current.stop_sequences().to_vec()),
    )
    .map_err(|_| AnthropicRequestOverrideError)?;
    Ok(())
}

fn build_body(request: &CanonicalRequest) -> Result<Bytes, ()> {
    let value = build_request(request).map_err(|_| ())?;
    serde_json::to_vec(&value).map(Bytes::from).map_err(|_| ())
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn map_candidate_request_error(_error: RelayError) -> AnthropicRequestOverrideError {
    AnthropicRequestOverrideError
}

#[cfg(test)]
mod tests {
    use af_protocol::anthropic;
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn same_protocol_request_preserves_validated_bytes() {
        let body = Bytes::from_static(
            br#"{
  "model": "claude-test",
  "max_tokens": 32,
  "messages": [{"role":"user","content":"hello"}]
}"#,
        );
        let request = anthropic::parse_request_envelope(body.clone()).unwrap();

        let prepared = prepare_anthropic_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert_eq!(prepared.client_model, "claude-test");
    }

    #[test]
    fn candidate_overrides_rebuild_streaming_model_and_sampling() {
        let request = anthropic::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","max_tokens":64,"messages":[{"role":"user","content":"hello"}],"stream":true,"temperature":0.9}"#,
        ))
        .unwrap();
        let overrides = AnthropicRequestOverrides::new(
            "private-upstream-model",
            Some(0.25),
            Some(0.8),
            Some(32),
            Some(vec!["private-stop-canary".to_owned()]),
        )
        .unwrap();

        let candidate = overrides.prepare(request).unwrap();
        let (model, body) = candidate.into_parts();
        let value: Value = serde_json::from_slice(&body.unwrap()).unwrap();

        assert_eq!(model, "private-upstream-model");
        assert_eq!(value["model"], "private-upstream-model");
        assert_eq!(value["temperature"], 0.25);
        assert_eq!(value["top_p"], 0.8);
        assert_eq!(value["max_tokens"], 32);
        assert_eq!(value["stop_sequences"], json!(["private-stop-canary"]));
        assert_eq!(value["stream"], true);
        let debug = format!("{overrides:?}");
        assert!(!debug.contains("private-upstream-model"));
        assert!(!debug.contains("private-stop-canary"));
    }

    #[test]
    fn maximum_output_override_only_tightens_client_limit() {
        let request = anthropic::parse_request_envelope(Bytes::from_static(
            br#"{"model":"public-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap();
        let overrides =
            AnthropicRequestOverrides::new("public-model", None, None, Some(64), None).unwrap();

        let (_, body) = overrides.prepare(request).unwrap().into_parts();
        let value: Value = serde_json::from_slice(&body.unwrap()).unwrap();
        assert_eq!(value["max_tokens"], 32);
    }

    #[test]
    fn invalid_anthropic_temperature_and_stop_sequences_fail_closed() {
        assert!(AnthropicRequestOverrides::new("claude", Some(1.01), None, None, None).is_err());
        assert!(
            AnthropicRequestOverrides::new("claude", None, None, None, Some(vec![String::new()]),)
                .is_err()
        );
    }
}
