use af_domain::PublicErrorCode;
use serde::Serialize;
use serde_json::Value;

/// 将公开错误码和已审定文案包装为 Google JSON API 错误结构。
///
/// `message` 必须来自 HTTP 边界维护的公开文案，不能传入内部诊断或外部输入。
#[must_use]
pub fn encode_error(code: PublicErrorCode, message: &'static str) -> Value {
    let (status_code, status) = google_status(code);
    serde_json::to_value(GoogleErrorEnvelope {
        error: GoogleErrorBody {
            code: status_code,
            message,
            status,
        },
    })
    .expect("Gemini 错误结构只包含静态值，序列化必须成功")
}

#[derive(Serialize)]
struct GoogleErrorEnvelope {
    error: GoogleErrorBody,
}

#[derive(Serialize)]
struct GoogleErrorBody {
    code: u16,
    message: &'static str,
    status: &'static str,
}

const fn google_status(code: PublicErrorCode) -> (u16, &'static str) {
    match code {
        PublicErrorCode::InvalidRequest => (400, "INVALID_ARGUMENT"),
        PublicErrorCode::InvalidApiKey => (401, "UNAUTHENTICATED"),
        PublicErrorCode::InsufficientQuota | PublicErrorCode::RateLimited => {
            (429, "RESOURCE_EXHAUSTED")
        }
        PublicErrorCode::ModelNotFound | PublicErrorCode::TaskNotFound => (404, "NOT_FOUND"),
        PublicErrorCode::IdempotencyConflict => (409, "ALREADY_EXISTS"),
        PublicErrorCode::RequestOutcomeUnknown | PublicErrorCode::UpstreamUnavailable => {
            (503, "UNAVAILABLE")
        }
        PublicErrorCode::InternalError => (500, "INTERNAL"),
    }
}
