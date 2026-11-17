//! Safe, host-controlled request processing contracts.
//!
//! These types describe a proposal returned by a plugin.  They do not grant a
//! plugin permission to mutate a request: the host validates the proposal,
//! removes protected fields, and decides whether an effect is applicable to
//! the current stage.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ContractValidationError;

/// Capability used by a plugin that observes or proposes bounded request
/// processing effects.
pub const REQUEST_PROCESSING_CAPABILITY: &str = "request.processing.read";

const MAX_JSON_BYTES: usize = 64 * 1024;
const MAX_EFFECTS: usize = 32;
const MAX_PATCH_OPERATIONS: usize = 128;
const MAX_FINDINGS: usize = 32;
const MAX_HEADERS: usize = 64;

/// The point at which the host invokes a request processing plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProcessingStage {
    Ingress,
    Request,
    Outbound,
    Egress,
    Reply,
    Report,
}

/// A redacted request/response snapshot supplied to a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestProcessingInput {
    pub stage: RequestProcessingStage,
    pub request_id: String,
    pub format: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    /// Only non-sensitive headers selected by the host are included.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<Value>,
    /// Read-only usage facts.  The host owns quota and settlement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

impl RequestProcessingInput {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.request_id.trim() != self.request_id
            || self.format.is_empty()
            || self.format.len() > 64
            || self.format.chars().any(char::is_control)
            || self.path.is_empty()
            || self.path.len() > 2048
            || self
                .path
                .as_bytes()
                .iter()
                .any(|byte| byte.is_ascii_control())
            || self.headers.len() > MAX_HEADERS
            || self
                .headers
                .keys()
                .any(|name| !valid_header_name(name) || protected_header(name))
            || self
                .headers
                .values()
                .any(|value| value.len() > 8192 || value.chars().any(char::is_control))
            || json_size(&self.body) > MAX_JSON_BYTES
            || self
                .response_body
                .as_ref()
                .is_some_and(|value| json_size(value) > MAX_JSON_BYTES)
            || self
                .usage
                .as_ref()
                .is_some_and(|value| json_size(value) > MAX_JSON_BYTES)
            || self
                .response_status
                .is_some_and(|status| !(100..=599).contains(&status))
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        for value in [&self.model, &self.upstream_model].into_iter().flatten() {
            if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(ContractValidationError::InvalidExternalValue);
            }
        }
        Ok(())
    }
}

/// The host action requested by a plugin.  The host may downgrade a proposal
/// according to administrator policy or a failed stage check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProcessingAction {
    Continue,
    Reject,
    Stop,
}

/// A bounded JSON patch operation.  `move` and `copy` are deliberately not
/// supported because they make policy review harder and are unnecessary for
/// simple request adapters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JsonPatchOperation {
    pub op: JsonPatchOperationKind,
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonPatchOperationKind {
    Add,
    Remove,
    Replace,
}

/// Request body changes proposed by a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JsonPatchEffect {
    pub operations: Vec<JsonPatchOperation>,
}

/// Non-sensitive request header changes proposed by a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderPatchEffect {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// A host action and its optional bounded effects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RequestProcessingEffect {
    JsonPatch(JsonPatchEffect),
    HeaderPatch(HeaderPatchEffect),
}

/// A human-readable finding emitted for audit and dry-run views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestProcessingFinding {
    pub code: String,
    pub status: RequestProcessingFindingStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProcessingFindingStatus {
    Info,
    Warning,
    Fail,
}

