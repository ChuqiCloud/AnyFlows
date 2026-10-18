use std::fmt;

use af_domain::{Operation, Protocol};
use bytes::Bytes;

use crate::{CanonicalRequest, ContentBlock, Sampling, StreamOptions, TokenCount};

/// Canonical 请求及其可选的已验证源协议正文。
///
/// 协议解析器完成 JSON、结构预算和语义校验后才能附加源正文。调用方只能读取
/// Canonical 视图，不能在保留源正文时修改请求语义。
#[derive(Clone, PartialEq)]
pub struct CanonicalRequestEnvelope {
    canonical: CanonicalRequest,
    source: Option<ValidatedSourceRequest>,
    requested_model: Option<String>,
}

impl CanonicalRequestEnvelope {
    /// 包装程序内部构造的 Canonical 请求；这类请求不具备直通资格。
    #[must_use]
    pub const fn from_canonical(canonical: CanonicalRequest) -> Self {
        Self {
            canonical,
            source: None,
            requested_model: None,
        }
    }

    /// 保存模型别名归一前的客户端模型，并永久丢弃原始正文直通资格。
    pub(crate) fn from_normalized_model(
        canonical: CanonicalRequest,
        requested_model: String,
    ) -> Self {
        Self {
            canonical,
            source: None,
            requested_model: Some(requested_model),
        }
    }

    /// 由协议解析器附加已完成信任边界校验的原始正文。
    pub(crate) const fn from_validated_source(
        canonical: CanonicalRequest,
        protocol: Protocol,
        body: Bytes,
    ) -> Self {
        Self {
            canonical,
            source: Some(ValidatedSourceRequest { protocol, body }),
            requested_model: None,
        }
    }

    /// 只读访问 Canonical 语义；正文改写必须先消费信封并丢弃直通资格。
    #[must_use]
    pub const fn canonical(&self) -> &CanonicalRequest {
        &self.canonical
    }

    /// 为对话协议转换操作类型和流选项，保留消息、工具关联及客户端模型别名。
    /// 无法跨协议表达的私有参数不得静默丢弃；无状态等默认选项可安全归一化。
    #[must_use]
    pub fn for_conversation_protocol(mut self, protocol: Protocol) -> Option<Self> {
        if !matches!(
            self.canonical.operation,
            Operation::Chat | Operation::Responses
        ) {
            return None;
        }
        let operation = match protocol {
            Protocol::OpenAiResponses => Operation::Responses,
            Protocol::OpenAiChat | Protocol::Anthropic | Protocol::Gemini => Operation::Chat,
            _ => return None,
        };
        if protocol != Protocol::Gemini && normalize_text_tool_results(&mut self.canonical)? {
            self.source = None;
        }
        let foreign_raw = self
            .canonical
            .raw_passthrough()
            .filter(|raw| raw.source_protocol() != protocol);
        if let Some(raw) = foreign_raw {
            let fields = raw.fields_for_protocol(raw.source_protocol()).ok()?;
            if !fields.iter().all(|(key, value)| match key.as_str() {
                "store" | "logprobs" => value.as_bool() == Some(false),
                "n" => value.as_u64() == Some(1),
                "parallel_tool_calls" => self.canonical.tools.is_empty(),
                "include" => value.as_array().is_some_and(Vec::is_empty),
                _ => false,
            }) {
                return None;
            }
            self.canonical = self.canonical.with_validated_raw_passthrough(None);
            self.source = None;
        }
        if self.canonical.operation != operation
            || self
                .source_protocol()
                .is_some_and(|source| source != protocol)
        {
            self.source = None;
        }
        self.canonical.operation = operation;
        if protocol != Protocol::OpenAiChat {
            self.canonical.stream_options = StreamOptions::EMPTY;
        }
        Some(self)
    }

