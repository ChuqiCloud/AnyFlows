use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability identifier used when a manifest opts into Provider lifecycle hooks.
pub const PROVIDER_LIFECYCLE_CAPABILITY: &str = "provider.lifecycle";
/// Maximum JSON payload carried by one provider session request, state, or response.
pub const MAX_PROVIDER_SESSION_VALUE_BYTES: usize = 256 * 1024;
/// Maximum WebSocket message size exposed to a Provider plugin session.
pub const MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES: usize = 128 * 1024;
/// Maximum number of frames returned by one bounded Provider receive action.
pub const MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES: u8 = 16;
/// Maximum JSON challenge/context accepted for host-mediated WebSocket authentication.
pub const MAX_PROVIDER_SESSION_WEBSOCKET_AUTH_REQUEST_BYTES: usize = 16 * 1024;

/// Provider lifecycle operations that a plugin may implement. The host still
/// owns credentials, the upstream host, network I/O, billing and public IDs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginProviderOperation {
    Discover {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        #[serde(default = "default_provider_page_size")]
        limit: u16,
    },
    Request {
        model: String,
        request: Value,
    },
    SessionStart {
        model: String,
        request: Value,
    },
    SessionStep {
        session_id: String,
        state: Value,
        response: Value,
    },
    SessionClose {
        session_id: String,
        state: Value,
        reason: String,
    },
    Response {
        status: u16,
        response: Value,
    },
    ClassifyFailure {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<u16>,
        error: Value,
    },
    RetryAdvice {
        retryable: bool,
        delay_ms: u32,
        reason_code: String,
    },
}

/// Safe provider projection returned by a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginProviderResult {
    Models {
        models: Vec<PluginProviderModel>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
    Request {
        method: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint_id: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        body: Value,
    },
    Session {
        session_id: String,
        state: Value,
        action: PluginProviderSessionAction,
    },
    Response {
        response: Value,
    },
    Failure {
        class: PluginProviderFailureClass,
        code: String,
    },
    Retry {
        retryable: bool,
        delay_ms: u32,
        reason_code: String,
    },
}

/// Host-executed steps for a bounded multi-request provider session.
///
/// The plugin never receives credentials or an absolute URL. The host applies
/// the same channel, header, body and response limits as a normal Relay turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginProviderSessionAction {
    Request {
        method: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint_id: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        body: Value,
        #[serde(default)]
        stream: bool,
    },
    WebSocketConnect {
        endpoint_id: String,
        path: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
    },
    WebSocketSendText {
        text: String,
    },
    WebSocketSendBinary {
        data_base64: String,
    },
    WebSocketReceive {
        timeout_ms: u32,
    },
    WebSocketReceiveBatch {
        timeout_ms: u32,
        max_messages: u8,
    },
    WebSocketAuthenticate {
        #[serde(default)]
        request: Value,
    },
    WebSocketClose,
    Wait {
        delay_ms: u32,
    },
    Complete {
        response: Value,
    },
    Abort {
        code: String,
    },
}

/// Public model metadata returned during provider discovery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProviderModel {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginProviderFailureClass {
    Authentication,
    Authorization,
    RateLimited,
    InvalidRequest,
    Unavailable,
    Timeout,
    Protocol,
    Unknown,
}

