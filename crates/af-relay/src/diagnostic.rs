use std::fmt;

use af_adapter::{Bytes, HeaderMap, UpstreamRequest, UpstreamResponse};
use serde_json::{Map, Value, json};
use url::{Url, form_urlencoded};

const REDACTED_VALUE: &str = "<已脱敏>";
const OMITTED_BINARY_VALUE: &str = "<二进制正文未保留>";
const OVERSIZED_VALUE: &str = "<正文超过安全解析上限，未保留>";
const MAX_SAFE_BODY_BYTES: usize = 64 * 1_024;
const MAX_JSON_PARSE_BYTES: usize = 1_024 * 1_024;
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_NODES: usize = 8_192;

/// 一次调试追踪允许采集的快照范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayDiagnosticPolicy {
    capture_headers: bool,
    capture_bodies: bool,
    max_body_bytes: usize,
}

impl RelayDiagnosticPolicy {
    /// 创建有界采集策略；正文上限为零或超过安全绝对值时拒绝。
    pub fn new(capture_headers: bool, capture_bodies: bool, max_body_bytes: usize) -> Option<Self> {
        (max_body_bytes > 0 && max_body_bytes <= MAX_SAFE_BODY_BYTES).then_some(Self {
            capture_headers,
            capture_bodies,
            max_body_bytes,
        })
    }

    #[must_use]
    pub const fn capture_headers(self) -> bool {
        self.capture_headers
    }

    #[must_use]
    pub const fn capture_bodies(self) -> bool {
        self.capture_bodies
    }

    #[must_use]
    pub const fn max_body_bytes(self) -> usize {
        self.max_body_bytes
    }
}

/// HTTP 入口在离开信任边界前生成的永久脱敏请求材料。
#[derive(Clone, Eq, PartialEq)]
pub struct RelayDiagnosticInput {
    method: String,
    path: String,
    headers: RelayDiagnosticSnapshot,
    body: RelayDiagnosticSnapshot,
}

impl RelayDiagnosticInput {
    /// 从下游请求生成安全快照；返回值不再包含认证凭据明文。
    #[must_use]
    pub fn capture(method: &str, path: &str, headers: &HeaderMap, body: &Bytes) -> Self {
        Self {
            method: method.to_owned(),
            path: sanitize_url_or_path(path),
            headers: RelayDiagnosticSnapshot::headers(headers),
            body: RelayDiagnosticSnapshot::body(body),
        }
    }

    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn headers_json(&self) -> &str {
        self.headers.json()
    }

    #[must_use]
    pub fn body_json(&self, max_bytes: usize) -> String {
        self.body.with_limit(max_bytes)
    }
}

impl fmt::Debug for RelayDiagnosticInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayDiagnosticInput")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("headers", &"<已脱敏>")
            .field("body", &"<已脱敏>")
            .finish()
    }
}

/// 已完成永久脱敏、可安全跨层传递的 JSON 快照。
#[derive(Clone, Eq, PartialEq)]
pub struct RelayDiagnosticSnapshot {
    json: String,
    content: Option<String>,
    original_bytes: usize,
    format: &'static str,
    truncated: bool,
}

