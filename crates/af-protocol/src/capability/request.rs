use std::fmt;

use af_domain::{Operation, Protocol, Role};

use super::{UnsupportedCapability, supports_request_capability};
use crate::{CacheHint, CanonicalRequest, ContentBlock, MediaSource, ReasoningEffort};

/// Canonical 请求中的单项可表达能力。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequestCapability {
    /// 请求操作类型。
    Operation(Operation),
    /// 消息角色。
    MessageRole(Role),
    /// 文本内容块。
    Text,
    /// HTTPS 图片地址。
    ImageUrl,
    /// Base64 图片。
    ImageBase64,
    /// 远程音频地址。
    AudioUrl,
    /// Base64 音频。
    AudioBase64,
    /// 函数工具声明。
    ToolDefinitions,
    /// 严格函数工具声明。
    StrictToolDefinitions,
    /// 工具选择策略。
    ToolChoice,
    /// 助手工具调用。
    ToolCalls,
    /// 工具调用思考签名。
    ToolCallSignatures,
    /// 工具结果。
    ToolResults,
    /// 结构化工具结果。
    StructuredToolResults,
    /// 失败工具结果。
    ToolResultErrors,
    /// 助手思考内容。
    Thinking,
    /// 助手思考签名。
    ThinkingSignatures,
    /// 提示缓存断点。
    CacheControl(CacheHint),
    Compaction,
    /// 指定推理强度。
    ReasoningEffort(ReasoningEffort),
    /// 指定推理令牌预算。
    ReasoningBudget,
    /// 请求返回思考内容。
    ReasoningOutput,
    /// temperature 采样参数。
    Temperature,
    /// top_p 采样参数。
    TopP,
    /// 最大输出令牌参数。
    MaxOutputTokens,
    /// 自定义停止序列。
    StopSequences,
    /// 请求流式响应。
    Streaming,
    /// 请求流末 usage。
    StreamUsage,
    /// 上游用户标识。
    UserMetadata,
    /// 上游会话标识。
    SessionMetadata,
    /// 上一响应续接标识。
    PreviousResponseContinuation,
    /// 持久会话续接标识。
    ConversationContinuation,
    /// 提示缓存亲和键。
    PromptCacheContinuation,
    /// 独立附件。
    Attachments,
    /// 已校验的同协议私有字段。
    SameProtocolRaw,
}

impl RequestCapability {
    /// 返回当前契约定义的全部请求能力。
    pub const ALL: &'static [Self] = &[
        Self::Operation(Operation::Chat),
        Self::Operation(Operation::Responses),
        Self::Operation(Operation::ResponsesCompact),
        Self::Operation(Operation::Embedding),
        Self::Operation(Operation::Image),
        Self::Operation(Operation::Audio),
        Self::Operation(Operation::Rerank),
        Self::Operation(Operation::Video),
        Self::Operation(Operation::CountTokens),
        Self::MessageRole(Role::System),
        Self::MessageRole(Role::Developer),
        Self::MessageRole(Role::User),
        Self::MessageRole(Role::Assistant),
        Self::MessageRole(Role::Tool),
        Self::Text,
        Self::ImageUrl,
        Self::ImageBase64,
        Self::AudioUrl,
        Self::AudioBase64,
        Self::ToolDefinitions,
        Self::StrictToolDefinitions,
        Self::ToolChoice,
        Self::ToolCalls,
        Self::ToolCallSignatures,
        Self::ToolResults,
        Self::StructuredToolResults,
        Self::ToolResultErrors,
        Self::Thinking,
        Self::ThinkingSignatures,
        Self::CacheControl(CacheHint::Ephemeral5Minutes),
        Self::CacheControl(CacheHint::Ephemeral1Hour),
        Self::Compaction,
        Self::ReasoningEffort(ReasoningEffort::None),
        Self::ReasoningEffort(ReasoningEffort::Minimal),
        Self::ReasoningEffort(ReasoningEffort::Low),
        Self::ReasoningEffort(ReasoningEffort::Medium),
        Self::ReasoningEffort(ReasoningEffort::High),
        Self::ReasoningEffort(ReasoningEffort::ExtraHigh),
        Self::ReasoningEffort(ReasoningEffort::Max),
        Self::ReasoningBudget,
        Self::ReasoningOutput,
        Self::Temperature,
        Self::TopP,
        Self::MaxOutputTokens,
        Self::StopSequences,
        Self::Streaming,
        Self::StreamUsage,
        Self::UserMetadata,
        Self::SessionMetadata,
        Self::PreviousResponseContinuation,
        Self::ConversationContinuation,
        Self::PromptCacheContinuation,
        Self::Attachments,
        Self::SameProtocolRaw,
    ];
}

