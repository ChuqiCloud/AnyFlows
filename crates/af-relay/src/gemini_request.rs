use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{CanonicalRequest, CanonicalRequestEnvelope, Sampling, TokenCount};
use thiserror::Error;

use crate::{RelayCandidateRequest, RelayError};

const MAX_OUTPUT_TOKENS: i64 = 1_000_000;
const MAX_STOP_SEQUENCES: usize = 4;
const MAX_STOP_BYTES: usize = 1_024;

/// 目标 Gemini 渠道对单次请求施加的强类型语义覆盖。
#[derive(Clone, PartialEq)]
pub struct GeminiRequestOverrides {
    model: String,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Option<Vec<String>>,
}

impl GeminiRequestOverrides {
    /// 校验映射后模型和 Gemini `generationConfig` 支持的采样覆盖。
    pub fn new(
        model: impl Into<String>,
        temperature: Option<f64>,
        top_p: Option<f64>,
        max_output_tokens: Option<i64>,
        stop_sequences: Option<Vec<String>>,
    ) -> Result<Self, GeminiRequestOverrideError> {
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
            return Err(GeminiRequestOverrideError);
        }
        let max_output_tokens = max_output_tokens
            .map(|tokens| {
                if !(1..=MAX_OUTPUT_TOKENS).contains(&tokens) {
                    return Err(GeminiRequestOverrideError);
                }
                TokenCount::new(tokens).map_err(|_| GeminiRequestOverrideError)
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
    ) -> Result<RelayCandidateRequest, GeminiRequestOverrideError> {
        let sampling = apply_sampling(
            &request.canonical().sampling,
            self.temperature,
            self.top_p,
            self.max_output_tokens,
            self.stop_sequences.as_deref(),
        )?;
        let (_, body) = request
            .with_routed_parameters(self.model.clone(), sampling)
            .plan_conversation(Protocol::Gemini)
            .ok_or(GeminiRequestOverrideError)?
            .into_parts();
        RelayCandidateRequest::new(self.model.clone(), Some(body))
            .map_err(map_candidate_request_error)
    }
}

impl fmt::Debug for GeminiRequestOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeminiRequestOverrides")
            .field("model", &"<已脱敏>")
            .field("has_temperature", &self.temperature.is_some())
            .field("has_top_p", &self.top_p.is_some())
            .field("has_max_output_tokens", &self.max_output_tokens.is_some())
            .field("has_stop_sequences", &self.stop_sequences.is_some())
            .finish()
    }
}

/// Gemini 请求覆盖在验证或重构时失败。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("Gemini 请求覆盖无效")]
pub struct GeminiRequestOverrideError;

/// 已决定正文来源的原生 Gemini 上游请求。
pub(crate) struct PreparedGeminiRequest {
    pub(crate) canonical: CanonicalRequest,
    pub(crate) body: Bytes,
    pub(crate) client_model: String,
}

/// 使用统一目标转换计划，保留同源扩展并执行跨协议归一。
pub(crate) fn prepare_gemini_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedGeminiRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    let (request, body) = request
        .plan_conversation(Protocol::Gemini)
        .ok_or(AfError::InvalidRequest)?
        .into_parts();
    Ok(PreparedGeminiRequest {
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
) -> Result<Sampling, GeminiRequestOverrideError> {
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
    .map_err(|_| GeminiRequestOverrideError)
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.contains('/')
        && !model.chars().any(char::is_control)
}

fn map_candidate_request_error(_error: RelayError) -> GeminiRequestOverrideError {
    GeminiRequestOverrideError
}

#[cfg(test)]
mod tests {
    use af_protocol::gemini;
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn same_protocol_request_preserves_validated_bytes() {
        let body = Bytes::from_static(
            br#"{
  "contents": [{"role":"user","parts":[{"text":"hello"}]}]
}"#,
        );
        let request = gemini::parse_request_envelope("models/gemini-test", body.clone()).unwrap();

        let prepared = prepare_gemini_request(request).unwrap();

        assert_eq!(prepared.body, body);
        assert_eq!(prepared.client_model, "gemini-test");
    }

    #[test]
    fn candidate_overrides_rebuild_streaming_model_and_sampling() {
        let request = gemini::parse_stream_request_envelope(
            "models/public-model",
            Bytes::from_static(
                br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{"temperature":1.5,"maxOutputTokens":64}}"#,
            ),
        )
        .unwrap();
        let overrides = GeminiRequestOverrides::new(
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
        assert_eq!(value["generationConfig"]["temperature"], 0.25);
        assert_eq!(value["generationConfig"]["topP"], 0.8);
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 32);
        assert_eq!(
            value["generationConfig"]["stopSequences"],
            json!(["private-stop-canary"])
        );
        assert!(value.get("model").is_none());
        assert!(value.get("stream").is_none());
        let debug = format!("{overrides:?}");
        assert!(!debug.contains("private-upstream-model"));
        assert!(!debug.contains("private-stop-canary"));
    }

    #[test]
    fn maximum_output_override_only_tightens_client_limit() {
        let request = gemini::parse_request_envelope(
            "models/public-model",
            Bytes::from_static(
                br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{"maxOutputTokens":32}}"#,
            ),
        )
        .unwrap();
        let overrides =
            GeminiRequestOverrides::new("public-model", None, None, Some(64), None).unwrap();

        let (_, body) = overrides.prepare(request).unwrap().into_parts();
        let value: Value = serde_json::from_slice(&body.unwrap()).unwrap();
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 32);
    }

    #[test]
    fn invalid_gemini_model_and_stop_sequences_fail_closed() {
        assert!(GeminiRequestOverrides::new("private/model", None, None, None, None).is_err());
        assert!(GeminiRequestOverrides::new("gemini", Some(2.01), None, None, None).is_err());
        assert!(
            GeminiRequestOverrides::new("gemini", None, None, None, Some(vec![String::new()]),)
                .is_err()
        );
    }

    #[test]
    fn native_extensions_survive_defaults_and_candidate_overrides() {
        let original = json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "hello", "x_part": {"keep": true}}],
                "x_message": {"keep": [1, 2]}
            }],
            "generationConfig": {"x_vendor_option": {"keep": true}},
            "x_vendor": {"keep": {"enabled": true}}
        });
        let request = gemini::parse_stream_request_envelope(
            "models/public-model",
            Bytes::from(serde_json::to_vec(&original).unwrap()),
        )
        .unwrap()
        .with_default_max_output_tokens(TokenCount::new(256).unwrap());
        let prepared = prepare_gemini_request(request.clone()).unwrap();
        assert!(prepared.canonical.stream);
        assert_eq!(prepared.client_model, "public-model");
        let (_, candidate) =
            GeminiRequestOverrides::new("upstream-model", Some(0.25), None, Some(128), None)
                .unwrap()
                .prepare(request)
                .unwrap()
                .into_parts();
        for body in [prepared.body, candidate.unwrap()] {
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["contents"], original["contents"]);
            assert_eq!(value["x_vendor"], original["x_vendor"]);
            assert_eq!(
                value["generationConfig"]["x_vendor_option"],
                original["generationConfig"]["x_vendor_option"]
            );
            assert!(value.get("model").is_none());
            assert!(value.get("stream").is_none());
        }
    }
}