impl RelayDiagnosticSnapshot {
    /// 生成保留重复字段、但敏感值固定替换的 Header 列表。
    #[must_use]
    pub fn headers(headers: &HeaderMap) -> Self {
        let entries = headers
            .iter()
            .map(|(name, value)| {
                let name = name.as_str().to_ascii_lowercase();
                let value = if is_sensitive_header_name(&name) {
                    REDACTED_VALUE.to_owned()
                } else {
                    value
                        .to_str()
                        .map(redact_inline_secrets)
                        .unwrap_or_else(|_| format!("<非 UTF-8:{} 字节>", value.as_bytes().len()))
                };
                json!({ "name": name, "value": value })
            })
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&entries).expect("Header 快照必须可序列化");
        Self {
            json,
            content: None,
            original_bytes: 0,
            format: "headers",
            truncated: false,
        }
    }

    /// 生成正文快照；JSON 先递归脱敏，超预算和二进制正文只保留元数据。
    #[must_use]
    pub fn body(body: &Bytes) -> Self {
        let original_bytes = body.len();
        let (format, content, forced_truncated) = if original_bytes > MAX_JSON_PARSE_BYTES {
            ("omitted", OVERSIZED_VALUE.to_owned(), true)
        } else if let Ok(mut value) = serde_json::from_slice::<Value>(body) {
            let mut nodes = 0;
            redact_json(&mut value, 0, &mut nodes);
            (
                "json",
                serde_json::to_string_pretty(&value).expect("已解析 JSON 必须可重新序列化"),
                false,
            )
        } else if let Ok(value) = std::str::from_utf8(body) {
            ("text", redact_inline_secrets(value), false)
        } else {
            (
                "binary",
                OMITTED_BINARY_VALUE.to_owned(),
                original_bytes > 0,
            )
        };
        let mut snapshot = Self {
            json: String::new(),
            content: Some(content),
            original_bytes,
            format,
            truncated: forced_truncated,
        };
        snapshot.json = snapshot.with_limit(MAX_SAFE_BODY_BYTES);
        snapshot
    }

    /// 返回当前完整安全快照 JSON。
    #[must_use]
    pub fn json(&self) -> &str {
        &self.json
    }

    /// 按设置的 UTF-8 字节预算重新生成合法 JSON 包装，截断不会破坏外层结构。
    #[must_use]
    pub fn with_limit(&self, max_bytes: usize) -> String {
        let max_bytes = max_bytes.clamp(1, MAX_SAFE_BODY_BYTES);
        let Some(content) = self.content.as_deref() else {
            return self.json.clone();
        };
        let (content, truncated) = truncate_utf8(content, max_bytes);
        serde_json::to_string(&json!({
            "format": self.format,
            "content": content,
            "original_bytes": self.original_bytes,
            "truncated": self.truncated || truncated,
        }))
        .expect("正文快照包装必须可序列化")
    }
}

impl fmt::Debug for RelayDiagnosticSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RelayDiagnosticSnapshot(<已脱敏>)")
    }
}

/// 单个候选在最终发送边界生成的安全诊断快照。
#[derive(Clone, Eq, PartialEq)]
pub struct RelayAttemptDiagnostic {
    request_method: String,
    request_url: String,
    request_headers_json: Option<String>,
    request_body_json: Option<String>,
    response_status: Option<u16>,
    response_headers_json: Option<String>,
    response_body_json: Option<String>,
    response_streamed: bool,
    policy: RelayDiagnosticPolicy,
}

impl RelayAttemptDiagnostic {
    pub(crate) fn capture_request(
        request: &UpstreamRequest,
        policy: RelayDiagnosticPolicy,
    ) -> Self {
        Self {
            request_method: request.method().as_str().to_owned(),
            request_url: sanitize_url_or_path(request.target()),
            request_headers_json: policy.capture_headers().then(|| {
                RelayDiagnosticSnapshot::headers(request.headers())
                    .json()
                    .to_owned()
            }),
            request_body_json: policy.capture_bodies().then(|| {
                request.body().map_or_else(
                    || "null".to_owned(),
                    |body| RelayDiagnosticSnapshot::body(body).with_limit(policy.max_body_bytes()),
                )
            }),
            response_status: None,
            response_headers_json: None,
            response_body_json: None,
            response_streamed: false,
            policy,
        }
    }

    pub(crate) fn capture_response_head(&mut self, response: &UpstreamResponse) {
        self.response_status = Some(response.status().as_u16());
        self.response_headers_json = self.policy.capture_headers().then(|| {
            RelayDiagnosticSnapshot::headers(response.headers())
                .json()
                .to_owned()
        });
        self.response_streamed = response.full_body().is_none();
        if self.policy.capture_bodies()
            && let Some(body) = response.full_body()
        {
            self.response_body_json =
                Some(RelayDiagnosticSnapshot::body(body).with_limit(self.policy.max_body_bytes()));
        }
    }

    pub(crate) fn capture_buffered_response_body(&mut self, body: &Bytes) {
        if self.policy.capture_bodies() {
            self.response_body_json =
                Some(RelayDiagnosticSnapshot::body(body).with_limit(self.policy.max_body_bytes()));
        }
        self.response_streamed = false;
    }

    #[must_use]
    pub fn request_method(&self) -> &str {
        &self.request_method
    }

    #[must_use]
    pub fn request_url(&self) -> &str {
        &self.request_url
    }

