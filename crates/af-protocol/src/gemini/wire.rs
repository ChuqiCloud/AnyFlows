use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;

/// 区分字段缺失与显式 `null`，后者在协议边界直接拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 请求未提供该字段。
    #[default]
    Missing,
    /// 请求提供了非空字段值。
    Value(T),
}

pub(super) fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// Gemini `generateContent` 入站请求正文。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerateContentRequestWire {
    /// 当前对话和历史消息。
    pub(super) contents: Vec<ContentWire>,
    /// 顶层系统指令。
    #[serde(
        rename = "systemInstruction",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) system_instruction: Field<ContentWire>,
    /// 客户端声明的工具集合。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tools: Field<Vec<ToolWire>>,
    /// 工具调用策略。
    #[serde(rename = "toolConfig", default, deserialize_with = "deserialize_field")]
    pub(super) tool_config: Field<ToolConfigWire>,
    /// 生成和思考参数。
    #[serde(
        rename = "generationConfig",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) generation_config: Field<GenerationConfigWire>,
}

/// 一条 Gemini 对话内容。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContentWire {
    /// `user`、`model` 或缺省角色。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) role: Field<String>,
    /// 按原始顺序排列的内容块。
    pub(super) parts: Vec<PartWire>,
}

/// Gemini 内容块及其思考修饰字段。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartWire {
    /// 文本内容。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) text: Field<String>,
    /// 内联媒体。
    #[serde(rename = "inlineData", default, deserialize_with = "deserialize_field")]
    pub(super) inline_data: Field<BlobWire>,
    /// 模型发起的函数调用。
    #[serde(
        rename = "functionCall",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) function_call: Field<FunctionCallWire>,
    /// 客户端返回的函数执行结果。
    #[serde(
        rename = "functionResponse",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) function_response: Field<FunctionResponseWire>,
    /// 文本是否属于模型思考。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) thought: Field<bool>,
    /// 用于延续思考上下文的不透明签名。
    #[serde(
        rename = "thoughtSignature",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) thought_signature: Field<String>,
}

/// Gemini 内联媒体载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlobWire {
    /// MIME 类型。
    #[serde(rename = "mimeType")]
    pub(super) mime_type: String,
    /// 标准 Base64 编码内容。
    pub(super) data: String,
}

/// Gemini 函数调用。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallWire {
    /// 可选调用标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// 函数名称。
    pub(super) name: String,
    /// 函数参数对象。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) args: Field<Value>,
}

/// Gemini 函数执行结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionResponseWire {
    /// 可选调用标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// 函数名称。
    pub(super) name: String,
    /// 任意 JSON 对象结果。
    pub(super) response: Value,
}

/// Gemini 工具容器。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolWire {
    /// 当前容器中的函数声明。
    #[serde(
        rename = "functionDeclarations",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) function_declarations: Field<Vec<FunctionDeclarationWire>>,
}

/// Gemini 函数声明。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionDeclarationWire {
    /// 函数名称。
    pub(super) name: String,
    /// 函数说明。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) description: Field<String>,
    /// OpenAPI 子集参数结构。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) parameters: Field<Value>,
    /// 标准 JSON Schema 参数结构。
    #[serde(
        rename = "parametersJsonSchema",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) parameters_json_schema: Field<Value>,
}

/// Gemini 工具配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolConfigWire {
    /// 函数调用模式。
    #[serde(
        rename = "functionCallingConfig",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) function_calling_config: Field<FunctionCallingConfigWire>,
}

/// Gemini 函数调用模式配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallingConfigWire {
    /// 调用模式；缺省为 `AUTO`。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) mode: Field<FunctionCallingModeWire>,
    /// 模式允许调用的函数名。
    #[serde(
        rename = "allowedFunctionNames",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) allowed_function_names: Field<Vec<String>>,
}

/// Gemini 函数调用模式。
#[derive(Deserialize)]
pub(super) enum FunctionCallingModeWire {
    /// 未指定模式；显式传入该值无有效语义。
    #[serde(rename = "MODE_UNSPECIFIED")]
    Unspecified,
    /// 模型自行决定是否调用函数。
    #[serde(rename = "AUTO")]
    Auto,
    /// 模型必须调用函数。
    #[serde(rename = "ANY")]
    Any,
    /// 模型不得调用函数。
    #[serde(rename = "NONE")]
    None,
    /// 模型可选择自然语言或受约束的函数调用。
    #[serde(rename = "VALIDATED")]
    Validated,
}

/// Gemini 生成配置中可无损归一的字段。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationConfigWire {
    /// 采样温度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) temperature: Field<f64>,
    /// 核采样概率阈值。
    #[serde(rename = "topP", default, deserialize_with = "deserialize_field")]
    pub(super) top_p: Field<f64>,
    /// 最大输出令牌数。
    #[serde(
        rename = "maxOutputTokens",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) max_output_tokens: Field<i64>,
    /// 最多五个停止序列。
    #[serde(
        rename = "stopSequences",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) stop_sequences: Field<Vec<String>>,
    /// 响应候选数，当前只能为一。
    #[serde(
        rename = "candidateCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) candidate_count: Field<i64>,
    /// Gemini 思考配置。
    #[serde(
        rename = "thinkingConfig",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) thinking_config: Field<ThinkingConfigWire>,
}

/// Gemini 思考配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThinkingConfigWire {
    /// 是否返回思考内容。
    #[serde(
        rename = "includeThoughts",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) include_thoughts: Field<bool>,
    /// 固定或动态思考令牌预算。
    #[serde(
        rename = "thinkingBudget",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) thinking_budget: Field<i64>,
    /// 模型支持的离散思考强度。
    #[serde(
        rename = "thinkingLevel",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) thinking_level: Field<ThinkingLevelWire>,
}

/// Gemini 思考强度。
#[derive(Deserialize)]
pub(super) enum ThinkingLevelWire {
    /// 未指定强度；显式传入该值无有效语义。
    #[serde(rename = "THINKING_LEVEL_UNSPECIFIED")]
    Unspecified,
    /// 最低思考强度。
    #[serde(rename = "MINIMAL")]
    Minimal,
    /// 低思考强度。
    #[serde(rename = "LOW")]
    Low,
    /// 中等思考强度。
    #[serde(rename = "MEDIUM")]
    Medium,
    /// 高思考强度。
    #[serde(rename = "HIGH")]
    High,
}
