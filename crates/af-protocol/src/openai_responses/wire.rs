use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Map, Value};

/// 区分字段缺失与显式提供；协议边界统一拒绝 `null`。
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

/// OpenAI Responses 入站请求。
#[derive(Deserialize)]
pub(super) struct ResponsesRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 字符串或有序输入 Item 列表。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) input: Field<InputWire>,
    /// 本轮顶层开发者指令。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) instructions: Field<String>,
    /// 持久会话引用。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) conversation: Field<ConversationWire>,
    /// 上一响应引用。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) previous_response_id: Field<String>,
    /// 提示缓存亲和键。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) prompt_cache_key: Field<String>,
    /// 客户端声明的函数工具。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tools: Field<Vec<ToolWire>>,
    /// 模型选择工具的策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tool_choice: Field<ToolChoiceWire>,
    /// 推理强度配置。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) reasoning: Field<ReasoningWire>,
    /// 采样温度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) temperature: Field<f64>,
    /// 核采样概率阈值。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) top_p: Field<f64>,
    /// 最大输出令牌数。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) max_output_tokens: Field<i64>,
    /// 旧式终端用户标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) user: Field<String>,
    /// 是否请求流式响应。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream: Field<bool>,
    /// 服务端自动压缩策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) context_management: Field<Vec<ContextManagementWire>>,
    /// 等待静态白名单校验的顶层同源字段。
    #[serde(flatten)]
    pub(super) extra: Map<String, Value>,
}

/// Responses 自动压缩策略项。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextManagementWire {
    #[serde(rename = "type")]
    pub(super) kind: ContextManagementTypeWire,
    pub(super) compact_threshold: i64,
}

/// 自动压缩策略类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ContextManagementTypeWire {
    Compaction,
}

/// 持久会话可以使用字符串简写或显式对象。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ConversationWire {
    /// 会话标识字符串。
    Id(String),
    /// 带 `id` 的会话引用对象。
    Object(ConversationObjectWire),
}

/// 显式会话引用。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConversationObjectWire {
    /// 会话标识。
    pub(super) id: String,
}

/// Responses 输入支持字符串简写或有序 Item 列表。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum InputWire {
    /// 等价于单条用户文本消息。
    Text(String),
    /// 有序输入 Item 列表。
    Items(Vec<InputItemWire>),
}

/// 当前 Canonical 能无损表达的输入 Item。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum InputItemWire {
    /// 系统、开发者、用户或助手消息。
    Message(InputMessageWire),
    /// 无状态续传使用的加密推理 Item。
    Reasoning(ReasoningInputItemWire),
    /// 先前的函数调用。
    FunctionCall(FunctionCallItemWire),
    /// 客户端提供的函数调用结果。
    FunctionCallOutput(FunctionCallOutputItemWire),
    /// 上一轮服务端返回的不透明压缩 Item。
    Compaction(CompactionInputItemWire),
}

/// Responses 输入中的不透明压缩 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompactionInputItemWire {
    #[serde(rename = "type")]
    pub(super) kind: CompactionItemTypeWire,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    pub(super) encrypted_content: String,
}

/// 压缩 Item 类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CompactionItemTypeWire {
    Compaction,
}

/// 客户端回传的加密推理 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasoningInputItemWire {
    /// 固定的推理 Item 类型。
    #[serde(rename = "type")]
    pub(super) kind: ReasoningInputTypeWire,
    /// 上游或前一跳分配的可选 Item 标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// 无状态续传所需的不透明加密推理内容。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) encrypted_content: Field<String>,
    /// 可选的人类可读推理摘要。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) summary: Field<Vec<ReasoningSummaryWire>>,
    /// 前一响应中的可选生命周期状态。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) status: Field<ItemStatusWire>,
}

/// 固定为 `reasoning` 的输入 Item 类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReasoningInputTypeWire {
    /// 加密推理 Item。
    Reasoning,
}

/// 推理 Item 中的单段摘要。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasoningSummaryWire {
    /// 固定的摘要类型。
    #[serde(rename = "type")]
    pub(super) kind: ReasoningSummaryTypeWire,
    /// 摘要文本。
    pub(super) text: String,
}

/// 固定为 `summary_text` 的摘要类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReasoningSummaryTypeWire {
    /// 推理摘要文本。
    SummaryText,
}

/// 一条输入消息。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputMessageWire {
    /// 消息内容。
    pub(super) content: MessageContentWire,
    /// 指令层级或对话角色。
    pub(super) role: MessageRoleWire,
    /// 可选的固定 Item 类型。
    #[serde(rename = "type", default, deserialize_with = "deserialize_field")]
    pub(super) kind: Field<MessageTypeWire>,
    /// 助手输出阶段当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) phase: Field<MessagePhaseWire>,
    /// 返回 Item 的生命周期状态当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) status: Field<ItemStatusWire>,
}

/// 消息角色。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessageRoleWire {
    /// 系统指令。
    System,
    /// 开发者指令。
    Developer,
    /// 用户输入。
    User,
    /// 先前助手输出。
    Assistant,
}

/// 固定消息 Item 类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessageTypeWire {
    /// 消息 Item。
    Message,
}

/// 助手输出阶段。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessagePhaseWire {
    /// 中间说明。
    Commentary,
    /// 最终回答。
    FinalAnswer,
}

/// 返回 Item 的生命周期状态。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ItemStatusWire {
    /// 尚在生成。
    InProgress,
    /// 已完成。
    Completed,
    /// 未完整结束。
    Incomplete,
}

