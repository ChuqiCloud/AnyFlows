use std::fmt;

use af_domain::{Operation, Protocol};

use super::{UnsupportedCapability, supports_response_capability};
use crate::{
    CacheHint, CanonicalResponse, ContentBlock, FinishReason, MediaSource, RawPassthrough, Usage,
};

/// Canonical 非流式响应中的单项可表达能力。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResponseCapability {
    /// 响应操作类型。
    Operation(Operation),
    /// 多候选响应。
    MultipleChoices,
    /// 非零候选索引。
    ArbitraryChoiceIndex,
    /// 提示在生成前整体被拦截。
    PromptBlocked,
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
    /// 思考内容。
    Thinking,
    /// 思考签名。
    ThinkingSignatures,
    /// 工具调用。
    ToolCalls,
    /// 工具调用思考签名。
    ToolCallSignatures,
    /// 工具结果内容块。
    ToolResults,
    /// 提示缓存断点。
    CacheControl(CacheHint),
    Compaction,
    /// 候选结束原因。
    FinishReason(FinishReason),
    /// 实际命中的停止序列。
    StopSequence,
    /// 基础 usage。
    Usage,
    /// 缓存读取 usage。
    UsageCacheRead,
    /// 指定 TTL 的缓存写入 usage。
    UsageCacheCreation(CacheHint),
    /// 推理 usage。
    UsageReasoning,
    /// 音频输入 usage。
    UsageAudioInput,
    /// 音频输出 usage。
    UsageAudioOutput,
    /// 已校验的同协议私有字段。
    SameProtocolRaw,
}

impl ResponseCapability {
    /// 返回当前契约定义的全部非流式响应能力。
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
        Self::MultipleChoices,
        Self::ArbitraryChoiceIndex,
        Self::PromptBlocked,
        Self::Text,
        Self::ImageUrl,
        Self::ImageBase64,
        Self::AudioUrl,
        Self::AudioBase64,
        Self::Thinking,
        Self::ThinkingSignatures,
        Self::ToolCalls,
        Self::ToolCallSignatures,
        Self::ToolResults,
        Self::CacheControl(CacheHint::Ephemeral5Minutes),
        Self::CacheControl(CacheHint::Ephemeral1Hour),
        Self::Compaction,
        Self::FinishReason(FinishReason::Stop),
        Self::FinishReason(FinishReason::Length),
        Self::FinishReason(FinishReason::ToolCalls),
        Self::FinishReason(FinishReason::ContentFilter),
        Self::StopSequence,
        Self::Usage,
        Self::UsageCacheRead,
        Self::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
        Self::UsageCacheCreation(CacheHint::Ephemeral1Hour),
        Self::UsageReasoning,
        Self::UsageAudioInput,
        Self::UsageAudioOutput,
        Self::SameProtocolRaw,
    ];
}

impl fmt::Display for ResponseCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(operation) => write!(formatter, "operation.{operation}"),
            Self::MultipleChoices => formatter.write_str("choices.multiple"),
            Self::ArbitraryChoiceIndex => formatter.write_str("choices.arbitrary_index"),
            Self::PromptBlocked => formatter.write_str("prompt.blocked"),
            Self::Text => formatter.write_str("content.text"),
            Self::ImageUrl => formatter.write_str("content.image.url"),
            Self::ImageBase64 => formatter.write_str("content.image.base64"),
            Self::AudioUrl => formatter.write_str("content.audio.url"),
            Self::AudioBase64 => formatter.write_str("content.audio.base64"),
            Self::Thinking => formatter.write_str("content.thinking"),
            Self::ThinkingSignatures => formatter.write_str("content.thinking.signature"),
            Self::ToolCalls => formatter.write_str("content.tool_call"),
            Self::ToolCallSignatures => formatter.write_str("content.tool_call.signature"),
            Self::ToolResults => formatter.write_str("content.tool_result"),
            Self::CacheControl(CacheHint::Ephemeral5Minutes) => {
                formatter.write_str("content.cache_control.5m")
            }
            Self::CacheControl(CacheHint::Ephemeral1Hour) => {
                formatter.write_str("content.cache_control.1h")
            }
            Self::Compaction => formatter.write_str("responses.compaction"),
            Self::FinishReason(reason) => {
                write!(formatter, "finish.{}", finish_reason_name(*reason))
            }
            Self::StopSequence => formatter.write_str("finish.stop_sequence"),
            Self::Usage => formatter.write_str("usage"),
            Self::UsageCacheRead => formatter.write_str("usage.cache_read"),
            Self::UsageCacheCreation(CacheHint::Ephemeral5Minutes) => {
                formatter.write_str("usage.cache_creation.5m")
            }
            Self::UsageCacheCreation(CacheHint::Ephemeral1Hour) => {
                formatter.write_str("usage.cache_creation.1h")
            }
            Self::UsageReasoning => formatter.write_str("usage.reasoning"),
            Self::UsageAudioInput => formatter.write_str("usage.audio_input"),
            Self::UsageAudioOutput => formatter.write_str("usage.audio_output"),
            Self::SameProtocolRaw => formatter.write_str("raw.same_protocol"),
        }
    }
}

