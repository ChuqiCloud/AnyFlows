use serde::{Deserialize, Serialize};

use crate::{ConnectorError, ContractValidationError};

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

/// Request frame for the language-neutral newline-delimited sidecar protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHostRequest {
    pub protocol_version: u16,
    pub plugin_id: crate::PluginId,
    pub plugin_version: crate::PluginVersion,
    pub artifact_sha256: String,
    pub deployment_generation: u64,
    pub request_id: String,
    pub deadline_unix_ms: u64,
    pub operation: PluginHostOperation,
}

impl PluginHostRequest {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.protocol_version != 1
            || self.artifact_sha256.len() != 64
            || !self
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.deployment_generation == 0
            || self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.request_id.trim() != self.request_id
            || self.deadline_unix_ms == 0
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// Supported sidecar operations. These DTOs have no credential, HTTP-client,
/// database, or wallet authority fields; host policy must validate JSON values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginHostOperation {
    Manifest,
    Health,
    InvokeCapability {
        #[serde(rename = "connectionId", alias = "connection_id")]
        connection_id: String,
        #[serde(rename = "publicConfig", alias = "public_config")]
        public_config: serde_json::Value,
        #[serde(rename = "capabilityId", alias = "capability_id")]
        capability_id: String,
        #[serde(rename = "capabilityVersion", alias = "capability_version")]
        capability_version: u16,
        input: serde_json::Value,
    },
}

/// Response frame for the language-neutral newline-delimited sidecar protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginHostResponse {
    pub protocol_version: u16,
    pub plugin_id: crate::PluginId,
    pub plugin_version: crate::PluginVersion,
    pub artifact_sha256: String,
    pub deployment_generation: u64,
    pub request_id: String,
    pub result: PluginHostResult,
}

/// Typed result of one sidecar operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PluginHostResult {
    Manifest(crate::PluginManifest),
    Health(ConnectorHealth),
    Capability(CapabilityResult),
    Error(ConnectorError),
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
    use super::{
        CapabilityInvocation, CapabilityResult, PluginHostOperation, PluginHostRequest,
        PluginHostResponse, PluginHostResult,
    };

    fn catalogue_request() -> PluginHostRequest {
        PluginHostRequest {
            protocol_version: 1,
            plugin_id: crate::PluginId::new("org.anyflows.example-catalog").unwrap(),
            plugin_version: crate::PluginVersion::new("1.0.0").unwrap(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 7,
            request_id: "wire-test-1".to_owned(),
            deadline_unix_ms: 2000,
            operation: PluginHostOperation::InvokeCapability {
                connection_id: "catalogue".to_owned(),
                public_config: serde_json::json!({}),
                capability_id: "catalog.models.read".to_owned(),
                capability_version: 1,
                input: serde_json::json!({"query":null,"cursor":null,"limit":20}),
            },
        }
    }

    #[test]
    fn sidecar_invocation_uses_documented_camel_case_and_reads_legacy_fields() {
        let request = catalogue_request();
        let mut wire = serde_json::to_value(&request).unwrap();
        assert_eq!(
            wire["operation"],
            serde_json::json!({
                "operation":"invoke_capability", "connectionId":"catalogue", "publicConfig":{},
                "capabilityId":"catalog.models.read", "capabilityVersion":1,
                "input":{"query":null,"cursor":null,"limit":20}
            })
        );
        let operation = wire["operation"].as_object_mut().unwrap();
        for (current, legacy) in [
            ("connectionId", "connection_id"),
            ("publicConfig", "public_config"),
            ("capabilityId", "capability_id"),
            ("capabilityVersion", "capability_version"),
        ] {
            let value = operation.remove(current).unwrap();
            operation.insert(legacy.to_owned(), value);
        }
        assert_eq!(
            serde_json::from_value::<PluginHostRequest>(wire).unwrap(),
            request
        );
    }

    #[test]
    fn real_host_frame_round_trips_through_the_python_sidecar_sdk() {
        use std::{
            io::Write as _,
            process::{Command, Stdio},
        };

        let sdk = include_str!("../../../tools/plugins/sdk/python/anyflows_plugin.py");
        let manifest = include_str!("../../../tools/plugins/examples/catalog/manifest.json");
        let code = format!(
            "{sdk}\nmanifest = json.loads(sys.argv[1])\nrequest = json.load(sys.stdin)\nplugin = Plugin(manifest, lambda *args: {{'models': [], 'nextCursor': None}}, clock_ms=lambda: 1000)\nprint(json.dumps(plugin.respond(request), allow_nan=False))\n"
        );
        let python = if cfg!(windows) { "python" } else { "python3" };
        let mut child = Command::new(python)
            .args(["-c", &code, manifest])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Python 3 is required for the cross-language contract check");
        let request = catalogue_request();
        {
            let mut input = child.stdin.take().unwrap();
            input
                .write_all(&serde_json::to_vec(&request).unwrap())
                .unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "Python SDK failed to decode the Rust host frame"
        );
        let response: PluginHostResponse = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response.request_id, request.request_id);
        assert_eq!(
            response.deployment_generation,
            request.deployment_generation
        );
        assert_eq!(response.artifact_sha256, request.artifact_sha256);
        assert_eq!(
            response.result,
            PluginHostResult::Capability(CapabilityResult {
                output: serde_json::json!({"models":[],"nextCursor":null})
            })
        );
    }

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
