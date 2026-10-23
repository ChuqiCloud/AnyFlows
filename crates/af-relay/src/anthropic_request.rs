use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, Sampling, TokenCount,
    anthropic::{MAX_OUTPUT_TOKENS, MAX_STOP_BYTES, MAX_STOP_SEQUENCES},
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

    /// 应用渠道模型与采样策略，再由统一转换计划生成候选正文。
    pub fn prepare(
        &self,
        request: CanonicalRequestEnvelope,
    ) -> Result<RelayCandidateRequest, AnthropicRequestOverrideError> {
        let sampling = apply_sampling(
            &request.canonical().sampling,
            self.temperature,
            self.top_p,
            self.max_output_tokens,
            self.stop_sequences.as_deref(),
        )?;
        let (_, body) = request
            .with_routed_parameters(self.model.clone(), sampling)
            .plan_conversation(Protocol::Anthropic)
            .ok_or(AnthropicRequestOverrideError)?
            .into_parts();
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

/// 使用统一目标转换计划，保留同源扩展并执行跨协议归一。
pub(crate) fn prepare_anthropic_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedAnthropicRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    let (request, body) = request
        .plan_conversation(Protocol::Anthropic)
        .ok_or(AfError::InvalidRequest)?
        .into_parts();
    Ok(PreparedAnthropicRequest {
        canonical: request.into_canonical(),
        body,
        client_model,
    })
}

fn apply_sampling(
    current: &Sampling,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Option<&[String]>,
) -> Result<Sampling, AnthropicRequestOverrideError> {
    let max_output_tokens = match (current.max_output_tokens(), max_output_tokens) {
        (Some(requested), Some(limit)) => Some(requested.min(limit)),
        (requested, None) => requested,
        (None, limit) => limit,
    };
    Sampling::new(
        temperature.or(current.temperature()),
        top_p.or(current.top_p()),
        max_output_tokens,
        stop_sequences
            .map(<[String]>::to_vec)
            .unwrap_or_else(|| current.stop_sequences().to_vec()),
    )
    .map_err(|_| AnthropicRequestOverrideError)
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

    #[test]
    fn native_extensions_survive_preparation_and_candidate_overrides() {
        let original = json!({
            "model": "public-model",
            "max_tokens": 256,
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "hello", "x_part": {"keep": true}}],
                "x_message": {"keep": [1, 2]}
            }],
            "stream": true,
            "x_vendor": {"keep": {"enabled": true}}
        });
        let request =
            anthropic::parse_request_envelope(Bytes::from(serde_json::to_vec(&original).unwrap()))
                .unwrap();
        let prepared = prepare_anthropic_request(request.clone()).unwrap();
        assert_eq!(prepared.client_model, "public-model");
        let (_, candidate) =
            AnthropicRequestOverrides::new("upstream-model", Some(0.25), None, Some(128), None)
                .unwrap()
                .prepare(request)
                .unwrap()
                .into_parts();
        for body in [prepared.body, candidate.unwrap()] {
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["messages"], original["messages"]);
            assert_eq!(value["x_vendor"], original["x_vendor"]);
            assert_eq!(value["stream"], true);
        }
    }
}