/// The only output a request processing plugin can return.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestProcessingOutput {
    pub action: RequestProcessingAction,
    #[serde(default)]
    pub effects: Vec<RequestProcessingEffect>,
    #[serde(default)]
    pub findings: Vec<RequestProcessingFinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl RequestProcessingOutput {
    pub fn validate_for(
        &self,
        stage: RequestProcessingStage,
    ) -> Result<(), ContractValidationError> {
        if self.effects.len() > MAX_EFFECTS
            || self.findings.len() > MAX_FINDINGS
            || self.message.as_ref().is_some_and(|value| {
                value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control)
            })
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        if matches!(stage, RequestProcessingStage::Report)
            && (!self.effects.is_empty() || self.action != RequestProcessingAction::Continue)
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        if matches!(stage, RequestProcessingStage::Reply) && !self.effects.is_empty() {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        if self.findings.iter().any(|finding| {
            !valid_code(&finding.code)
                || finding.message.is_empty()
                || finding.message.len() > 1024
                || finding.message.chars().any(char::is_control)
                || finding
                    .recommendation
                    .as_ref()
                    .is_some_and(|value| value.len() > 1024 || value.chars().any(char::is_control))
        }) {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        let mut patch_operations = 0;
        for effect in &self.effects {
            match effect {
                RequestProcessingEffect::JsonPatch(patch) => {
                    patch_operations += patch.operations.len();
                    if patch.operations.len() > MAX_PATCH_OPERATIONS
                        || patch.operations.iter().any(invalid_json_patch)
                    {
                        return Err(ContractValidationError::InvalidExternalValue);
                    }
                }
                RequestProcessingEffect::HeaderPatch(patch) => {
                    if patch.set.len() > MAX_HEADERS
                        || patch.remove.len() > MAX_HEADERS
                        || patch.set.iter().any(|(name, value)| {
                            !valid_header_name(name)
                                || protected_header(name)
                                || value.len() > 8192
                                || value.chars().any(char::is_control)
                        })
                        || patch
                            .remove
                            .iter()
                            .any(|name| !valid_header_name(name) || protected_header(name))
                    {
                        return Err(ContractValidationError::InvalidExternalValue);
                    }
                }
            }
        }
        if patch_operations > MAX_PATCH_OPERATIONS {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

fn invalid_json_patch(operation: &JsonPatchOperation) -> bool {
    operation.path.len() > 32
        || operation.path.iter().any(|segment| {
            segment.is_empty()
                || segment.len() > 256
                || segment.chars().any(char::is_control)
                || is_protected_json_path(segment)
        })
        || match operation.op {
            JsonPatchOperationKind::Add | JsonPatchOperationKind::Replace => operation
                .value
                .as_ref()
                .is_none_or(|value| json_size(value) > MAX_JSON_BYTES),
            JsonPatchOperationKind::Remove => operation.value.is_some(),
        }
}

fn is_protected_json_path(segment: &str) -> bool {
    matches!(
        segment.to_ascii_lowercase().as_str(),
        "model"
            | "stream"
            | "stream_options"
            | "n"
            | "size"
            | "quality"
            | "resolution"
            | "seconds"
            | "duration"
            | "service_tier"
            | "quota"
            | "price"
            | "cost"
            | "wallet"
    )
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn protected_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "host"
            | "content-length"
            | "content-type"
            | "content-encoding"
            | "transfer-encoding"
            | "connection"
            | "upgrade"
            | "x-api-key"
            | "api-key"
            | "mj-api-secret"
            | "x-goog-api-key"
    )
}

fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn json_size(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(stage: RequestProcessingStage) -> RequestProcessingInput {
        RequestProcessingInput {
            stage,
            request_id: "req-1".to_owned(),
            format: "openai".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            model: Some("gpt-4o".to_owned()),
            upstream_model: Some("gpt-4o".to_owned()),
            headers: BTreeMap::from([("accept".to_owned(), "application/json".to_owned())]),
            body: serde_json::json!({"messages": []}),
            response_status: None,
            response_body: None,
            usage: None,
        }
    }

    #[test]
    fn input_rejects_sensitive_headers() {
        let mut value = input(RequestProcessingStage::Ingress);
        value
            .headers
            .insert("Authorization".to_owned(), "secret".to_owned());
        assert!(value.validate().is_err());
    }

    #[test]
    fn output_rejects_billing_and_credential_effects() {
        let value = RequestProcessingOutput {
            action: RequestProcessingAction::Continue,
            effects: vec![
                RequestProcessingEffect::JsonPatch(JsonPatchEffect {
                    operations: vec![JsonPatchOperation {
                        op: JsonPatchOperationKind::Replace,
                        path: vec!["model".to_owned()],
                        value: Some(serde_json::json!("other")),
                    }],
                }),
                RequestProcessingEffect::HeaderPatch(HeaderPatchEffect {
                    set: BTreeMap::from([("Authorization".to_owned(), "bad".to_owned())]),
                    remove: Vec::new(),
                }),
            ],
            findings: Vec::new(),
            message: None,
        };
        assert!(
            value
                .validate_for(RequestProcessingStage::Outbound)
                .is_err()
        );
    }

    #[test]
    fn report_is_observation_only() {
        let value = RequestProcessingOutput {
            action: RequestProcessingAction::Continue,
            effects: Vec::new(),
            findings: vec![RequestProcessingFinding {
                code: "latency.high".to_owned(),
                status: RequestProcessingFindingStatus::Warning,
                message: "slow".to_owned(),
                recommendation: None,
            }],
            message: None,
        };
        assert!(value.validate_for(RequestProcessingStage::Report).is_ok());
    }
}
