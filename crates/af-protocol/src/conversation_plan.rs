use std::{error::Error, fmt};

use af_domain::{Operation, Protocol, Role};
use bytes::Bytes;
use serde_json::{Map, Value, json};

use crate::{
    CanonicalRequest, CanonicalRequestEnvelope, ContentBlock, Message, ReasoningConfig,
    ReasoningEffort, RequestMetadata, SameProtocolDecision, Sampling, StreamOptions, TokenCount,
    ToolChoice, anthropic, gemini, openai_chat, openai_responses,
};

/// 已完成目标归一和实际正文构造的候选计划；路由与执行使用相同入口。
#[derive(Clone)]
pub struct ConversationConversionPlan {
    request: CanonicalRequestEnvelope,
    body: Bytes,
}

impl ConversationConversionPlan {
    pub fn prepare(
        request: CanonicalRequestEnvelope,
        target: Protocol,
    ) -> Result<Self, ConversationConversionError> {
        let request = normalize(request, target)?;
        let body = match request.clone().into_same_protocol(target) {
            SameProtocolDecision::Passthrough(native) => native.body().clone(),
            SameProtocolDecision::Rebuild(_) => crate::conversation_features::apply(
                &request,
                target,
                build(request.canonical(), target)?,
            )?,
        };
        let body = transport_body(body, request.canonical(), target)?;
        Ok(Self { request, body })
    }

    #[must_use]
    pub const fn request(&self) -> &CanonicalRequestEnvelope {
        &self.request
    }

    #[must_use]
    pub const fn body(&self) -> &Bytes {
        &self.body
    }

    #[must_use]
    pub fn into_parts(self) -> (CanonicalRequestEnvelope, Bytes) {
        (self.request, self.body)
    }
}

impl fmt::Debug for ConversationConversionPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConversationConversionPlan")
            .field("request", &self.request)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// 转换错误仅描述类别，不包含请求内容或凭据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationConversionError {
    UnsupportedOperation,
    NativeContentRequired,
    ContinuationRequired,
    InvalidRequest,
}

impl fmt::Display for ConversationConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedOperation => "unsupported conversation operation",
            Self::NativeContentRequired => "native content requires its source protocol",
            Self::ContinuationRequired => "conversation state must be resolved before conversion",
            Self::InvalidRequest => "conversation request could not be encoded",
        })
    }
}
impl Error for ConversationConversionError {}