impl fmt::Display for RequestCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(operation) => write!(formatter, "operation.{operation}"),
            Self::MessageRole(role) => write!(formatter, "message.role.{role}"),
            Self::Text => formatter.write_str("content.text"),
            Self::ImageUrl => formatter.write_str("content.image.url"),
            Self::ImageBase64 => formatter.write_str("content.image.base64"),
            Self::AudioUrl => formatter.write_str("content.audio.url"),
            Self::AudioBase64 => formatter.write_str("content.audio.base64"),
            Self::ToolDefinitions => formatter.write_str("tools.definition"),
            Self::StrictToolDefinitions => formatter.write_str("tools.strict"),
            Self::ToolChoice => formatter.write_str("tools.choice"),
            Self::ToolCalls => formatter.write_str("content.tool_call"),
            Self::ToolCallSignatures => formatter.write_str("content.tool_call.signature"),
            Self::ToolResults => formatter.write_str("content.tool_result"),
            Self::StructuredToolResults => formatter.write_str("content.tool_result.structured"),
            Self::ToolResultErrors => formatter.write_str("content.tool_result.error"),
            Self::Thinking => formatter.write_str("content.thinking"),
            Self::ThinkingSignatures => formatter.write_str("content.thinking.signature"),
            Self::CacheControl(CacheHint::Ephemeral5Minutes) => {
                formatter.write_str("content.cache_control.5m")
            }
            Self::CacheControl(CacheHint::Ephemeral1Hour) => {
                formatter.write_str("content.cache_control.1h")
            }
            Self::Compaction => formatter.write_str("responses.compaction"),
            Self::ReasoningEffort(effort) => {
                write!(
                    formatter,
                    "reasoning.effort.{}",
                    reasoning_effort_name(*effort)
                )
            }
            Self::ReasoningBudget => formatter.write_str("reasoning.budget"),
            Self::ReasoningOutput => formatter.write_str("reasoning.output"),
            Self::Temperature => formatter.write_str("sampling.temperature"),
            Self::TopP => formatter.write_str("sampling.top_p"),
            Self::MaxOutputTokens => formatter.write_str("sampling.max_output_tokens"),
            Self::StopSequences => formatter.write_str("sampling.stop_sequences"),
            Self::Streaming => formatter.write_str("stream.enabled"),
            Self::StreamUsage => formatter.write_str("stream.include_usage"),
            Self::UserMetadata => formatter.write_str("metadata.user"),
            Self::SessionMetadata => formatter.write_str("metadata.session"),
            Self::PreviousResponseContinuation => {
                formatter.write_str("continuation.previous_response")
            }
            Self::ConversationContinuation => formatter.write_str("continuation.conversation"),
            Self::PromptCacheContinuation => formatter.write_str("continuation.prompt_cache"),
            Self::Attachments => formatter.write_str("attachments"),
            Self::SameProtocolRaw => formatter.write_str("raw.same_protocol"),
        }
    }
}

pub(super) const OPENAI_CHAT: &[RequestCapability] = &[
    RequestCapability::Operation(Operation::Chat),
    RequestCapability::MessageRole(Role::System),
    RequestCapability::MessageRole(Role::Developer),
    RequestCapability::MessageRole(Role::User),
    RequestCapability::MessageRole(Role::Assistant),
    RequestCapability::MessageRole(Role::Tool),
    RequestCapability::Text,
    RequestCapability::ImageUrl,
    RequestCapability::ImageBase64,
    RequestCapability::AudioBase64,
    RequestCapability::ToolDefinitions,
    RequestCapability::StrictToolDefinitions,
    RequestCapability::ToolChoice,
    RequestCapability::ToolCalls,
    RequestCapability::ToolResults,
    RequestCapability::ReasoningEffort(ReasoningEffort::None),
    RequestCapability::ReasoningEffort(ReasoningEffort::Minimal),
    RequestCapability::ReasoningEffort(ReasoningEffort::Low),
    RequestCapability::ReasoningEffort(ReasoningEffort::Medium),
    RequestCapability::ReasoningEffort(ReasoningEffort::High),
    RequestCapability::ReasoningEffort(ReasoningEffort::ExtraHigh),
    RequestCapability::ReasoningEffort(ReasoningEffort::Max),
    RequestCapability::Temperature,
    RequestCapability::TopP,
    RequestCapability::MaxOutputTokens,
    RequestCapability::StopSequences,
    RequestCapability::Streaming,
    RequestCapability::StreamUsage,
    RequestCapability::UserMetadata,
    RequestCapability::SameProtocolRaw,
];

