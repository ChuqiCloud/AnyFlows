use std::{error::Error, fmt};

/// Canonical 压缩请求构造错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalResponsesCompactionRequestError {
    /// 字段或上下文无效。
    InvalidValue,
    /// 请求没有输入窗口或上一响应引用。
    MissingContext,
}

/// Canonical 压缩 usage 校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponsesCompactionUsageError {
    /// 令牌数为负数。
    NegativeTokenCount,
    /// 输入缓存明细超过输入总量。
    InputDetailsExceedTotal,
    /// 推理明细超过输出总量。
    OutputDetailsExceedTotal,
    /// 总量与输入加输出不一致。
    TotalMismatch,
    /// 令牌 checked 运算溢出。
    Overflow,
}

/// 压缩请求解析错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseResponsesCompactionRequestError {
    /// 正文过大。
    BodyTooLarge,
    /// JSON 无效。
    InvalidJson,
    /// 存在重复 JSON 键。
    DuplicateKey,
    /// 结构超过预算。
    StructureLimitExceeded,
    /// 字段值无效。
    InvalidValue,
    /// 使用了当前切片未建模的 Item 或参数。
    UnsupportedFeature,
}

/// 压缩请求构造错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponsesCompactionRequestError {
    /// Canonical 无法重新通过协议校验。
    InvalidValue,
}

/// 压缩响应解析错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseResponsesCompactionResponseError {
    /// 正文过大。
    BodyTooLarge,
    /// JSON 无效。
    InvalidJson,
    /// 存在重复 JSON 键。
    DuplicateKey,
    /// 结构超过预算。
    StructureLimitExceeded,
    /// 字段值无效。
    InvalidValue,
    /// 用量字段缺失或不一致。
    InvalidUsage,
    /// 使用了当前切片未建模的 Item 或参数。
    UnsupportedFeature,
}

/// 压缩响应构造错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResponsesCompactionResponseError {
    /// Canonical 无法重新通过协议校验。
    InvalidValue,
}

macro_rules! display_error {
    ($type:ty, { $($variant:path => $message:literal),+ $(,)? }) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                let message = match self { $($variant => $message),+ };
                formatter.write_str(message)
            }
        }
        impl Error for $type {}
    };
}

display_error!(CanonicalResponsesCompactionRequestError, {
    CanonicalResponsesCompactionRequestError::InvalidValue => "Responses 压缩请求字段无效",
    CanonicalResponsesCompactionRequestError::MissingContext => "Responses 压缩请求缺少上下文",
});
display_error!(ResponsesCompactionUsageError, {
    ResponsesCompactionUsageError::NegativeTokenCount => "Responses 压缩 usage 令牌数不能为负数",
    ResponsesCompactionUsageError::InputDetailsExceedTotal => "Responses 压缩 usage 输入明细超过总量",
    ResponsesCompactionUsageError::OutputDetailsExceedTotal => "Responses 压缩 usage 输出明细超过总量",
    ResponsesCompactionUsageError::TotalMismatch => "Responses 压缩 usage 总量不一致",
    ResponsesCompactionUsageError::Overflow => "Responses 压缩 usage 计算溢出",
});
display_error!(ParseResponsesCompactionRequestError, {
    ParseResponsesCompactionRequestError::BodyTooLarge => "Responses 压缩请求体超过大小限制",
    ParseResponsesCompactionRequestError::InvalidJson => "Responses 压缩请求不是有效 JSON",
    ParseResponsesCompactionRequestError::DuplicateKey => "Responses 压缩请求包含重复字段",
    ParseResponsesCompactionRequestError::StructureLimitExceeded => "Responses 压缩请求结构超过限制",
    ParseResponsesCompactionRequestError::InvalidValue => "Responses 压缩请求字段无效",
    ParseResponsesCompactionRequestError::UnsupportedFeature => "Responses 压缩请求包含未支持特性",
});
display_error!(BuildResponsesCompactionRequestError, {
    BuildResponsesCompactionRequestError::InvalidValue => "无法构造有效的 Responses 压缩请求",
});
display_error!(ParseResponsesCompactionResponseError, {
    ParseResponsesCompactionResponseError::BodyTooLarge => "Responses 压缩响应体超过大小限制",
    ParseResponsesCompactionResponseError::InvalidJson => "Responses 压缩响应不是有效 JSON",
    ParseResponsesCompactionResponseError::DuplicateKey => "Responses 压缩响应包含重复字段",
    ParseResponsesCompactionResponseError::StructureLimitExceeded => "Responses 压缩响应结构超过限制",
    ParseResponsesCompactionResponseError::InvalidValue => "Responses 压缩响应字段无效",
    ParseResponsesCompactionResponseError::InvalidUsage => "Responses 压缩响应 usage 无效",
    ParseResponsesCompactionResponseError::UnsupportedFeature => "Responses 压缩响应包含未支持特性",
});
display_error!(BuildResponsesCompactionResponseError, {
    BuildResponsesCompactionResponseError::InvalidValue => "无法构造有效的 Responses 压缩响应",
});
