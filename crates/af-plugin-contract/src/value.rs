use serde::{Deserialize, Serialize};

use crate::ContractValidationError;

/// Correlation and time budget supplied for one host-to-connector invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvocationMetadata {
    pub request_id: String,
    pub deadline_unix_ms: u64,
}

/// Versioned input envelope for one generic plugin capability invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityInvocation {
    pub capability_version: u16,
    pub input: serde_json::Value,
}

impl CapabilityInvocation {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.capability_version == 0 {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// JSON result envelope returned by a generic plugin capability.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityResult {
    pub output: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorHealthStatus {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorHealth {
    pub status: ConnectorHealthStatus,
    pub checked_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl ConnectorHealth {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.checked_at_unix_ms == 0
            || self
                .error_code
                .as_ref()
                .is_some_and(|code| code.is_empty() || code.len() > 64)
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

impl InvocationMetadata {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.request_id.trim() != self.request_id
            || self.deadline_unix_ms == 0
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// Platform-neutral external account identity; tokens and passwords are excluded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountProfile {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl ExternalAccountProfile {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.subject.is_empty()
            || self.subject.len() > 512
            || self.subject.trim() != self.subject
            || self.subject.chars().any(char::is_control)
            || self
                .display_name
                .as_ref()
                .is_some_and(|name| name.len() > 256 || name.chars().any(char::is_control))
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// Point-in-time external balance in its native unit, without currency conversion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalBalanceSnapshot {
    /// Exact non-negative decimal representation returned by the platform.
    pub amount: String,
    /// Platform-native unit, such as a currency code or quota unit identifier.
    pub unit: String,
    pub observed_at_unix_ms: u64,
}

impl ExternalBalanceSnapshot {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        let (whole, fractional) = self
            .amount
            .split_once('.')
            .map_or((self.amount.as_str(), None), |(whole, fractional)| {
                (whole, Some(fractional))
            });
        let valid_whole = whole == "0"
            || (!whole.is_empty()
                && !whole.starts_with('0')
                && whole.bytes().all(|byte| byte.is_ascii_digit()));
        if self.amount.len() > 128
            || !valid_whole
            || fractional.is_some_and(|fractional| {
                fractional.is_empty() || !fractional.bytes().all(|byte| byte.is_ascii_digit())
            })
            || self.unit.is_empty()
            || self.unit.len() > 64
            || self.unit.trim() != self.unit
            || self.unit.chars().any(char::is_control)
            || self.observed_at_unix_ms == 0
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityInvocation, CapabilityResult};

    #[test]
    fn capability_wire_envelopes_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<CapabilityInvocation>(serde_json::json!({
                "capabilityVersion": 1,
                "input": {},
                "unexpected": true
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CapabilityResult>(serde_json::json!({
                "output": {},
                "unexpected": true
            }))
            .is_err()
        );
    }
}
