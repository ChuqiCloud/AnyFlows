use std::fmt;

use af_domain::Protocol;
use bytes::Bytes;

use crate::{AnthropicBetaError, CanonicalRequest, Sampling, TokenCount, normalize_anthropic_beta};

/// Canonical 请求及其可选的已验证源协议正文。
///
/// 协议解析器完成 JSON、结构预算和语义校验后才能附加源正文。调用方只能读取
/// Canonical 视图，不能在保留源正文时修改请求语义。
#[derive(Clone, PartialEq)]
pub struct CanonicalRequestEnvelope {
    canonical: CanonicalRequest,
    source: Option<ValidatedSourceRequest>,
    // 来源证明独立于原始正文；重建正文不能使供应商签名跨协议重放。
    origin_protocol: Option<Protocol>,
    requested_model: Option<String>,
    native_template: Option<ValidatedSourceRequest>,
    // 仅供语义转换读取；不能用作恢复直通资格的底稿。
    source_context: Option<ValidatedSourceRequest>,
    native_only: bool,
    normalized_protocol: Option<Protocol>,
    patch_reasoning: bool,
    normalization_dirty: bool,
    anthropic_beta: Option<String>,
}

impl CanonicalRequestEnvelope {
    /// 包装程序内部构造的 Canonical 请求；这类请求不具备直通资格。
    #[must_use]
    pub const fn from_canonical(canonical: CanonicalRequest) -> Self {
        Self {
            canonical,
            source: None,
            origin_protocol: None,
            requested_model: None,
            native_template: None,
            source_context: None,
            native_only: false,
            normalized_protocol: None,
            patch_reasoning: false,
            normalization_dirty: false,
            anthropic_beta: None,
        }
    }

    /// 保存模型别名归一前的客户端模型与来源协议，并永久丢弃原始正文直通资格。
    #[cfg(test)]
    pub(crate) fn from_normalized_model(
        canonical: CanonicalRequest,
        requested_model: String,
        origin_protocol: Option<Protocol>,
    ) -> Self {
        Self {
            canonical,
            source: None,
            origin_protocol,
            requested_model: Some(requested_model),
            native_template: None,
            source_context: None,
            native_only: false,
            normalized_protocol: None,
            patch_reasoning: false,
            normalization_dirty: false,
            anthropic_beta: None,
        }
    }

    /// 模型后缀只改变网关拥有的模型/推理字段，保留来源扩展和原生底稿。
    pub(crate) fn with_normalized_model(
        mut self,
        canonical: CanonicalRequest,
        requested_model: String,
    ) -> Self {
        self.canonical = canonical;
        self.requested_model = Some(requested_model);
        self.source = None;
        self.patch_reasoning = true;
        self.normalization_dirty = self.normalized_protocol.is_some();
        self
    }

    /// 原生预算受渠道输出上限约束时，只覆盖推理字段并保留其余原生内容。
    pub(crate) fn with_native_reasoning(mut self, reasoning: crate::ReasoningConfig) -> Self {
        self.canonical.reasoning = Some(reasoning);
        self.source = None;
        self.patch_reasoning = true;
        self
    }

    pub(crate) fn from_compatible_source(
        canonical: CanonicalRequest,
        protocol: Protocol,
        body: Bytes,
        native_only: bool,
    ) -> Self {
        Self {
            canonical,
            source: Some(ValidatedSourceRequest {
                protocol,
                body: body.clone(),
            }),
            origin_protocol: Some(protocol),
            requested_model: None,
            native_template: Some(ValidatedSourceRequest {
                protocol,
                body: body.clone(),
            }),
            source_context: Some(ValidatedSourceRequest { protocol, body }),
            native_only,
            normalized_protocol: None,
            patch_reasoning: false,
            normalization_dirty: false,
            anthropic_beta: None,
        }
    }

    /// 只读访问 Canonical 语义；正文改写必须先消费信封并丢弃直通资格。
    #[must_use]
    pub const fn canonical(&self) -> &CanonicalRequest {
        &self.canonical
    }

