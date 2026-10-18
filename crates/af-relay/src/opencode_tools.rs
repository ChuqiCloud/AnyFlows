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
    let patch = model.contains("gpt-") && !model.contains("oss") && !model.contains("gpt-4");
    tools
        .iter()
        .filter(|tool| match tool.name.as_str() {
            "apply_patch" => patch,
            "edit" | "write" => !patch,
            _ => true,
        })
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
    fn registry_uses_official_model_tool_variants_and_complete_contracts() {
        for entry in [OpenCodeEntry::InferenceV1, OpenCodeEntry::ZenV1] {
            let normal = opencode_compatibility_tools(entry, "mimo-v2.6-flash-free");
            assert!(normal.iter().any(|t| t.name == "bash"
                && t.input_schema["required"] == serde_json::json!(["command", "description"])));
            assert!(normal.iter().any(|t| t.name == "edit"));
            assert!(!normal.iter().any(|t| t.name == "apply_patch"));
            let gpt = opencode_compatibility_tools(entry, "gpt-5");
            assert!(gpt.iter().any(|t| t.name == "apply_patch"));
            assert!(!gpt.iter().any(|t| t.name == "edit" || t.name == "write"));
        }
    }
}