/// 消息内容支持字符串简写或内容块列表。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum MessageContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 文本、图片或文件内容块列表。
    Parts(Vec<InputContentWire>),
}

/// 消息输入内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum InputContentWire {
    /// 文本输入。
    #[serde(rename = "input_text")]
    Text(InputTextWire),
    /// 上一轮助手返回的文本输出。
    #[serde(rename = "output_text")]
    OutputText(OutputTextInputWire),
    /// 图片输入。
    #[serde(rename = "input_image")]
    Image(InputImageWire),
    /// 文件输入，当前仅用于给出明确的不支持错误。
    #[serde(rename = "input_file")]
    File(InputFileWire),
}

/// 文本输入块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputTextWire {
    /// 文本内容。
    pub(super) text: String,
    /// 显式缓存断点当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) prompt_cache_breakpoint: Field<Value>,
}

/// 手动回传上一轮助手输出时使用的文本块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputTextInputWire {
    /// 助手返回的文本内容。
    pub(super) text: String,
    /// 官方输出 Item 可能携带的引用标注；非空标注尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) annotations: Field<Vec<Value>>,
    /// 官方输出 Item 可能携带的对数概率；非空数据尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) logprobs: Field<Vec<Value>>,
}

/// 图片输入块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputImageWire {
    /// 图片细节级别。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) detail: Field<ImageDetailWire>,
    /// 绑定 OpenAI 账号的文件标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) file_id: Field<String>,
    /// HTTPS URL 或图片 data URL。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) image_url: Field<String>,
    /// 显式缓存断点当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) prompt_cache_breakpoint: Field<Value>,
}

/// 图片细节级别。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ImageDetailWire {
    /// 自动选择。
    Auto,
    /// 低细节。
    Low,
    /// 高细节。
    High,
    /// 原始分辨率。
    Original,
}

/// 文件输入块；字段会在转换阶段统一拒绝。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputFileWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) detail: Field<Value>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) file_data: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) file_id: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) file_url: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) filename: Field<String>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) prompt_cache_breakpoint: Field<Value>,
}

/// 函数调用 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallItemWire {
    #[serde(rename = "type")]
    pub(super) kind: FunctionCallTypeWire,
    /// JSON 编码的函数参数对象。
    pub(super) arguments: String,
    /// 工具结果关联使用的稳定调用标识。
    pub(super) call_id: String,
    /// 函数名称。
    pub(super) name: String,
    /// 返回 Item 的对象标识当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// 调用来源当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) caller: Field<Value>,
    /// 函数命名空间当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) namespace: Field<String>,
    /// 返回 Item 的生命周期状态当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) status: Field<ItemStatusWire>,
}

/// 固定函数调用 Item 类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FunctionCallTypeWire {
    /// 函数调用。
    FunctionCall,
}

/// 函数调用结果 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallOutputItemWire {
    #[serde(rename = "type")]
    pub(super) kind: FunctionCallOutputTypeWire,
    /// 对应函数调用的稳定标识。
    pub(super) call_id: String,
    /// 文本或多模态结果。
    pub(super) output: FunctionOutputWire,
    /// 返回 Item 的对象标识当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) id: Field<String>,
    /// 调用来源当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) caller: Field<Value>,
}

/// 固定函数调用结果 Item 类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FunctionCallOutputTypeWire {
    /// 函数调用结果。
    FunctionCallOutput,
}

/// 函数结果支持字符串或内容块列表。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum FunctionOutputWire {
    /// 原样保留的结果字符串。
    Text(String),
    /// 文本、图片或文件结果块。
    Parts(Vec<InputContentWire>),
}

/// Responses 函数工具定义。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolWire {
    /// 客户端函数工具。
    #[serde(rename = "function")]
    Function(FunctionToolWire),
}

/// 函数工具载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionToolWire {
    /// 工具名称。
    pub(super) name: String,
    /// 工具参数 JSON Schema。
    pub(super) parameters: Value,
    /// 是否严格遵循 Schema。
    pub(super) strict: bool,
    /// 工具说明。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) description: Field<String>,
    /// 程序化调用约束当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) allowed_callers: Field<Value>,
    /// 延迟加载当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) defer_loading: Field<bool>,
    /// 工具输出 Schema 当前尚未进入 Canonical。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_schema: Field<Value>,
}

/// 工具选择策略。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ToolChoiceWire {
    /// 简单策略名称。
    Mode(ToolChoiceModeWire),
    /// 指定必须调用的函数。
    Named(NamedToolChoiceWire),
}

/// 简单工具选择策略。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ToolChoiceModeWire {
    /// 禁止调用工具。
    None,
    /// 自动选择。
    Auto,
    /// 必须调用至少一个工具。
    Required,
}

/// 指定函数工具的策略。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedToolChoiceWire {
    #[serde(rename = "type")]
    pub(super) kind: FunctionTypeWire,
    /// 被指定的函数名称。
    pub(super) name: String,
}

/// 固定函数工具类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FunctionTypeWire {
    /// 函数工具。
    Function,
}

/// Responses 推理配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasoningWire {
    /// 推理强度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) effort: Field<ReasoningEffortWire>,
}

/// OpenAI 推理强度。
#[derive(Deserialize)]
pub(super) enum ReasoningEffortWire {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "xhigh")]
    ExtraHigh,
    #[serde(rename = "max")]
    Max,
}