    /// 附加唯一允许穿过协议入口的专用 Anthropic beta 列表。
    pub fn with_anthropic_beta(mut self, value: Option<&str>) -> Result<Self, AnthropicBetaError> {
        self.anthropic_beta = value
            .map(normalize_anthropic_beta)
            .transpose()?
            .filter(|value| !value.is_empty());
        Ok(self)
    }

    /// 返回已校验并去重的客户端 Anthropic beta 列表。
    #[must_use]
    pub fn anthropic_beta(&self) -> Option<&str> {
        self.anthropic_beta.as_deref()
    }

    /// 为对话协议转换操作类型和流选项，保留消息、工具关联及客户端模型别名。
    /// 无法跨协议表达的私有参数不得静默丢弃；无状态等默认选项可安全归一化。
    #[must_use]
    pub fn for_conversation_protocol(self, protocol: Protocol) -> Option<Self> {
        crate::conversation_plan::normalize(self, protocol).ok()
    }

    /// 生成与执行共用的候选正文，避免能力预选与实际构造不一致。
    pub fn plan_conversation(
        self,
        protocol: Protocol,
    ) -> Option<crate::ConversationConversionPlan> {
        crate::ConversationConversionPlan::prepare(self, protocol).ok()
    }

    pub(crate) const fn native_only(&self) -> bool {
        self.native_only
    }

    pub(crate) const fn normalized_protocol(&self) -> Option<Protocol> {
        self.normalized_protocol
    }

    pub(crate) const fn normalization_dirty(&self) -> bool {
        self.normalization_dirty
    }

    pub(crate) fn with_conversation_canonical(
        self,
        canonical: CanonicalRequest,
        target: Protocol,
    ) -> Self {
        let mut request = self.with_rebuilt_canonical(canonical);
        request.normalized_protocol = Some(target);
        request.normalization_dirty = false;
        request
    }

