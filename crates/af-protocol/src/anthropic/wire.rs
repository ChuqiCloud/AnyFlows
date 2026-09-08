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

fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// Anthropic Messages 入站请求。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessagesRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 最大输出令牌数。
    pub(super) max_tokens: i64,
    /// 顶层系统指令。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) system: Field<SystemWire>,
    /// 按请求顺序提供的对话消息。
    pub(super) messages: Vec<MessageWire>,
    /// 客户端声明的函数工具。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tools: Field<Vec<ToolWire>>,
    /// 模型选择工具的策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) tool_choice: Field<ToolChoiceWire>,
    /// 扩展思考配置。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) thinking: Field<ThinkingWire>,
    /// 自适应思考强度配置。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_config: Field<OutputConfigWire>,
    /// 采样温度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) temperature: Field<f64>,
    /// 核采样概率阈值。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) top_p: Field<f64>,
    /// 停止生成的序列。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stop_sequences: Field<Vec<String>>,
    /// 终端用户元数据。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) metadata: Field<MetadataWire>,
    /// 是否请求流式响应。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream: Field<bool>,
}

/// 顶层系统指令可以使用字符串简写或文本块列表。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum SystemWire {
    /// 单个系统指令字符串。
    Text(String),
    /// 按顺序排列的系统文本块。
    Parts(Vec<SystemTextBlockWire>),
}

/// 顶层系统文本块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum SystemTextBlockWire {
    /// 文本系统指令。
    #[serde(rename = "text")]
    Text(TextBlockWire),
}

/// 按角色闭合分派的 Anthropic 消息。
#[derive(Deserialize)]
#[serde(tag = "role")]
pub(super) enum MessageWire {
    /// 用户输入或工具结果消息。
    #[serde(rename = "user")]
    User(UserMessageWire),
    /// 助手历史输出或工具调用消息。
    #[serde(rename = "assistant")]
    Assistant(AssistantMessageWire),
}

/// Anthropic 用户消息。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UserMessageWire {
    /// 字符串或结构化内容块列表。
    pub(super) content: UserContentWire,
}

/// Anthropic 助手消息。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssistantMessageWire {
    /// 字符串或结构化内容块列表。
    pub(super) content: AssistantContentWire,
}

/// 用户消息内容。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum UserContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 文本、图片或工具结果块列表。
    Parts(Vec<UserContentPartWire>),
}

/// 助手消息内容。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AssistantContentWire {
    /// 单个文本字符串。
    Text(String),
    /// 文本或工具调用块列表。
    Parts(Vec<AssistantContentPartWire>),
}

/// 用户消息允许的内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum UserContentPartWire {
    /// 文本内容块。
    #[serde(rename = "text")]
    Text(TextBlockWire),
    /// 图片内容块。
    #[serde(rename = "image")]
    Image(ImageBlockWire),
    /// 工具执行结果块。
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultBlockWire),
}

/// 助手消息允许的内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum AssistantContentPartWire {
    /// 文本内容块。
    #[serde(rename = "text")]
    Text(TextBlockWire),
    /// 工具调用块。
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseBlockWire),
}

/// 文本内容块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TextBlockWire {
    /// 文本内容。
    pub(super) text: String,
    /// 可选提示缓存断点。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) cache_control: Field<CacheControlWire>,
}

/// 图片内容块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageBlockWire {
    /// Base64 或 HTTPS 图片来源。
    pub(super) source: ImageSourceWire,
    /// 可选提示缓存断点。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) cache_control: Field<CacheControlWire>,
}

/// Anthropic 图片来源。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ImageSourceWire {
    /// Base64 内联图片。
    #[serde(rename = "base64")]
    Base64(Base64ImageSourceWire),
    /// HTTPS 远程图片。
    #[serde(rename = "url")]
    Url(UrlImageSourceWire),
}

/// Base64 图片来源。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Base64ImageSourceWire {
    /// 图片媒体类型。
    pub(super) media_type: ImageMediaTypeWire,
    /// Base64 编码的图片内容。
    pub(super) data: String,
}

/// URL 图片来源。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UrlImageSourceWire {
    /// HTTPS 图片地址。
    pub(super) url: String,
}

/// Anthropic 接受的图片媒体类型。
#[derive(Deserialize)]
pub(super) enum ImageMediaTypeWire {
    /// JPEG 图片。
    #[serde(rename = "image/jpeg")]
    Jpeg,
    /// PNG 图片。
    #[serde(rename = "image/png")]
    Png,
    /// GIF 图片。
    #[serde(rename = "image/gif")]
    Gif,
    /// WebP 图片。
    #[serde(rename = "image/webp")]
    Webp,
}

