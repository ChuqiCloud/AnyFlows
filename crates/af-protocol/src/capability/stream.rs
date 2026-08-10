use std::fmt;

use af_domain::Protocol;

use super::{UnsupportedCapability, supports_stream_capability};
use crate::{CacheHint, CanonicalStreamEvent, ContentDelta, FinishReason, Usage};

/// Canonical 流式事件中的单项可表达能力。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StreamCapability {
    /// 候选消息开始事件。
    MessageStart,
    /// 多候选流。
    MultipleChoices,
    /// 非零候选索引。
    ArbitraryChoiceIndex,
    /// 文本增量。
    TextDelta,
    /// 图片增量。
    ImageDelta,
    /// 音频增量。
    AudioDelta,
    /// 推理增量。
    ReasoningDelta,
    /// 推理签名增量。
    ReasoningSignatures,
    /// 工具调用生命周期。
    ToolCalls,
    Compaction,
    /// 工具调用思考签名。
    ToolCallSignatures,
    /// 候选结束原因。
    FinishReason(FinishReason),
    /// 实际命中的停止序列。
    StopSequence,
    /// 提示整体拦截事件。
    PromptBlocked,
    /// 基础 usage 事件。
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
    /// 保活事件。
    Ping,
    /// 完整流终止事件。
    StreamEnd,
    /// 已脱敏上游错误事件。
    Error,
}

impl StreamCapability {
    /// 返回当前契约定义的全部流式能力。
    pub const ALL: &'static [Self] = &[
        Self::MessageStart,
        Self::MultipleChoices,
        Self::ArbitraryChoiceIndex,
        Self::TextDelta,
        Self::ImageDelta,
        Self::AudioDelta,
        Self::ReasoningDelta,
        Self::ReasoningSignatures,
        Self::ToolCalls,
        Self::Compaction,
        Self::ToolCallSignatures,
        Self::FinishReason(FinishReason::Stop),
        Self::FinishReason(FinishReason::Length),
        Self::FinishReason(FinishReason::ToolCalls),
        Self::FinishReason(FinishReason::ContentFilter),
        Self::StopSequence,
        Self::PromptBlocked,
        Self::Usage,
        Self::UsageCacheRead,
        Self::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
        Self::UsageCacheCreation(CacheHint::Ephemeral1Hour),
        Self::UsageReasoning,
        Self::UsageAudioInput,
        Self::UsageAudioOutput,
        Self::Ping,
        Self::StreamEnd,
        Self::Error,
    ];
}

impl fmt::Display for StreamCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageStart => formatter.write_str("message.start"),
            Self::MultipleChoices => formatter.write_str("choices.multiple"),
            Self::ArbitraryChoiceIndex => formatter.write_str("choices.arbitrary_index"),
            Self::TextDelta => formatter.write_str("content.text_delta"),
            Self::ImageDelta => formatter.write_str("content.image_delta"),
            Self::AudioDelta => formatter.write_str("content.audio_delta"),
            Self::ReasoningDelta => formatter.write_str("content.reasoning_delta"),
            Self::ReasoningSignatures => formatter.write_str("content.reasoning_signature"),
            Self::ToolCalls => formatter.write_str("tool_call.lifecycle"),
            Self::Compaction => formatter.write_str("responses.compaction"),
            Self::ToolCallSignatures => formatter.write_str("tool_call.signature"),
            Self::FinishReason(reason) => {
                write!(formatter, "finish.{}", finish_reason_name(*reason))
            }
            Self::StopSequence => formatter.write_str("finish.stop_sequence"),
            Self::PromptBlocked => formatter.write_str("prompt.blocked"),
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
            Self::Ping => formatter.write_str("ping"),
            Self::StreamEnd => formatter.write_str("stream.end"),
            Self::Error => formatter.write_str("error"),
        }
    }
}