pub(crate) fn normalize(
    mut request: CanonicalRequestEnvelope,
    target: Protocol,
) -> Result<CanonicalRequestEnvelope, ConversationConversionError> {
    let operation = match target {
        Protocol::OpenAiResponses => Operation::Responses,
        Protocol::OpenAiChat | Protocol::Anthropic | Protocol::Gemini => Operation::Chat,
        _ => return Err(ConversationConversionError::UnsupportedOperation),
    };
    if !matches!(
        request.canonical().operation,
        Operation::Chat | Operation::Responses
    ) {
        return Err(ConversationConversionError::UnsupportedOperation);
    }
    if target == Protocol::Anthropic {
        let default = request
            .canonical()
            .reasoning
            .and_then(|reasoning| reasoning.budget_tokens())
            .map_or(4096, |budget| budget.get().saturating_add(1).max(4096))
            .min(anthropic::MAX_OUTPUT_TOKENS);
        request = request.with_default_max_output_tokens(
            TokenCount::new(default).map_err(|_| ConversationConversionError::InvalidRequest)?,
        );
    }
    if request.normalized_protocol() == Some(target) && !request.normalization_dirty() {
        return Ok(request);
    }
    if request.origin_protocol() == Some(target) && request.normalized_protocol().is_none() {
        if target == Protocol::Anthropic
            && request.source_protocol().is_none()
            && let Some(reasoning) = request.canonical().reasoning
        {
            let normalized = normalize_reasoning(
                reasoning,
                target,
                request.canonical().sampling.max_output_tokens(),
            )?;
            if normalized != reasoning {
                request = request.with_native_reasoning(normalized);
            }
        }
        // 原生扩展、签名、输出偏好由其原生渠道解释，不能因中间表示未建模而丢弃。
        return Ok(request);
    }
    if request.native_only() {
        return Err(ConversationConversionError::NativeContentRequired);
    }
    if request.canonical().continuation.has_server_managed_state() {
        return Err(ConversationConversionError::ContinuationRequired);
    }
    let mut canonical = request.canonical().clone();
    canonical.operation = operation;
    canonical = canonical.with_validated_raw_passthrough(None);
    if target != Protocol::OpenAiChat {
        canonical.stream_options = StreamOptions::EMPTY;
    }
    canonical.metadata = RequestMetadata::new(
        (target != Protocol::Gemini)
            .then(|| canonical.metadata.user_id().map(str::to_owned))
            .flatten(),
        None,
    );
    for tool in &mut canonical.tools {
        if matches!(target, Protocol::Anthropic | Protocol::Gemini) {
            tool.strict = None;
        } else if target == Protocol::OpenAiResponses {
            tool.strict.get_or_insert(false);
        }
        if tool.input_schema.is_null() {
            tool.input_schema = json!({"type":"object","properties":{}});
        }
    }
    if canonical.tools.is_empty()
        && matches!(
            canonical.tool_choice,
            ToolChoice::Required | ToolChoice::Named { .. }
        )
    {
        canonical.tool_choice = ToolChoice::Auto;
    }
    let mut system = Vec::new();
    for message in &mut canonical.messages {
        if matches!(target, Protocol::Anthropic | Protocol::Gemini)
            && matches!(message.role, Role::System | Role::Developer)
        {
            system.append(&mut message.content);
            message.role = Role::System;
        }
        normalize_blocks(&mut message.content, target)?;
    }
    if !system.is_empty() {
        normalize_blocks(&mut system, target)?;
        canonical
            .messages
            .retain(|message| message.role != Role::System);
        canonical
            .messages
            .insert(0, Message::new(Role::System, system));
    }
    let max_temperature = if target == Protocol::Anthropic {
        1.0
    } else {
        2.0
    };
    let stop_limit = match target {
        Protocol::OpenAiResponses => 0,
        Protocol::Gemini => 5,
        _ => 4,
    };
    let stop_bytes = if target == Protocol::Gemini {
        4096
    } else {
        1024
    };
    canonical.sampling = Sampling::new(
        canonical
            .sampling
            .temperature()
            .map(|temperature| temperature.min(max_temperature)),
        canonical.sampling.top_p(),
        canonical.sampling.max_output_tokens(),
        canonical
            .sampling
            .stop_sequences()
            .iter()
            .filter(|s| !s.is_empty() && s.len() <= stop_bytes)
            .take(stop_limit)
            .cloned()
            .collect(),
    )
    .map_err(|_| ConversationConversionError::InvalidRequest)?;
    canonical.reasoning = canonical
        .reasoning
        .map(|reasoning| {
            normalize_reasoning(reasoning, target, canonical.sampling.max_output_tokens())
        })
        .transpose()?;
    crate::conversation_content::normalize_content(&mut canonical, target)
        .map_err(|_| ConversationConversionError::InvalidRequest)?;
    crate::conversation_history::normalize_history(&mut canonical, target)
        .map_err(|_| ConversationConversionError::InvalidRequest)?;
    Ok(request.with_conversation_canonical(canonical, target))
}

fn normalize_blocks(
    blocks: &mut Vec<ContentBlock>,
    target: Protocol,
) -> Result<(), ConversationConversionError> {
    blocks.retain(|block| {
        target == Protocol::Anthropic || !matches!(block, ContentBlock::CacheControl(_))
    });
    for block in blocks.iter_mut() {
        match block {
            ContentBlock::Thinking { text, signature } => {
                *signature = None;
                if target == Protocol::Anthropic {
                    *block = ContentBlock::Text(std::mem::take(text));
                }
            }
            ContentBlock::ToolUse { signature, .. } => {
                *signature = None;
            }
            ContentBlock::ToolResult {
                content,
                structured_content,
                is_error,
                ..
            } => {
                normalize_blocks(content, target)?;
                if target != Protocol::Gemini
                    && let Some(value) = structured_content.take()
                {
                    let text = if !*is_error && value.as_object().is_some_and(|o| o.len() == 1) {
                        value
                            .get("result")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| value.to_string());
                    content.push(ContentBlock::Text(text));
                }
                if matches!(target, Protocol::OpenAiChat | Protocol::OpenAiResponses) && *is_error {
                    content.insert(0, ContentBlock::Text("[tool execution failed]".to_owned()));
                    *is_error = false;
                }
            }
            ContentBlock::Compaction(_) if target != Protocol::OpenAiResponses => {
                return Err(ConversationConversionError::ContinuationRequired);
            }
            ContentBlock::AnthropicCompaction(_) if target != Protocol::Anthropic => {
                return Err(ConversationConversionError::ContinuationRequired);
            }
            _ => {}
        }
    }
    blocks.retain(|block| !matches!(block, ContentBlock::Thinking { text, signature: None } if text.is_empty()));
    Ok(())
}