/// 助手发起的工具调用。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolUseBlockWire {
    /// 工具调用稳定标识。
    pub(super) id: String,
    /// 被调用的工具名称。
    pub(super) name: String,
    /// 已解析的工具参数对象。
    pub(super) input: Value,
    /// 可选提示缓存断点。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) cache_control: Field<CacheControlWire>,
}

/// 用户返回的工具执行结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolResultBlockWire {
    /// 对应工具调用的稳定标识。
    pub(super) tool_use_id: String,
    /// 字符串或结构化结果内容。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) content: Field<ToolResultContentWire>,
    /// 工具是否以错误结束。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) is_error: Field<bool>,
    /// 可选提示缓存断点。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) cache_control: Field<CacheControlWire>,
}

/// 工具结果内容。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ToolResultContentWire {
    /// 单个结果字符串。
    Text(String),
    /// 文本或图片结果块列表。
    Parts(Vec<ToolResultContentPartWire>),
}

/// 工具结果内允许的内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolResultContentPartWire {
    /// 文本结果块。
    #[serde(rename = "text")]
    Text(TextBlockWire),
    /// 图片结果块。
    #[serde(rename = "image")]
    Image(ImageBlockWire),
}

/// 客户端声明的函数工具。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolWire {
    /// 工具名称。
    pub(super) name: String,
    /// 工具用途说明。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) description: Field<String>,
    /// 工具参数 JSON Schema。
    pub(super) input_schema: Value,
}

/// Anthropic 工具选择策略。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolChoiceWire {
    /// 由模型决定是否调用工具。
    #[serde(rename = "auto")]
    Auto(ToolChoiceParallelWire),
    /// 禁止调用工具。
    #[serde(rename = "none")]
    None(EmptyWire),
    /// 必须调用任意工具。
    #[serde(rename = "any")]
    Any(ToolChoiceParallelWire),
    /// 必须调用指定工具。
    #[serde(rename = "tool")]
    Tool(NamedToolChoiceWire),
}

/// 可控制并行工具调用的策略载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolChoiceParallelWire {
    /// 是否禁止并行工具调用。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) disable_parallel_tool_use: Field<bool>,
}

/// 指定工具的策略载荷。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedToolChoiceWire {
    /// 必须调用的工具名称。
    pub(super) name: String,
    /// 是否禁止并行工具调用。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) disable_parallel_tool_use: Field<bool>,
}

/// 不携带额外字段的闭合对象。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyWire {}

/// Anthropic 扩展思考配置。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ThinkingWire {
    /// 使用固定令牌预算启用思考。
    #[serde(rename = "enabled")]
    Enabled(ThinkingEnabledWire),
    /// 显式关闭思考。
    #[serde(rename = "disabled")]
    Disabled(EmptyWire),
    /// 由模型自适应决定思考预算。
    #[serde(rename = "adaptive")]
    Adaptive(ThinkingAdaptiveWire),
}

/// 固定预算思考配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThinkingEnabledWire {
    /// 思考令牌预算。
    pub(super) budget_tokens: i64,
    /// 思考内容展示策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) display: Field<ThinkingDisplayWire>,
}

/// 自适应思考配置。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThinkingAdaptiveWire {
    /// 思考内容展示策略。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) display: Field<ThinkingDisplayWire>,
}

/// 思考内容展示策略。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ThinkingDisplayWire {
    /// 返回摘要后的思考内容。
    Summarized,
    /// 不返回思考文本，仅保留连续会话所需签名。
    Omitted,
}

/// Anthropic 输出配置；当前只建模可无损归一化的思考强度。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputConfigWire {
    /// 自适应思考强度。
    pub(super) effort: OutputEffortWire,
}

/// Anthropic 自适应思考强度。
#[derive(Deserialize)]
pub(super) enum OutputEffortWire {
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

/// Anthropic 请求元数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MetadataWire {
    /// 不含直接身份信息的终端用户标识。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) user_id: Field<String>,
}

/// 提示缓存断点。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CacheControlWire {
    /// 固定为 `ephemeral`。
    #[serde(rename = "type")]
    pub(super) kind: CacheControlTypeWire,
    /// 缓存有效期；缺省为五分钟。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) ttl: Field<CacheTtlWire>,
}

/// 提示缓存断点类型。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CacheControlTypeWire {
    /// 临时缓存。
    Ephemeral,
}

/// 提示缓存有效期。
#[derive(Deserialize)]
pub(super) enum CacheTtlWire {
    /// 五分钟。
    #[serde(rename = "5m")]
    FiveMinutes,
    /// 一小时。
    #[serde(rename = "1h")]
    OneHour,
}
