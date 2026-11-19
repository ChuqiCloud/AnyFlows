//! AnyFlows Plugin Protocol v1 helpers for Rust sidecars.
//! Requires `serde` and `serde_json` in the plugin crate.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct Frame {
    #[serde(rename = "protocolVersion")] pub protocol_version: u16,
    #[serde(rename = "pluginId")] pub plugin_id: String,
    #[serde(rename = "pluginVersion")] pub plugin_version: String,
    #[serde(rename = "artifactSha256")] pub artifact_sha256: String,
    #[serde(rename = "deploymentGeneration")] pub deployment_generation: u64,
    #[serde(rename = "requestId")] pub request_id: String,
    pub operation: Value,
}

pub fn serve<M, F>(manifest: M, mut handler: F) -> io::Result<()>
where
    M: Serialize + Clone,
    F: FnMut(&Frame) -> Result<Value, String>,
{
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let frame: Frame = serde_json::from_str(&line).map_err(invalid_data)?;
        let operation = frame.operation.get("operation").and_then(Value::as_str).unwrap_or_default();
        let result = match operation {
            "manifest" => json!({"kind":"manifest", "value":manifest.clone()}),
            "health" => json!({"kind":"health", "value":{"status":"healthy", "checkedAtUnixMs": now_ms()}}),
            "invoke_capability" | "invoke_task" | "invoke_provider" | "invoke_credential" | "plugin_route" => {
                let value = handler(&frame).map_err(invalid_data)?;
                let kind = match operation { "invoke_task" => "task", "invoke_provider" => "provider", "invoke_credential" => "credential", _ => "capability" };
                if kind == "capability" { json!({"kind":kind,"value":{"output":value}}) } else { json!({"kind":kind,"value":value}) }
            }
            _ => return Err(invalid_data("unsupported operation")),
        };
        let mut response = serde_json::to_value(&frame).map_err(invalid_data)?;
        let object = response.as_object_mut().expect("frame is an object");
        object.remove("operation");
        object.insert("result".to_owned(), result);
        serde_json::to_writer(&mut output, &object).map_err(invalid_data)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|value| value.as_millis() as u64).unwrap_or_default() }

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

pub fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value.clone())
}
