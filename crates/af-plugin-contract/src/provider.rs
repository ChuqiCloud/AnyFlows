use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability identifier used when a manifest opts into Provider lifecycle hooks.
pub const PROVIDER_LIFECYCLE_CAPABILITY: &str = "provider.lifecycle";
/// Maximum JSON payload carried by one provider session request, state, or response.
pub const MAX_PROVIDER_SESSION_VALUE_BYTES: usize = 256 * 1024;

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
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        body: Value,
        #[serde(default)]
        stream: bool,
    },
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
                headers,
                body,
            } => {
                validate_text(method, 16, ProviderContractError::InvalidMethod)?;
                validate_provider_path(path)?;
                validate_session_headers(headers)?;
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
                headers,
                body,
                stream,
            } => {
                validate_text(method, 16, ProviderContractError::InvalidMethod)?;
                validate_provider_path(path)?;
                validate_session_headers(headers)?;
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
            Self::Complete { response } => validate_session_value(response)?,
            Self::Abort { code } => validate_text(code, 128, ProviderContractError::InvalidReason)?,
        }
        Ok(())
    }
}

fn validate_session_text(value: &str) -> Result<(), ProviderContractError> {
    validate_text(value, 128, ProviderContractError::InvalidReason)
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
    if path.is_empty()
        || path.len() > 2048
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.bytes().any(|byte| matches!(byte, b'#' | b'\\'))
        || path.chars().any(char::is_control)
    {
        return Err(ProviderContractError::InvalidPath);
    }
    Ok(())
}

fn validate_session_headers(
    headers: &BTreeMap<String, String>,
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
            headers: BTreeMap::new(),
            body: serde_json::json!({"model":"gpt-test"}),
        };
        assert!(result.validate().is_ok());
        let absolute = PluginProviderResult::Request {
            method: "POST".to_owned(),
            path: "https://example.invalid".to_owned(),
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
                headers: BTreeMap::new(),
                body: serde_json::json!({}),
                stream: false,
            };
            assert_eq!(action.validate(), Err(ProviderContractError::InvalidPath));
        }

        let streaming = PluginProviderSessionAction::Request {
            method: "GET".to_owned(),
            path: "/v1/events".to_owned(),
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
}