pub(super) const OPENAI_RESPONSES: &[RequestCapability] = &[
    RequestCapability::Operation(Operation::Responses),
    RequestCapability::MessageRole(Role::System),
    RequestCapability::MessageRole(Role::Developer),
    RequestCapability::MessageRole(Role::User),
    RequestCapability::MessageRole(Role::Assistant),
    RequestCapability::MessageRole(Role::Tool),
    RequestCapability::Text,
    RequestCapability::ImageUrl,
    RequestCapability::ImageBase64,
    RequestCapability::ToolDefinitions,
    RequestCapability::StrictToolDefinitions,
    RequestCapability::ToolChoice,
    RequestCapability::ToolCalls,
    RequestCapability::ToolResults,
    RequestCapability::StructuredToolResults,
    RequestCapability::Thinking,
    RequestCapability::ThinkingSignatures,
    RequestCapability::ReasoningEffort(ReasoningEffort::None),
    RequestCapability::ReasoningEffort(ReasoningEffort::Minimal),
    RequestCapability::ReasoningEffort(ReasoningEffort::Low),
    RequestCapability::ReasoningEffort(ReasoningEffort::Medium),
    RequestCapability::ReasoningEffort(ReasoningEffort::High),
    RequestCapability::ReasoningEffort(ReasoningEffort::ExtraHigh),
    RequestCapability::ReasoningEffort(ReasoningEffort::Max),
    RequestCapability::Temperature,
    RequestCapability::TopP,
    RequestCapability::MaxOutputTokens,
    RequestCapability::Streaming,
    RequestCapability::UserMetadata,
    RequestCapability::PreviousResponseContinuation,
    RequestCapability::ConversationContinuation,
    RequestCapability::PromptCacheContinuation,
    RequestCapability::Compaction,
    RequestCapability::SameProtocolRaw,
];

pub(super) const ANTHROPIC: &[RequestCapability] = &[
    RequestCapability::Operation(Operation::Chat),
    RequestCapability::MessageRole(Role::System),
    RequestCapability::MessageRole(Role::User),
    RequestCapability::MessageRole(Role::Assistant),
    RequestCapability::MessageRole(Role::Tool),
    RequestCapability::Text,
    RequestCapability::ImageUrl,
    RequestCapability::ImageBase64,
    RequestCapability::ToolDefinitions,
    RequestCapability::ToolChoice,
    RequestCapability::ToolCalls,
    RequestCapability::ToolResults,
    RequestCapability::ToolResultErrors,
    RequestCapability::CacheControl(CacheHint::Ephemeral5Minutes),
    RequestCapability::CacheControl(CacheHint::Ephemeral1Hour),
    RequestCapability::ReasoningEffort(ReasoningEffort::None),
    RequestCapability::ReasoningEffort(ReasoningEffort::Low),
    RequestCapability::ReasoningEffort(ReasoningEffort::Medium),
    RequestCapability::ReasoningEffort(ReasoningEffort::High),
    RequestCapability::ReasoningEffort(ReasoningEffort::ExtraHigh),
    RequestCapability::ReasoningEffort(ReasoningEffort::Max),
    RequestCapability::ReasoningBudget,
    RequestCapability::ReasoningOutput,
    RequestCapability::Temperature,
    RequestCapability::TopP,
    RequestCapability::MaxOutputTokens,
    RequestCapability::StopSequences,
    RequestCapability::Streaming,
    RequestCapability::UserMetadata,
];

pub(super) const GEMINI: &[RequestCapability] = &[
    RequestCapability::Operation(Operation::Chat),
    RequestCapability::MessageRole(Role::System),
    RequestCapability::MessageRole(Role::User),
    RequestCapability::MessageRole(Role::Assistant),
    RequestCapability::MessageRole(Role::Tool),
    RequestCapability::Text,
    RequestCapability::ImageBase64,
    RequestCapability::AudioBase64,
    RequestCapability::ToolDefinitions,
    RequestCapability::ToolChoice,
    RequestCapability::ToolCalls,
    RequestCapability::ToolCallSignatures,
    RequestCapability::ToolResults,
    RequestCapability::StructuredToolResults,
    RequestCapability::ToolResultErrors,
    RequestCapability::Thinking,
    RequestCapability::ThinkingSignatures,
    RequestCapability::ReasoningEffort(ReasoningEffort::None),
    RequestCapability::ReasoningEffort(ReasoningEffort::Minimal),
    RequestCapability::ReasoningEffort(ReasoningEffort::Low),
    RequestCapability::ReasoningEffort(ReasoningEffort::Medium),
    RequestCapability::ReasoningEffort(ReasoningEffort::High),
    RequestCapability::ReasoningBudget,
    RequestCapability::ReasoningOutput,
    RequestCapability::Temperature,
    RequestCapability::TopP,
    RequestCapability::MaxOutputTokens,
    RequestCapability::StopSequences,
];

