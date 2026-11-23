use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability used by adapters that own a remote credential lifecycle.
pub const PLUGIN_CREDENTIAL_LIFECYCLE_CAPABILITY: &str = "credential.lifecycle";
/// Maximum private authentication frames returned for one upstream handshake.
pub const MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAMES: usize = 8;
/// Maximum encoded authentication frame payload in bytes.
pub const MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES: usize = 16 * 1024;

/// Private material is sent only to the explicitly bound credential adapter.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginCredentialOperation {
    Bound {
        channel_id: i64,
        credential_id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        material: Option<Value>,
        operation: Box<PluginCredentialOperation>,
    },
    Login {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization_code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        redirect_uri: Option<String>,
    },
    Refresh,
    Revoke,
    Sign {
        method: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Value>,
    },
    WebSocketAuth {
        request: Value,
    },
    Balance,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginCredentialResult {
    /// Persist privately in the host vault, then expose only the inner result.
    Session {
        material: Value,
        result: Box<PluginCredentialResult>,
    },
    Login {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        complete: bool,
    },
    Refreshed {
        expires_at_unix_ms: Option<u64>,
    },
    Revoked,
    Signed {
        headers: std::collections::BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<std::collections::BTreeMap<String, String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Value>,
    },
    /// Private application-level authentication frame; consumed and sent by the host.
    WebSocketAuth {
        frames: Vec<PluginCredentialWebSocketFrame>,
    },
    Balance {
        amount: String,
        currency: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of_unix_ms: Option<u64>,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginCredentialWebSocketFrame {
    Text { text: String },
    Binary { data_base64: String },
}

impl PluginCredentialOperation {
    pub fn validate(&self) -> Result<(), CredentialContractError> {
        match self {
            Self::Bound {
                channel_id,
                credential_id,
                material,
                operation,
            } => {
                if *channel_id <= 0
                    || *credential_id <= 0
                    || matches!(operation.as_ref(), Self::Bound { .. })
                    || material
                        .as_ref()
                        .is_some_and(|value| !valid_material(value))
                {
                    return Err(CredentialContractError::InvalidBody);
                }
                operation.validate()?;
            }
            Self::Login {
                authorization_code,
                redirect_uri,
            } => {
                validate_optional_text(authorization_code.as_deref(), 2048)?;
                validate_optional_text(redirect_uri.as_deref(), 2048)?;
            }
            Self::Refresh | Self::Revoke | Self::Balance => {}
            Self::WebSocketAuth { request } => {
                let Some(endpoint_id) = request.get("endpoint_id").and_then(Value::as_str) else {
                    return Err(CredentialContractError::InvalidBody);
                };
                let Some(path) = request.get("path").and_then(Value::as_str) else {
                    return Err(CredentialContractError::InvalidPath);
                };
                if !valid_endpoint_id(endpoint_id)
                    || !valid_relative_path(path)
                    || request
                        .get("challenge")
                        .is_none_or(|challenge| challenge.is_null())
                    || serde_json::to_vec(request).map_or(true, |bytes| bytes.len() > 16 * 1024)
                {
                    return Err(CredentialContractError::InvalidBody);
                }
            }
            Self::Sign {
                method,
                path,
                endpoint_id,
                body,
            } => {
                validate_text(method, 16, CredentialContractError::InvalidMethod)?;
                validate_text(path, 2048, CredentialContractError::InvalidPath)?;
                let (path_only, query) = path
                    .split_once('?')
                    .map_or((path.as_str(), None), |(path, query)| (path, Some(query)));
                if !valid_relative_path(path_only)
                    || query.is_some_and(|query| {
                        query.chars().any(|character| {
                            character == '#' || character == '\\' || character.is_control()
                        })
                    })
                    || body.as_ref().is_some_and(Value::is_null)
                    || endpoint_id
                        .as_deref()
                        .is_some_and(|value| !valid_endpoint_id(value))
                {
                    return Err(CredentialContractError::InvalidPath);
                }
            }
        }
        Ok(())
    }
}

impl PluginCredentialResult {
    pub fn validate(&self) -> Result<(), CredentialContractError> {
        match self {
            Self::Session { material, result } => {
                if !valid_material(material) || matches!(result.as_ref(), Self::Session { .. }) {
                    return Err(CredentialContractError::InvalidBody);
                }
                result.validate()?;
            }
            Self::Login {
                authorization_url,
                state,
                ..
            } => {
                validate_optional_text(authorization_url.as_deref(), 4096)?;
                validate_optional_text(state.as_deref(), 512)?;
            }
            Self::Refreshed { .. } | Self::Revoked => {}
            Self::WebSocketAuth { frames } => {
                if frames.is_empty() || frames.len() > MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAMES {
                    return Err(CredentialContractError::InvalidBody);
                }
                for frame in frames {
                    match frame {
                        PluginCredentialWebSocketFrame::Text { text } => {
                            if text.len() > MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES {
                                return Err(CredentialContractError::InvalidBody);
                            }
                        }
                        PluginCredentialWebSocketFrame::Binary { data_base64 } => {
                            if !valid_base64(data_base64)
                                || data_base64.len()
                                    > MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES.div_ceil(3) * 4
                            {
                                return Err(CredentialContractError::InvalidBody);
                            }
                        }
                    }
                }
            }
            Self::Signed {
                headers,
                query,
                body,
            } => {
                if headers.len() > 64 || query.as_ref().is_some_and(|items| items.len() > 64) {
                    return Err(CredentialContractError::TooManyFields);
                }
                for (name, value) in headers {
                    if protected_header(name) {
                        return Err(CredentialContractError::InvalidHeader);
                    }
                    validate_text(name, 128, CredentialContractError::InvalidHeader)?;
                    if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control)
                    {
                        return Err(CredentialContractError::InvalidHeader);
                    }
                }
                if let Some(query) = query {
                    for (name, value) in query {
                        validate_text(name, 128, CredentialContractError::InvalidHeader)?;
                        validate_text(value, 8192, CredentialContractError::InvalidHeader)?;
                    }
                }
                if body.as_ref().is_some_and(Value::is_null) {
                    return Err(CredentialContractError::InvalidBody);
                }
            }
            Self::Balance {
                amount, currency, ..
            } => {
                validate_text(amount, 128, CredentialContractError::InvalidAmount)?;
                validate_text(currency, 16, CredentialContractError::InvalidCurrency)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CredentialContractError {
    #[error("credential method is invalid")]
    InvalidMethod,
    #[error("credential path is invalid")]
    InvalidPath,
    #[error("credential header is invalid")]
    InvalidHeader,
    #[error("credential body is invalid")]
    InvalidBody,
    #[error("credential amount is invalid")]
    InvalidAmount,
    #[error("credential currency is invalid")]
    InvalidCurrency,
    #[error("credential field count is invalid")]
    TooManyFields,
}

fn validate_optional_text(value: Option<&str>, max: usize) -> Result<(), CredentialContractError> {
    if let Some(value) = value {
        validate_text(value, max, CredentialContractError::InvalidPath)?;
    }
    Ok(())
}

fn validate_text(
    value: &str,
    max: usize,
    error: CredentialContractError,
) -> Result<(), CredentialContractError> {
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

fn protected_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "cookie"
            | "content-length"
            | "proxy-authorization"
            | "connection"
            | "transfer-encoding"
            | "upgrade"
            | "trailer"
            | "te"
    )
}

fn valid_material(value: &Value) -> bool {
    value.is_object() && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 4096)
}

fn valid_endpoint_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2_048
        && value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains(['?', '#', '\\', '%'])
        && !value.chars().any(char::is_control)
        && !value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
}

fn valid_base64(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes.is_ascii() || !bytes.len().is_multiple_of(4) {
        return false;
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
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        || (padding == 1 && data_len % 4 != 3)
        || (padding == 2 && data_len % 4 != 2)
        || (padding == 0 && !data_len.is_multiple_of(4))
    {
        return false;
    }
    if padding > 0 {
        let last = bytes[data_len - 1];
        let value = match last {
            b'A'..=b'Z' => last - b'A',
            b'a'..=b'z' => last - b'a' + 26,
            b'0'..=b'9' => last - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return false,
        };
        let unused = if padding == 1 { 2 } else { 4 };
        if value & ((1 << unused) - 1) != 0 {
            return false;
        }
    }
    let decoded = (bytes.len() / 4) * 3 - padding;
    decoded <= MAX_PLUGIN_CREDENTIAL_WEBSOCKET_FRAME_BYTES
}

impl std::fmt::Debug for PluginCredentialOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginCredentialOperation(<redacted>)")
    }
}
impl std::fmt::Debug for PluginCredentialResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginCredentialResult(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_signing_is_relative_and_bounded() {
        let operation = PluginCredentialOperation::Sign {
            method: "POST".to_owned(),
            path: "/v1/request".to_owned(),
            endpoint_id: None,
            body: Some(serde_json::json!({"ok": true})),
        };
        assert!(operation.validate().is_ok());
        assert!(
            PluginCredentialOperation::Sign {
                method: "GET".to_owned(),
                path: "https://example.invalid".to_owned(),
                endpoint_id: None,
                body: None
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn websocket_auth_frames_are_bounded_and_redacted() {
        let result = PluginCredentialResult::WebSocketAuth {
            frames: vec![PluginCredentialWebSocketFrame::Text {
                text: "{\"token\":\"private\"}".to_owned(),
            }],
        };
        assert!(result.validate().is_ok());
        assert!(!format!("{result:?}").contains("private"));
        assert!(
            PluginCredentialResult::WebSocketAuth { frames: vec![] }
                .validate()
                .is_err()
        );
        assert!(
            PluginCredentialOperation::WebSocketAuth {
                request: serde_json::json!({
                    "endpoint_id": "realtime",
                    "path": "/events",
                    "challenge": {"nonce": "n"}
                }),
            }
            .validate()
            .is_ok()
        );
    }
}
