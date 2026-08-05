use std::{collections::BTreeSet, fmt};

use serde_json::{Map, Value};

use crate::bounded_json::{JsonLimits, parse_value};

const MAX_ERROR_BODY_BYTES: usize = 64 * 1_024;
const MAX_ERROR_SIGNAL_BYTES: usize = 128;
const MAX_ERROR_EVIDENCE_BYTES: usize = 512;
const ERROR_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: 256,
    max_object_entries: 64,
    max_array_items: 32,
    max_string_bytes: 16 * 1_024,
    max_key_bytes: 256,
};

/// 仅由标准错误标识字段组成的有界匹配证据。
///
/// 本类型只提取 `error.type`、`error.code`、`error.status` 与 `detail.code`，不会保留
/// `message`、`details` 或完整上游正文；`Debug` 也只显示长度。
#[derive(Clone, Eq, PartialEq)]
pub struct StructuredErrorEvidence {
    text: Box<str>,
}

impl StructuredErrorEvidence {
    /// 返回仅供内存关键词匹配使用的已审定文本；调用方不得记录或持久化该值。
    #[must_use]
    pub fn as_match_text(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for StructuredErrorEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StructuredErrorEvidence")
            .field("byte_len", &self.text.len())
            .finish()
    }
}

/// 从受限 JSON 中提取标准错误标识，不读取可能包含请求内容或凭据的消息字段。
#[must_use]
pub fn extract_structured_error_evidence(body: &[u8]) -> Option<StructuredErrorEvidence> {
    if body.len() > MAX_ERROR_BODY_BYTES {
        return None;
    }
    let value = parse_value(body, ERROR_JSON_LIMITS).ok()?;
    let mut signals = BTreeSet::new();
    if let Some(error) = value.get("error").and_then(Value::as_object) {
        collect_signal(error, "type", &mut signals);
        collect_signal(error, "code", &mut signals);
        collect_signal(error, "status", &mut signals);
    }
    if let Some(detail) = value.get("detail").and_then(Value::as_object) {
        collect_signal(detail, "code", &mut signals);
    }
    if signals.is_empty() {
        return None;
    }
    let text = signals.into_iter().collect::<Vec<_>>().join(" ");
    (text.len() <= MAX_ERROR_EVIDENCE_BYTES).then(|| StructuredErrorEvidence {
        text: text.into_boxed_str(),
    })
}

fn collect_signal(object: &Map<String, Value>, key: &str, signals: &mut BTreeSet<String>) {
    let Some(signal) = object.get(key).and_then(Value::as_str) else {
        return;
    };
    if !signal.is_empty()
        && signal.len() <= MAX_ERROR_SIGNAL_BYTES
        && !signal.chars().any(char::is_control)
    {
        signals.insert(signal.to_lowercase());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_keeps_only_bounded_standard_identifiers() {
        const SECRET: &str = "upstream-message-secret-canary";
        let body = format!(
            r#"{{"error":{{"type":"private_type","code":"workspace_disabled","status":"DENIED","message":"{SECRET}","details":[{{"secret":"{SECRET}"}}]}}}}"#
        );
        let evidence = extract_structured_error_evidence(body.as_bytes()).unwrap();

        assert_eq!(
            evidence.as_match_text(),
            "denied private_type workspace_disabled"
        );
        let rendered = format!("{evidence:?}");
        assert!(!rendered.contains(SECRET));
        assert!(!rendered.contains("workspace_disabled"));
    }

    #[test]
    fn evidence_rejects_messages_malformed_json_and_oversized_signals() {
        assert!(
            extract_structured_error_evidence(br#"{"error":{"message":"workspace_disabled"}}"#)
                .is_none()
        );
        assert!(
            extract_structured_error_evidence(br#"{"error":{"code":"first","code":"second"}}"#)
                .is_none()
        );
        let body = format!(r#"{{"error":{{"code":"{}"}}}}"#, "x".repeat(129));
        assert!(extract_structured_error_evidence(body.as_bytes()).is_none());
    }
}
