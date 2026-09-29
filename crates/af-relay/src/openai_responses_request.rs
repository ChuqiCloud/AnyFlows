use std::fmt;

use af_adapter::Bytes;
use af_domain::{AfError, MAX_MODEL_NAME_BYTES, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, SameProtocolDecision, Sampling, TokenCount,
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
        let (mut canonical, source_body) = into_openai_responses_parts(request);
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
        let body = build_stateless_body(&canonical, source_body.as_ref())
            .map_err(|_| OpenAiResponsesRequestOverrideError)?;
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

/// 从 Canonical 或已验证的同协议底稿重构 Responses 请求，并固定 `store:false`。
///
/// 该边界避免产生网关无法管理的上游状态，同时保留已校验的无状态 Agent 扩展。
pub(crate) fn prepare_openai_responses_request(
    request: CanonicalRequestEnvelope,
) -> Result<PreparedOpenAiResponsesRequest, AfError> {
    let client_model = request.requested_model().to_owned();
    let (canonical, source_body) = into_openai_responses_parts(request);
    let body = build_stateless_body(&canonical, source_body.as_ref())?;
    Ok(PreparedOpenAiResponsesRequest {
        canonical,
        body,
        client_model,
    })
}

fn into_openai_responses_parts(
    request: CanonicalRequestEnvelope,
) -> (CanonicalRequest, Option<Bytes>) {
    match request.into_same_protocol(Protocol::OpenAiResponses) {
        SameProtocolDecision::Passthrough(passthrough) => {
            let (canonical, body) = passthrough.into_parts();
            (canonical, Some(body))
        }
        SameProtocolDecision::Rebuild(request) => (request.into_canonical(), None),
    }
}

fn build_stateless_body(
    request: &CanonicalRequest,
    source_body: Option<&Bytes>,
) -> Result<Bytes, AfError> {
    if request.continuation.previous_response_id().is_some()
        || request.continuation.conversation_id().is_some()
    {
        // 续接需要公开 ID 到上游 ID 的持久映射；未实现前不能透传伪造标识。
        return Err(AfError::InvalidRequest);
    }
    // 同协议请求以已验证原文为底稿，只覆盖网关拥有的字段，避免 Agent 扩展被重编码丢失。
    let mut value = match source_body {
        Some(body) => serde_json::from_slice(body).map_err(|_| AfError::InvalidRequest)?,
        None => build_request(request).map_err(|_| AfError::InvalidRequest)?,
    };
    let object = value.as_object_mut().ok_or(AfError::Internal)?;
    if object.get("store").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(AfError::InvalidRequest);
    }
    object.insert(
        "model".to_owned(),
        serde_json::Value::String(request.model.clone()),
    );
    insert_optional_number(object, "temperature", request.sampling.temperature())?;
    insert_optional_number(object, "top_p", request.sampling.top_p())?;
    match request.sampling.max_output_tokens() {
        Some(tokens) => {
            object.insert(
                "max_output_tokens".to_owned(),
                serde_json::Value::Number(tokens.get().into()),
            );
        }
        None => {
            object.remove("max_output_tokens");
        }
    }
    object.insert("store".to_owned(), serde_json::Value::Bool(false));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)
}

fn insert_optional_number(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<f64>,
) -> Result<(), AfError> {
    match value {
        Some(value) => {
            let number = serde_json::Number::from_f64(value).ok_or(AfError::InvalidRequest)?;
            object.insert(key.to_owned(), serde_json::Value::Number(number));
        }
        None => {
            object.remove(key);
        }
    }
    Ok(())
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