const COMMON_FINISH: &[StreamCapability] = &[
    StreamCapability::FinishReason(FinishReason::Stop),
    StreamCapability::FinishReason(FinishReason::Length),
    StreamCapability::FinishReason(FinishReason::ToolCalls),
    StreamCapability::FinishReason(FinishReason::ContentFilter),
];

pub(super) const OPENAI_CHAT: &[StreamCapability] = &[
    StreamCapability::MessageStart,
    StreamCapability::MultipleChoices,
    StreamCapability::ArbitraryChoiceIndex,
    StreamCapability::TextDelta,
    StreamCapability::ReasoningDelta,
    StreamCapability::ToolCalls,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    StreamCapability::Usage,
    StreamCapability::UsageCacheRead,
    StreamCapability::UsageReasoning,
    StreamCapability::UsageAudioInput,
    StreamCapability::UsageAudioOutput,
    StreamCapability::Ping,
    StreamCapability::StreamEnd,
];

pub(super) const OPENAI_RESPONSES: &[StreamCapability] = &[
    StreamCapability::MessageStart,
    StreamCapability::TextDelta,
    StreamCapability::ReasoningDelta,
    StreamCapability::ReasoningSignatures,
    StreamCapability::ToolCalls,
    StreamCapability::Compaction,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    StreamCapability::Usage,
    StreamCapability::UsageCacheRead,
    StreamCapability::UsageReasoning,
    StreamCapability::Ping,
    StreamCapability::StreamEnd,
    StreamCapability::Error,
];

pub(super) const ANTHROPIC: &[StreamCapability] = &[
    StreamCapability::MessageStart,
    StreamCapability::TextDelta,
    StreamCapability::ReasoningDelta,
    StreamCapability::ReasoningSignatures,
    StreamCapability::ToolCalls,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    StreamCapability::StopSequence,
    StreamCapability::Usage,
    StreamCapability::UsageCacheRead,
    StreamCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
    StreamCapability::UsageCacheCreation(CacheHint::Ephemeral1Hour),
    StreamCapability::UsageReasoning,
    StreamCapability::Ping,
    StreamCapability::StreamEnd,
];

pub(super) const GEMINI: &[StreamCapability] = &[
    StreamCapability::MessageStart,
    StreamCapability::MultipleChoices,
    StreamCapability::ArbitraryChoiceIndex,
    StreamCapability::TextDelta,
    StreamCapability::ImageDelta,
    StreamCapability::AudioDelta,
    StreamCapability::ReasoningDelta,
    StreamCapability::ReasoningSignatures,
    StreamCapability::ToolCalls,
    StreamCapability::ToolCallSignatures,
    COMMON_FINISH[0],
    COMMON_FINISH[1],
    COMMON_FINISH[2],
    COMMON_FINISH[3],
    StreamCapability::PromptBlocked,
    StreamCapability::Usage,
    StreamCapability::UsageCacheRead,
    StreamCapability::UsageReasoning,
    StreamCapability::UsageAudioInput,
    StreamCapability::UsageAudioOutput,
    StreamCapability::Ping,
    StreamCapability::StreamEnd,
];

