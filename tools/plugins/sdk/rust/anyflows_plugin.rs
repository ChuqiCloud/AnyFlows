//! AnyFlows Plugin Protocol v1 helpers for Rust sidecars.
//! Requires `serde` and `serde_json` in the plugin crate.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u16,
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
    #[serde(rename = "pluginVersion")]
    pub plugin_version: String,
    #[serde(rename = "artifactSha256")]
    pub artifact_sha256: String,
    #[serde(rename = "deploymentGeneration")]
    pub deployment_generation: u64,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "deadlineUnixMs")]
    pub deadline_unix_ms: u64,
    pub operation: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginOperationOutcome {
    NotApplied,
    Applied,
    Unknown,
}

impl PluginOperationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::NotApplied => "not_applied",
            Self::Applied => "applied",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginError {
    pub code: String,
    pub outcome: PluginOperationOutcome,
}

impl PluginError {
    #[must_use]
    pub fn new(code: impl Into<String>, outcome: PluginOperationOutcome) -> Self {
        Self {
            code: code.into(),
            outcome,
        }
    }

    #[must_use]
    pub fn unknown(code: impl Into<String>) -> Self {
        Self::new(code, PluginOperationOutcome::Unknown)
    }
}

pub fn serve<M, F>(manifest: M, mut handler: F) -> io::Result<()>
where
    M: Serialize + Clone,
    F: FnMut(&Frame) -> Result<Value, String>,
{
    serve_with_outcomes(manifest, |frame| {
        handler(frame).map_err(PluginError::unknown)
    })
}

pub fn serve_with_outcomes<M, F>(manifest: M, mut handler: F) -> io::Result<()>
where
    M: Serialize + Clone,
    F: FnMut(&Frame) -> Result<Value, PluginError>,
{
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let frame: Frame = serde_json::from_str(&line).map_err(invalid_data)?;
        let operation = frame
            .operation
            .get("operation")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if frame.protocol_version != 1
            || frame.plugin_id.is_empty()
            || frame.plugin_version.is_empty()
            || frame.artifact_sha256.len() != 64
            || !frame
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || frame.deployment_generation == 0
            || frame.request_id.is_empty()
            || frame.request_id.len() > 128
            || frame.deadline_unix_ms == 0
        {
            return Err(invalid_data("invalid host envelope"));
        }
        let result = if frame.deadline_unix_ms <= now_ms() {
            connector_error_with_outcome("temporarily_unavailable", "not_applied")
        } else {
            match operation {
                "manifest" => json!({"kind":"manifest", "value":manifest.clone()}),
                "health" => {
                    json!({"kind":"health", "value":{"status":"healthy", "checkedAtUnixMs": now_ms()}})
                }
                "invoke_capability" | "invoke_task" | "invoke_provider" | "invoke_credential"
                | "plugin_route" => {
                    let kind = match operation {
                        "invoke_task" => "task",
                        "invoke_provider" => "provider",
                        "invoke_credential" => "credential",
                        _ => "capability",
                    };
                    match handler(&frame) {
                        Ok(_) if frame.deadline_unix_ms <= now_ms() => {
                            connector_error("outcome_unknown")
                        }
                        Ok(value) if kind == "capability" => {
                            json!({"kind":kind,"value":{"output":value}})
                        }
                        Ok(value) if value.is_object() => json!({"kind":kind,"value":value}),
                        Ok(_) => connector_error("invalid_response"),
                        Err(error) => {
                            connector_error_with_outcome(&error.code, error.outcome.as_str())
                        }
                    }
                }
                _ => connector_error_with_outcome("unsupported", "not_applied"),
            }
        };
        let response = json!({
            "protocolVersion": frame.protocol_version,
            "pluginId": frame.plugin_id,
            "pluginVersion": frame.plugin_version,
            "artifactSha256": frame.artifact_sha256,
            "deploymentGeneration": frame.deployment_generation,
            "requestId": frame.request_id,
            "result": result,
        });
        serde_json::to_writer(&mut output, &response).map_err(invalid_data)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn connector_error(code: &str) -> Value {
    connector_error_with_outcome(code, "unknown")
}

fn connector_error_with_outcome(code: &str, outcome: &str) -> Value {
    let code = match code {
        "invalid_configuration"
        | "unsupported"
        | "authentication_required"
        | "forbidden"
        | "rate_limited"
        | "temporarily_unavailable"
        | "conflict"
        | "upstream_failure"
        | "partial_success"
        | "outcome_unknown"
        | "invalid_response" => code,
        _ => "upstream_failure",
    };
    let outcome = match outcome {
        "not_applied" | "applied" | "unknown" => outcome,
        _ => "unknown",
    };
    json!({"kind":"error","value":{"code":code,"outcome":outcome}})
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

pub fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value.clone())
}

#[cfg(test)]
mod tests {
    use super::{PluginError, PluginOperationOutcome, connector_error_with_outcome};
    use serde_json::json;

    #[test]
    fn connector_error_uses_only_contract_codes_and_outcomes() {
        assert_eq!(
            connector_error_with_outcome("forbidden", "not_applied"),
            json!({"kind":"error","value":{"code":"forbidden","outcome":"not_applied"}})
        );
        assert_eq!(
            connector_error_with_outcome("unsupported", "applied"),
            json!({"kind":"error","value":{"code":"unsupported","outcome":"applied"}})
        );
    }

    #[test]
    fn rust_plugin_error_preserves_operation_outcome() {
        let error = PluginError::new(
            "temporarily_unavailable",
            PluginOperationOutcome::NotApplied,
        );

        assert_eq!(
            connector_error_with_outcome(&error.code, error.outcome.as_str()),
            json!({"kind":"error","value":{"code":"temporarily_unavailable","outcome":"not_applied"}})
        );
    }
}
