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

impl RequestProcessingStage {
    /// Whether this stage may carry a bounded request or response effect.
    ///
    /// `reply` and `report` are observation-only: a plugin can still return
    /// findings or a terminal recommendation, but it cannot rewrite the
    /// payload after the host has started producing the client response.
    #[must_use]
    pub const fn allows_effects(self) -> bool {
        !matches!(self, Self::Reply | Self::Report)
    }

    /// Whether a terminal plugin action can stop the host pipeline at this
    /// stage. The host remains responsible for mapping a reject to its public
    /// protocol error and for deciding whether a stop is retryable.
    #[must_use]
    pub const fn allows_terminal_action(self) -> bool {
        !matches!(self, Self::Report)
    }
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

    /// Apply a validated plugin proposal before the next processing stage.
    ///
    /// This is deliberately implemented by the host contract crate so every
    /// runtime uses the same bounded JSON and header semantics. Protected
    /// fields have already been rejected by `RequestProcessingOutput::validate_for`;
    /// a missing path is still treated as an invalid proposal instead of being
    /// silently ignored.
    pub fn apply_effects(
        &mut self,
        effects: &[RequestProcessingEffect],
    ) -> Result<(), ContractValidationError> {
        validate_effects(effects)?;
        let mut next = self.clone();
        for effect in effects {
            match effect {
                RequestProcessingEffect::JsonPatch(patch) => {
                    for operation in &patch.operations {
                        apply_json_patch(&mut next.body, operation)?;
                    }
                }
                RequestProcessingEffect::HeaderPatch(patch) => {
                    for name in &patch.remove {
                        next.headers
                            .retain(|current, _| !current.eq_ignore_ascii_case(name));
                    }
                    for (name, value) in &patch.set {
                        next.headers
                            .retain(|current, _| !current.eq_ignore_ascii_case(name));
                        next.headers.insert(name.clone(), value.clone());
                    }
                }
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
}

/// Host-side result of running a sequence of request-processing stages.
/// `request` contains the validated effects accumulated through the last
/// non-terminal stage; plugins never receive this structure as a wire frame.
#[derive(Clone, Debug, PartialEq)]
pub struct RequestProcessingPipelineResult {
    pub request: RequestProcessingInput,
    pub outputs: Vec<RequestProcessingOutput>,
}

impl RequestProcessingPipelineResult {
    /// Return the terminal output, if a stage requested `reject` or `stop`.
    #[must_use]
    pub fn terminal_output(&self) -> Option<&RequestProcessingOutput> {
        self.outputs.iter().find(|output| output.is_terminal())
    }

    /// Whether the pipeline reached the end of the requested stages.
    #[must_use]
    pub fn completed(&self, stages: &[RequestProcessingStage]) -> bool {
        self.outputs.len() == stages.len() && self.terminal_output().is_none()
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
    /// Whether this output asks the host to stop before the next stage.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self.action,
            RequestProcessingAction::Reject | RequestProcessingAction::Stop
        )
    }

    /// Apply the output using the host's stage policy.
    ///
    /// Terminal outputs are deliberately not applied: a plugin must not be
    /// able to combine a rejection with a hidden request rewrite. The return
    /// value indicates whether the caller may continue to the next stage.
    pub fn apply_to(
        &self,
        request: &mut RequestProcessingInput,
    ) -> Result<bool, ContractValidationError> {
        self.validate_for(request.stage)?;
        if self.is_terminal() {
            return Ok(false);
        }
        request.apply_effects(&self.effects)?;
        Ok(true)
    }

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
        if (!stage.allows_effects() && !self.effects.is_empty())
            || (!stage.allows_terminal_action() && self.is_terminal())
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
        validate_effects(&self.effects)?;
        Ok(())
    }
}

fn validate_effects(effects: &[RequestProcessingEffect]) -> Result<(), ContractValidationError> {
    let mut patch_operations: usize = 0;
    if effects.len() > MAX_EFFECTS {
        return Err(ContractValidationError::InvalidExternalValue);
    }
    for effect in effects {
        match effect {
            RequestProcessingEffect::JsonPatch(patch) => {
                if patch.operations.len() > MAX_PATCH_OPERATIONS
                    || patch.operations.iter().any(invalid_json_patch)
                {
                    return Err(ContractValidationError::InvalidExternalValue);
                }
                patch_operations = patch_operations
                    .checked_add(patch.operations.len())
                    .ok_or(ContractValidationError::InvalidExternalValue)?;
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

fn invalid_json_patch(operation: &JsonPatchOperation) -> bool {
    operation.path.is_empty()
        || operation.path.len() > 32
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

fn apply_json_patch(
    body: &mut Value,
    operation: &JsonPatchOperation,
) -> Result<(), ContractValidationError> {
    if operation.path.is_empty() {
        return Err(ContractValidationError::InvalidExternalValue);
    }
    let (parent_path, leaf) = operation
        .path
        .split_at(operation.path.len().saturating_sub(1));
    let parent = value_at_path_mut(body, parent_path)?;
    match parent {
        Value::Object(object) => match operation.op {
            JsonPatchOperationKind::Add | JsonPatchOperationKind::Replace => {
                let value = operation
                    .value
                    .clone()
                    .ok_or(ContractValidationError::InvalidExternalValue)?;
                if operation.op == JsonPatchOperationKind::Replace && !object.contains_key(&leaf[0])
                {
                    return Err(ContractValidationError::InvalidExternalValue);
                }
                object.insert(leaf[0].clone(), value);
            }
            JsonPatchOperationKind::Remove => {
                if object.remove(&leaf[0]).is_none() {
                    return Err(ContractValidationError::InvalidExternalValue);
                }
            }
        },
        Value::Array(array) => {
            let index = leaf[0]
                .parse::<usize>()
                .map_err(|_| ContractValidationError::InvalidExternalValue)?;
            match operation.op {
                JsonPatchOperationKind::Add => {
                    let value = operation
                        .value
                        .clone()
                        .ok_or(ContractValidationError::InvalidExternalValue)?;
                    if index > array.len() {
                        return Err(ContractValidationError::InvalidExternalValue);
                    }
                    array.insert(index, value);
                }
                JsonPatchOperationKind::Replace => {
                    let value = operation
                        .value
                        .clone()
                        .ok_or(ContractValidationError::InvalidExternalValue)?;
                    let target = array
                        .get_mut(index)
                        .ok_or(ContractValidationError::InvalidExternalValue)?;
                    *target = value;
                }
                JsonPatchOperationKind::Remove => {
                    if index >= array.len() {
                        return Err(ContractValidationError::InvalidExternalValue);
                    }
                    array.remove(index);
                }
            }
        }
        _ => return Err(ContractValidationError::InvalidExternalValue),
    }
    Ok(())
}

fn value_at_path_mut<'a>(
    mut value: &'a mut Value,
    path: &[String],
) -> Result<&'a mut Value, ContractValidationError> {
    for segment in path {
        value = match value {
            Value::Object(object) => object
                .get_mut(segment)
                .ok_or(ContractValidationError::InvalidExternalValue)?,
            Value::Array(array) => {
                let index = segment
                    .parse::<usize>()
                    .map_err(|_| ContractValidationError::InvalidExternalValue)?;
                array
                    .get_mut(index)
                    .ok_or(ContractValidationError::InvalidExternalValue)?
            }
            _ => return Err(ContractValidationError::InvalidExternalValue),
        };
    }
    Ok(value)
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

    #[test]
    fn effects_are_applied_with_shared_json_and_header_semantics() {
        let mut value = input(RequestProcessingStage::Ingress);
        value.body = serde_json::json!({"metadata": {"tenant": "old"}, "items": ["a"]});
        value.headers.insert("x-trace".to_owned(), "old".to_owned());
        let effects = vec![
            RequestProcessingEffect::JsonPatch(JsonPatchEffect {
                operations: vec![JsonPatchOperation {
                    op: JsonPatchOperationKind::Replace,
                    path: vec!["metadata".to_owned(), "tenant".to_owned()],
                    value: Some(serde_json::json!("new")),
                }],
            }),
            RequestProcessingEffect::HeaderPatch(HeaderPatchEffect {
                set: BTreeMap::from([("X-Trace".to_owned(), "next".to_owned())]),
                remove: vec!["accept".to_owned()],
            }),
        ];
        value.apply_effects(&effects).unwrap();
        assert_eq!(value.body["metadata"]["tenant"], "new");
        assert_eq!(value.headers.get("X-Trace"), Some(&"next".to_owned()));
        assert!(!value.headers.contains_key("accept"));
    }

    #[test]
    fn effects_reject_missing_paths_and_root_replacement() {
        let mut value = input(RequestProcessingStage::Ingress);
        let missing = vec![RequestProcessingEffect::JsonPatch(JsonPatchEffect {
            operations: vec![JsonPatchOperation {
                op: JsonPatchOperationKind::Replace,
                path: vec!["missing".to_owned()],
                value: Some(serde_json::json!(true)),
            }],
        })];
        assert!(value.apply_effects(&missing).is_err());
        let root = vec![RequestProcessingEffect::JsonPatch(JsonPatchEffect {
            operations: vec![JsonPatchOperation {
                op: JsonPatchOperationKind::Replace,
                path: Vec::new(),
                value: Some(serde_json::json!({})),
            }],
        })];
        assert!(value.apply_effects(&root).is_err());
    }

    #[test]
    fn failed_effect_batch_does_not_partially_mutate_input() {
        let mut value = input(RequestProcessingStage::Ingress);
        let original = value.clone();
        let effects = vec![RequestProcessingEffect::JsonPatch(JsonPatchEffect {
            operations: vec![
                JsonPatchOperation {
                    op: JsonPatchOperationKind::Add,
                    path: vec!["annotation".to_owned()],
                    value: Some(serde_json::json!("temporary")),
                },
                JsonPatchOperation {
                    op: JsonPatchOperationKind::Replace,
                    path: vec!["missing".to_owned()],
                    value: Some(serde_json::json!(true)),
                },
            ],
        })];
        assert!(value.apply_effects(&effects).is_err());
        assert_eq!(value, original);
    }

    #[test]
    fn terminal_output_is_not_applied_to_request_state() {
        let mut request = input(RequestProcessingStage::Request);
        let original = request.clone();
        let output = RequestProcessingOutput {
            action: RequestProcessingAction::Reject,
            effects: vec![RequestProcessingEffect::JsonPatch(JsonPatchEffect {
                operations: vec![JsonPatchOperation {
                    op: JsonPatchOperationKind::Replace,
                    path: vec!["body".to_owned()],
                    value: Some(serde_json::json!({"blocked": true})),
                }],
            })],
            findings: Vec::new(),
            message: Some("blocked".to_owned()),
        };
        assert!(!output.apply_to(&mut request).unwrap());
        assert_eq!(request, original);
    }

    #[test]
    fn reply_and_report_reject_effects_and_terminal_actions() {
        let effects = RequestProcessingEffect::HeaderPatch(HeaderPatchEffect {
            set: BTreeMap::from([("x-plugin".to_owned(), "value".to_owned())]),
            remove: Vec::new(),
        });
        for stage in [
            RequestProcessingStage::Reply,
            RequestProcessingStage::Report,
        ] {
            let output = RequestProcessingOutput {
                action: RequestProcessingAction::Continue,
                effects: vec![effects.clone()],
                findings: Vec::new(),
                message: None,
            };
            assert!(output.validate_for(stage).is_err());
        }
        let output = RequestProcessingOutput {
            action: RequestProcessingAction::Stop,
            effects: Vec::new(),
            findings: Vec::new(),
            message: None,
        };
        assert!(output.validate_for(RequestProcessingStage::Report).is_err());
    }
}
