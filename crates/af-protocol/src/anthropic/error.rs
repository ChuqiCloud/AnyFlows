use af_domain::PublicErrorCode;
use serde::Serialize;
use serde_json::Value;

/// 将公开错误码和已审定文案包装为 Anthropic Messages 错误结构。
///
/// `message` 必须来自 HTTP 边界维护的公开文案，不能传入内部诊断或外部输入。
#[must_use]
pub fn encode_error(code: PublicErrorCode, message: &'static str) -> Value {
    serde_json::to_value(AnthropicErrorEnvelope {
        kind: "error",
        error: AnthropicErrorBody {
            kind: anthropic_error_type(code),
            message,
        },
    })
    .expect("Anthropic 错误结构只包含静态字符串，序列化必须成功")
}

#[derive(Serialize)]
struct AnthropicErrorEnvelope {
    #[serde(rename = "type")]
    kind: &'static str,
    error: AnthropicErrorBody,
}

#[derive(Serialize)]
struct AnthropicErrorBody {
    #[serde(rename = "type")]
    kind: &'static str,
    message: &'static str,
}

const fn anthropic_error_type(code: PublicErrorCode) -> &'static str {
    match code {
        PublicErrorCode::InvalidRequest => "invalid_request_error",
        PublicErrorCode::InvalidApiKey => "authentication_error",
        PublicErrorCode::InsufficientQuota => "billing_error",
        PublicErrorCode::ModelNotFound | PublicErrorCode::TaskNotFound => "not_found_error",
        PublicErrorCode::IdempotencyConflict => "invalid_request_error",
        PublicErrorCode::RequestOutcomeUnknown | PublicErrorCode::UpstreamUnavailable => {
            "overloaded_error"
        }
        PublicErrorCode::RateLimited => "rate_limit_error",
        PublicErrorCode::InternalError => "api_error",
    }
}