    #[must_use]
    pub fn request_headers_json(&self) -> Option<&str> {
        self.request_headers_json.as_deref()
    }

    #[must_use]
    pub fn request_body_json(&self) -> Option<&str> {
        self.request_body_json.as_deref()
    }

    #[must_use]
    pub const fn response_status(&self) -> Option<u16> {
        self.response_status
    }

    #[must_use]
    pub fn response_headers_json(&self) -> Option<&str> {
        self.response_headers_json.as_deref()
    }

    #[must_use]
    pub fn response_body_json(&self) -> Option<&str> {
        self.response_body_json.as_deref()
    }

    #[must_use]
    pub const fn response_streamed(&self) -> bool {
        self.response_streamed
    }
}

impl fmt::Debug for RelayAttemptDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayAttemptDiagnostic")
            .field("request_method", &self.request_method)
            .field("request_url", &"<已脱敏>")
            .field("response_status", &self.response_status)
            .field("response_streamed", &self.response_streamed)
            .finish()
    }
}

/// 对上游目标执行凭据、敏感查询参数和内联令牌脱敏。
#[must_use]
pub(crate) fn sanitize_url_or_path(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return sanitize_relative_target(value).unwrap_or_else(|| redact_inline_secrets(value));
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let pairs = url
        .query_pairs()
        .map(|(name, value)| {
            let value = if is_sensitive_name(name.as_ref()) {
                REDACTED_VALUE.to_owned()
            } else {
                redact_inline_secrets(value.as_ref())
            };
            (name.into_owned(), value)
        })
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

fn sanitize_relative_target(value: &str) -> Option<String> {
    let (path_and_query, fragment) = value
        .split_once('#')
        .map_or((value, None), |(path, fragment)| (path, Some(fragment)));
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    if !path.starts_with('/') {
        return None;
    }

    let mut sanitized = redact_inline_secrets(path);
    if let Some(query) = query {
        let pairs = form_urlencoded::parse(query.as_bytes()).map(|(name, value)| {
            let value = if is_sensitive_name(name.as_ref()) {
                REDACTED_VALUE.to_owned()
            } else {
                redact_inline_secrets(value.as_ref())
            };
            (name, value)
        });
        let query = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        sanitized.push('?');
        sanitized.push_str(&query);
    }
    if let Some(fragment) = fragment {
        sanitized.push('#');
        sanitized.push_str(&redact_inline_secrets(fragment));
    }
    Some(sanitized)
}

fn redact_json(value: &mut Value, depth: usize, nodes: &mut usize) {
    *nodes = nodes.saturating_add(1);
    if depth >= MAX_JSON_DEPTH || *nodes > MAX_JSON_NODES {
        *value = Value::String(OVERSIZED_VALUE.to_owned());
        return;
    }
    match value {
        Value::Object(object) => redact_object(object, depth, nodes),
        Value::Array(values) => {
            for value in values {
                redact_json(value, depth + 1, nodes);
            }
        }
        Value::String(value) => *value = redact_inline_secrets(value),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn redact_object(object: &mut Map<String, Value>, depth: usize, nodes: &mut usize) {
    for (name, value) in object {
        if is_sensitive_name(name) {
            *value = Value::String(REDACTED_VALUE.to_owned());
        } else {
            redact_json(value, depth + 1, nodes);
        }
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy_authorization"
            | "x_api_key"
            | "x_goog_api_key"
            | "api_key"
            | "apikey"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "client_secret"
            | "password"
            | "passwd"
            | "cookie"
            | "set_cookie"
            | "session"
            | "session_id"
            | "code_verifier"
            | "private_key"
            | "key"
            | "secret"
            | "token"
    ) || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_password")
        || normalized.ends_with("_api_key")
}

fn is_sensitive_header_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(normalized.as_str(), "user-agent" | "x-app") || is_sensitive_name(&normalized)
}

fn redact_inline_secrets(value: &str) -> String {
    let mut redacted = value.to_owned();
    for marker in ["Bearer ", "Basic ", "sk-"] {
        let mut start = 0;
        while let Some(offset) = redacted[start..].find(marker) {
            let token_start = start + offset;
            let value_start = token_start + marker.len();
            let token_end = redacted[value_start..]
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, '"' | '\'' | ',' | '&')
                })
                .map_or(redacted.len(), |end| value_start + end);
            redacted.replace_range(token_start..token_end, REDACTED_VALUE);
            start = token_start + REDACTED_VALUE.len();
        }
    }
    redacted
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (&str, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], true)
}

