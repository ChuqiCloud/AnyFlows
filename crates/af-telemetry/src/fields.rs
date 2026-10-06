use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

use serde_json::{Map, Number, Value};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::Record,
};
use tracing_subscriber::{
    field::RecordFields,
    fmt::{
        FmtContext, FormattedFields,
        format::{FormatEvent, FormatFields, Writer},
        time::{FormatTime, SystemTime},
    },
    registry::LookupSpan,
};

const REDACTED: &str = "<redacted>";

/// 直接生成脱敏 JSON 事件，避免敏感字段先经过 Debug/Display 格式化。
#[derive(Debug, Default)]
pub(crate) struct RedactingJsonFormat;

impl<S, N> FormatEvent<S, N> for RedactingJsonFormat
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut timestamp = String::new();
        SystemTime.format_time(&mut Writer::new(&mut timestamp))?;

        let mut event_fields = SanitizedFields::default();
        event.record(&mut event_fields);

        let mut root = Map::new();
        root.insert("timestamp".to_owned(), Value::String(timestamp));
        root.insert(
            "level".to_owned(),
            Value::String(event.metadata().level().as_str().to_owned()),
        );
        root.insert("fields".to_owned(), Value::Object(event_fields.finish()));
        root.insert(
            "target".to_owned(),
            Value::String(event.metadata().target().to_owned()),
        );

        if let Some(scope) = context.event_scope() {
            let spans = scope
                .from_root()
                .map(|span| {
                    let extensions = span.extensions();
                    let mut fields = extensions
                        .get::<FormattedFields<N>>()
                        .and_then(|fields| {
                            serde_json::from_str::<Map<String, Value>>(&fields.fields).ok()
                        })
                        .unwrap_or_default();
                    fields.insert(
                        "name".to_owned(),
                        Value::String(span.metadata().name().to_owned()),
                    );
                    Value::Object(fields)
                })
                .collect();
            root.insert("spans".to_owned(), Value::Array(spans));
        }

        let rendered = serde_json::to_string(&root).map_err(|_| fmt::Error)?;
        writer.write_str(&rendered)?;
        writer.write_char('\n')
    }
}

/// 在私有缓冲中格式化 span 字段，失败时只存储空 JSON 对象。
#[derive(Debug, Default)]
pub(crate) struct FailClosedJsonFields;

impl FailClosedJsonFields {
    fn render_fields<R: RecordFields>(&self, fields: R) -> Option<Map<String, Value>> {
        let mut visitor = SanitizedFields::default();
        let formatted = catch_unwind(AssertUnwindSafe(|| {
            fields.record(&mut visitor);
        }));
        if formatted.is_err() {
            return None;
        }
        Some(visitor.finish())
    }

    fn serialize_fields(fields: &Map<String, Value>) -> String {
        serde_json::to_string(fields).unwrap_or_else(|_| "{}".to_owned())
    }
}

impl<'writer> FormatFields<'writer> for FailClosedJsonFields {
    fn format_fields<R: RecordFields>(
        &self,
        mut writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let fields = self.render_fields(fields).unwrap_or_default();
        writer.write_str(&Self::serialize_fields(&fields))
    }

    fn add_fields(
        &self,
        current: &'writer mut FormattedFields<Self>,
        fields: &Record<'_>,
    ) -> fmt::Result {
        let Some(new_fields) = self.render_fields(fields) else {
            return Ok(());
        };
        let mut merged =
            serde_json::from_str::<Map<String, Value>>(&current.fields).unwrap_or_default();
        merged.extend(new_fields);
        current.fields = Self::serialize_fields(&merged);
        Ok(())
    }
}

/// 按字段名与值类型收集 JSON 字段；敏感 Debug 值不会被调用。
#[derive(Default)]
struct SanitizedFields {
    fields: Map<String, Value>,
}

impl SanitizedFields {
    fn insert(&mut self, field: &Field, value: Value) {
        let value = if should_redact_field(field.name(), value.is_number()) {
            Value::String(REDACTED.to_owned())
        } else {
            value
        };
        self.fields.insert(output_field_name(field), value);
    }

