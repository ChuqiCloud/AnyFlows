use af_domain::OpenCodeEntry;
use af_protocol::ToolDef;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct RegisteredTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// 冻结的官方工具契约，只提供声明，网关没有工具执行器。
pub fn opencode_compatibility_tools(entry: OpenCodeEntry, model: &str) -> Vec<ToolDef> {
    static TOOLS: OnceLock<Vec<RegisteredTool>> = OnceLock::new();
    let tools = match entry {
        OpenCodeEntry::InferenceV1 | OpenCodeEntry::ZenV1 => TOOLS.get_or_init(|| {
            serde_json::from_str(include_str!("opencode/tools-v1.json"))
                .expect("程序内置 OpenCode 工具契约必须有效")
        }),
    };
    let _ = model;
    tools
        .iter()
        .map(|tool| ToolDef {
            name: tool.name.clone(),
            description: Some(tool.description.clone()),
            input_schema: tool.input_schema.clone(),
            strict: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_preserves_current_official_contracts() {
        for entry in [OpenCodeEntry::InferenceV1, OpenCodeEntry::ZenV1] {
            let tools = opencode_compatibility_tools(entry, "mimo-v2.6-flash-free");
            assert_eq!(tools.len(), 12);
            assert!(tools.iter().any(|t| t.name == "shell"
                && t.input_schema["required"] == serde_json::json!(["command"])));
            assert!(tools.iter().any(|t| t.name == "execute"
                && t.input_schema["required"] == serde_json::json!(["code"])));
            assert!(tools.iter().any(
                |t| t.name == "read" && t.input_schema["properties"]["offset"]["minimum"] == 0
            ));
        }
    }
}