/// 校验目标协议能否逐项表达 Canonical 请求。
///
/// 消息顺序、工具关联、字段取值和大小预算仍由具体构造器校验。
pub fn validate_request_capabilities(
    protocol: Protocol,
    request: &CanonicalRequest,
) -> Result<(), UnsupportedCapability> {
    require(protocol, RequestCapability::Operation(request.operation))?;
    if !request.tools.is_empty() {
        require(protocol, RequestCapability::ToolDefinitions)?;
        require(protocol, RequestCapability::ToolChoice)?;
    }
    if request.tools.iter().any(|tool| tool.strict == Some(true)) {
        require(protocol, RequestCapability::StrictToolDefinitions)?;
    }
    if let Some(reasoning) = request.reasoning {
        if let Some(effort) = reasoning.effort() {
            require(protocol, RequestCapability::ReasoningEffort(effort))?;
        }
        if reasoning.budget_tokens().is_some() {
            require(protocol, RequestCapability::ReasoningBudget)?;
        }
        if reasoning.include_thinking() {
            require(protocol, RequestCapability::ReasoningOutput)?;
        }
    }
    if request.sampling.temperature().is_some() {
        require(protocol, RequestCapability::Temperature)?;
    }
    if request.sampling.top_p().is_some() {
        require(protocol, RequestCapability::TopP)?;
    }
    if request.sampling.max_output_tokens().is_some() {
        require(protocol, RequestCapability::MaxOutputTokens)?;
    }
    if !request.sampling.stop_sequences().is_empty() {
        require(protocol, RequestCapability::StopSequences)?;
    }
    if request.stream {
        require(protocol, RequestCapability::Streaming)?;
    }
    if request.stream_options.include_usage() {
        require(protocol, RequestCapability::StreamUsage)?;
    }
    if !request.attachments.is_empty() {
        require(protocol, RequestCapability::Attachments)?;
    }
    if request.metadata.user_id().is_some() {
        require(protocol, RequestCapability::UserMetadata)?;
    }
    if request.metadata.session_id().is_some() {
        require(protocol, RequestCapability::SessionMetadata)?;
    }
    if request.continuation.previous_response_id().is_some() {
        require(protocol, RequestCapability::PreviousResponseContinuation)?;
    }
    if request.continuation.conversation_id().is_some() {
        require(protocol, RequestCapability::ConversationContinuation)?;
    }
    if request.continuation.prompt_cache_key().is_some() {
        require(protocol, RequestCapability::PromptCacheContinuation)?;
    }
    for message in &request.messages {
        require(protocol, RequestCapability::MessageRole(message.role))?;
        validate_content(protocol, &message.content)?;
    }
    Ok(())
}

fn validate_content(
    protocol: Protocol,
    blocks: &[ContentBlock],
) -> Result<(), UnsupportedCapability> {
    for block in blocks {
        match block {
            ContentBlock::Text(_) => require(protocol, RequestCapability::Text)?,
            ContentBlock::Image { source, .. } => match source {
                MediaSource::Url(_) => require(protocol, RequestCapability::ImageUrl)?,
                MediaSource::Base64(_) => require(protocol, RequestCapability::ImageBase64)?,
            },
            ContentBlock::Audio { source, .. } => match source {
                MediaSource::Url(_) => require(protocol, RequestCapability::AudioUrl)?,
                MediaSource::Base64(_) => require(protocol, RequestCapability::AudioBase64)?,
            },
            ContentBlock::ToolUse { signature, .. } => {
                require(protocol, RequestCapability::ToolCalls)?;
                if signature.is_some() {
                    require(protocol, RequestCapability::ToolCallSignatures)?;
                }
            }
            ContentBlock::ToolResult {
                content,
                structured_content,
                is_error,
                ..
            } => {
                require(protocol, RequestCapability::ToolResults)?;
                if structured_content.is_some() {
                    require(protocol, RequestCapability::StructuredToolResults)?;
                }
                if *is_error {
                    require(protocol, RequestCapability::ToolResultErrors)?;
                }
                validate_content(protocol, content)?;
            }
            ContentBlock::Thinking { signature, .. } => {
                require(protocol, RequestCapability::Thinking)?;
                if signature.is_some() {
                    require(protocol, RequestCapability::ThinkingSignatures)?;
                }
            }
            ContentBlock::CacheControl(hint) => {
                require(protocol, RequestCapability::CacheControl(*hint))?;
            }
            ContentBlock::Compaction(_) => require(protocol, RequestCapability::Compaction)?,
        }
    }
    Ok(())
}

fn require(protocol: Protocol, capability: RequestCapability) -> Result<(), UnsupportedCapability> {
    if supports_request_capability(protocol, capability) {
        Ok(())
    } else {
        Err(UnsupportedCapability::request(protocol, capability))
    }
}

const fn reasoning_effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::ExtraHigh => "xhigh",
        ReasoningEffort::Max => "max",
    }
}
