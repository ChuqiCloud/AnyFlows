use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Map, Value};

/// 区分字段缺失与字段已提供，显式 `null` 的语义由反序列化入口决定。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 请求未提供该字段。
    #[default]
    Missing,
    /// 请求提供了非空字段值。
    Value(T),
}

fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// 历史消息的可选字段将显式 `null` 视为缺省；顶层参数仍使用严格解析器。
fn deserialize_nullable_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(|value| match value {
        Some(value) => Field::Value(value),
        None => Field::Missing,
    })
}

/// OpenAI Chat Completions 入站请求。
#[derive(Deserialize)]
pub(super) struct ChatRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 按请求顺序提供的消息。
    pub(super) messages: Vec<MessageWire>,
    /// 客户端声明的函数工具。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tools: Field<Vec<ToolWire>>,
    /// 模型选择工具的策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tool_choice: Field<ToolChoiceWire>,
    /// 模型使用的推理强度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) reasoning_effort: Field<ReasoningEffortWire>,
    /// 采样温度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) temperature: Field<f64>,
    /// 核采样概率阈值。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) top_p: Field<f64>,
    /// 旧版最大输出令牌数。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) max_tokens: Field<i64>,
    /// 最大补全令牌数。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) max_completion_tokens: Field<i64>,
    /// 停止生成的序列。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stop: Field<StopWire>,
    /// 终端用户标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) user: Field<String>,
    /// 是否请求流式响应。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream: Field<bool>,
    /// 流末 usage 等可选数据。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream_options: Field<StreamOptionsWire>,
    /// 等待静态白名单校验的顶层扩展字段。
    #[serde(flatten)]
    pub(super) extra: Map<String, Value>,
}

/// OpenAI Chat 流式响应选项。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamOptionsWire {
    /// 是否要求在 `[DONE]` 前返回完整 usage 块。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) include_usage: Field<bool>,
}

/// 按角色闭合分派的消息。
#[derive(Deserialize)]
#[serde(tag = "role")]
pub(super) enum MessageWire {
    /// 系统级指令消息。
    #[serde(rename = "system")]
    System(SystemMessageWire),
    /// 开发者级指令消息。
    #[serde(rename = "developer")]
    Developer(DeveloperMessageWire),
    /// 用户输入消息。
    #[serde(rename = "user")]
    User(UserMessageWire),
    /// 助手输出或工具调用消息。
    #[serde(rename = "assistant")]
    Assistant(AssistantMessageWire),
    /// 工具执行结果消息。
    #[serde(rename = "tool")]
    Tool(ToolMessageWire),
}

/// 系统级指令消息载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SystemMessageWire {
    /// 字符串或文本内容块列表。
    pub(super) content: TextContentWire,
}

/// 开发者级指令消息载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeveloperMessageWire {
    /// 字符串或文本内容块列表。
    pub(super) content: TextContentWire,
}

/// 用户消息载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UserMessageWire {
    /// 字符串或多模态内容块列表。
    pub(super) content: UserContentWire,
}

/// 助手消息载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssistantMessageWire {
    /// 可缺省或为 `null` 的文本内容。
    #[serde(default)]
    pub(super) content: Option<AssistantContentWire>,
    /// DeepSeek 兼容接口返回的助手推理文本；续聊时客户端可能原样带回。
    #[serde(default, rename = "reasoning_content")]
    _reasoning_content: Option<String>,
    /// 部分 OpenAI 兼容服务使用的推理文本别名。
    #[serde(default, rename = "reasoning")]
    _reasoning: Option<String>,
    /// 助手发起的函数工具调用。
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub(super) tool_calls: Field<Vec<ToolCallWire>>,
    // Empty response metadata is safe to omit from the canonical history.
    #[serde(default, rename = "refusal")]
    _refusal: (),
    #[serde(default, rename = "audio")]
    _audio: (),
    #[serde(default, rename = "function_call")]
    _function_call: (),
    #[serde(default, rename = "annotations")]
    _annotations: Option<[(); 0]>,
}

/// 工具执行结果消息载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolMessageWire {
    /// 字符串或文本内容块列表。
    pub(super) content: TextContentWire,
    /// 对应的工具调用标识。
    pub(super) tool_call_id: String,
}

/// 仅包含文本的消息内容。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum TextContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 按顺序排列的文本内容块。
    Parts(Vec<TextPartWire>),
}

/// 用户消息内容。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum UserContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 按顺序排列的文本、图片或音频内容块。
    Parts(Vec<UserContentPartWire>),
}

/// 助手消息的非空内容值。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AssistantContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 按顺序排列的文本内容块。
    Parts(Vec<TextPartWire>),
}

/// 闭合的文本内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum TextPartWire {
    /// 文本内容块。
    #[serde(rename = "text")]
    Text(TextPartPayloadWire),
}