    /// 在客户端未声明输出上限时注入网关默认值，并取消原始正文直通资格。
    ///
    /// 默认值必须同时进入计费预估与上游重建正文，禁止只降低预扣额度却保留无界上游请求。
    #[must_use]
    pub fn with_default_max_output_tokens(mut self, default: TokenCount) -> Self {
        if self.canonical.sampling.max_output_tokens().is_some() {
            return self;
        }
        self.canonical.sampling = Sampling::new(
            self.canonical.sampling.temperature(),
            self.canonical.sampling.top_p(),
            Some(default),
            self.canonical.sampling.stop_sequences().to_vec(),
        )
        .expect("已验证的采样参数补充输出上限后必须仍然有效");
        self.source = None;
        self
    }

    /// 返回已验证源正文所属协议；程序内部构造的请求返回 `None`。
    #[must_use]
    pub const fn source_protocol(&self) -> Option<Protocol> {
        match &self.source {
            Some(source) => Some(source.protocol),
            None => None,
        }
    }

    /// 返回客户端提交的原始模型别名；未发生归一时返回 Canonical 模型。
    ///
    /// 返回值属于敏感请求上下文，不得直接写入日志。
    #[must_use]
    pub fn requested_model(&self) -> &str {
        self.requested_model
            .as_deref()
            .unwrap_or(&self.canonical.model)
    }

    /// 消费信封并仅保留 Canonical 请求，同时永久丢弃原始正文。
    #[must_use]
    pub fn into_canonical(self) -> CanonicalRequest {
        self.canonical
    }

    /// 用重建后的 Canonical 请求替换正文，同时保留客户端模型别名并取消直通资格。
    ///
    /// 仅协议层的受控语义补丁可以调用该方法，调用方不得试图恢复已验证的原始字节。
    #[must_use]
    pub(crate) fn with_rebuilt_canonical(self, canonical: CanonicalRequest) -> Self {
        Self {
            canonical,
            source: None,
            requested_model: self.requested_model,
        }
    }

    /// 仅当目标协议与已验证源协议相同时，交付不可变的原始正文。
    ///
    /// 模型映射、参数覆盖或内部字段注入后不得再原字节直通。调用方通常应改走
    /// [`Self::into_canonical`] 并由目标协议构造器重新编码；协议专属构造器只有在重新解析
    /// 已验证正文、受控覆盖自有字段且保持其余字段语义时，才可把原文作为重构底稿。
    pub fn into_same_protocol(self, target_protocol: Protocol) -> SameProtocolDecision {
        if self
            .source
            .as_ref()
            .is_none_or(|source| source.protocol != target_protocol)
        {
            return SameProtocolDecision::Rebuild(self);
        }

        let Self {
            canonical,
            source: Some(source),
            requested_model,
        } = self
        else {
            unreachable!("已检查的源协议正文必须存在");
        };
        SameProtocolDecision::Passthrough(SameProtocolRequest {
            protocol: source.protocol,
            canonical,
            body: source.body,
            requested_model,
        })
    }
}

// 文本工具协议使用完整 JSON 表达结构化结果，原生 Gemini 继续保留对象。
fn normalize_text_tool_results(request: &mut CanonicalRequest) -> Option<bool> {
    let mut changed = false;
    for message in &mut request.messages {
        for block in &mut message.content {
            let ContentBlock::ToolResult {
                content,
                structured_content,
                is_error,
                ..
            } = block
            else {
                continue;
            };
            let Some(value) = structured_content.as_ref() else {
                continue;
            };
            if !content.is_empty() {
                return None;
            }
            // 普通文本在 Gemini 中使用单字段 result 包装；其余对象完整序列化。
            let text = if !*is_error
                && value.as_object().is_some_and(|object| object.len() == 1)
                && let Some(text) = value.get("result").and_then(serde_json::Value::as_str)
            {
                text.to_owned()
            } else {
                serde_json::to_string(value).ok()?
            };
            content.push(ContentBlock::Text(text));
            *structured_content = None;
            changed = true;
        }
    }
    Some(changed)
}