fn normalize_reasoning(
    reasoning: ReasoningConfig,
    target: Protocol,
    output: Option<TokenCount>,
) -> Result<ReasoningConfig, ConversationConversionError> {
    let mut effort = reasoning.effort();
    let mut budget = reasoning.budget_tokens();
    if target == Protocol::Anthropic {
        if effort == Some(ReasoningEffort::Minimal) {
            effort = Some(ReasoningEffort::Low);
        }
        if let Some(tokens) = budget {
            let maximum = output.map_or(4096, TokenCount::get);
            if maximum <= 1024 {
                effort = Some(
                    reasoning
                        .effort_for_effort_only_protocol()
                        .unwrap_or(ReasoningEffort::Low),
                );
                if effort == Some(ReasoningEffort::Minimal) {
                    effort = Some(ReasoningEffort::Low);
                }
                budget = None;
            } else {
                budget = TokenCount::new(tokens.get().max(1024).min(maximum - 1)).ok();
            }
        }
    }
    if target == Protocol::Gemini {
        if matches!(
            effort,
            Some(ReasoningEffort::ExtraHigh | ReasoningEffort::Max)
        ) {
            effort = Some(ReasoningEffort::High);
        }
        if budget.is_some() {
            effort = None;
        }
    }
    let mut normalized = ReasoningConfig::new(effort, budget, reasoning.include_thinking())
        .map_err(|_| ConversationConversionError::InvalidRequest)?;
    if reasoning.dynamic_budget() && effort.is_none() && budget.is_none() {
        normalized = normalized
            .with_dynamic_budget(true)
            .map_err(|_| ConversationConversionError::InvalidRequest)?;
    }
    if target == Protocol::OpenAiResponses {
        normalized = normalized.with_summary(reasoning.summary());
    }
    Ok(normalized)
}

fn build(
    request: &CanonicalRequest,
    target: Protocol,
) -> Result<Bytes, ConversationConversionError> {
    let value = match target {
        Protocol::OpenAiChat => openai_chat::build_request(request).map_err(|_| ()),
        Protocol::OpenAiResponses => openai_responses::build_request(request).map_err(|_| ()),
        Protocol::Anthropic => anthropic::build_request(request).map_err(|_| ()),
        Protocol::Gemini => {
            let mut body = request.clone();
            body.stream = false;
            body.stream_options = StreamOptions::EMPTY;
            gemini::build_request(&body).map_err(|_| ())
        }
        _ => return Err(ConversationConversionError::UnsupportedOperation),
    }
    .map_err(|()| ConversationConversionError::InvalidRequest)?;
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ConversationConversionError::InvalidRequest)
}

/// 网关传输策略在路由验证和实际转发中只执行同一份实现。
fn transport_body(
    body: Bytes,
    request: &CanonicalRequest,
    target: Protocol,
) -> Result<Bytes, ConversationConversionError> {
    if target == Protocol::OpenAiResponses && request.continuation.has_server_managed_state() {
        return Err(ConversationConversionError::ContinuationRequired);
    }
    if !(target == Protocol::OpenAiChat && request.stream || target == Protocol::OpenAiResponses) {
        return Ok(body);
    }
    let mut value: Value =
        serde_json::from_slice(&body).map_err(|_| ConversationConversionError::InvalidRequest)?;
    let root = value
        .as_object_mut()
        .ok_or(ConversationConversionError::InvalidRequest)?;
    if target == Protocol::OpenAiChat {
        let options = root
            .entry("stream_options")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or(ConversationConversionError::InvalidRequest)?;
        if options.get("include_usage") == Some(&Value::Bool(true)) {
            return Ok(body);
        }
        options.insert("include_usage".to_owned(), Value::Bool(true));
    } else {
        if root.get("store") == Some(&Value::Bool(false)) {
            return Ok(body);
        }
        // 请求完整回放由客户端携带；网关尚未持久绑定上游私有响应 ID。
        root.insert("store".to_owned(), Value::Bool(false));
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ConversationConversionError::InvalidRequest)
}

