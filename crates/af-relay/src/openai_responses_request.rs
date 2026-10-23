use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, Sampling, TokenCount,
    openai_responses::MAX_OUTPUT_TOKENS,
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
        let current = &request.canonical().sampling;
        let max_output_tokens = match (current.max_output_tokens(), self.max_output_tokens) {
            (Some(requested), Some(limit)) => Some(requested.min(limit)),
            (requested, None) => requested,
            (None, limit) => limit,
        };
        let sampling = Sampling::new(
            self.temperature.or(current.temperature()),
            self.top_p.or(current.top_p()),
            max_output_tokens,
            current.stop_sequences().to_vec(),
        )
        .map_err(|_| OpenAiResponsesRequestOverrideError)?;
        let (_, body) = request
            .with_routed_parameters(self.model.clone(), sampling)
            .plan_conversation(Protocol::OpenAiResponses)
            .ok_or(OpenAiResponsesRequestOverrideError)?
            .into_parts();
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

/// 使用统一目标转换计划，并执行其无状态 Responses 策略。
pub(crate) fn prepare_openai_responses_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedOpenAiResponsesRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    let (request, body) = request
        .plan_conversation(Protocol::OpenAiResponses)
        .ok_or(AfError::InvalidRequest)?
        .into_parts();
    Ok(PreparedOpenAiResponsesRequest {
        canonical: request.into_canonical(),
        body,
        client_model,
    })
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
                "include": ["reasoning.encrypted_content"],
                "stream": stream
            })))
            .unwrap();
            let value: Value = serde_json::from_slice(&prepared.body).unwrap();
            assert_eq!(value["store"], false);
            assert_eq!(value["include"], json!(["reasoning.encrypted_content"]));
            assert_eq!(value["stream"], stream);
            assert_eq!(prepared.canonical.stream, stream);
            assert_eq!(prepared.client_model, "gpt-test");
        }
    }

    #[test]
    fn candidate_overrides_preserve_stateless_agent_reasoning() {
        let request = envelope(json!({
            "model": "public-model",
            "input": [{
                "type": "reasoning",
                "id": "rs_public",
                "encrypted_content": "encrypted-reasoning-canary",
                "summary": []
            }, {
                "role": "user",
                "content": [{"type": "input_text", "text": "continue"}]
            }],
            "include": ["reasoning.encrypted_content"],
            "store": false,
            "stream": true
        }));
        let overrides =
            OpenAiResponsesRequestOverrides::new("private-upstream-model", None, None, None)
                .unwrap();

        let (_, body) = overrides.prepare(request).unwrap().into_parts();
        let value: Value = serde_json::from_slice(&body.unwrap()).unwrap();
        assert_eq!(value["model"], "private-upstream-model");
        assert_eq!(value["store"], false);
        assert_eq!(value["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(
            value["input"][0]["encrypted_content"],
            "encrypted-reasoning-canary"
        );
        assert_eq!(value["input"][0]["id"], "rs_public");
    }

    #[tokio::test]
    async fn codex_finalization_removes_billing_default_and_channel_sampling_overrides() {
        use af_adapter::{
            Adaptor as _, AdaptorTarget, Credential, HeaderMap, HttpClientConfig, HttpClientPool,
            Method, OpenAiAdaptor, RelayContext, ResponseMode, UpstreamRequest,
        };

        let request = envelope(json!({
            "model": "public-model",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "hello"}]}],
            "include": ["reasoning.encrypted_content"],
            "store": false,
            "stream": true
        }))
        .with_default_max_output_tokens(TokenCount::new(8192).unwrap());
        let adaptor = OpenAiAdaptor::with_protocol_and_supported_models(
            Protocol::OpenAiResponses,
            ["upstream-model"],
        )
        .unwrap();
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
        .with_oauth_identity(Some("codex"), Some("account-test"));
        // 同时覆盖平台缺省上限与渠道显式上限；二者都不能泄漏到 Codex 请求。
        for limit in [None, Some(128)] {
            let (_, body) =
                OpenAiResponsesRequestOverrides::new("upstream-model", Some(0.5), Some(0.9), limit)
                    .unwrap()
                    .prepare(request.clone())
                    .unwrap()
                    .into_parts();
            let before: Value = serde_json::from_slice(body.as_ref().unwrap()).unwrap();
            assert_eq!(before["max_output_tokens"], limit.unwrap_or(8192));
            let outgoing = UpstreamRequest::new(
                Method::POST,
                adaptor
                    .build_url(
                        &context,
                        AdaptorTarget::new(
                            "upstream-model",
                            Operation::Responses,
                            ResponseMode::Stream,
                        ),
                    )
                    .unwrap(),
                HeaderMap::new(),
                body,
            )
            .unwrap()
            .with_response_mode(ResponseMode::Stream);
            let outgoing = adaptor
                .finalize_request(
                    outgoing,
                    &Credential::oauth("oauth-test").unwrap(),
                    &context,
                )
                .await
                .unwrap();
            let after: Value = serde_json::from_slice(outgoing.body().unwrap()).unwrap();
            assert!(after.get("max_output_tokens").is_none());
            assert!(after.get("temperature").is_none());
            assert!(after.get("top_p").is_none());
            assert_eq!(after["model"], "upstream-model");
            assert_eq!(after["input"], before["input"]);
            assert_eq!(after["include"], before["include"]);
            assert_eq!(after["store"], false);
            assert_eq!(after["stream"], true);
        }
        assert_eq!(
            request
                .canonical()
                .sampling
                .max_output_tokens()
                .unwrap()
                .get(),
            8192
        );
    }

    #[test]
    fn remote_continuation_requires_resolved_local_history() {
        for request in [
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

    #[test]
    fn storage_preference_is_normalized_and_extensions_survive_routing() {
        let original = json!({
            "model": "public-model",
            "input": [{
                "role": "user",
                "content": [{"type": "input_text", "text": "hello", "x_part": {"keep": true}}],
                "x_message": {"keep": [1, 2]}
            }],
            "stream": true,
            "store": true,
            "x_vendor": {"keep": {"enabled": true}}
        });
        let request = envelope(original.clone())
            .with_default_max_output_tokens(TokenCount::new(256).unwrap());
        let prepared = prepare_openai_responses_request(request.clone()).unwrap();
        assert_eq!(prepared.client_model, "public-model");
        let (_, candidate) =
            OpenAiResponsesRequestOverrides::new("upstream-model", Some(0.25), None, Some(128))
                .unwrap()
                .prepare(request)
                .unwrap()
                .into_parts();
        for body in [prepared.body, candidate.unwrap()] {
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["input"], original["input"]);
            assert_eq!(value["x_vendor"], original["x_vendor"]);
            assert_eq!(value["store"], false);
            assert_eq!(value["stream"], true);
        }
    }

    #[test]
    fn candidate_overrides_cannot_bypass_remote_continuation_policy() {
        let overrides =
            OpenAiResponsesRequestOverrides::new("upstream-model", None, None, None).unwrap();
        for request in [
            json!({"model": "gpt-test", "previous_response_id": "resp-private"}),
            json!({"model": "gpt-test", "conversation": "conv-private"}),
        ] {
            assert!(overrides.prepare(envelope(request)).is_err());
        }
    }
}