impl From<CanonicalRequest> for CanonicalRequestEnvelope {
    fn from(canonical: CanonicalRequest) -> Self {
        Self::from_canonical(canonical)
    }
}

impl fmt::Debug for CanonicalRequestEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalRequestEnvelope")
            .field("canonical", &self.canonical)
            .field("source_protocol", &self.source_protocol())
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .field(
                "source_body_bytes",
                &self.source.as_ref().map(|source| source.body.len()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq)]
struct ValidatedSourceRequest {
    protocol: Protocol,
    body: Bytes,
}

/// 同协议正文资格检查的闭合决策。
///
/// 调用方必须显式处理重构分支，不能在协议不匹配或缺少已验证正文时静默直通。
#[derive(Debug)]
pub enum SameProtocolDecision {
    /// 可以向同协议目标发送已验证的原始正文。
    Passthrough(SameProtocolRequest),
    /// 必须消费 Canonical 语义并由目标协议构造器重新编码。
    Rebuild(CanonicalRequestEnvelope),
}

/// 已证明源协议与目标协议一致、且请求语义未被改写的正文。
pub struct SameProtocolRequest {
    protocol: Protocol,
    canonical: CanonicalRequest,
    body: Bytes,
    requested_model: Option<String>,
}

impl SameProtocolRequest {
    /// 返回直通正文使用的协议。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 只读访问已经通过源协议信任边界校验的 Canonical 语义。
    #[must_use]
    pub const fn canonical(&self) -> &CanonicalRequest {
        &self.canonical
    }

    /// 返回保持客户端原始表示的正文。
    #[must_use]
    pub const fn body(&self) -> &Bytes {
        &self.body
    }

    /// 返回客户端提交的原始模型别名；未发生归一时返回 Canonical 模型。
    #[must_use]
    pub fn requested_model(&self) -> &str {
        self.requested_model
            .as_deref()
            .unwrap_or(&self.canonical.model)
    }

    /// 消费证明并拆分 Canonical 语义与原始正文。
    #[must_use]
    pub fn into_parts(self) -> (CanonicalRequest, Bytes) {
        (self.canonical, self.body)
    }
}

