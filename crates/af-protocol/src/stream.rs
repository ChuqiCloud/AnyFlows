use std::fmt;

use af_domain::{Role, UpstreamError};

use crate::{ResponsesCompactionItem, Usage};

/// 流式内容的增量片段。
#[derive(Clone, Eq, PartialEq)]
pub enum ContentDelta {
    /// 文本增量。
    Text(String),
    /// 图片增量；数据通常为编码后的二进制内容。
    Image {
        /// 图片数据。
        data: String,
        /// 图片媒体类型。
        mime_type: String,
    },
    /// 音频增量；数据通常为编码后的二进制内容。
    Audio {
        /// 音频数据。
        data: String,
        /// 音频媒体类型。
        mime_type: String,
    },
}

impl fmt::Debug for ContentDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(_) => formatter.debug_tuple("Text").field(&Redacted).finish(),
            Self::Image {
                data: _,
                mime_type: _,
            } => formatter
                .debug_struct("Image")
                .field("data", &Redacted)
                .field("mime_type", &Redacted)
                .finish(),
            Self::Audio {
                data: _,
                mime_type: _,
            } => formatter
                .debug_struct("Audio")
                .field("data", &Redacted)
                .field("mime_type", &Redacted)
                .finish(),
        }
    }
}

/// 模型结束当前候选结果的原因。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FinishReason {
    /// 模型自然停止或命中停止词。
    Stop,
    /// 输出达到长度限制。
    Length,
    /// 模型请求调用工具。
    ToolCalls,
    /// 输出被内容安全策略拦截。
    ContentFilter,
}

impl FinishReason {
    /// 返回当前契约定义的全部结束原因。
    pub const ALL: &'static [Self] = &[
        Self::Stop,
        Self::Length,
        Self::ToolCalls,
        Self::ContentFilter,
    ];
}

/// 跨协议转换使用的闭合流事件。
///
/// 新增变体会强制所有无通配分支的协议编码器同步处理，避免静默丢弃事件。
#[derive(Clone, Eq, PartialEq)]
pub enum CanonicalStreamEvent {
    /// 候选消息开始。
    MessageStart {
        /// 候选结果索引。
        choice_index: u32,
        /// 消息角色。
        role: Role,
    },
    /// 候选消息中的内容块增量。
    ContentDelta {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的内容块索引。
        content_index: u32,
        /// 内容增量。
        delta: ContentDelta,
    },
    /// 候选消息中的推理内容增量。
    ReasoningDelta {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的内容块索引。
        content_index: u32,
        /// 推理文本增量。
        text: String,
        /// 上游提供的推理签名。
        signature: Option<String>,
    },
    /// 工具调用开始。
    ToolCallStart {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的工具调用索引。
        tool_index: u32,
        /// 工具调用标识。
        id: String,
        /// 工具名称。
        name: String,
    },
    /// 工具调用携带的上游思考签名。
    ToolCallSignature {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的工具调用索引。
        tool_index: u32,
        /// 上游返回的不透明签名。
        signature: String,
    },
    /// 工具调用参数的 JSON 增量。
    ToolCallArgsDelta {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的工具调用索引。
        tool_index: u32,
        /// 尚未完成的 JSON 文本。
        partial_json: String,
    },
    /// 工具调用结束。
    ToolCallEnd {
        /// 候选结果索引。
        choice_index: u32,
        /// 候选结果内的工具调用索引。
        tool_index: u32,
    },
    /// Responses 压缩 Item 已开始返回。
    CompactionStart {
        /// 候选结果索引。
        choice_index: u32,
        /// Responses 输出序号。
        output_index: u32,
        /// 不透明压缩 Item。
        item: ResponsesCompactionItem,
    },
    /// Responses 压缩 Item 已完成返回。
    CompactionEnd {
        /// 候选结果索引。
        choice_index: u32,
        /// Responses 输出序号。
        output_index: u32,
        /// 不透明压缩 Item。
        item: ResponsesCompactionItem,
    },
    /// 候选结果结束。
    Finish {
        /// 候选结果索引。
        choice_index: u32,
        /// 结束原因。
        reason: FinishReason,
        /// 上游实际命中的停止序列；仅停止原因支持时存在。
        stop_sequence: Option<String>,
    },
    /// 提示在生成任何候选结果前被内容策略整体拦截。
    PromptBlocked,
    /// 上游返回的用量数据。
    Usage(Usage),
    /// 上游保活事件。
    Ping,
    /// 流已完整结束。
    StreamEnd,
    /// 已归一化且不含原始上游内容的错误。
    Error(UpstreamError),
}

impl fmt::Debug for CanonicalStreamEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageStart { choice_index, role } => formatter
                .debug_struct("MessageStart")
                .field("choice_index", choice_index)
                .field("role", role)
                .finish(),
            Self::ContentDelta {
                choice_index,
                content_index,
                delta,
            } => formatter
                .debug_struct("ContentDelta")
                .field("choice_index", choice_index)
                .field("content_index", content_index)
                .field("delta", delta)
                .finish(),
            Self::ReasoningDelta {
                choice_index,
                content_index,
                text: _,
                signature: _,
            } => formatter
                .debug_struct("ReasoningDelta")
                .field("choice_index", choice_index)
                .field("content_index", content_index)
                .field("text", &Redacted)
                .field("signature", &Redacted)
                .finish(),
            Self::ToolCallStart {
                choice_index,
                tool_index,
                id: _,
                name: _,
            } => formatter
                .debug_struct("ToolCallStart")
                .field("choice_index", choice_index)
                .field("tool_index", tool_index)
                .field("id", &Redacted)
                .field("name", &Redacted)
                .finish(),
            Self::ToolCallSignature {
                choice_index,
                tool_index,
                signature: _,
            } => formatter
                .debug_struct("ToolCallSignature")
                .field("choice_index", choice_index)
                .field("tool_index", tool_index)
                .field("signature", &Redacted)
                .finish(),
            Self::ToolCallArgsDelta {
                choice_index,
                tool_index,
                partial_json: _,
            } => formatter
                .debug_struct("ToolCallArgsDelta")
                .field("choice_index", choice_index)
                .field("tool_index", tool_index)
                .field("partial_json", &Redacted)
                .finish(),
            Self::ToolCallEnd {
                choice_index,
                tool_index,
            } => formatter
                .debug_struct("ToolCallEnd")
                .field("choice_index", choice_index)
                .field("tool_index", tool_index)
                .finish(),
            Self::CompactionStart {
                choice_index,
                output_index,
                ..
            } => formatter
                .debug_struct("CompactionStart")
                .field("choice_index", choice_index)
                .field("output_index", output_index)
                .finish(),
            Self::CompactionEnd {
                choice_index,
                output_index,
                ..
            } => formatter
                .debug_struct("CompactionEnd")
                .field("choice_index", choice_index)
                .field("output_index", output_index)
                .finish(),
            Self::Finish {
                choice_index,
                reason,
                stop_sequence,
            } => formatter
                .debug_struct("Finish")
                .field("choice_index", choice_index)
                .field("reason", reason)
                .field("stop_sequence", &stop_sequence.as_ref().map(|_| "<已脱敏>"))
                .finish(),
            Self::PromptBlocked => formatter.write_str("PromptBlocked"),
            Self::Usage(usage) => formatter.debug_tuple("Usage").field(usage).finish(),
            Self::Ping => formatter.write_str("Ping"),
            Self::StreamEnd => formatter.write_str("StreamEnd"),
            Self::Error(error) => formatter.debug_tuple("Error").field(error).finish(),
        }
    }
}

struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}
