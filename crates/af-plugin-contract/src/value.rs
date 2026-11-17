use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ConnectorError, ContractValidationError};
use crate::{PluginTaskOperation, PluginTaskResult};

/// Capability used by a plugin that proposes bounded Provider request changes.
pub const PROVIDER_REQUEST_TRANSFORM_CAPABILITY: &str = "provider.request.transform";

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

/// A host-projected provider request passed to a request-transform plugin.
///
/// The host must remove credentials and other protected headers before it
/// serializes this value. `body` is the client/provider JSON body before the
/// provider request is sent; it is never a live host object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequestTransformInput {
    pub protocol: String,
    pub method: String,
    pub path: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Value,
}

impl ProviderRequestTransformInput {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        if self.protocol.is_empty()
            || self.protocol.len() > 64
            || self
                .protocol
                .bytes()
                .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
            || !METHODS.contains(&self.method.as_str())
            || !valid_provider_path(&self.path)
            || self.model.is_empty()
            || self.model.len() > 256
            || self.model.chars().any(char::is_control)
            || self.upstream_model.as_ref().is_some_and(|model| {
                model.is_empty() || model.len() > 256 || model.chars().any(char::is_control)
            })
            || self.headers.len() > 64
            || self.headers.iter().any(|(name, value)| {
                !valid_header_name(name)
                    || value.len() > 4096
                    || value.bytes().any(|byte| byte.is_ascii_control())
                    || is_protected_header(name)
            })
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// Whether a provider transform should continue the request or return a
/// bounded plugin-authored rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTransformAction {
    Continue,
    Reject,
}

/// Header changes a provider transform may request. Authentication and
/// transport headers are deliberately rejected by `validate_against`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHeaderPatch {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Public error returned when a transform rejects a request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderTransformRejection {
    pub code: String,
    pub message: String,
}

/// Result of a `provider.request.transform` invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequestTransformOutput {
    pub action: ProviderTransformAction,
    /// Relative upstream path. The host keeps the configured upstream host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Complete replacement JSON body. Protected billing fields must be
    /// preserved byte-for-byte as JSON values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub headers: ProviderHeaderPatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection: Option<ProviderTransformRejection>,
}

impl ProviderRequestTransformOutput {
    /// Validate the output against the original host projection. This is the
    /// final host-side guard against changing billing, authentication or
    /// transport semantics.
    pub fn validate_against(
        &self,
        input: &ProviderRequestTransformInput,
    ) -> Result<(), ContractValidationError> {
        if let Some(path) = &self.path {
            if !valid_provider_path(path) {
                return Err(ContractValidationError::InvalidExternalValue);
            }
        }
        if self.headers.set.len() > 32
            || self.headers.remove.len() > 32
            || self.headers.set.iter().any(|(name, value)| {
                !valid_header_name(name)
                    || value.len() > 4096
                    || value.bytes().any(|byte| byte.is_ascii_control())
                    || is_protected_header(name)
            })
            || self
                .headers
                .remove
                .iter()
                .any(|name| !valid_header_name(name) || is_protected_header(name))
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        let protected = [
            "model",
            "stream",
            "stream_options",
            "n",
            "size",
            "quality",
            "resolution",
            "seconds",
            "duration",
            "service_tier",
        ];
        if let Some(body) = &self.body {
            if let (Some(input_object), Some(body_object)) =
                (input.body.as_object(), body.as_object())
            {
                for key in protected {
                    if input_object.get(key) != body_object.get(key) {
                        return Err(ContractValidationError::InvalidExternalValue);
                    }
                }
            } else if body != &input.body {
                return Err(ContractValidationError::InvalidExternalValue);
            }
        }
        match (self.action, self.rejection.as_ref()) {
            (ProviderTransformAction::Continue, None) => Ok(()),
            (ProviderTransformAction::Continue, Some(_)) => {
                Err(ContractValidationError::InvalidExternalValue)
            }
            (ProviderTransformAction::Reject, Some(rejection))
                if self.path.is_none()
                    && self.body.is_none()
                    && self.headers.set.is_empty()
                    && self.headers.remove.is_empty()
                    && valid_rejection(&rejection.code, &rejection.message) =>
            {
                Ok(())
            }
            (ProviderTransformAction::Reject, _) => {
                Err(ContractValidationError::InvalidExternalValue)
            }
        }
    }
}

fn valid_provider_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2048
        && value.starts_with('/')
        && !value.starts_with("//")
        && !value
            .bytes()
            .any(|byte| matches!(byte, b'?' | b'#' | b'\\'))
        && !value.split('/').any(|segment| segment == "..")
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_protected_header(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "host"
            | "content-length"
            | "content-type"
            | "content-encoding"
            | "transfer-encoding"
            | "x-api-key"
            | "x-goog-api-key"
            | "mj-api-secret"
    )
}