    pub(crate) fn source_context(&self) -> Option<(Protocol, &Bytes)> {
        self.source_context
            .as_ref()
            .map(|source| (source.protocol, &source.body))
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
        self.normalization_dirty = self.normalized_protocol.is_some();
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

    pub(crate) const fn origin_protocol(&self) -> Option<Protocol> {
        self.origin_protocol
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

    /// 冻结已校验的渠道模型与采样策略，并保留下游模型别名。
    /// 修改后永久丢弃源字节直通资格。
    #[must_use]
    pub fn with_routed_parameters(mut self, model: String, sampling: Sampling) -> Self {
        self.requested_model = Some(self.requested_model().to_owned());
        self.canonical.model = model;
        self.canonical.sampling = sampling;
        self.source = None;
        self.normalization_dirty = self.normalized_protocol.is_some();
        self
    }

    /// 消费信封并仅保留 Canonical 请求，同时永久丢弃原始正文。
    #[must_use]
    pub fn into_canonical(self) -> CanonicalRequest {
        self.canonical
    }

    /// 修改传输模式而保留来源上下文、扩展字段和目标归一状态。
    #[must_use]
    pub fn with_stream_mode(mut self, stream: bool) -> Self {
        if self.canonical.stream != stream {
            self.canonical.stream = stream;
            self.source = None;
            self.normalization_dirty = self.normalized_protocol.is_some();
        }
        self
    }

    /// 用重建后的 Canonical 请求替换正文，同时保留客户端模型别名并取消直通资格。
    ///
    /// 仅协议层的受控语义补丁可以调用该方法，调用方不得试图恢复已验证的原始字节。
    #[must_use]
    pub(crate) fn with_rebuilt_canonical(self, canonical: CanonicalRequest) -> Self {
        Self {
            canonical,
            source: None,
            origin_protocol: self.origin_protocol,
            requested_model: self.requested_model,
            native_template: None,
            source_context: self.source_context,
            native_only: self.native_only,
            normalized_protocol: self.normalized_protocol,
            patch_reasoning: self.patch_reasoning,
            normalization_dirty: self.normalized_protocol.is_some(),
            anthropic_beta: self.anthropic_beta,
        }
    }

    /// 仅当目标协议与已验证源协议相同时，交付不可变的原始正文。
    ///
    /// 模型映射、参数覆盖或内部字段注入后不得再原字节直通。调用方通常应改走
    /// [`Self::into_canonical`] 并由目标协议构造器重新编码；协议专属构造器只有在重新解析
    /// 已验证正文、受控覆盖自有字段且保持其余字段语义时，才可把原文作为重构底稿。
    pub fn into_same_protocol(mut self, target_protocol: Protocol) -> SameProtocolDecision {
        if self.source.is_none()
            && let Some(template) = self.native_template.as_ref()
            && template.protocol == target_protocol
            && let Ok(body) = crate::conversation_plan::patch_native_body(
                template.protocol,
                &template.body,
                &self.canonical,
                self.patch_reasoning,
            )
        {
            self.source = Some(ValidatedSourceRequest {
                protocol: template.protocol,
                body,
            });
        }
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
            origin_protocol,
            requested_model,
            anthropic_beta,
            ..
        } = self
        else {
            unreachable!("已检查的源协议正文必须存在");
        };
        SameProtocolDecision::Passthrough(SameProtocolRequest {
            protocol: source.protocol,
            canonical,
            body: source.body,
            origin_protocol,
            requested_model,
            anthropic_beta,
        })
    }
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
            .field("origin_protocol", &self.origin_protocol)
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .field("native_only", &self.native_only)
            .field("has_anthropic_beta", &self.anthropic_beta.is_some())
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
    origin_protocol: Option<Protocol>,
    canonical: CanonicalRequest,
    body: Bytes,
    requested_model: Option<String>,
    anthropic_beta: Option<String>,
}

impl SameProtocolRequest {
    /// 返回随原生正文保留的客户端 Anthropic beta 列表。
    #[must_use]
    pub fn anthropic_beta(&self) -> Option<&str> {
        self.anthropic_beta.as_deref()
    }

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
            .field("origin_protocol", &self.origin_protocol)
            .field("canonical", &self.canonical)
            .field("body_bytes", &self.body.len())
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .field("has_anthropic_beta", &self.anthropic_beta.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::{ContentBlock, openai_responses};
    use af_domain::Operation;

    #[test]
    fn anthropic_beta_metadata_survives_envelope_clones_and_canonical_rebuilds() {
        let request = crate::anthropic::parse_request_envelope(Bytes::from_static(
            br#"{"model":"claude-test","max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
        ))
        .unwrap()
        .with_anthropic_beta(Some("client-beta,compact-2026-09-04,client-beta"))
        .unwrap();
        assert_eq!(
            request.anthropic_beta(),
            Some("client-beta,compact-2026-09-04")
        );
        assert_eq!(request.clone().anthropic_beta(), request.anthropic_beta());
        let rebuilt = request
            .clone()
            .with_rebuilt_canonical(request.canonical().clone());
        assert_eq!(rebuilt.anthropic_beta(), request.anthropic_beta());
        assert_eq!(
            rebuilt
                .for_conversation_protocol(Protocol::Anthropic)
                .unwrap()
                .anthropic_beta(),
            request.anthropic_beta()
        );
        match request.clone().into_same_protocol(Protocol::Anthropic) {
            SameProtocolDecision::Passthrough(request) => {
                assert_eq!(
                    request.anthropic_beta(),
                    Some("client-beta,compact-2026-09-04")
                );
            }
            SameProtocolDecision::Rebuild(_) => panic!("valid native request should pass through"),
        }
        assert_eq!(
            request
                .clone()
                .with_anthropic_beta(Some("bad beta"))
                .unwrap_err(),
            AnthropicBetaError::InvalidValue
        );
    }

    #[test]
    fn repeated_target_normalization_and_planning_are_idempotent() {
        let request = crate::openai_chat::parse_request_envelope(Bytes::from_static(br#"{"model":"model","messages":[{"role":"user","content":"question"}],"max_completion_tokens":4096,"seed":17,"response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object","properties":{"answer":{"type":"string"}}},"strict":true}}}"#)).unwrap();
        for target in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
        ] {
            let direct = request.clone().plan_conversation(target).unwrap();
            let normalized = request.clone().for_conversation_protocol(target).unwrap();
            assert_eq!(
                normalized
                    .clone()
                    .for_conversation_protocol(target)
                    .unwrap(),
                normalized
            );
            assert_eq!(normalized.source_context(), request.source_context());
            let planned = normalized.plan_conversation(target).unwrap();
            assert_eq!(planned.body(), direct.body(), "{target:?}");
            let repeated = planned.request().clone().plan_conversation(target).unwrap();
            assert_eq!(repeated.body(), planned.body(), "{target:?}");
            assert_eq!(repeated.request(), planned.request());
        }
    }

    #[test]
    fn returning_to_origin_converts_current_operation_instead_of_taking_native_shortcut() {
        let source =
            responses_request(br#"{"model":"model","input":"hello","max_output_tokens":4096}"#);
        let converted = source
            .for_conversation_protocol(Protocol::OpenAiChat)
            .unwrap();
        let returned = converted
            .for_conversation_protocol(Protocol::OpenAiResponses)
            .unwrap();
        assert_eq!(returned.canonical().operation, Operation::Responses);
        assert!(
            returned
                .plan_conversation(Protocol::OpenAiResponses)
                .is_some()
        );
    }

    #[test]
    fn routed_sampling_invalidates_only_the_normalization_result() {
        let source = crate::openai_chat::parse_request_envelope(Bytes::from_static(br#"{"model":"model","messages":[{"role":"user","content":"hello"}],"max_completion_tokens":4096}"#)).unwrap();
        let normalized = source
            .for_conversation_protocol(Protocol::Anthropic)
            .unwrap();
        let routed = normalized.with_routed_parameters(
            "upstream".to_owned(),
            Sampling::new(
                Some(1.7),
                None,
                Some(TokenCount::new(128).unwrap()),
                Vec::new(),
            )
            .unwrap(),
        );
        let normalized = routed
            .for_conversation_protocol(Protocol::Anthropic)
            .unwrap();
        assert_eq!(normalized.canonical().sampling.temperature(), Some(1.0));
        assert_eq!(normalized.canonical().model, "upstream");
        assert_eq!(normalized.requested_model(), "model");
    }

    #[test]
    fn native_anthropic_routed_output_limit_also_constrains_thinking_budget() {
        let request = crate::anthropic::parse_request_envelope(Bytes::from_static(br#"{"model":"model","max_tokens":8192,"thinking":{"type":"enabled","budget_tokens":4096},"messages":[{"role":"user","content":"hello"},{"role":"assistant","content":[{"type":"thinking","thinking":"recorded plan","signature":"native-signature"},{"type":"text","text":"answer"}]},{"role":"user","content":"continue"}],"x_vendor":{"keep":true}}"#)).unwrap();
        let original_messages = request.canonical().messages.clone();
        for output in [1024, 2048] {
            let routed = request.clone().with_routed_parameters(
                "upstream".to_owned(),
                Sampling::new(
                    None,
                    None,
                    Some(TokenCount::new(output).unwrap()),
                    Vec::new(),
                )
                .unwrap(),
            );
            let plan = routed.plan_conversation(Protocol::Anthropic).unwrap();
            assert_eq!(plan.request().canonical().messages, original_messages);
            let value: serde_json::Value = serde_json::from_slice(plan.body()).unwrap();
            assert_eq!(value["model"], "upstream");
            assert_eq!(value["max_tokens"], output);
            assert_eq!(value["x_vendor"]["keep"], true);
            assert_eq!(
                value["messages"][1]["content"][0]["signature"],
                "native-signature"
            );
            if output > 1024 {
                assert_eq!(value["thinking"]["budget_tokens"], output - 1);
            } else {
                assert_eq!(value["thinking"]["type"], "adaptive");
            }
            let repeated = plan
                .request()
                .clone()
                .plan_conversation(Protocol::Anthropic)
                .unwrap();
            assert_eq!(repeated.body(), plan.body());
        }
    }

    #[test]
    fn oversized_foreign_stop_is_omitted_as_a_whole_and_source_is_retained() {
        let long_stop = "界".repeat(400);
        let body = Bytes::from(serde_json::to_vec(&serde_json::json!({"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{"maxOutputTokens":4096,"stopSequences":[long_stop,"valid stop"]}})).unwrap());
        let request = crate::gemini::parse_request_envelope("models/model", body.clone()).unwrap();
        for target in [Protocol::OpenAiChat, Protocol::Anthropic] {
            let plan = request.clone().plan_conversation(target).unwrap();
            assert_eq!(
                plan.request().canonical().sampling.stop_sequences(),
                &["valid stop"]
            );
            assert_eq!(plan.request().source_context().unwrap().1, &body);
            let value: serde_json::Value = serde_json::from_slice(plan.body()).unwrap();
            let key = if target == Protocol::Anthropic {
                "stop_sequences"
            } else {
                "stop"
            };
            assert_eq!(value[key], serde_json::json!(["valid stop"]));
        }
    }

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
    fn prompt_cache_affinity_does_not_block_cross_protocol_routes() {
        let request = responses_request(
            br#"{"model":"model-test","input":"hi","store":false,"prompt_cache_key":"agent-session","max_output_tokens":128}"#,
        );
        for target in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
            let converted = request.clone().for_conversation_protocol(target).unwrap();
            assert_eq!(
                converted.canonical().continuation.prompt_cache_key(),
                Some("agent-session")
            );
            match target {
                Protocol::OpenAiChat => {
                    assert!(crate::openai_chat::build_request(converted.canonical()).is_ok())
                }
                Protocol::Anthropic => {
                    assert!(crate::anthropic::build_request(converted.canonical()).is_ok())
                }
                Protocol::Gemini => {
                    assert!(crate::gemini::build_request(converted.canonical()).is_ok())
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn optional_encrypted_reasoning_output_does_not_block_cross_protocol_routes() {
        let request = responses_request(
            br#"{"model":"model-test","input":"hi","max_output_tokens":128,"store":false,"include":["reasoning.encrypted_content"],"reasoning":{"effort":"low","summary":"auto"}}"#,
        );
        for target in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
            let converted = request.clone().for_conversation_protocol(target);
            assert!(converted.is_some(), "target {target:?}");
            let converted = converted.unwrap();
            assert!(converted.canonical().raw_passthrough().is_none());
            let built = match target {
                Protocol::OpenAiChat => {
                    crate::openai_chat::build_request(converted.canonical()).is_ok()
                }
                Protocol::Anthropic => {
                    crate::anthropic::build_request(converted.canonical()).is_ok()
                }
                Protocol::Gemini => crate::gemini::build_request(converted.canonical()).is_ok(),
                _ => unreachable!(),
            };
            assert!(built, "target {target:?}");
        }
    }

    #[test]
    fn output_preferences_and_foreign_encrypted_history_do_not_block_readable_messages() {
        let request = responses_request(
            br#"{"model":"model-test","input":"hi","include":["file_search_call.results"]}"#,
        );
        assert!(
            request
                .for_conversation_protocol(Protocol::OpenAiChat)
                .is_some()
        );
        let request = responses_request(
            br#"{"model":"model-test","input":[{"type":"reasoning","id":"rs_test","encrypted_content":"opaque","summary":[]},{"role":"user","content":"hi"}],"include":["reasoning.encrypted_content"]}"#,
        );
        assert!(
            request
                .clone()
                .for_conversation_protocol(Protocol::OpenAiResponses)
                .is_some()
        );
        assert!(
            request
                .for_conversation_protocol(Protocol::OpenAiChat)
                .is_some()
        );
    }

    #[test]
    fn provider_signatures_stay_native_while_readable_history_can_cross_protocols() {
        let request = crate::gemini::parse_request_envelope(
            "models/gemini-test",
            Bytes::from_static(
                br#"{"tools":[{"functionDeclarations":[{"name":"lookup","parameters":{"type":"object"}}]}],"contents":[{"role":"user","parts":[{"text":"run a lookup"}]},{"role":"model","parts":[{"text":"I should inspect it","thought":true,"thoughtSignature":"c2ln"},{"functionCall":{"id":"call-1","name":"lookup","args":{}},"thoughtSignature":"dG9vbC1zaWc="}]},{"role":"user","parts":[{"functionResponse":{"id":"call-1","name":"lookup","response":{"ok":true}}}]}]}"#,
            ),
        )
        .unwrap();

        assert_signed_origin_survives_rebuilds(request, Protocol::Gemini);
    }

    fn assert_signed_origin_survives_rebuilds(request: CanonicalRequestEnvelope, origin: Protocol) {
        let with_default = request
            .clone()
            .with_default_max_output_tokens(TokenCount::new(8_192).unwrap());
        let routed = request.clone().with_routed_parameters(
            "routed-model".to_owned(),
            with_default.canonical().sampling.clone(),
        );
        let rebuilt = request
            .clone()
            .with_rebuilt_canonical(request.canonical().clone());
        for rewritten in [with_default, routed, rebuilt] {
            assert_eq!(rewritten.source_protocol(), None);
            for target in [
                Protocol::OpenAiChat,
                Protocol::OpenAiResponses,
                Protocol::Anthropic,
                Protocol::Gemini,
            ] {
                let converted = rewritten.clone().for_conversation_protocol(target);
                assert!(converted.is_some(), "target {target:?}");
                if let Some(converted) = converted {
                    if target == origin {
                        assert_eq!(converted.canonical().messages, request.canonical().messages);
                    } else {
                        for message in &converted.canonical().messages {
                            assert!(message.content.iter().all(|block| !matches!(
                                block,
                                ContentBlock::Thinking {
                                    signature: Some(_),
                                    ..
                                } | ContentBlock::ToolUse {
                                    signature: Some(_),
                                    ..
                                }
                            )));
                        }
                    }
                    assert!(
                        converted.plan_conversation(target).is_some(),
                        "target {target:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rewritten_encrypted_responses_history_keeps_its_origin() {
        let request = responses_request(
            br#"{"model":"gpt-test","input":[{"type":"reasoning","encrypted_content":"opaque","summary":[]},{"role":"user","content":"hi"}]}"#,
        );
        assert_signed_origin_survives_rebuilds(request, Protocol::OpenAiResponses);
    }

    #[test]
    fn rewritten_unsigned_history_still_allows_cross_protocol_routes() {
        let request = responses_request(br#"{"model":"model-test","input":"hi"}"#)
            .with_default_max_output_tokens(TokenCount::new(128).unwrap());
        let request = request.with_routed_parameters(
            "routed-model".to_owned(),
            Sampling::new(None, None, Some(TokenCount::new(128).unwrap()), Vec::new()).unwrap(),
        );
        for target in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
            assert!(request.clone().for_conversation_protocol(target).is_some());
        }
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
    fn storage_preferences_are_optional_but_missing_remote_history_is_explicit() {
        let request = responses_request(br#"{"model":"gpt-test","input":"hi","store":true}"#);
        assert!(
            request
                .for_conversation_protocol(Protocol::OpenAiChat)
                .is_some()
        );
        let request = responses_request(
            br#"{"model":"gpt-test","input":"hi","previous_response_id":"resp_previous"}"#,
        );
        assert!(
            request
                .for_conversation_protocol(Protocol::OpenAiChat)
                .is_none()
        );
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
