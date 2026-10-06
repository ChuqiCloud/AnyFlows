use af_domain::PublicErrorCode;
use serde::Serialize;
use serde_json::Value;

/// 将公开错误码和已审定文案包装为 OpenAI 兼容错误结构。
///
/// `message` 必须来自 HTTP 边界维护的公开文案，不能传入内部诊断或外部输入。
#[must_use]
pub fn encode_error(code: PublicErrorCode, message: &'static str) -> Value {
    serde_json::to_value(OpenAiErrorEnvelope {
        error: OpenAiErrorBody {
            message,
            kind: openai_error_type(code),
            param: None,
            code,
        },
    })
    .expect("OpenAI 错误结构只包含字符串、空值和稳定枚举，序列化必须成功")
}

#[derive(Serialize)]
struct OpenAiErrorEnvelope {
    error: OpenAiErrorBody,
}

#[derive(Serialize)]
struct OpenAiErrorBody {
    message: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    // 第一阶段不保存参数路径；显式 null 比省略字段更兼容 OpenAI 客户端。
    param: Option<&'static str>,
    code: PublicErrorCode,
}

fn openai_error_type(code: PublicErrorCode) -> &'static str {
    match code {
        PublicErrorCode::InvalidRequest
        | PublicErrorCode::InvalidApiKey
        | PublicErrorCode::ModelNotFound
        | PublicErrorCode::TaskNotFound
        | PublicErrorCode::IdempotencyConflict => "invalid_request_error",
        PublicErrorCode::InsufficientQuota => "insufficient_quota",
        PublicErrorCode::RequestOutcomeUnknown
        | PublicErrorCode::UpstreamUnavailable
        | PublicErrorCode::InternalError => "server_error",
        PublicErrorCode::RateLimited => "rate_limit_error",
    }
}
