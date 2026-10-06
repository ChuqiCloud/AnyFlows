use std::{error::Error, fmt};

use crate::UnsupportedCapability;
use crate::sse::SseParseError;

/// Anthropic Messages 流式解码错误；分类不保留上游正文或元数据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseStreamError {
    /// SSE framing 层拒绝输入。
    Sse(SseParseError),
    /// data 不是单个合法 JSON 值。
    InvalidJson,
    /// data 的任意 JSON 对象包含重复键。
    DuplicateKey,
    /// 单事件或累计流状态超过预算。
    StructureLimitExceeded,
    /// Anthropic 事件字段或取值无效。
    InvalidValue,
    /// 事件使用当前 Canonical 无法无损表达的已知能力。
    UnsupportedFeature,
    /// 消息、内容块、停止原因或用量快照的顺序无效。
    InvalidSequence,
    /// 传输结束前没有收到 `message_stop` 或终止错误。
    UnexpectedEof,
    /// 逻辑终点后仍收到事件。
    DataAfterDone,
    /// decoder 已因前一次错误进入终止状态。
    DecoderFailed,
}

impl From<SseParseError> for ParseStreamError {
    fn from(error: SseParseError) -> Self {
        Self::Sse(error)
    }
}

impl fmt::Display for ParseStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Sse(_) => "Anthropic 流式 SSE 无效",
            Self::InvalidJson => "Anthropic 流式事件不是有效 JSON",
            Self::DuplicateKey => "Anthropic 流式事件包含重复字段",
            Self::StructureLimitExceeded => "Anthropic 流式事件超过结构限制",
            Self::InvalidValue => "Anthropic 流式事件字段值无效",
            Self::UnsupportedFeature => "Anthropic 流式事件包含当前不支持的特性",
            Self::InvalidSequence => "Anthropic 流式事件顺序无效",
            Self::UnexpectedEof => "Anthropic 流在逻辑终点前截断",
            Self::DataAfterDone => "Anthropic 流在逻辑终点后仍有数据",
            Self::DecoderFailed => "Anthropic 流式 decoder 已失败",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseStreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sse(error) => Some(error),
            _ => None,
        }
    }
}

/// Canonical 流事件编码为 Anthropic Messages SSE 时的固定错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeStreamError {
    /// 响应标识、模型名或初始用量不符合 Anthropic 边界。
    InvalidMetadata,
    /// Canonical 事件顺序或索引关系无效。
    InvalidSequence,
    /// 单事件或累计编码状态超过预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的流式能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Anthropic Messages 无法无损表达该 Canonical 事件。
    UnsupportedEvent,
    /// 用量快照无法按 Anthropic 累计口径安全表达。
    InvalidUsage,
    /// 固定 wire DTO 序列化失败。
    Serialization,
    /// encoder 已因前一次错误进入终止状态。
    EncoderFailed,
}

impl fmt::Display for EncodeStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidMetadata => "Anthropic 流式响应元数据无效",
            Self::InvalidSequence => "Canonical 流事件顺序无效",
            Self::StructureLimitExceeded => "Canonical 流事件超过编码限制",
            Self::UnsupportedCapability(_) => "Anthropic 缺少所需流式能力",
            Self::UnsupportedEvent => "Anthropic Messages 无法表达该流事件",
            Self::InvalidUsage => "Anthropic Messages 无法表达该 usage 快照",
            Self::Serialization => "Anthropic 流式事件序列化失败",
            Self::EncoderFailed => "Anthropic 流式 encoder 已失败",
        };
        match self {
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            _ => formatter.write_str(message),
        }
    }
}

impl Error for EncodeStreamError {}