const COMMON_FINISH: &[ResponseCapability] = &[
    ResponseCapability::FinishReason(FinishReason::Stop),
    ResponseCapability::FinishReason(FinishReason::Length),
    ResponseCapability::FinishReason(FinishReason::ToolCalls),
    ResponseCapability::FinishReason(FinishReason::ContentFilter),
];

pub(super) const OPENAI_CHAT: &[ResponseCapability] = &[
    ResponseCapability::Operation(Operation::Chat),
    ResponseCapability::MultipleChoices,
    ResponseCapability::ArbitraryChoiceIndex,
    ResponseCapability::Text,
    ResponseCapability::ToolCalls,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    ResponseCapability::Usage,
    ResponseCapability::UsageCacheRead,
    ResponseCapability::UsageReasoning,
    ResponseCapability::UsageAudioInput,
    ResponseCapability::UsageAudioOutput,
    ResponseCapability::SameProtocolRaw,
];

pub(super) const OPENAI_RESPONSES: &[ResponseCapability] = &[
    ResponseCapability::Operation(Operation::Responses),
    ResponseCapability::Text,
    ResponseCapability::Thinking,
    ResponseCapability::ThinkingSignatures,
    ResponseCapability::Compaction,
    ResponseCapability::ToolCalls,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    ResponseCapability::Usage,
    ResponseCapability::UsageCacheRead,
    ResponseCapability::UsageReasoning,
    ResponseCapability::SameProtocolRaw,
];

pub(super) const ANTHROPIC: &[ResponseCapability] = &[
    ResponseCapability::Operation(Operation::Chat),
    ResponseCapability::Text,
    ResponseCapability::Thinking,
    ResponseCapability::ThinkingSignatures,
    ResponseCapability::ToolCalls,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    ResponseCapability::StopSequence,
    ResponseCapability::Usage,
    ResponseCapability::UsageCacheRead,
    ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
    ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral1Hour),
    ResponseCapability::UsageReasoning,
    ResponseCapability::SameProtocolRaw,
];

pub(super) const GEMINI: &[ResponseCapability] = &[
    ResponseCapability::Operation(Operation::Chat),
    ResponseCapability::MultipleChoices,
    ResponseCapability::ArbitraryChoiceIndex,
    ResponseCapability::PromptBlocked,
    ResponseCapability::Text,
    ResponseCapability::ImageBase64,
    ResponseCapability::AudioBase64,
    ResponseCapability::Thinking,
    ResponseCapability::ThinkingSignatures,
    ResponseCapability::ToolCalls,
    ResponseCapability::ToolCallSignatures,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    ResponseCapability::Usage,
    ResponseCapability::UsageCacheRead,
    ResponseCapability::UsageReasoning,
    ResponseCapability::UsageAudioInput,
    ResponseCapability::UsageAudioOutput,
    ResponseCapability::SameProtocolRaw,
];