/// 校验目标协议能否表达一个 Canonical 流式事件。
///
/// 事件顺序、索引连续性、工具生命周期和累计预算仍由具体 encoder 校验。
pub fn validate_stream_event_capabilities(
    protocol: Protocol,
    event: &CanonicalStreamEvent,
) -> Result<(), UnsupportedCapability> {
    match event {
        CanonicalStreamEvent::MessageStart { choice_index, .. } => {
            require(protocol, StreamCapability::MessageStart)?;
            validate_choice_index(protocol, *choice_index)
        }
        CanonicalStreamEvent::ContentDelta {
            choice_index,
            delta,
            ..
        } => {
            validate_choice_index(protocol, *choice_index)?;
            match delta {
                ContentDelta::Text(_) => require(protocol, StreamCapability::TextDelta),
                ContentDelta::Image { .. } => require(protocol, StreamCapability::ImageDelta),
                ContentDelta::Audio { .. } => require(protocol, StreamCapability::AudioDelta),
            }
        }
        CanonicalStreamEvent::ReasoningDelta {
            choice_index,
            signature,
            ..
        } => {
            validate_choice_index(protocol, *choice_index)?;
            require(protocol, StreamCapability::ReasoningDelta)?;
            if signature.is_some() {
                require(protocol, StreamCapability::ReasoningSignatures)?;
            }
            Ok(())
        }
        CanonicalStreamEvent::ToolCallStart { choice_index, .. }
        | CanonicalStreamEvent::ToolCallArgsDelta { choice_index, .. }
        | CanonicalStreamEvent::ToolCallEnd { choice_index, .. } => {
            validate_choice_index(protocol, *choice_index)?;
            require(protocol, StreamCapability::ToolCalls)
        }
        CanonicalStreamEvent::CompactionStart { choice_index, .. }
        | CanonicalStreamEvent::CompactionEnd { choice_index, .. } => {
            validate_choice_index(protocol, *choice_index)?;
            require(protocol, StreamCapability::Compaction)
        }
        CanonicalStreamEvent::ToolCallSignature { choice_index, .. } => {
            validate_choice_index(protocol, *choice_index)?;
            require(protocol, StreamCapability::ToolCalls)?;
            require(protocol, StreamCapability::ToolCallSignatures)
        }
        CanonicalStreamEvent::Finish {
            choice_index,
            reason,
            stop_sequence,
        } => {
            validate_choice_index(protocol, *choice_index)?;
            require(protocol, StreamCapability::FinishReason(*reason))?;
            if stop_sequence.is_some() {
                require(protocol, StreamCapability::StopSequence)?;
            }
            Ok(())
        }
        CanonicalStreamEvent::PromptBlocked => require(protocol, StreamCapability::PromptBlocked),
        CanonicalStreamEvent::Usage(usage) => validate_usage(protocol, usage),
        CanonicalStreamEvent::Ping => require(protocol, StreamCapability::Ping),
        CanonicalStreamEvent::StreamEnd => require(protocol, StreamCapability::StreamEnd),
        CanonicalStreamEvent::Error(_) => require(protocol, StreamCapability::Error),
    }
}

fn validate_choice_index(
    protocol: Protocol,
    choice_index: u32,
) -> Result<(), UnsupportedCapability> {
    if choice_index != 0 {
        require(protocol, StreamCapability::ArbitraryChoiceIndex)?;
    }
    Ok(())
}

fn validate_usage(protocol: Protocol, usage: &Usage) -> Result<(), UnsupportedCapability> {
    require(protocol, StreamCapability::Usage)?;
    let details = usage.details();
    if details.cache_read().get() != 0 {
        require(protocol, StreamCapability::UsageCacheRead)?;
    }
    if details.cache_creation_5m().get() != 0 {
        require(
            protocol,
            StreamCapability::UsageCacheCreation(CacheHint::Ephemeral5Minutes),
        )?;
    }
    if details.cache_creation_1h().get() != 0 {
        require(
            protocol,
            StreamCapability::UsageCacheCreation(CacheHint::Ephemeral1Hour),
        )?;
    }
    if details.reasoning().get() != 0 {
        require(protocol, StreamCapability::UsageReasoning)?;
    }
    if details.audio_input().get() != 0 {
        require(protocol, StreamCapability::UsageAudioInput)?;
    }
    if details.audio_output().get() != 0 {
        require(protocol, StreamCapability::UsageAudioOutput)?;
    }
    Ok(())
}

fn require(protocol: Protocol, capability: StreamCapability) -> Result<(), UnsupportedCapability> {
    if supports_stream_capability(protocol, capability) {
        Ok(())
    } else {
        Err(UnsupportedCapability::stream(protocol, capability))
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
