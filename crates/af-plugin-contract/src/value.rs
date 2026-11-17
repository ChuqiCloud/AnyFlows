use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ConnectorError, ContractValidationError, PluginTaskOperation, PluginTaskResult};

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
            || !self
                .protocol
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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
                !valid_transform_header_name(name)
                    || value.len() > 4096
                    || value.bytes().any(|byte| byte.is_ascii_control())
                    || protected_transform_header(name)
            })
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTransformAction {
    Continue,
    Reject,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderHeaderPatch {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderTransformRejection {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequestTransformOutput {
    pub action: ProviderTransformAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub headers: ProviderHeaderPatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection: Option<ProviderTransformRejection>,
}

impl ProviderRequestTransformOutput {
    pub fn validate_against(
        &self,
        input: &ProviderRequestTransformInput,
    ) -> Result<(), ContractValidationError> {
        if self
            .path
            .as_ref()
            .is_some_and(|path| !valid_provider_path(path))
            || self.headers.set.len() > 32
            || self.headers.remove.len() > 32
            || self.headers.set.iter().any(|(name, value)| {
                !valid_transform_header_name(name)
                    || value.len() > 4096
                    || value.bytes().any(|byte| byte.is_ascii_control())
                    || protected_transform_header(name)
            })
            || self
                .headers
                .remove
                .iter()
                .any(|name| !valid_transform_header_name(name) || protected_transform_header(name))
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
            if let (Some(before), Some(after)) = (input.body.as_object(), body.as_object()) {
                if protected
                    .iter()
                    .any(|key| before.get(*key) != after.get(*key))
                {
                    return Err(ContractValidationError::InvalidExternalValue);
                }
            } else if body != &input.body {
                return Err(ContractValidationError::InvalidExternalValue);
            }
        }
        match (&self.action, &self.rejection) {
            (ProviderTransformAction::Continue, None) => Ok(()),
            (ProviderTransformAction::Reject, Some(rejection))
                if self.path.is_none()
                    && self.body.is_none()
                    && self.headers.set.is_empty()
                    && self.headers.remove.is_empty()
                    && valid_transform_rejection(rejection) =>
            {
                Ok(())
            }
            _ => Err(ContractValidationError::InvalidExternalValue),
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
            .any(|byte| matches!(byte, b'?' | b'#' | b'\\') || byte.is_ascii_control())
        && !value.split('/').any(|segment| segment == "..")
}
fn valid_transform_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn protected_transform_header(value: &str) -> bool {
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
fn valid_transform_rejection(value: &ProviderTransformRejection) -> bool {
    !value.code.is_empty()
        && value.code.len() <= 128
        && value.code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && !value.message.is_empty()
        && value.message.len() <= 512
        && !value.message.chars().any(char::is_control)
}

/// The only request shape a future host-managed external HTTP proxy may accept.
///
/// This proposal contains no URL, headers, credential, or body. `endpoint_id`
/// must resolve through a host-owned, generation-bound allowlist; query keys are
/// data only and must be independently validated by that endpoint's policy.
/// Defining this value does not enable proxy access by itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostHttpRequestProposal {
    /// Host-registered endpoint identifier, never an address or path.
    pub endpoint_id: String,
    pub method: HostHttpMethod,
    /// Bounded, non-sensitive query fields. The host applies the endpoint's
    /// exact key allowlist before URL encoding.
    #[serde(default)]
    pub query: BTreeMap<String, String>,
}

impl HostHttpRequestProposal {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if !valid_endpoint_id(&self.endpoint_id)
            || self.query.len() > 32
            || self.query.iter().any(|(key, value)| {
                !valid_query_key(key) || value.len() > 512 || value.chars().any(char::is_control)
            })
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// Safe methods permitted by the host-managed external read proxy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HostHttpMethod {
    Get,
    Head,
}

/// A bounded JSON projection returned by a future host proxy continuation.
///
/// The host must project the upstream response using its endpoint schema before
/// constructing this value. Raw response bytes, headers, cookies, and credential
/// material are intentionally absent. Defining this value does not enable proxy
/// access by itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostHttpResponseProjection {
    pub status_code: u16,
    pub payload: serde_json::Value,
}

impl HostHttpResponseProjection {
    pub fn validate(&self, max_payload_bytes: usize) -> Result<(), ContractValidationError> {
        if !(200..=299).contains(&self.status_code)
            || serde_json::to_vec(&self.payload)
                .map_err(|_| ContractValidationError::InvalidExternalValue)?
                .len()
                > max_payload_bytes
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

/// A nested sidecar-to-host proxy callback bound to one in-flight invocation.
///
/// The continuation has no target URL or authorization header. The host resolves
/// `endpoint_id` from the installed manifest, checks the granted capability and
/// invocation generation again, injects the linked account credential in memory,
/// applies the endpoint's query allowlist and returns only a JSON projection.
/// Current Sidecar protocol handlers do not yet execute this continuation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProxyRequest {
    pub protocol_version: u16,
    pub plugin_id: crate::PluginId,
    pub plugin_version: crate::PluginVersion,
    pub artifact_sha256: String,
    pub deployment_generation: u64,
    /// The parent capability invocation's request ID.
    pub request_id: String,
    /// Capability grant that authorizes this exact external read operation.
    pub capability_id: crate::CapabilityId,
    pub capability_version: u16,
    /// Unique request/response correlation ID within that invocation.
    pub proxy_request_id: String,
    pub request: HostHttpRequestProposal,
}

impl PluginProxyRequest {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.protocol_version != 1
            || !valid_digest(&self.artifact_sha256)
            || self.deployment_generation == 0
            || !valid_wire_token(&self.request_id, 128)
            || self.capability_version == 0
            || !matches!(
                self.capability_id.as_str(),
                "external.account.profile.read" | "external.account.balance.read"
            )
            || !valid_wire_token(&self.proxy_request_id, 64)
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        self.request.validate()
    }
}

/// Host continuation result for one authenticated proxy callback.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProxyResponse {
    pub protocol_version: u16,
    pub plugin_id: crate::PluginId,
    pub plugin_version: crate::PluginVersion,
    pub artifact_sha256: String,
    pub deployment_generation: u64,
    pub request_id: String,
    pub capability_id: crate::CapabilityId,
    pub capability_version: u16,
    pub proxy_request_id: String,
    pub result: PluginProxyResult,
}

impl PluginProxyResponse {
    pub fn validate(&self, max_payload_bytes: usize) -> Result<(), ContractValidationError> {
        if self.protocol_version != 1
            || !valid_digest(&self.artifact_sha256)
            || self.deployment_generation == 0
            || !valid_wire_token(&self.request_id, 128)
            || self.capability_version == 0
            || !matches!(
                self.capability_id.as_str(),
                "external.account.profile.read" | "external.account.balance.read"
            )
            || !valid_wire_token(&self.proxy_request_id, 64)
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        if let PluginProxyResult::Response(projection) = &self.result {
            projection.validate(max_payload_bytes)?;
        }
        Ok(())
    }

    /// Check that a continuation belongs to the exact in-flight request.
    #[must_use]
    pub fn matches_request(&self, request: &PluginProxyRequest) -> bool {
        self.protocol_version == request.protocol_version
            && self.plugin_id == request.plugin_id
            && self.plugin_version == request.plugin_version
            && self.artifact_sha256 == request.artifact_sha256
            && self.deployment_generation == request.deployment_generation
            && self.request_id == request.request_id
            && self.capability_id == request.capability_id
            && self.capability_version == request.capability_version
            && self.proxy_request_id == request.proxy_request_id
    }
}

/// Either a sanitized success projection or a fixed error classification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PluginProxyResult {
    Response(HostHttpResponseProjection),
    Error(PluginProxyError),
}

/// Fixed proxy error classification; upstream body and headers are never
/// reflected through the continuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginProxyErrorCode {
    AuthenticationRequired,
    Forbidden,
    RateLimited,
    TemporarilyUnavailable,
    Unsupported,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProxyError {
    pub code: PluginProxyErrorCode,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_wire_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_endpoint_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_query_key(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return false;
    }
    let mut normalized = String::with_capacity(value.len());
    let characters = value.chars().collect::<Vec<_>>();
    for (index, character) in characters.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|index| characters.get(index));
        let next = characters.get(index + 1);
        let camel_case_boundary = character.is_ascii_uppercase()
            && previous.is_some_and(|previous| {
                previous.is_ascii_lowercase()
                    || (previous.is_ascii_uppercase() && next.is_some_and(char::is_ascii_lowercase))
            });
        if camel_case_boundary {
            normalized.push('_');
        }
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
        } else {
            normalized.push('_');
        }
    }
    let compact = normalized.replace(['_', '-', '.'], "");
    !(compact.contains("apikey")
        || normalized.split(['_', '-', '.']).any(|part| {
            [
                "authorization",
                "auth",
                "code",
                "cookie",
                "credential",
                "key",
                "password",
                "secret",
                "signature",
                "token",
            ]
            .contains(&part)
        }))
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
            && (connection_id.is_empty()
                || connection_id.len() > 128
                || !public_config.is_object()
                || capability_id.is_empty()
                || *capability_version == 0
                || task.validate().is_err())
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
        CapabilityInvocation, CapabilityResult, HostHttpMethod, HostHttpRequestProposal,
        HostHttpResponseProjection, PluginHostOperation, PluginHostRequest, PluginHostResponse,
        PluginHostResult, PluginProxyError, PluginProxyErrorCode, PluginProxyRequest,
        PluginProxyResponse, PluginProxyResult,
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
    fn host_http_proxy_proposals_cannot_name_urls_or_carry_credentials() {
        let request = HostHttpRequestProposal {
            endpoint_id: "external.account.self.read".to_owned(),
            method: HostHttpMethod::Get,
            query: BTreeMap::from([("page".to_owned(), "1".to_owned())]),
        };
        request.validate().unwrap();
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["method"], "GET");
        assert!(encoded.get("url").is_none());
        assert!(encoded.get("headers").is_none());
        assert!(encoded.get("credential").is_none());

        let mut credential_query = request.clone();
        credential_query
            .query
            .insert("access_token".to_owned(), "must-not-pass".to_owned());
        assert!(credential_query.validate().is_err());
        for name in ["apikey", "api_key", "api-key", "api.key", "APIKey"] {
            let mut secret_query = request.clone();
            secret_query
                .query
                .insert(name.to_owned(), "must-not-pass".to_owned());
            assert!(
                secret_query.validate().is_err(),
                "accepted query key: {name}"
            );
        }
        let mut secretish_query = request.clone();
        secretish_query
            .query
            .insert("clientKeyId".to_owned(), "must-not-pass".to_owned());
        assert!(secretish_query.validate().is_err());

        let mut arbitrary_url = request.clone();
        arbitrary_url.endpoint_id = "https://attacker.example/private".to_owned();
        assert!(arbitrary_url.validate().is_err());
    }

    #[test]
    fn host_http_projection_is_correlated_and_bounded_json_only() {
        let response = HostHttpResponseProjection {
            status_code: 200,
            payload: serde_json::json!({ "subject": "external-42" }),
        };
        response.validate(128).unwrap();
        assert!(response.validate(1).is_err());
        assert!(
            HostHttpResponseProjection {
                status_code: 302,
                payload: serde_json::json!({}),
            }
            .validate(128)
            .is_err()
        );
        assert!(
            HostHttpResponseProjection {
                status_code: 200,
                payload: serde_json::json!({}),
            }
            .validate(128)
            .is_ok()
        );

        assert!(
            serde_json::from_value::<HostHttpResponseProjection>(serde_json::json!({
                "callId": "call-1",
                "statusCode": 200,
                "payload": {},
                "headers": {"set-cookie": "secret"}
            }))
            .is_err()
        );
    }

    #[test]
    fn proxy_continuation_binds_package_generation_and_parent_invocation() {
        let request = PluginProxyRequest {
            protocol_version: 1,
            plugin_id: crate::PluginId::new("org.example.connector").unwrap(),
            plugin_version: crate::PluginVersion::new("1.0.0").unwrap(),
            artifact_sha256: "a".repeat(64),
            deployment_generation: 17,
            request_id: "host-invocation-1".to_owned(),
            capability_id: crate::CapabilityId::new("external.account.profile.read").unwrap(),
            capability_version: 1,
            proxy_request_id: "proxy-1".to_owned(),
            request: HostHttpRequestProposal {
                endpoint_id: "external.account.self.read".to_owned(),
                method: HostHttpMethod::Get,
                query: BTreeMap::new(),
            },
        };
        request.validate().unwrap();
        let mut arbitrary_capability = request.clone();
        arbitrary_capability.capability_id =
            crate::CapabilityId::new("catalog.models.search").unwrap();
        assert!(arbitrary_capability.validate().is_err());
        let response = PluginProxyResponse {
            protocol_version: request.protocol_version,
            plugin_id: request.plugin_id.clone(),
            plugin_version: request.plugin_version.clone(),
            artifact_sha256: request.artifact_sha256.clone(),
            deployment_generation: request.deployment_generation,
            request_id: request.request_id.clone(),
            capability_id: request.capability_id.clone(),
            capability_version: request.capability_version,
            proxy_request_id: request.proxy_request_id.clone(),
            result: PluginProxyResult::Response(HostHttpResponseProjection {
                status_code: 200,
                payload: serde_json::json!({"subject":"external-7"}),
            }),
        };
        response.validate(128).unwrap();
        assert!(response.matches_request(&request));
        let mut different_capability = response.clone();
        different_capability.capability_id =
            crate::CapabilityId::new("external.account.balance.read").unwrap();
        assert!(different_capability.validate(128).is_ok());
        assert!(!different_capability.matches_request(&request));
        let mut stale = response;
        stale.deployment_generation = 16;
        assert!(stale.validate(128).is_ok());
        assert!(!stale.matches_request(&request));
        assert_eq!(
            serde_json::to_value(PluginProxyResult::Error(PluginProxyError {
                code: PluginProxyErrorCode::TemporarilyUnavailable,
            }))
            .unwrap(),
            serde_json::json!({
                "kind": "error",
                "value": {"code": "temporarily_unavailable"}
            })
        );
    }
}
