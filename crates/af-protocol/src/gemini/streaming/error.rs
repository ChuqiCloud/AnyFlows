use std::{error::Error, fmt};

use crate::UnsupportedCapability;
use crate::sse::SseParseError;

/// Gemini `streamGenerateContent` 解码错误；分类不保留上游正文或元数据。
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
    /// Gemini chunk 字段或取值无效。
    InvalidValue,
    /// chunk 使用当前 Canonical 无法无损表达的已知能力。
    UnsupportedFeature,
    /// 响应身份、候选、结束原因或 usage 快照顺序无效。
    InvalidSequence,
    /// 传输结束时候选仍未完整结束。
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
            Self::Sse(_) => "Gemini 流式 SSE 无效",
            Self::InvalidJson => "Gemini 流式事件不是有效 JSON",
            Self::DuplicateKey => "Gemini 流式事件包含重复字段",
            Self::StructureLimitExceeded => "Gemini 流式事件超过结构限制",
            Self::InvalidValue => "Gemini 流式事件字段值无效",
            Self::UnsupportedFeature => "Gemini 流式事件包含当前不支持的特性",
            Self::InvalidSequence => "Gemini 流式事件顺序无效",
            Self::UnexpectedEof => "Gemini 流在候选结束前截断",
            Self::DataAfterDone => "Gemini 流在逻辑终点后仍有数据",
            Self::DecoderFailed => "Gemini 流式 decoder 已失败",
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

/// Canonical 流事件编码为 Gemini SSE 时的固定错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeStreamError {
    /// 响应标识或模型版本不符合 Gemini 边界。
    InvalidMetadata,
    /// Canonical 事件顺序或索引关系无效。
    InvalidSequence,
    /// 单事件或累计编码状态超过预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的流式能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Gemini 无法无损表达该 Canonical 事件。
    UnsupportedEvent,
    /// usage 无法按 Gemini 口径安全表达。
    InvalidUsage,
    /// 固定 wire 数据序列化失败。
    Serialization,
    /// encoder 已因前一次错误进入终止状态。
    EncoderFailed,
}

impl fmt::Display for EncodeStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidMetadata => "Gemini 流式响应元数据无效",
            Self::InvalidSequence => "Canonical 流事件顺序无效",
            Self::StructureLimitExceeded => "Canonical 流事件超过编码限制",
            Self::UnsupportedCapability(_) => "Gemini 缺少所需流式能力",
            Self::UnsupportedEvent => "Gemini 无法表达该流事件",
            Self::InvalidUsage => "Gemini 无法表达该 usage",
            Self::Serialization => "Gemini 流式事件序列化失败",
            Self::EncoderFailed => "Gemini 流式 encoder 已失败",
        };
        match self {
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            _ => formatter.write_str(message),
        }
    }
}

impl Error for EncodeStreamError {}