impl fmt::Debug for SameProtocolRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SameProtocolRequest")
            .field("protocol", &self.protocol)
            .field("canonical", &self.canonical)
            .field("body_bytes", &self.body.len())
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::openai_responses;

    #[test]
    fn native_requests_keep_standard_options_and_source_body() {
        let chat = crate::openai_chat::parse_request_envelope(Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}],"stream":true,"stream_options":{"include_usage":true},"seed":17}"#,
        ))
        .unwrap();
        assert_eq!(
            chat.clone()
                .for_conversation_protocol(Protocol::OpenAiChat)
                .unwrap(),
            chat
        );
        let responses = responses_request(
            br#"{"model":"gpt-test","input":"hi","store":true,"previous_response_id":"resp_previous"}"#,
        );
        assert_eq!(
            responses
                .clone()
                .for_conversation_protocol(Protocol::OpenAiResponses)
                .unwrap(),
            responses
        );
        let anthropic = crate::anthropic::parse_request_envelope(Bytes::from_static(
            br#"{"model":"claude-test","max_tokens":128,"messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        ))
        .unwrap();
        assert_eq!(
            anthropic
                .clone()
                .for_conversation_protocol(Protocol::Anthropic)
                .unwrap(),
            anthropic
        );
        let gemini = crate::gemini::parse_stream_request_envelope(
            "models/gemini-test",
            Bytes::from_static(br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#),
        )
        .unwrap();
        assert_eq!(
            gemini
                .clone()
                .for_conversation_protocol(Protocol::Gemini)
                .unwrap(),
            gemini
        );
    }

    #[test]
    fn responses_default_options_can_be_rebuilt_as_chat_without_foreign_raw() {
        let request = responses_request(br#"{"model":"gpt-test","input":"hi","store":false}"#)
            .for_conversation_protocol(Protocol::OpenAiChat)
            .unwrap();
        assert_eq!(request.canonical().operation, Operation::Chat);
        assert_eq!(request.source_protocol(), None);
        assert!(crate::openai_chat::build_request(request.canonical()).is_ok());
    }

    #[test]
    fn gemini_tool_result_objects_remain_native_or_convert_to_complete_json_text() {
        let request = crate::gemini::parse_request_envelope(
            "models/gemini-test",
            Bytes::from_static(br#"{"generationConfig":{"maxOutputTokens":128},"contents":[{"role":"user","parts":[{"text":"Weather?"}]},{"role":"model","parts":[{"functionCall":{"id":"call_weather","name":"weather","args":{}}}]},{"role":"user","parts":[{"functionResponse":{"id":"call_weather","name":"weather","response":{"temperature":25,"details":{"condition":"sunny"},"nullable":null}}}]}]}"#),
        ).unwrap();
        assert_eq!(
            request
                .clone()
                .for_conversation_protocol(Protocol::Gemini)
                .unwrap(),
            request
        );
        for target in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
        ] {
            let converted = request.clone().for_conversation_protocol(target).unwrap();
            let ContentBlock::ToolResult {
                tool_use_id,
                content,
                structured_content,
                is_error,
            } = &converted.canonical().messages[2].content[0]
            else {
                panic!("expected tool result")
            };
            assert_eq!(tool_use_id, "call_weather");
            assert!(!is_error);
            assert!(structured_content.is_none());
            let [ContentBlock::Text(text)] = content.as_slice() else {
                panic!("expected JSON text")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(text).unwrap(),
                serde_json::json!({"temperature":25,"details":{"condition":"sunny"},"nullable":null})
            );
            let built = match target {
                Protocol::OpenAiChat => {
                    crate::openai_chat::build_request(converted.canonical()).is_ok()
                }
                Protocol::OpenAiResponses => {
                    crate::openai_responses::build_request(converted.canonical()).is_ok()
                }
                Protocol::Anthropic => {
                    crate::anthropic::build_request(converted.canonical()).is_ok()
                }
                _ => unreachable!(),
            };
            assert!(built, "{target:?}");
        }
    }

    #[test]
    fn conversion_preserves_unrepresentable_options_as_explicit_failures() {
        let request = responses_request(br#"{"model":"gpt-test","input":"hi","store":true}"#);
        assert!(
            request
                .for_conversation_protocol(Protocol::OpenAiChat)
                .is_none()
        );
        let request = responses_request(
            br#"{"model":"gpt-test","input":"hi","previous_response_id":"resp_previous"}"#,
        )
        .for_conversation_protocol(Protocol::OpenAiChat)
        .unwrap();
        assert!(crate::openai_chat::build_request(request.canonical()).is_err());
    }

    fn responses_request(body: &'static [u8]) -> CanonicalRequestEnvelope {
        openai_responses::parse_request_envelope(Bytes::from_static(body)).unwrap()
    }

    #[test]
    fn missing_output_limit_uses_default_and_drops_source_passthrough() {
        let request = responses_request(br#"{"model":"gpt-test","input":"hi","store":false}"#)
            .with_default_max_output_tokens(TokenCount::new(8_192).unwrap());

        assert_eq!(
            request
                .canonical()
                .sampling
                .max_output_tokens()
                .unwrap()
                .get(),
            8_192
        );
        assert_eq!(request.source_protocol(), None);
    }

    #[test]
    fn explicit_output_limit_is_preserved_with_source_passthrough() {
        let request = responses_request(
            br#"{"model":"gpt-test","input":"hi","store":false,"max_output_tokens":2048}"#,
        )
        .with_default_max_output_tokens(TokenCount::new(8_192).unwrap());

        assert_eq!(
            request
                .canonical()
                .sampling
                .max_output_tokens()
                .unwrap()
                .get(),
            2_048
        );
        assert_eq!(request.source_protocol(), Some(Protocol::OpenAiResponses));
    }
}