#[cfg(test)]
mod tests {
    use af_adapter::{HeaderName, HeaderValue};

    use super::*;

    #[test]
    fn headers_and_json_secrets_are_permanently_redacted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer header-secret-canary"),
        );
        headers.insert(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_static("request-1"),
        );
        let body = Bytes::from_static(
            br#"{"api_key":"body-secret-canary","messages":[{"content":"hello"}]}"#,
        );

        let input = RelayDiagnosticInput::capture("POST", "/v1/chat/completions", &headers, &body);
        assert!(!input.headers_json().contains("header-secret-canary"));
        assert!(input.headers_json().contains("request-1"));
        let body = input.body_json(MAX_SAFE_BODY_BYTES);
        assert!(!body.contains("body-secret-canary"));
        assert!(body.contains(REDACTED_VALUE));
    }

    #[test]
    fn client_simulation_identity_headers_never_survive_capture() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("claude-cli/2.1.114 (external, sdk-cli)"),
        );
        headers.insert(
            HeaderName::from_static("x-app"),
            HeaderValue::from_static("cli"),
        );

        let snapshot = RelayDiagnosticSnapshot::headers(&headers);

        assert!(!snapshot.json().contains("claude-cli/2.1.114"));
        assert!(!snapshot.json().contains("sdk-cli"));
        assert!(!snapshot.json().contains("\"cli\""));
        assert_eq!(snapshot.json().matches(REDACTED_VALUE).count(), 2);
    }

    #[test]
    fn cookies_proxy_credentials_and_nested_json_canaries_never_survive_capture() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("session=cookie-secret-canary"),
        );
        headers.insert(
            HeaderName::from_static("proxy-authorization"),
            HeaderValue::from_static("Basic proxy-secret-canary"),
        );
        headers.insert(
            HeaderName::from_static("x-diagnostic-note"),
            HeaderValue::from_static("Bearer inline-secret-canary"),
        );
        let body = Bytes::from_static(
            br#"{"credentials":{"access_token":"nested-secret-canary"},"items":[{"password":"password-secret-canary"}],"note":"sk-inline-body-canary"}"#,
        );

        let input = RelayDiagnosticInput::capture(
            "POST",
            "/v1/messages?api_key=query-secret-canary",
            &headers,
            &body,
        );
        let snapshot = format!(
            "{}\n{}\n{}",
            input.path(),
            input.headers_json(),
            input.body_json(MAX_SAFE_BODY_BYTES)
        );
        for canary in [
            "cookie-secret-canary",
            "proxy-secret-canary",
            "inline-secret-canary",
            "nested-secret-canary",
            "password-secret-canary",
            "inline-body-canary",
            "query-secret-canary",
        ] {
            assert!(!snapshot.contains(canary));
        }
    }

    #[test]
    fn url_credentials_and_sensitive_query_values_are_redacted() {
        let sanitized = sanitize_url_or_path(
            "https://user:pass@example.test/v1/models?key=query-secret&model=gpt-5",
        );
        assert!(!sanitized.contains("user"));
        assert!(!sanitized.contains("pass"));
        assert!(!sanitized.contains("query-secret"));
        assert!(sanitized.contains("model=gpt-5"));
    }

    #[test]
    fn truncation_preserves_valid_wrapper_and_metadata() {
        let body = RelayDiagnosticSnapshot::body(&Bytes::from_static(br#"{"message":"abcdefgh"}"#));
        let value: Value = serde_json::from_str(&body.with_limit(4)).unwrap();
        assert_eq!(value["truncated"], true);
        assert_eq!(value["original_bytes"], 22);
    }

    #[test]
    fn utf8_truncation_stops_on_character_boundary() {
        let body = RelayDiagnosticSnapshot::body(&Bytes::from_static("你好世界".as_bytes()));
        let value: Value = serde_json::from_str(&body.with_limit(5)).unwrap();
        assert_eq!(value["content"], "你");
        assert_eq!(value["truncated"], true);
        assert_eq!(value["original_bytes"], 12);
    }
}
