use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, Sampling, TokenCount,
    openai_responses::{MAX_OUTPUT_TOKENS, build_request},
};
use thiserror::Error;

use crate::{RelayCandidateRequest, RelayError};

/// 目标 OpenAI Responses 渠道对单次请求施加的强类型语义覆盖。
#[derive(Clone, PartialEq)]
pub struct OpenAiResponsesRequestOverrides {
    model: String,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
}

impl OpenAiResponsesRequestOverrides {
    /// 校验映射后模型与 Responses 支持的 Canonical 采样覆盖。
    pub fn new(
        model: impl Into<String>,
        temperature: Option<f64>,
        top_p: Option<f64>,
        max_output_tokens: Option<i64>,
    ) -> Result<Self, OpenAiResponsesRequestOverrideError> {
        let model = model.into();
        if !valid_model(&model)
            || temperature.is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
            || top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(OpenAiResponsesRequestOverrideError);
        }
        let max_output_tokens = max_output_tokens
            .map(|tokens| {
                if !(1..=MAX_OUTPUT_TOKENS).contains(&tokens) {
                    return Err(OpenAiResponsesRequestOverrideError);
                }
                TokenCount::new(tokens).map_err(|_| OpenAiResponsesRequestOverrideError)
            })
            .transpose()?;
        Ok(Self {
            model,
            temperature,
            top_p,
            max_output_tokens,
        })
    }

    /// 消费请求信封、应用渠道覆盖并构造候选级无状态 Responses 正文。
    pub fn prepare(
        &self,
        request: CanonicalRequestEnvelope,
    ) -> Result<RelayCandidateRequest, OpenAiResponsesRequestOverrideError> {
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
            current.stop_sequences().to_vec(),
        )
        .map_err(|_| OpenAiResponsesRequestOverrideError)?;
        let body =
            build_stateless_body(&canonical).map_err(|_| OpenAiResponsesRequestOverrideError)?;
        RelayCandidateRequest::new(self.model.clone(), Some(body))
            .map_err(map_candidate_request_error)
    }
}

impl fmt::Debug for OpenAiResponsesRequestOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiResponsesRequestOverrides")
            .field("model", &"<已脱敏>")
            .field("has_temperature", &self.temperature.is_some())
            .field("has_top_p", &self.top_p.is_some())
            .field("has_max_output_tokens", &self.max_output_tokens.is_some())
            .finish()
    }
}

/// OpenAI Responses 请求覆盖在验证或重构时失败。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("OpenAI Responses 请求覆盖无效")]
pub struct OpenAiResponsesRequestOverrideError;

/// 已完成无状态策略与协议校验的 OpenAI Responses 上游请求。
pub(crate) struct PreparedOpenAiResponsesRequest {
    pub(crate) canonical: CanonicalRequest,
    pub(crate) body: Bytes,
    pub(crate) client_model: String,
}

/// 重构 Responses 请求并固定 `store:false`，避免产生网关无法管理的上游状态。
pub(crate) fn prepare_openai_responses_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedOpenAiResponsesRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    let canonical = request.into_canonical();
    let body = build_stateless_body(&canonical)?;
    Ok(PreparedOpenAiResponsesRequest {
        canonical,
        body,
        client_model,
    })
}

fn build_stateless_body(request: &CanonicalRequest) -> Result<Bytes, AfError> {
    if request.continuation.previous_response_id().is_some()
        || request.continuation.conversation_id().is_some()
    {
        // 续接需要公开 ID 到上游 ID 的持久映射；未实现前不能透传伪造标识。
        return Err(AfError::InvalidRequest);
    }
    let mut value = build_request(request).map_err(|_| AfError::InvalidRequest)?;
    let object = value.as_object_mut().ok_or(AfError::Internal)?;
    if object.get("store").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(AfError::InvalidRequest);
    }
    object.insert("store".to_owned(), serde_json::Value::Bool(false));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn map_candidate_request_error(_error: RelayError) -> OpenAiResponsesRequestOverrideError {
    OpenAiResponsesRequestOverrideError
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Role};
    use af_protocol::{ContentBlock, Message, openai_responses};
    use serde_json::{Value, json};

    use super::*;

    fn envelope(body: Value) -> CanonicalRequestEnvelope {
        openai_responses::parse_request_envelope(Bytes::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    #[test]
    fn production_request_is_always_stateless_and_preserves_stream_mode() {
        for stream in [false, true] {
            let prepared = prepare_openai_responses_request(envelope(json!({
                "model": "gpt-test",
                "input": "hello",
                "stream": stream
            })))
            .unwrap();
            let value: Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(value["store"], false);
            assert_eq!(value["stream"], stream);
            assert_eq!(prepared.canonical.stream, stream);
            assert_eq!(prepared.client_model, "gpt-test");
        }
    }

    #[test]
    fn requested_storage_and_remote_continuation_fail_closed() {
        for request in [
            json!({"model": "gpt-test", "input": "hello", "store": true}),
            json!({"model": "gpt-test", "previous_response_id": "resp-private"}),
            json!({"model": "gpt-test", "conversation": "conv-private"}),
        ] {
            assert!(prepare_openai_responses_request(envelope(request)).is_err());
        }
    }

    #[test]
    fn candidate_overrides_rebuild_model_and_sampling_without_leaks() {
        let request = CanonicalRequest::new(
            Operation::Responses,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("hello".to_owned())],
            )],
            false,
        )
        .into();
        let overrides = OpenAiResponsesRequestOverrides::new(
            "private-upstream-model",
            Some(0.25),
            Some(0.8),
            Some(128),
        )
        .unwrap();
        let candidate = overrides.prepare(request).unwrap();
        let debug = format!("{overrides:?}{candidate:?}");
        assert!(!debug.contains("private-upstream-model"));
    }
}