/// 校验目标协议能否逐项表达 Canonical 非流式响应。
///
/// 候选顺序、工具关联、必填元数据和大小预算仍由具体构造器校验。
pub fn validate_response_capabilities(
    protocol: Protocol,
    response: &CanonicalResponse,
) -> Result<(), UnsupportedCapability> {
    require(protocol, ResponseCapability::Operation(response.operation))?;
    if response.choices.is_empty() {
        require(protocol, ResponseCapability::PromptBlocked)?;
    }
    if response.choices.len() > 1 {
        require(protocol, ResponseCapability::MultipleChoices)?;
    }
    for choice in &response.choices {
        if choice.index != 0 {
            require(protocol, ResponseCapability::ArbitraryChoiceIndex)?;
        }
        validate_content(protocol, &choice.message.content)?;
        require(
            protocol,
            ResponseCapability::FinishReason(choice.finish_reason),
        )?;
        if choice.stop_sequence.is_some() {
            require(protocol, ResponseCapability::StopSequence)?;
        }
    }
    if let Some(usage) = response.usage.as_ref() {
        validate_usage(protocol, usage)?;
    }
    validate_same_protocol_raw(protocol, response.raw_passthrough())
}

fn validate_content(
    protocol: Protocol,
    blocks: &[ContentBlock],
) -> Result<(), UnsupportedCapability> {
    for block in blocks {
        match block {
            ContentBlock::Text(_) => require(protocol, ResponseCapability::Text)?,
            ContentBlock::Image { source, .. } => match source {
                MediaSource::Url(_) => require(protocol, ResponseCapability::ImageUrl)?,
                MediaSource::Base64(_) => require(protocol, ResponseCapability::ImageBase64)?,
            },
            ContentBlock::Audio { source, .. } => match source {
                MediaSource::Url(_) => require(protocol, ResponseCapability::AudioUrl)?,
                MediaSource::Base64(_) => require(protocol, ResponseCapability::AudioBase64)?,
            },
            ContentBlock::ToolUse { signature, .. } => {
                require(protocol, ResponseCapability::ToolCalls)?;
                if signature.is_some() {
                    require(protocol, ResponseCapability::ToolCallSignatures)?;
                }
            }
            ContentBlock::ToolResult { content, .. } => {
                require(protocol, ResponseCapability::ToolResults)?;
                validate_content(protocol, content)?;
            }
            ContentBlock::Thinking { signature, .. } => {
                require(protocol, ResponseCapability::Thinking)?;
                if signature.is_some() {
                    require(protocol, ResponseCapability::ThinkingSignatures)?;
                }
            }
            ContentBlock::CacheControl(hint) => {
                require(protocol, ResponseCapability::CacheControl(*hint))?;
            }
            ContentBlock::Compaction(_) => require(protocol, ResponseCapability::Compaction)?,
        }
    }
    Ok(())
}

pub(super) fn validate_usage(
    protocol: Protocol,
    usage: &Usage,
) -> Result<(), UnsupportedCapability> {
    require(protocol, ResponseCapability::Usage)?;
    let details = usage.details();
    if details.cache_read().get() != 0 {
        require(protocol, ResponseCapability::UsageCacheRead)?;
    }
    if details.cache_creation_5m().get() != 0 {
        require(
            protocol,
            ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
        )?;
    }
    if details.cache_creation_1h().get() != 0 {
        require(
            protocol,
            ResponseCapability::UsageCacheCreation(CacheHint::Ephemeral1Hour),
        )?;
    }
    if details.reasoning().get() != 0 {
        require(protocol, ResponseCapability::UsageReasoning)?;
    }
    if details.audio_input().get() != 0 {
        require(protocol, ResponseCapability::UsageAudioInput)?;
    }
    if details.audio_output().get() != 0 {
        require(protocol, ResponseCapability::UsageAudioOutput)?;
    }
    Ok(())
}

fn validate_same_protocol_raw(
    protocol: Protocol,
    raw: Option<&RawPassthrough>,
) -> Result<(), UnsupportedCapability> {
    if raw.is_some_and(|raw| raw.source_protocol() == protocol && !raw.is_empty()) {
        require(protocol, ResponseCapability::SameProtocolRaw)?;
    }
    Ok(())
}

fn require(
    protocol: Protocol,
    capability: ResponseCapability,
) -> Result<(), UnsupportedCapability> {
    if supports_response_capability(protocol, capability) {
        Ok(())
    } else {
        Err(UnsupportedCapability::response(protocol, capability))
    }
}

const fn finish_reason_name(reason: FinishReason) -> &'static str {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::ContentFilter => "content_filter",
    }
}
