//! OpenAI Responses 独立上下文压缩协议。
//!
//! 本模块只负责 `POST /v1/responses/compact` 的协议边界。上下文 Item 以经过字段、
//! 预算和类型校验的 JSON 保留，避免把供应商私有 Item 错误归一成通用消息；后续
//! 调度和计费只能消费本模块的 Canonical 类型。

mod codec;
mod error;
mod model;
mod validation;

pub use codec::{build_request, build_response, parse_request, parse_response};
pub use error::{
    BuildResponsesCompactionRequestError, BuildResponsesCompactionResponseError,
    CanonicalResponsesCompactionRequestError, ParseResponsesCompactionRequestError,
    ParseResponsesCompactionResponseError, ResponsesCompactionUsageError,
};
pub use model::{
    CanonicalResponsesCompactionRequest, CanonicalResponsesCompactionResponse,
    ResponsesCompactionInput, ResponsesCompactionItem, ResponsesCompactionUsage,
};

/// 压缩请求和响应正文的最大字节数。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// 单个上下文窗口允许的最大 Item 数量。
pub const MAX_CONTEXT_ITEMS: usize = 1_024;
/// 压缩 Item 的加密摘要最大字节数。
pub const MAX_ENCRYPTED_CONTENT_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests;
