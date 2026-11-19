use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability used by adapters that own a remote credential lifecycle.
pub const PLUGIN_CREDENTIAL_LIFECYCLE_CAPABILITY: &str = "credential.lifecycle";

/// Host-controlled credential lifecycle operation. Secrets are never included
/// in the operation; the host binds the credential id and secret envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginCredentialOperation {
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
        body: Option<Value>,
    },
    Balance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginCredentialResult {
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
    Balance {
        amount: String,
        currency: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of_unix_ms: Option<u64>,
    },
}

impl PluginCredentialOperation {
    pub fn validate(&self) -> Result<(), CredentialContractError> {
        match self {
            Self::Login {
                authorization_code,
                redirect_uri,
            } => {
                validate_optional_text(authorization_code.as_deref(), 2048)?;
                validate_optional_text(redirect_uri.as_deref(), 2048)?;
            }
            Self::Refresh | Self::Revoke | Self::Balance => {}
            Self::Sign { method, path, body } => {
                validate_text(method, 16, CredentialContractError::InvalidMethod)?;
                validate_text(path, 2048, CredentialContractError::InvalidPath)?;
                if !path.starts_with('/') || body.as_ref().is_some_and(Value::is_null) {
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
            Self::Login {
                authorization_url,
                state,
                ..
            } => {
                validate_optional_text(authorization_url.as_deref(), 4096)?;
                validate_optional_text(state.as_deref(), 512)?;
            }
            Self::Refreshed { .. } | Self::Revoked => {}
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
                    validate_text(value, 8192, CredentialContractError::InvalidHeader)?;
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
        "authorization" | "host" | "cookie" | "content-length" | "proxy-authorization"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_signing_is_relative_and_bounded() {
        let operation = PluginCredentialOperation::Sign {
            method: "POST".to_owned(),
            path: "/v1/request".to_owned(),
            body: Some(serde_json::json!({"ok": true})),
        };
        assert!(operation.validate().is_ok());
        assert!(
            PluginCredentialOperation::Sign {
                method: "GET".to_owned(),
                path: "https://example.invalid".to_owned(),
                body: None
            }
            .validate()
            .is_err()
        );
    }
}
