use af_domain::RateLimitScope;
use serde_json::Value;

use crate::bounded_json::{JsonLimits, parse_value};

/// 上游错误 JSON 的最大解析大小；传输缓冲仍由适配层 32 MiB 硬上限保护。
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

/// 从 OpenAI-compatible 错误正文提取的闭合安全分类。
///
/// 原始 `message`、未知 `type/code` 和字段路径永远不会离开协议边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiUpstreamErrorKind {
    /// 明确的上游 API Key 无效或已撤销。
    AuthRevoked,
    /// 明确的上游账号、组织或工作区已停用。
    AccountDisabled,
    /// 明确的请求或 token 速率限制。
    RateLimited {
        /// 从稳定错误码中推断的最小影响范围。
        scope: RateLimitScope,
    },
    /// 明确的上游账户额度或账单硬上限。
    QuotaExhausted,
    /// 明确的模型不存在或不可用。
    ModelUnsupported,
    /// 正文缺失、无效或没有命中审定信号。
    Unknown,
}

/// 在受限 JSON 边界内检查 OpenAI-compatible 错误的 `type/code`。
///
/// OpenAI 的 429 同时可能表示临时限流和账户额度耗尽；调用方只有在本函数返回
/// 明确分类时才能对外区分，未知 429 应按上游不可用处理。
#[must_use]
pub fn classify_error_response(body: &[u8]) -> OpenAiUpstreamErrorKind {
    if body.len() > MAX_ERROR_BODY_BYTES {
        return OpenAiUpstreamErrorKind::Unknown;
    }
    let Ok(value) = parse_value(body, ERROR_JSON_LIMITS) else {
        return OpenAiUpstreamErrorKind::Unknown;
    };
    let error = value.get("error").and_then(Value::as_object);
    let detail = value.get("detail").and_then(Value::as_object);
    if error.is_none() && detail.is_none() {
        return OpenAiUpstreamErrorKind::Unknown;
    }
    let signals = [
        error.and_then(|error| safe_signal(error.get("code"))),
        error.and_then(|error| safe_signal(error.get("type"))),
        detail.and_then(|detail| safe_signal(detail.get("code"))),
    ];

    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "invalid_api_key" | "token_invalidated" | "token_revoked"
        )
    }) {
        return OpenAiUpstreamErrorKind::AuthRevoked;
    }
    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "account_deactivated"
                | "deactivated_workspace"
                | "organization_deactivated"
                | "organization_disabled"
                | "workspace_deactivated"
        )
    }) {
        return OpenAiUpstreamErrorKind::AccountDisabled;
    }
    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "insufficient_quota" | "billing_hard_limit_reached" | "quota_exceeded"
        )
    }) {
        return OpenAiUpstreamErrorKind::QuotaExhausted;
    }
    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "model_rate_limit_exceeded" | "model_rate_limit_error"
        )
    }) {
        return OpenAiUpstreamErrorKind::RateLimited {
            scope: RateLimitScope::Model,
        };
    }
    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "account_rate_limit_exceeded"
                | "organization_rate_limit_exceeded"
                | "project_rate_limit_exceeded"
        )
    }) {
        return OpenAiUpstreamErrorKind::RateLimited {
            scope: RateLimitScope::Credential,
        };
    }
    if signals.iter().flatten().any(|signal| {
        matches!(
            *signal,
            "rate_limit_exceeded" | "rate_limit_error" | "requests" | "tokens"
        )
    }) {
        return OpenAiUpstreamErrorKind::RateLimited {
            scope: RateLimitScope::Window,
        };
    }
    if signals
        .iter()
        .flatten()
        .any(|signal| matches!(*signal, "model_not_found" | "model_not_supported"))
    {
        return OpenAiUpstreamErrorKind::ModelUnsupported;
    }
    OpenAiUpstreamErrorKind::Unknown
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
    fn classifies_only_reviewed_type_and_code_signals() {
        for (body, expected) in [
            (
                br#"{"error":{"message":"private","type":"invalid_request_error","code":"invalid_api_key"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::AuthRevoked,
            ),
            (
                br#"{"error":{"message":"private","type":"insufficient_quota","code":null}}"#.as_slice(),
                OpenAiUpstreamErrorKind::QuotaExhausted,
            ),
            (
                br#"{"error":{"message":"private","type":"tokens","code":"rate_limit_exceeded"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::RateLimited {
                    scope: RateLimitScope::Window,
                },
            ),
            (
                br#"{"error":{"message":"private","type":"invalid_request_error","code":"model_rate_limit_exceeded"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::RateLimited {
                    scope: RateLimitScope::Model,
                },
            ),
            (
                br#"{"error":{"message":"private","type":"invalid_request_error","code":"token_revoked"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::AuthRevoked,
            ),
            (
                br#"{"detail":{"code":"deactivated_workspace","message":"private"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::AccountDisabled,
            ),
            (
                br#"{"error":{"message":"private","type":"invalid_request_error","code":"model_not_found"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::ModelUnsupported,
            ),
            (
                br#"{"error":{"message":"private","type":"unknown","code":"unknown"}}"#.as_slice(),
                OpenAiUpstreamErrorKind::Unknown,
            ),
        ] {
            assert_eq!(classify_error_response(body), expected);
        }
    }

    #[test]
    fn malformed_duplicate_or_oversized_errors_fail_closed() {
        for body in [
            br#"{"error":{"code":"rate_limit_exceeded","code":"insufficient_quota"}}"#.to_vec(),
            br#"{"error":"rate_limit_exceeded"}"#.to_vec(),
            vec![b'x'; MAX_ERROR_BODY_BYTES + 1],
        ] {
            assert_eq!(
                classify_error_response(&body),
                OpenAiUpstreamErrorKind::Unknown
            );
        }
    }

    #[test]
    fn classification_never_retains_message_or_unknown_signals() {
        const SECRET: &str = "upstream-error-message-secret";
        let body = format!(
            r#"{{"error":{{"message":"{SECRET}","type":"private-type","code":"private-code"}}}}"#
        );
        let kind = classify_error_response(body.as_bytes());
        assert_eq!(kind, OpenAiUpstreamErrorKind::Unknown);
        assert!(!format!("{kind:?}").contains(SECRET));
        assert!(!format!("{kind:?}").contains("private-type"));
    }
}