/// 文本内容块载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextPartPayloadWire {
    /// 文本内容。
    pub(super) text: String,
}

/// 闭合的用户多模态内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum UserContentPartWire {
    /// 文本内容块。
    #[serde(rename = "text")]
    Text(TextPartPayloadWire),
    /// 图片内容块。
    #[serde(rename = "image_url")]
    ImageUrl(ImageUrlPartPayloadWire),
    /// 输入音频内容块。
    #[serde(rename = "input_audio")]
    InputAudio(InputAudioPartPayloadWire),
}

/// 图片内容块载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageUrlPartPayloadWire {
    /// 图片来源及细节级别。
    pub(super) image_url: ImageUrlWire,
}

/// 图片来源描述。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageUrlWire {
    /// HTTPS URL 或图片 data URL。
    pub(super) url: String,
    /// 请求的图片细节级别。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) detail: Field<ImageDetailWire>,
}

/// 图片细节级别。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ImageDetailWire {
    /// 由服务端自动选择。
    Auto,
    /// 使用低细节输入。
    Low,
    /// 使用高细节输入。
    High,
}

/// 输入音频内容块载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputAudioPartPayloadWire {
    /// 输入音频数据及格式。
    pub(super) input_audio: InputAudioWire,
}

/// Base64 编码的输入音频。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputAudioWire {
    /// Base64 编码的音频数据。
    pub(super) data: String,
    /// 音频容器格式。
    pub(super) format: InputAudioFormatWire,
}

/// OpenAI Chat 支持的输入音频格式。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum InputAudioFormatWire {
    /// WAV 音频。
    Wav,
    /// MP3 音频。
    Mp3,
}

/// 客户端声明的工具。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolWire {
    /// 函数工具。
    #[serde(rename = "function")]
    Function(FunctionToolPayloadWire),
}

/// 函数工具载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionToolPayloadWire {
    /// 函数定义。
    pub(super) function: FunctionDefinitionWire,
}

/// 函数工具定义。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionDefinitionWire {
    /// 函数名称。
    pub(super) name: String,
    /// 函数用途说明。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) description: Field<String>,
    /// 函数参数的 JSON Schema。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) parameters: Field<Value>,
    /// 是否启用严格的 Schema 约束。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) strict: Field<bool>,
}

/// 助手发起的工具调用。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolCallWire {
    /// 函数工具调用。
    #[serde(rename = "function")]
    Function(FunctionToolCallPayloadWire),
}

/// 函数工具调用载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionToolCallPayloadWire {
    /// 当前调用的稳定标识。
    pub(super) id: String,
    /// 被调用的函数及其参数。
    pub(super) function: FunctionCallWire,
}

/// 函数工具调用详情。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallWire {
    /// 被调用的函数名称。
    pub(super) name: String,
    /// JSON 编码的函数参数对象。
    pub(super) arguments: String,
}

/// 模型选择工具的策略。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ToolChoiceWire {
    /// 简单策略名称。
    Mode(ToolChoiceModeWire),
    /// 指定必须调用的函数工具。
    Named(NamedToolChoiceWire),
}

/// 简单工具选择策略。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ToolChoiceModeWire {
    /// 由模型决定是否调用工具。
    Auto,
    /// 禁止调用工具。
    None,
    /// 必须调用任意工具。
    Required,
}

/// 指定函数的工具选择策略。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedToolChoiceWire {
    /// 工具类型，当前仅允许函数。
    #[serde(rename = "type")]
    pub(super) kind: FunctionTypeWire,
    /// 被指定的函数。
    pub(super) function: NamedFunctionWire,
}

/// 指定函数的名称载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedFunctionWire {
    /// 被指定的函数名称。
    pub(super) name: String,
}

/// 固定为 `function` 的工具类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FunctionTypeWire {
    /// 函数工具。
    Function,
}

/// OpenAI Chat 推理强度。
#[derive(Deserialize)]
pub(super) enum ReasoningEffortWire {
    /// 显式关闭推理。
    #[serde(rename = "none")]
    None,
    /// 最低推理强度。
    #[serde(rename = "minimal")]
    Minimal,
    /// 低推理强度。
    #[serde(rename = "low")]
    Low,
    /// 中等推理强度。
    #[serde(rename = "medium")]
    Medium,
    /// 高推理强度。
    #[serde(rename = "high")]
    High,
    /// 超高推理强度。
    #[serde(rename = "xhigh")]
    ExtraHigh,
    /// 厂商允许的最大推理强度。
    #[serde(rename = "max")]
    Max,
}

/// 停止生成的序列参数。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum StopWire {
    /// 单个停止序列。
    One(String),
    /// 多个停止序列。
    Many(Vec<String>),
}