/// 仅覆盖网关拥有的模型与采样字段；原生扩展与消息原样保留。
pub(crate) fn patch_native_body(
    protocol: Protocol,
    body: &Bytes,
    request: &CanonicalRequest,
    patch_reasoning: bool,
) -> Result<Bytes, ConversationConversionError> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|_| ConversationConversionError::InvalidRequest)?;
    let root = value
        .as_object_mut()
        .ok_or(ConversationConversionError::InvalidRequest)?;
    if protocol != Protocol::Gemini {
        root.insert("model".to_owned(), json!(request.model));
        if request.stream || root.contains_key("stream") {
            root.insert("stream".to_owned(), Value::Bool(request.stream));
        }
    }
    let object = if protocol == Protocol::Gemini {
        root.entry("generationConfig")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or(ConversationConversionError::InvalidRequest)?
    } else {
        root
    };
    let (temperature, top_p, max_tokens, stop) = if protocol == Protocol::Gemini {
        ("temperature", "topP", "maxOutputTokens", "stopSequences")
    } else if protocol == Protocol::OpenAiResponses {
        ("temperature", "top_p", "max_output_tokens", "stop")
    } else if protocol == Protocol::OpenAiChat {
        object.remove("max_tokens");
        ("temperature", "top_p", "max_completion_tokens", "stop")
    } else {
        ("temperature", "top_p", "max_tokens", "stop_sequences")
    };
    insert_optional(
        object,
        temperature,
        request.sampling.temperature().map(|v| json!(v)),
    );
    insert_optional(object, top_p, request.sampling.top_p().map(|v| json!(v)));
    insert_optional(
        object,
        max_tokens,
        request.sampling.max_output_tokens().map(|v| json!(v.get())),
    );
    if protocol != Protocol::OpenAiResponses {
        insert_optional(
            object,
            stop,
            (!request.sampling.stop_sequences().is_empty())
                .then(|| json!(request.sampling.stop_sequences())),
        );
    }
    if patch_reasoning {
        patch_native_reasoning(&mut value, request, protocol)?;
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ConversationConversionError::InvalidRequest)
}

fn patch_native_reasoning(
    value: &mut Value,
    request: &CanonicalRequest,
    protocol: Protocol,
) -> Result<(), ConversationConversionError> {
    // 只借用构造器编码推理字段，不能让其余原生扩展或历史形状阻止受控覆盖。
    let operation = if protocol == Protocol::OpenAiResponses {
        Operation::Responses
    } else {
        Operation::Chat
    };
    let mut reasoning_request = CanonicalRequest::new(
        operation,
        request.model.clone(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("reasoning configuration".to_owned())],
        )],
        false,
    );
    reasoning_request.reasoning = request.reasoning;
    reasoning_request.tool_choice = ToolChoice::None;
    reasoning_request.sampling =
        Sampling::new(None, None, request.sampling.max_output_tokens(), Vec::new())
            .map_err(|_| ConversationConversionError::InvalidRequest)?;
    let built = build(&reasoning_request, protocol)?;
    let built: Value =
        serde_json::from_slice(&built).map_err(|_| ConversationConversionError::InvalidRequest)?;
    let root = value
        .as_object_mut()
        .ok_or(ConversationConversionError::InvalidRequest)?;
    if protocol == Protocol::Gemini {
        let config = root
            .entry("generationConfig")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or(ConversationConversionError::InvalidRequest)?;
        merge_owned_fields(
            config,
            "thinkingConfig",
            built
                .get("generationConfig")
                .and_then(|config| config.get("thinkingConfig")),
            &["thinkingLevel", "thinkingBudget", "includeThoughts"],
        )?;
    } else if protocol == Protocol::Anthropic {
        merge_owned_fields(
            root,
            "thinking",
            built.get("thinking"),
            &["type", "budget_tokens", "display"],
        )?;
        if let Some(effort) = built
            .get("output_config")
            .and_then(|config| config.get("effort"))
        {
            root.entry("output_config")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or(ConversationConversionError::InvalidRequest)?
                .insert("effort".to_owned(), effort.clone());
        }
    } else if protocol == Protocol::OpenAiResponses {
        let config = root
            .entry("reasoning")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or(ConversationConversionError::InvalidRequest)?;
        if let Some(built) = built.get("reasoning").and_then(Value::as_object) {
            config.extend(built.clone());
        }
    } else if protocol == Protocol::OpenAiChat {
        insert_optional(
            root,
            "reasoning_effort",
            built.get("reasoning_effort").cloned(),
        );
    }
    Ok(())
}

fn merge_owned_fields(
    root: &mut Map<String, Value>,
    key: &str,
    built: Option<&Value>,
    fields: &[&str],
) -> Result<(), ConversationConversionError> {
    if let Some(built) = built.and_then(Value::as_object) {
        let object = root
            .entry(key.to_owned())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or(ConversationConversionError::InvalidRequest)?;
        for field in fields {
            insert_optional(object, field, built.get(*field).cloned());
        }
    } else {
        root.remove(key);
    }
    Ok(())
}

fn insert_optional(object: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        object.insert(key.to_owned(), value);
    } else {
        object.remove(key);
    }
}