fn valid_rejection(code: &str, message: &str) -> bool {
    !code.is_empty()
        && code.len() <= 128
        && code.as_bytes()[0].is_ascii_lowercase()
        && code.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'_'
                || byte == b'.'
                || byte == b'-'
        })
        && !message.is_empty()
        && message.len() <= 512
        && !message.chars().any(char::is_control)
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
        if let PluginHostOperation::InvokeTask {
            connection_id,
            public_config,
            capability_id,
            capability_version,
            task,
        } = &self.operation
        {
            if connection_id.is_empty()
                || connection_id.len() > 128
                || !public_config.is_object()
                || capability_id.is_empty()
                || *capability_version == 0
            {
                return Err(ContractValidationError::InvalidExternalValue);
            }
            task.validate()
                .map_err(|_| ContractValidationError::InvalidExternalValue)?;
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
    /// Invoke the task adapter without granting it task ids, billing or
    /// credential authority. The host owns all lifecycle state around it.
    InvokeTask {
        #[serde(rename = "connectionId", alias = "connection_id")]
        connection_id: String,
        #[serde(rename = "publicConfig", alias = "public_config")]
        public_config: serde_json::Value,
        #[serde(rename = "capabilityId", alias = "capability_id")]
        capability_id: String,
        #[serde(rename = "capabilityVersion", alias = "capability_version")]
        capability_version: u16,
        task: PluginTaskOperation,
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
    Task(PluginTaskResult),
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
    use std::collections::BTreeMap;

    use super::{
        CapabilityInvocation, CapabilityResult, PluginHostOperation, PluginHostRequest,
        PluginHostResponse, PluginHostResult, ProviderHeaderPatch, ProviderRequestTransformInput,
        ProviderRequestTransformOutput, ProviderTransformAction, ProviderTransformRejection,
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

    #[test]
    fn task_operation_uses_an_explicit_wire_kind() {
        let operation = PluginHostOperation::InvokeTask {
            connection_id: "task-adapter".to_owned(),
            public_config: serde_json::json!({}),
            capability_id: "task.video.adapter".to_owned(),
            capability_version: 1,
            task: crate::PluginTaskOperation::Status {
                task_id: "provider-1".to_owned(),
            },
        };
        let value = serde_json::to_value(operation).unwrap();
        assert_eq!(value["operation"]["operation"], "invoke_task");
        assert_eq!(value["operation"]["task"]["kind"], "status");
        assert_eq!(value["operation"]["task"]["task_id"], "provider-1");
    }

    #[test]
    fn provider_transform_rejects_protected_headers_and_billing_changes() {
        let input = ProviderRequestTransformInput {
            protocol: "openai".to_owned(),
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            model: "gpt-6.1-sol".to_owned(),
            upstream_model: Some("gpt-6.1-sol".to_owned()),
            headers: BTreeMap::from([("accept".to_owned(), "application/json".to_owned())]),
            body: serde_json::json!({"model":"gpt-6.1-sol", "stream":false, "messages":[]}),
        };
        input.validate().unwrap();
        let mut output = ProviderRequestTransformOutput {
            action: ProviderTransformAction::Continue,
            path: Some("/v1/chat/completions".to_owned()),
            body: Some(serde_json::json!({"model":"other", "stream":false, "messages":[]})),
            headers: ProviderHeaderPatch::default(),
            rejection: None,
        };
        assert!(output.validate_against(&input).is_err());
        output.body = None;
        output
            .headers
            .set
            .insert("Authorization".to_owned(), "bad".to_owned());
        assert!(output.validate_against(&input).is_err());
    }

    #[test]
    fn provider_transform_rejection_is_bounded_and_explicit() {
        let input = ProviderRequestTransformInput {
            protocol: "openai".to_owned(),
            method: "POST".to_owned(),
            path: "/v1/responses".to_owned(),
            model: "gpt-6.1-sol".to_owned(),
            upstream_model: None,
            headers: BTreeMap::new(),
            body: serde_json::json!({"model":"gpt-6.1-sol"}),
        };
        let output = ProviderRequestTransformOutput {
            action: ProviderTransformAction::Reject,
            path: None,
            body: None,
            headers: ProviderHeaderPatch::default(),
            rejection: Some(ProviderTransformRejection {
                code: "provider.policy_blocked".to_owned(),
                message: "该请求被 Provider 策略拒绝".to_owned(),
            }),
        };
        assert!(output.validate_against(&input).is_ok());
    }
}
