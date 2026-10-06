use serde_json::Value;

use crate::bounded_json::{JsonLimits, parse_value};

const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const MAX_ERROR_SIGNAL_BYTES: usize = 128;
const ERROR_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: 256,
    max_object_entries: 64,
    max_array_items: 32,
    max_string_bytes: 16 * 1024,
    max_key_bytes: 256,
};

/// 从 Google JSON API 错误正文提取的闭合安全分类。
///
/// 解析只消费与 HTTP 状态一致的数字 `code` 和稳定 `status`，不会保留 `message`、
/// `details` 或未知状态名。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeminiUpstreamErrorKind {
    /// 请求参数不符合 Gemini 协议。
    InvalidRequest,
    /// 凭据未通过认证，但不能据此确认永久吊销。
    Authentication,
    /// 凭据或项目没有当前资源权限。
    Permission,
    /// 请求资源或模型不存在。
    ModelUnsupported,
    /// 项目、模型或请求窗口资源耗尽。
    RateLimited,
    /// 上游明确返回暂时不可用。
    Overloaded,
    /// 其他结构化 5xx 服务器错误。
    Server,
    /// 正文无效、超限、状态不一致或未命中审定信号。
    Unknown,
}

/// 在受限 JSON 边界内检查 Google JSON API 的 `error.code/status`。
#[must_use]
pub fn classify_error_response(http_status: u16, body: &[u8]) -> GeminiUpstreamErrorKind {
    if body.len() > MAX_ERROR_BODY_BYTES {
        return GeminiUpstreamErrorKind::Unknown;
    }
    let Ok(value) = parse_value(body, ERROR_JSON_LIMITS) else {
        return GeminiUpstreamErrorKind::Unknown;
    };
    let Some(error) = value.get("error").and_then(Value::as_object) else {
        return GeminiUpstreamErrorKind::Unknown;
    };
    let Some(code) = error
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|code| u16::try_from(code).ok())
    else {
        return GeminiUpstreamErrorKind::Unknown;
    };
    let Some(status) = safe_signal(error.get("status")) else {
        return GeminiUpstreamErrorKind::Unknown;
    };
    if code != http_status {
        return GeminiUpstreamErrorKind::Unknown;
    }

    match (code, status) {
        (400, "INVALID_ARGUMENT") => GeminiUpstreamErrorKind::InvalidRequest,
        (401, "UNAUTHENTICATED") => GeminiUpstreamErrorKind::Authentication,
        (403, "PERMISSION_DENIED") => GeminiUpstreamErrorKind::Permission,
        (404, "NOT_FOUND") => GeminiUpstreamErrorKind::ModelUnsupported,
        (429, "RESOURCE_EXHAUSTED") => GeminiUpstreamErrorKind::RateLimited,
        (503, "UNAVAILABLE") => GeminiUpstreamErrorKind::Overloaded,
        (500..=599, _) => GeminiUpstreamErrorKind::Server,
        _ => GeminiUpstreamErrorKind::Unknown,
    }
}

fn safe_signal(value: Option<&Value>) -> Option<&str> {
    let signal = value?.as_str()?;
    (!signal.is_empty()
        && signal.len() <= MAX_ERROR_SIGNAL_BYTES
        && !signal.chars().any(char::is_control))
    .then_some(signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_only_matching_reviewed_code_and_status() {
        for (code, status, expected) in [
            (
                400,
                "INVALID_ARGUMENT",
                GeminiUpstreamErrorKind::InvalidRequest,
            ),
            (
                401,
                "UNAUTHENTICATED",
                GeminiUpstreamErrorKind::Authentication,
            ),
            (
                403,
                "PERMISSION_DENIED",
                GeminiUpstreamErrorKind::Permission,
            ),
            (404, "NOT_FOUND", GeminiUpstreamErrorKind::ModelUnsupported),
            (
                429,
                "RESOURCE_EXHAUSTED",
                GeminiUpstreamErrorKind::RateLimited,
            ),
            (503, "UNAVAILABLE", GeminiUpstreamErrorKind::Overloaded),
            (500, "INTERNAL", GeminiUpstreamErrorKind::Server),
        ] {
            let body = format!(
                r#"{{"error":{{"code":{code},"status":"{status}","message":"private","details":[{{"private":"canary"}}]}}}}"#
            );
            assert_eq!(classify_error_response(code, body.as_bytes()), expected);
        }
    }

    #[test]
    fn malformed_duplicate_mismatched_and_oversized_errors_fail_closed() {
        for (http_status, body) in [
            (
                429,
                br#"{"error":{"code":429,"code":503,"status":"UNAVAILABLE"}}"#.to_vec(),
            ),
            (
                429,
                br#"{"error":{"code":503,"status":"UNAVAILABLE"}}"#.to_vec(),
            ),
            (
                429,
                br#"{"error":{"code":429,"message":"RESOURCE_EXHAUSTED"}}"#.to_vec(),
            ),
            (429, vec![b'x'; MAX_ERROR_BODY_BYTES + 1]),
        ] {
            assert_eq!(
                classify_error_response(http_status, &body),
                GeminiUpstreamErrorKind::Unknown
            );
        }
    }

    #[test]
    fn classification_never_retains_message_or_details() {
        const SECRET: &str = "gemini-upstream-message-secret";
        let body = format!(
            r#"{{"error":{{"code":418,"status":"PRIVATE_STATUS","message":"{SECRET}","details":[{{"secret":"{SECRET}"}}]}}}}"#
        );
        let kind = classify_error_response(418, body.as_bytes());
        assert_eq!(kind, GeminiUpstreamErrorKind::Unknown);
        let debug = format!("{kind:?}");
        assert!(!debug.contains(SECRET));
        assert!(!debug.contains("PRIVATE_STATUS"));
    }
}
