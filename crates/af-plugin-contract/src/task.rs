use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Maximum size of an opaque provider task handle.
pub const MAX_PLUGIN_TASK_ID_BYTES: usize = 512;
/// Maximum size of a client idempotency key forwarded to a provider adapter.
pub const MAX_PLUGIN_IDEMPOTENCY_KEY_BYTES: usize = 256;

/// A task operation sent to a plugin. The host owns the public task id,
/// reservation and settlement; the plugin only sees the provider-facing
/// request or a provider task handle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginTaskOperation {
    /// Build a provider submission from a canonical request.
    Submit {
        model: String,
        request: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        idempotency_key: Option<String>,
    },
    /// Build or validate a provider status request for an existing task.
    Status { task_id: String },
    /// Request cancellation of an in-flight provider task.
    ///
    /// The host decides whether cancellation is exposed to callers and owns
    /// the public task state. The plugin only translates the provider-side
    /// cancellation request.
    Cancel {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Project a completed provider result into a host-safe result.
    Result { task_id: String, response: Value },
}

impl PluginTaskOperation {
    /// Validate fields that are common to all task operations.
    pub fn validate(&self) -> Result<(), TaskContractError> {
        match self {
            Self::Submit {
                model,
                request,
                idempotency_key,
            } => {
                validate_text(model, 128, TaskContractError::InvalidModel)?;
                if request.is_null() {
                    return Err(TaskContractError::InvalidRequest);
                }
                if let Some(key) = idempotency_key {
                    validate_text(
                        key,
                        MAX_PLUGIN_IDEMPOTENCY_KEY_BYTES,
                        TaskContractError::InvalidIdempotencyKey,
                    )?;
                }
            }
            Self::Status { task_id } => validate_task_id(task_id)?,
            Self::Cancel { task_id, reason } => {
                validate_task_id(task_id)?;
                if let Some(reason) = reason {
                    validate_text(reason, 256, TaskContractError::InvalidCancelReason)?;
                }
            }
            Self::Result { task_id, response } => {
                validate_task_id(task_id)?;
                if response.is_null() {
                    return Err(TaskContractError::InvalidResponse);
                }
            }
        }
        Ok(())
    }
}

/// The bounded lifecycle state returned by a task plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTaskState {
    Submitted,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl PluginTaskState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// A normalized task projection returned by a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PluginTaskResult {
    /// Provider accepted a submission. The host assigns its own public id.
    Submitted {
        task_id: String,
        state: PluginTaskState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress_basis_points: Option<u16>,
    },
    /// Current provider state for a submitted task.
    Status {
        state: PluginTaskState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress_basis_points: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure_code: Option<String>,
        /// Whether the host may retry after a failed provider attempt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retryable: Option<bool>,
        /// Optional provider hint. The host may clamp or ignore it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_after_seconds: Option<u32>,
    },
    /// Host-safe terminal output. Billing facts are deliberately not part of
    /// this value and remain owned by the host.
    Result { task_id: String, output: Value },
}