    fn insert_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if should_redact_field(field.name(), false) {
            self.fields
                .insert(output_field_name(field), Value::String(REDACTED.to_owned()));
            return;
        }

        // 非敏感 Debug 由调用方保证安全；捕获异常仅用于维持日志链路可用。
        let rendered = catch_unwind(AssertUnwindSafe(|| format!("{value:?}")))
            .unwrap_or_else(|_| REDACTED.to_owned());
        self.fields
            .insert(output_field_name(field), Value::String(rendered));
    }

    fn finish(self) -> Map<String, Value> {
        self.fields
    }
}

impl Visit for SanitizedFields {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        let value = Number::from_i128(value)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_string()));
        self.insert(field, value);
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        let value = Number::from_u128(value)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(value.to_string()));
        self.insert(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::from(value));
    }

    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        self.insert(field, Value::from(value));
    }

    fn record_error(&mut self, field: &Field, _: &(dyn std::error::Error + 'static)) {
        self.fields
            .insert(output_field_name(field), Value::String(REDACTED.to_owned()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_debug(field, value);
    }
}

fn output_field_name(field: &Field) -> String {
    field
        .name()
        .strip_prefix("r#")
        .unwrap_or(field.name())
        .to_owned()
}

/// token 计数仅在实际为 JSON 数值时放行，其余字段按敏感名规则处理。
fn should_redact_field(name: &str, is_number: bool) -> bool {
    if is_safe_token_counter(name) {
        return !is_number;
    }
    is_sensitive_field(name)
}

/// 仅允许标准 token 计数名及其 `usage` 命名空间。
fn is_safe_token_counter(name: &str) -> bool {
    let normalized = normalize_field_name(name);
    const SAFE_TOKEN_COUNTERS: &[&str] = &[
        "cached_tokens",
        "completion_tokens",
        "input_tokens",
        "output_tokens",
        "prompt_tokens",
        "reasoning_tokens",
        "total_tokens",
    ];
    let counter_name = normalized.strip_prefix("usage_").unwrap_or(&normalized);
    SAFE_TOKEN_COUNTERS.contains(&counter_name)
}

/// 稳定错误分类可观测，原始错误值和错误链仍默认脱敏。
fn is_safe_error_metadata(name: &str) -> bool {
    let normalized = normalize_field_name(name);
    const SAFE_SUFFIXES: &[&str] = &["error_code", "error_count", "error_kind", "error_type"];
    const SENSITIVE_PREFIX_COMPONENTS: &[&str] = &[
        "authorization",
        "body",
        "cookie",
        "credential",
        "credentials",
        "header",
        "headers",
        "password",
        "payload",
        "prompt",
        "query",
        "secret",
        "settings",
        "token",
        "uri",
        "url",
    ];
    matches!(normalized.as_str(), "has_error" | "is_error")
        || SAFE_SUFFIXES.iter().any(|suffix| {
            normalized == *suffix
                || normalized
                    .strip_suffix(suffix)
                    .filter(|prefix| prefix.ends_with('_'))
                    .map(|prefix| prefix.trim_end_matches('_'))
                    .is_some_and(|prefix| {
                        !prefix
                            .split('_')
                            .any(|component| SENSITIVE_PREFIX_COMPONENTS.contains(&component))
                    })
        })
}

/// 归一化后按精确名、后缀、组件和复合短语识别敏感字段。
pub(crate) fn is_sensitive_field(name: &str) -> bool {
    if is_safe_token_counter(name) || is_safe_error_metadata(name) {
        return false;
    }
    let normalized = normalize_field_name(name);
    const EXACT_NAMES: &[&str] = &[
        "access_token",
        "api_key",
        "apikey",
        "authorization",
        "base_url",
        "bearer_token",
        "body",
        "ciphertext",
        "client_secret",
        "connection_string",
        "cookie",
        "credential",
        "credentials",
        "database_url",
        "encryption_key",
        "endpoint",
        "error",
        "error_chain",
        "error_source",
        "header",
        "header_map",
        "header_override",
        "headers",
        "id_token",
        "jwt_secret",
        "key_hash",
        "master_key",
        "nonce",
        "oauth_token",
        "param_override",
        "passphrase",
        "passwd",
        "password",
        "password_hash",
        "payload",
        "private_key",
        "prompt",
        "proxy_authorization",
        "proxy_url",
        "query",
        "redis_url",
        "refresh_token",
        "request",
        "request_body",
        "request_headers",
        "request_json",
        "request_target",
        "request_uri",
        "response",
        "response_body",
        "response_headers",
        "response_json",
        "secret",
        "secret_key",
        "session_id",
        "set_cookie",
        "settings",
        "signing_key",
        "source",
        "sources",
        "token",
        "totp_secret",
        "tool_arguments",
        "uri",
        "url",
        "user_agent",
        "x_api_key",
        "x_goog_api_key",
    ];
    const SENSITIVE_SUFFIXES: &[&str] = &[
        "_access_token",
        "_api_key",
        "_authorization",
        "_bearer_token",
        "_body",
        "_ciphertext",
        "_client_secret",
        "_connection_string",
        "_cookie",
        "_credential",
        "_credentials",
        "_endpoint",
        "_error",
        "_errors",
        "_header",
        "_header_map",
        "_header_override",
        "_headers",
        "_id_token",
        "_jwt_secret",
        "_key_hash",
        "_master_key",
        "_nonce",
        "_oauth_token",
        "_param_override",
        "_passphrase",
        "_passwd",
        "_password",
        "_password_hash",
        "_payload",
        "_private_key",
        "_prompt",
        "_query",
        "_refresh_token",
        "_request_json",
        "_request_target",
        "_response_json",
        "_secret",
        "_secret_key",
        "_session_id",
        "_settings",
        "_signing_key",
        "_token",
        "_totp_secret",
        "_tool_arguments",
        "_uri",
        "_url",
        "_user_agent",
    ];
    const SENSITIVE_COMPONENTS: &[&str] = &[
        "authorization",
        "body",
        "ciphertext",
        "cookie",
        "credential",
        "credentials",
        "endpoint",
        "error",
        "errors",
        "header",
        "headers",
        "messages",
        "nonce",
        "passphrase",
        "passwd",
        "password",
        "payload",
        "prompt",
        "query",
        "secret",
        "settings",
        "uri",
        "url",
    ];
    const SENSITIVE_PHRASES: &[&str] = &[
        "access_token",
        "api_key",
        "bearer_token",
        "connection_string",
        "encryption_key",
        "id_token",
        "key_hash",
        "master_key",
        "oauth_token",
        "param_override",
        "private_key",
        "refresh_token",
        "request_target",
        "session_id",
        "signing_key",
        "tool_arguments",
        "user_agent",
    ];

    EXACT_NAMES.contains(&normalized.as_str())
        || SENSITIVE_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
        || normalized
            .split('_')
            .any(|component| SENSITIVE_COMPONENTS.contains(&component))
        || SENSITIVE_PHRASES
            .iter()
            .any(|phrase| contains_bounded_phrase(&normalized, phrase))
}

/// 仅匹配下划线分隔的完整复合短语，避免普通子串误伤观测字段。
fn contains_bounded_phrase(name: &str, phrase: &str) -> bool {
    name.match_indices(phrase).any(|(start, _)| {
        let end = start + phrase.len();
        (start == 0 || name.as_bytes()[start - 1] == b'_')
            && (end == name.len() || name.as_bytes()[end] == b'_')
    })
}

/// 将 raw identifier、camelCase 与常见分隔符统一为小写下划线形式。
fn normalize_field_name(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let mut normalized = String::with_capacity(name.len());
    let mut previous_separator = false;
    let mut previous_lowercase_or_digit = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && previous_lowercase_or_digit && !previous_separator
            {
                normalized.push('_');
            }
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
            previous_lowercase_or_digit =
                character.is_ascii_lowercase() || character.is_ascii_digit();
        } else if !previous_separator && !normalized.is_empty() {
            normalized.push('_');
            previous_separator = true;
            previous_lowercase_or_digit = false;
        }
    }
    if previous_separator {
        normalized.pop();
    }
    normalized
}
