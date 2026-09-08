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

/// 从 Anthropic 上游错误正文提取的闭合安全分类。
///
/// 原始 `message` 和未知错误类型不会离开协议边界，也不会进入 Debug 输出。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnthropicUpstreamErrorKind {
    /// API Key 或 OAuth 凭据未通过认证，但不能据此确认永久吊销。
    Authentication,
    /// 账户计费或余额硬限制阻止调用。
    Billing,
    /// 请求或令牌速率达到上游限制。
    RateLimited,
    /// 上游明确返回标准过载错误。
    Overloaded,
    /// 请求参数或正文大小不符合 Anthropic 协议。
    InvalidRequest,
    /// 请求资源或模型不存在。
    NotFound,
    /// 凭据没有当前资源权限，但不能据此确认永久吊销。
    Permission,
    /// 正文无效、超限或没有命中已审定信号。
    Unknown,
}

/// 在受限 JSON 边界内检查标准 Anthropic `error.type`。
#[must_use]
pub fn classify_error_response(body: &[u8]) -> AnthropicUpstreamErrorKind {
    if body.len() > MAX_ERROR_BODY_BYTES {
        return AnthropicUpstreamErrorKind::Unknown;
    }
    let Ok(value) = parse_value(body, ERROR_JSON_LIMITS) else {
        return AnthropicUpstreamErrorKind::Unknown;
    };
    if value.get("type").and_then(Value::as_str) != Some("error") {
        return AnthropicUpstreamErrorKind::Unknown;
    }
    let Some(signal) = value
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| safe_signal(error.get("type")))
    else {
        return AnthropicUpstreamErrorKind::Unknown;
    };
    match signal {
        "authentication_error" => AnthropicUpstreamErrorKind::Authentication,
        "billing_error" => AnthropicUpstreamErrorKind::Billing,
        "rate_limit_error" => AnthropicUpstreamErrorKind::RateLimited,
        "overloaded_error" => AnthropicUpstreamErrorKind::Overloaded,
        "invalid_request_error" | "request_too_large" => AnthropicUpstreamErrorKind::InvalidRequest,
        "not_found_error" => AnthropicUpstreamErrorKind::NotFound,
        "permission_error" => AnthropicUpstreamErrorKind::Permission,
        _ => AnthropicUpstreamErrorKind::Unknown,
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
    fn classifies_only_reviewed_anthropic_error_types() {
        for (kind, expected) in [
            (
                "authentication_error",
                AnthropicUpstreamErrorKind::Authentication,
            ),
            ("billing_error", AnthropicUpstreamErrorKind::Billing),
            ("rate_limit_error", AnthropicUpstreamErrorKind::RateLimited),
            ("overloaded_error", AnthropicUpstreamErrorKind::Overloaded),
            (
                "invalid_request_error",
                AnthropicUpstreamErrorKind::InvalidRequest,
            ),
            (
                "request_too_large",
                AnthropicUpstreamErrorKind::InvalidRequest,
            ),
            ("not_found_error", AnthropicUpstreamErrorKind::NotFound),
            ("permission_error", AnthropicUpstreamErrorKind::Permission),
        ] {
            let body =
                format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"private"}}}}"#);
            assert_eq!(classify_error_response(body.as_bytes()), expected);
        }
    }

    #[test]
    fn malformed_duplicate_and_oversized_errors_fail_closed() {
        for body in [
            br#"{"type":"error","error":{"type":"billing_error","type":"rate_limit_error"}}"#
                .to_vec(),
            br#"{"type":"error","error":"billing_error"}"#.to_vec(),
            vec![b'x'; MAX_ERROR_BODY_BYTES + 1],
        ] {
            assert_eq!(
                classify_error_response(&body),
                AnthropicUpstreamErrorKind::Unknown
            );
        }
    }

    #[test]
    fn classification_never_retains_message_or_unknown_type() {
        const SECRET: &str = "anthropic-upstream-message-secret";
        let body =
            format!(r#"{{"type":"error","error":{{"type":"private-type","message":"{SECRET}"}}}}"#);
        let kind = classify_error_response(body.as_bytes());
        assert_eq!(kind, AnthropicUpstreamErrorKind::Unknown);
        let debug = format!("{kind:?}");
        assert!(!debug.contains(SECRET));
        assert!(!debug.contains("private-type"));
    }
}