/// A bounded file produced by a completed task. The host stores bytes and
/// returns a short-lived download reference; plugins never choose a host path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTaskFile {
    pub name: String,
    pub media_type: String,
    pub bytes_base64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl PluginTaskFile {
    pub fn validate(&self) -> Result<(), TaskContractError> {
        validate_text(&self.name, 256, TaskContractError::InvalidFile)?;
        validate_text(&self.media_type, 128, TaskContractError::InvalidFile)?;
        if self.bytes_base64.is_empty() || self.bytes_base64.len() > 64 * 1024 * 1024 {
            return Err(TaskContractError::InvalidFile);
        }
        if self.sha256.as_ref().is_some_and(|digest| {
            digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err(TaskContractError::InvalidFile);
        }
        Ok(())
    }
}

/// Human-readable task report plus machine-readable summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTaskReport {
    pub title: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<PluginTaskFinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTaskFinding {
    pub code: String,
    pub severity: PluginTaskFindingSeverity,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTaskFindingSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginTaskCompletion {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<PluginTaskFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<PluginTaskReport>,
}

impl PluginTaskCompletion {
    pub fn validate(&self) -> Result<(), TaskContractError> {
        validate_task_id(&self.task_id)?;
        if self.files.len() > 64 {
            return Err(TaskContractError::InvalidFile);
        }
        for file in &self.files {
            file.validate()?;
        }
        if let Some(report) = &self.report {
            validate_text(&report.title, 256, TaskContractError::InvalidReport)?;
            validate_text(&report.summary, 16 * 1024, TaskContractError::InvalidReport)?;
            if report.findings.len() > 1024 {
                return Err(TaskContractError::InvalidReport);
            }
            for finding in &report.findings {
                validate_text(&finding.code, 128, TaskContractError::InvalidReport)?;
                validate_text(&finding.message, 4096, TaskContractError::InvalidReport)?;
            }
        }
        Ok(())
    }
}

impl PluginTaskResult {
    pub fn validate(&self) -> Result<(), TaskContractError> {
        match self {
            Self::Submitted {
                task_id,
                state,
                progress_basis_points,
            } => {
                validate_task_id(task_id)?;
                if state.is_terminal() {
                    return Err(TaskContractError::InvalidState);
                }
                validate_progress(*progress_basis_points)?;
            }
            Self::Status {
                state,
                progress_basis_points,
                failure_code,
                retryable,
                retry_after_seconds,
            } => {
                validate_progress(*progress_basis_points)?;
                if *state == PluginTaskState::Failed && failure_code.is_none() {
                    return Err(TaskContractError::MissingFailureCode);
                }
                if *state != PluginTaskState::Failed && failure_code.is_some() {
                    return Err(TaskContractError::InvalidFailureCode);
                }
                if *state != PluginTaskState::Failed
                    && (retryable.is_some() || retry_after_seconds.is_some())
                {
                    return Err(TaskContractError::InvalidRetryAdvice);
                }
                if let Some(code) = failure_code {
                    validate_text(code, 128, TaskContractError::InvalidFailureCode)?;
                }
                if retry_after_seconds.is_some_and(|seconds| seconds > 86_400) {
                    return Err(TaskContractError::InvalidRetryAdvice);
                }
                if retry_after_seconds.is_some() && *retryable != Some(true) {
                    return Err(TaskContractError::InvalidRetryAdvice);
                }
            }
            Self::Result { task_id, output } => {
                validate_task_id(task_id)?;
                if output.is_null() {
                    return Err(TaskContractError::InvalidResponse);
                }
            }
        }
        Ok(())
    }
}

/// Task-specific validation failures. Text never includes the supplied value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TaskContractError {
    #[error("task model is invalid")]
    InvalidModel,
    #[error("task request is invalid")]
    InvalidRequest,
    #[error("task idempotency key is invalid")]
    InvalidIdempotencyKey,
    #[error("task id is invalid")]
    InvalidTaskId,
    #[error("task response is invalid")]
    InvalidResponse,
    #[error("task state is invalid")]
    InvalidState,
    #[error("task progress is invalid")]
    InvalidProgress,
    #[error("task failure code is missing")]
    MissingFailureCode,
    #[error("task failure code is invalid")]
    InvalidFailureCode,
    #[error("task cancellation reason is invalid")]
    InvalidCancelReason,
    #[error("task retry advice is invalid")]
    InvalidRetryAdvice,
    #[error("task file is invalid")]
    InvalidFile,
    #[error("task report is invalid")]
    InvalidReport,
}

fn validate_task_id(value: &str) -> Result<(), TaskContractError> {
    validate_text(
        value,
        MAX_PLUGIN_TASK_ID_BYTES,
        TaskContractError::InvalidTaskId,
    )
}

fn validate_text(
    value: &str,
    max_bytes: usize,
    error: TaskContractError,
) -> Result<(), TaskContractError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        Err(error)
    } else {
        Ok(())
    }
}

fn validate_progress(value: Option<u16>) -> Result<(), TaskContractError> {
    if value.is_some_and(|progress| progress > 10_000) {
        Err(TaskContractError::InvalidProgress)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_operations_round_trip_and_reject_unsafe_ids() {
        let operation = PluginTaskOperation::Submit {
            model: "video-model".to_owned(),
            request: serde_json::json!({"prompt":"a cat"}),
            idempotency_key: Some("request-1".to_owned()),
        };
        operation.validate().unwrap();
        let wire = serde_json::to_value(&operation).unwrap();
        assert_eq!(wire["kind"], "submit");
        assert_eq!(
            serde_json::from_value::<PluginTaskOperation>(wire).unwrap(),
            operation
        );
        assert_eq!(
            (PluginTaskOperation::Status {
                task_id: "provider task".to_owned(),
            })
            .validate(),
            Err(TaskContractError::InvalidTaskId)
        );
    }

    #[test]
    fn task_results_keep_billing_out_of_plugin_output() {
        let result = PluginTaskResult::Status {
            state: PluginTaskState::Failed,
            progress_basis_points: Some(10_000),
            failure_code: Some("upstream_failure".to_owned()),
            retryable: Some(true),
            retry_after_seconds: Some(5),
        };
        result.validate().unwrap();
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["kind"], "status");
        assert!(wire.get("quota").is_none());
        assert!(wire.get("price").is_none());
    }

    #[test]
    fn terminal_submitted_state_and_bad_progress_are_rejected() {
        let terminal = PluginTaskResult::Submitted {
            task_id: "provider-1".to_owned(),
            state: PluginTaskState::Succeeded,
            progress_basis_points: Some(10_000),
        };
        assert_eq!(terminal.validate(), Err(TaskContractError::InvalidState));
        let bad_progress = PluginTaskResult::Status {
            state: PluginTaskState::Running,
            progress_basis_points: Some(10_001),
            failure_code: None,
            retryable: None,
            retry_after_seconds: None,
        };
        assert_eq!(
            bad_progress.validate(),
            Err(TaskContractError::InvalidProgress)
        );
    }

    #[test]
    fn task_completion_bounds_files_and_report_without_host_paths() {
        let completion = PluginTaskCompletion {
            task_id: "task-1".to_owned(),
            files: vec![PluginTaskFile {
                name: "result.json".to_owned(),
                media_type: "application/json".to_owned(),
                bytes_base64: "e30=".to_owned(),
                sha256: None,
            }],
            report: Some(PluginTaskReport {
                title: "完成".to_owned(),
                summary: "处理完成".to_owned(),
                findings: vec![PluginTaskFinding {
                    code: "ok".to_owned(),
                    severity: PluginTaskFindingSeverity::Info,
                    message: "ready".to_owned(),
                }],
            }),
        };
        completion.validate().unwrap();
        assert!(!serde_json::to_string(&completion).unwrap().contains("/tmp"));
    }
}
