use std::{error::Error, fmt};

use crate::UnsupportedCapability;
use crate::sse::SseParseError;

/// OpenAI Responses SSE 解码错误；分类不保留上游正文或字段值。
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
    /// Responses 事件字段或取值无效。
    InvalidValue,
    /// 事件使用当前 Canonical 无法无损表达的能力。
    UnsupportedFeature,
    /// 生命周期、索引、快照或序号顺序无效。
    InvalidSequence,
    /// 传输结束前没有收到 Responses 终态事件。
    UnexpectedEof,
    /// 逻辑终点后仍收到数据。
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
            Self::Sse(_) => "OpenAI Responses 流式 SSE 无效",
            Self::InvalidJson => "OpenAI Responses 流式事件不是有效 JSON",
            Self::DuplicateKey => "OpenAI Responses 流式事件包含重复字段",
            Self::StructureLimitExceeded => "OpenAI Responses 流式事件超过结构限制",
            Self::InvalidValue => "OpenAI Responses 流式事件字段值无效",
            Self::UnsupportedFeature => "OpenAI Responses 流式事件包含当前不支持的特性",
            Self::InvalidSequence => "OpenAI Responses 流式事件顺序无效",
            Self::UnexpectedEof => "OpenAI Responses 流在终态事件前截断",
            Self::DataAfterDone => "OpenAI Responses 流在逻辑终点后仍有数据",
            Self::DecoderFailed => "OpenAI Responses 流式 decoder 已失败",
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

/// Canonical 流事件编码为 OpenAI Responses SSE 时的固定错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeStreamError {
    /// 响应标识、模型名或创建时间不符合 Responses 边界。
    InvalidMetadata,
    /// Canonical 事件顺序、索引或生命周期无效。
    InvalidSequence,
    /// 单事件或累计编码状态超过预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的流式能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Responses 无法无损表达该 Canonical 事件。
    UnsupportedEvent,
    /// usage 无法按 Responses 的 Inclusive 口径安全表达。
    InvalidUsage,
    /// 固定 wire 数据序列化失败。
    Serialization,
    /// encoder 已因前一次错误进入终止状态。
    EncoderFailed,
}

impl fmt::Display for EncodeStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidMetadata => "OpenAI Responses 流式响应元数据无效",
            Self::InvalidSequence => "Canonical 流事件顺序无效",
            Self::StructureLimitExceeded => "Canonical 流事件超过编码限制",
            Self::UnsupportedCapability(_) => "OpenAI Responses 缺少所需流式能力",
            Self::UnsupportedEvent => "OpenAI Responses 无法表达该流事件",
            Self::InvalidUsage => "OpenAI Responses 无法表达该 usage",
            Self::Serialization => "OpenAI Responses 流式事件序列化失败",
            Self::EncoderFailed => "OpenAI Responses 流式 encoder 已失败",
        };
        match self {
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            _ => formatter.write_str(message),
        }
    }
}

impl Error for EncodeStreamError {}