impl PluginProviderOperation {
    pub fn validate(&self) -> Result<(), ProviderContractError> {
        match self {
            Self::Discover {
                query,
                cursor,
                limit,
            } => {
                if *limit == 0 || *limit > 500 {
                    return Err(ProviderContractError::InvalidLimit);
                }
                validate_optional_text(query.as_deref(), 128)?;
                validate_optional_text(cursor.as_deref(), 512)?;
            }
            Self::Request { model, request } => {
                validate_text(model, 128, ProviderContractError::InvalidModel)?;
                if request.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::SessionStart { model, request } => {
                validate_text(model, 128, ProviderContractError::InvalidModel)?;
                validate_session_value(request)?;
            }
            Self::SessionStep {
                session_id,
                state,
                response,
            } => {
                validate_session_text(session_id)?;
                validate_session_value(state)?;
                validate_session_value(response)?;
            }
            Self::SessionClose {
                session_id,
                state,
                reason,
            } => {
                validate_session_text(session_id)?;
                validate_session_value(state)?;
                validate_text(reason, 128, ProviderContractError::InvalidReason)?;
            }
            Self::Response { status, response } => {
                if !(100..=599).contains(status) || response.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::ClassifyFailure { status, error } => {
                if status.is_some_and(|value| !(100..=599).contains(&value)) || error.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::RetryAdvice {
                delay_ms,
                reason_code,
                ..
            } => {
                if *delay_ms > 86_400_000 {
                    return Err(ProviderContractError::InvalidDelay);
                }
                validate_text(reason_code, 128, ProviderContractError::InvalidReason)?;
            }
        }
        Ok(())
    }
}

impl PluginProviderResult {
    pub fn validate(&self) -> Result<(), ProviderContractError> {
        match self {
            Self::Models {
                models,
                next_cursor,
            } => {
                if models.len() > 500 {
                    return Err(ProviderContractError::InvalidLimit);
                }
                for model in models {
                    validate_text(&model.id, 128, ProviderContractError::InvalidModel)?;
                    if let Some(name) = &model.display_name {
                        validate_text(name, 256, ProviderContractError::InvalidReason)?;
                    }
                    if model
                        .modalities
                        .as_ref()
                        .is_some_and(|items| items.len() > 16)
                    {
                        return Err(ProviderContractError::InvalidLimit);
                    }
                }
                validate_optional_text(next_cursor.as_deref(), 512)?;
            }
            Self::Request {
                method,
                path,
                endpoint_id,
                headers,
                body,
            } => {
                validate_text(method, 16, ProviderContractError::InvalidMethod)?;
                if !matches!(
                    method.as_str(),
                    "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                ) {
                    return Err(ProviderContractError::InvalidMethod);
                }
                validate_provider_path(path)?;
                if let Some(endpoint_id) = endpoint_id {
                    validate_endpoint_id(endpoint_id)?;
                }
                validate_session_headers(headers, endpoint_id.is_some())?;
                if headers.len() > 64 || body.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::Session {
                session_id,
                state,
                action,
            } => {
                validate_session_text(session_id)?;
                validate_session_value(state)?;
                action.validate()?;
            }
            Self::Response { response } => {
                if response.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::Failure { code, .. } => {
                validate_text(code, 128, ProviderContractError::InvalidReason)?;
            }
            Self::Retry {
                delay_ms,
                reason_code,
                ..
            } => {
                if *delay_ms > 86_400_000 {
                    return Err(ProviderContractError::InvalidDelay);
                }
                validate_text(reason_code, 128, ProviderContractError::InvalidReason)?;
            }
        }
        Ok(())
    }
}

impl PluginProviderSessionAction {
    pub fn validate(&self) -> Result<(), ProviderContractError> {
        match self {
            Self::Request {
                method,
                path,
                endpoint_id,
                headers,
                body,
                stream,
            } => {
                validate_text(method, 16, ProviderContractError::InvalidMethod)?;
                if !valid_provider_method(method) {
                    return Err(ProviderContractError::InvalidMethod);
                }
                validate_provider_path(path)?;
                if let Some(endpoint_id) = endpoint_id {
                    validate_endpoint_id(endpoint_id)?;
                }
                validate_session_headers(headers, endpoint_id.is_some())?;
                if *stream || body.is_null() {
                    return Err(ProviderContractError::InvalidPayload);
                }
                validate_session_value(body)?;
            }
            Self::Wait { delay_ms } => {
                if *delay_ms > 30_000 {
                    return Err(ProviderContractError::InvalidDelay);
                }
            }
            Self::WebSocketConnect {
                endpoint_id,
                path,
                headers,
            } => {
                validate_endpoint_id(endpoint_id)?;
                validate_provider_path(path)?;
                validate_session_headers(headers, true)?;
            }
            Self::WebSocketSendText { text } => {
                if text.len() > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::WebSocketSendBinary { data_base64 } => {
                if data_base64.len() > MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES.div_ceil(3) * 4
                    || !validate_provider_base64(data_base64)
                {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::WebSocketReceive { timeout_ms } => {
                if *timeout_ms == 0 || *timeout_ms > 30_000 {
                    return Err(ProviderContractError::InvalidDelay);
                }
            }
            Self::WebSocketReceiveBatch {
                timeout_ms,
                max_messages,
            } => {
                if *timeout_ms == 0
                    || *timeout_ms > 30_000
                    || *max_messages == 0
                    || *max_messages > MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES
                {
                    return Err(ProviderContractError::InvalidLimit);
                }
            }
            Self::WebSocketAuthenticate { request } => {
                if !request.is_object()
                    || request.get("challenge").is_none_or(Value::is_null)
                    || serde_json::to_vec(request).map_or(true, |bytes| {
                        bytes.len() > MAX_PROVIDER_SESSION_WEBSOCKET_AUTH_REQUEST_BYTES
                    })
                {
                    return Err(ProviderContractError::InvalidPayload);
                }
            }
            Self::WebSocketClose => {}
            Self::Complete { response } => validate_session_value(response)?,
            Self::Abort { code } => validate_text(code, 128, ProviderContractError::InvalidReason)?,
        }
        Ok(())
    }
}

fn valid_provider_method(method: &str) -> bool {
    matches!(
        method,
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
    )
}

fn validate_session_text(value: &str) -> Result<(), ProviderContractError> {
    validate_text(value, 128, ProviderContractError::InvalidReason)
}

fn validate_endpoint_id(value: &str) -> Result<(), ProviderContractError> {
    if value.is_empty()
        || value.len() > 64
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
        || !value.as_bytes()[0].is_ascii_lowercase()
    {
        Err(ProviderContractError::InvalidReason)
    } else {
        Ok(())
    }
}

fn validate_provider_base64(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !bytes.is_ascii() || !bytes.len().is_multiple_of(4) {
        return false;
    }
    if bytes.is_empty() {
        return true;
    }

    let padding = if bytes.ends_with(b"==") {
        2
    } else if bytes.ends_with(b"=") {
        1
    } else {
        0
    };
    let data_len = bytes.len() - padding;
    if !bytes[..data_len]
        .iter()
        .all(|byte| base64_value(*byte).is_some())
        || (padding == 1 && data_len % 4 != 3)
        || (padding == 2 && data_len % 4 != 2)
        || (padding == 0 && !data_len.is_multiple_of(4))
    {
        return false;
    }

    if padding > 0 {
        let Some(last_value) = base64_value(bytes[data_len - 1]) else {
            return false;
        };
        let unused_bits = if padding == 1 { 2 } else { 4 };
        if last_value & ((1 << unused_bits) - 1) != 0 {
            return false;
        }
    }

    let decoded_len = (bytes.len() / 4) * 3 - padding;
    decoded_len <= MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES
}

const fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn validate_session_value(value: &Value) -> Result<(), ProviderContractError> {
    if value.is_null()
        || serde_json::to_vec(value)
            .map_or(true, |bytes| bytes.len() > MAX_PROVIDER_SESSION_VALUE_BYTES)
    {
        Err(ProviderContractError::InvalidPayload)
    } else {
        Ok(())
    }
}

fn validate_provider_path(path: &str) -> Result<(), ProviderContractError> {
    let path_only = path.split_once('?').map_or(path, |(path, _)| path);
    if path.is_empty()
        || path.len() > 2048
        || !path_only.starts_with('/')
        || path_only.starts_with("//")
        || path_only.contains(['#', '\\', '%'])
        || path_only.chars().any(char::is_control)
        || path_only
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(ProviderContractError::InvalidPath);
    }
    Ok(())
}

fn validate_session_headers(
    headers: &BTreeMap<String, String>,
    allow_endpoint_auth: bool,
) -> Result<(), ProviderContractError> {
    if headers.len() > 64 {
        return Err(ProviderContractError::InvalidLimit);
    }
    for (name, value) in headers {
        let valid_name = !name.is_empty()
            && name.len() <= 128
            && name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            });
        if !valid_name
            || value.len() > 8192
            || !value.is_ascii()
            || value.bytes().any(|byte| byte < b' ' || byte == 0x7f)
            || (!allow_endpoint_auth
                && matches!(
                    name.to_ascii_lowercase().as_str(),
                    "authorization" | "cookie"
                ))
        {
            return Err(ProviderContractError::InvalidPayload);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderContractError {
    #[error("provider model is invalid")]
    InvalidModel,
    #[error("provider payload is invalid")]
    InvalidPayload,
    #[error("provider limit is invalid")]
    InvalidLimit,
    #[error("provider delay is invalid")]
    InvalidDelay,
    #[error("provider reason is invalid")]
    InvalidReason,
    #[error("provider method is invalid")]
    InvalidMethod,
    #[error("provider path is invalid")]
    InvalidPath,
}

fn default_provider_page_size() -> u16 {
    100
}

fn validate_optional_text(value: Option<&str>, max: usize) -> Result<(), ProviderContractError> {
    if let Some(value) = value {
        validate_text(value, max, ProviderContractError::InvalidReason)?;
    }
    Ok(())
}

fn validate_text(
    value: &str,
    max: usize,
    error: ProviderContractError,
) -> Result<(), ProviderContractError> {
    if value.is_empty()
        || value.len() > max
        || value.trim() != value
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_operations_reject_invalid_limits_and_payloads() {
        assert!(
            PluginProviderOperation::Discover {
                query: None,
                cursor: None,
                limit: 0,
            }
            .validate()
            .is_err()
        );
        assert!(
            PluginProviderOperation::Request {
                model: "gpt-test".to_owned(),
                request: Value::Null,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn provider_results_keep_requests_relative_and_models_bounded() {
        let result = PluginProviderResult::Request {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            endpoint_id: None,
            headers: BTreeMap::new(),
            body: serde_json::json!({"model":"gpt-test"}),
        };
        assert!(result.validate().is_ok());
        let endpoint_auth = PluginProviderResult::Request {
            method: "POST".to_owned(),
            path: "/app/orgs/org/jams/delete".to_owned(),
            endpoint_id: Some("capy-api".to_owned()),
            headers: BTreeMap::from([
                ("authorization".to_owned(), "Bearer jwt".to_owned()),
                ("cookie".to_owned(), "__client=session".to_owned()),
            ]),
            body: serde_json::json!({"jamIds":["child"]}),
        };
        assert!(endpoint_auth.validate().is_ok());
        let unscoped_auth = PluginProviderResult::Request {
            method: "POST".to_owned(),
            path: "/app/orgs/org/jams/delete".to_owned(),
            endpoint_id: None,
            headers: BTreeMap::from([
                ("authorization".to_owned(), "Bearer jwt".to_owned()),
                ("cookie".to_owned(), "__client=session".to_owned()),
            ]),
            body: serde_json::json!({"jamIds":["child"]}),
        };
        assert_eq!(
            unscoped_auth.validate(),
            Err(ProviderContractError::InvalidPayload)
        );
        let absolute = PluginProviderResult::Request {
            method: "POST".to_owned(),
            path: "https://example.invalid".to_owned(),
            endpoint_id: None,
            headers: BTreeMap::new(),
            body: serde_json::json!({"model":"gpt-test"}),
        };
        assert_eq!(absolute.validate(), Err(ProviderContractError::InvalidPath));
    }

    #[test]
    fn provider_sessions_reject_unbounded_or_absolute_actions() {
        let valid = PluginProviderSessionAction::Request {
            method: "GET".to_owned(),
            path: "/v1/tasks/status?cursor=2".to_owned(),
            endpoint_id: Some("provider-api".to_owned()),
            headers: BTreeMap::new(),
            body: serde_json::json!({}),
            stream: false,
        };
        assert!(valid.validate().is_ok());

        for path in [
            "https://upstream.example/v1/tasks",
            "//other.example/v1/tasks",
            "/v1/tasks#fragment",
            "/v1\\tasks",
        ] {
            let action = PluginProviderSessionAction::Request {
                method: "GET".to_owned(),
                path: path.to_owned(),
                endpoint_id: Some("provider-api".to_owned()),
                headers: BTreeMap::new(),
                body: serde_json::json!({}),
                stream: false,
            };
            assert_eq!(action.validate(), Err(ProviderContractError::InvalidPath));
        }

        let streaming = PluginProviderSessionAction::Request {
            method: "GET".to_owned(),
            path: "/v1/events".to_owned(),
            endpoint_id: Some("provider-api".to_owned()),
            headers: BTreeMap::new(),
            body: serde_json::json!({}),
            stream: true,
        };
        assert_eq!(
            streaming.validate(),
            Err(ProviderContractError::InvalidPayload)
        );
    }

    #[test]
    fn provider_session_state_and_response_are_size_bounded() {
        let too_large = Value::String("x".repeat(MAX_PROVIDER_SESSION_VALUE_BYTES + 1));
        let start = PluginProviderOperation::SessionStart {
            model: "vendor-model".to_owned(),
            request: too_large.clone(),
        };
        assert_eq!(start.validate(), Err(ProviderContractError::InvalidPayload));

        let result = PluginProviderResult::Session {
            session_id: "thread-1".to_owned(),
            state: serde_json::json!({}),
            action: PluginProviderSessionAction::Complete {
                response: too_large,
            },
        };
        assert_eq!(
            result.validate(),
            Err(ProviderContractError::InvalidPayload)
        );
    }

    #[test]
    fn provider_websocket_actions_are_bounded_and_use_named_endpoints() {
        assert!(
            PluginProviderSessionAction::WebSocketConnect {
                endpoint_id: "capy-realtime".to_owned(),
                path: "/realtime".to_owned(),
                headers: BTreeMap::new(),
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketConnect {
                endpoint_id: "../private".to_owned(),
                path: "/realtime".to_owned(),
                headers: BTreeMap::new(),
            }
            .validate()
            .is_err()
        );
        assert!(
            PluginProviderSessionAction::WebSocketReceive { timeout_ms: 30_001 }
                .validate()
                .is_err()
        );
        assert!(
            PluginProviderSessionAction::WebSocketReceiveBatch {
                timeout_ms: 2_000,
                max_messages: MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES,
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketReceiveBatch {
                timeout_ms: 2_000,
                max_messages: MAX_PROVIDER_SESSION_WEBSOCKET_BATCH_FRAMES + 1,
            }
            .validate()
            .is_err()
        );
        assert!(
            PluginProviderSessionAction::WebSocketAuthenticate {
                request: serde_json::json!({"challenge":{"id":"nonce"}}),
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketAuthenticate {
                request: serde_json::json!({"endpoint_id":"provider-api"}),
            }
            .validate()
            .is_err()
        );
        assert!(
            PluginProviderSessionAction::WebSocketSendText {
                text: "hello".to_owned()
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketSendBinary {
                data_base64: "aGVsbG8=".to_owned()
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketSendBinary {
                data_base64: "%%%".to_owned()
            }
            .validate()
            .is_err()
        );
        for invalid_base64 in ["a===", "YR==", "YWJ="] {
            assert!(
                PluginProviderSessionAction::WebSocketSendBinary {
                    data_base64: invalid_base64.to_owned()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            PluginProviderSessionAction::WebSocketSendBinary {
                data_base64: "YWJj".to_owned()
            }
            .validate()
            .is_ok()
        );
        assert!(
            PluginProviderSessionAction::WebSocketSendText {
                text: "x".repeat(MAX_PROVIDER_SESSION_WEBSOCKET_FRAME_BYTES + 1)
            }
            .validate()
            .is_err()
        );
    }
}
