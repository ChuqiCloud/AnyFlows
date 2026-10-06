use serde::Deserialize;
use serde_json::Value;

/// Anthropic Messages 非流式响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessagesResponseWire {
    /// 上游消息标识。
    pub(super) id: String,
    /// 固定的响应对象类型。
    #[serde(rename = "type")]
    pub(super) _kind: MessageObjectWire,
    /// 固定的助手角色。
    #[serde(rename = "role")]
    pub(super) _role: AssistantRoleWire,
    /// 模型生成的有序内容块。
    pub(super) content: Vec<ResponseContentBlockWire>,
    /// 实际生成响应的模型名。
    pub(super) model: String,
    /// 代码执行容器；当前 Canonical 尚未建模。
    #[serde(default)]
    pub(super) container: Option<Value>,
    /// 非流式响应的终止原因。
    pub(super) stop_reason: StopReasonWire,
    /// 实际命中的自定义停止序列。
    #[serde(default)]
    pub(super) stop_sequence: Option<String>,
    /// 结构化拒绝详情；当前 Canonical 尚未建模。
    #[serde(default)]
    pub(super) stop_details: Option<RefusalStopDetailsWire>,
    /// 上游返回的令牌用量。
    pub(super) usage: UsageWire,
}

/// 固定为 `message` 的对象类型。
#[derive(Deserialize)]
pub(super) enum MessageObjectWire {
    /// Messages 非流式响应。
    #[serde(rename = "message")]
    Message,
}

/// 固定为 `assistant` 的响应角色。
#[derive(Deserialize)]
pub(super) enum AssistantRoleWire {
    /// 助手输出消息。
    #[serde(rename = "assistant")]
    Assistant,
}

/// Anthropic Messages 的终止原因。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StopReasonWire {
    /// 模型自然结束。
    EndTurn,
    /// 达到输出令牌上限。
    MaxTokens,
    /// 命中自定义停止序列。
    StopSequence,
    /// 模型生成了工具调用。
    ToolUse,
    /// 长任务暂停，等待客户端续传当前响应。
    PauseTurn,
    /// 安全策略拒绝继续生成。
    Refusal,
    /// 达到模型上下文窗口上限。
    ModelContextWindowExceeded,
}

/// Anthropic 返回的结构化拒绝详情。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum RefusalStopDetailsWire {
    /// 安全策略拒绝。
    #[serde(rename = "refusal")]
    Refusal(RefusalDetailsWire),
}

/// 安全策略拒绝的分类与说明。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RefusalDetailsWire {
    /// 触发拒绝的策略分类。
    #[serde(default)]
    pub(super) category: Option<RefusalCategoryWire>,
    /// 面向调用方的可选说明。
    #[serde(default)]
    pub(super) explanation: Option<String>,
}

/// Anthropic 当前公开的拒绝分类。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RefusalCategoryWire {
    /// 网络安全风险。
    Cyber,
    /// 生物安全风险。
    Bio,
    /// 前沿模型开发限制。
    FrontierLlm,
    /// 推理内容提取限制。
    ReasoningExtraction,
    /// 一般有害内容。
    GeneralHarms,
}

impl RefusalCategoryWire {
    /// 返回 Anthropic wire 字符串。
    pub(super) const fn as_str(&self) -> &'static str {
        match self {
            Self::Cyber => "cyber",
            Self::Bio => "bio",
            Self::FrontierLlm => "frontier_llm",
            Self::ReasoningExtraction => "reasoning_extraction",
            Self::GeneralHarms => "general_harms",
        }
    }
}

/// Anthropic 响应允许的已建模内容块。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ResponseContentBlockWire {
    /// 普通文本内容。
    #[serde(rename = "text")]
    Text(ResponseTextBlockWire),
    /// 可见思考内容及连续会话签名。
    #[serde(rename = "thinking")]
    Thinking(ResponseThinkingBlockWire),
    /// 客户端工具调用。
    #[serde(rename = "tool_use")]
    ToolUse(ResponseToolUseBlockWire),
}

/// Anthropic 响应文本块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseTextBlockWire {
    /// 模型输出文本。
    pub(super) text: String,
    /// 文本引用；非空引用等待 Canonical 建模后再放行。
    #[serde(default)]
    pub(super) citations: Option<Vec<Value>>,
}

/// Anthropic 响应思考块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseThinkingBlockWire {
    /// 可见的思考文本。
    pub(super) thinking: String,
    /// 连续会话所需的厂商签名。
    pub(super) signature: String,
}

/// Anthropic 响应工具调用块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseToolUseBlockWire {
    /// 工具调用标识。
    pub(super) id: String,
    /// 工具名称。
    pub(super) name: String,
    /// 已解析的工具参数。
    pub(super) input: Value,
    /// 调用来源；旧版响应可能不提供该字段。
    #[serde(default, rename = "caller")]
    pub(super) _caller: Option<ToolCallerWire>,
}

/// 工具调用来源。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ToolCallerWire {
    /// 模型直接调用客户端声明的工具。
    #[serde(rename = "direct")]
    Direct(EmptyResponseWire),
}

/// 不携带额外字段的响应对象。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyResponseWire {}

/// Anthropic Messages 返回的令牌用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UsageWire {
    /// 不含缓存读写的输入令牌数。
    pub(super) input_tokens: i64,
    /// 缓存写入令牌总数。
    #[serde(default)]
    pub(super) cache_creation_input_tokens: Option<i64>,
    /// 缓存命中令牌数。
    #[serde(default)]
    pub(super) cache_read_input_tokens: Option<i64>,
    /// 按 TTL 拆分的缓存写入令牌数。
    #[serde(default)]
    pub(super) cache_creation: Option<CacheCreationWire>,
    /// 输出令牌总数。
    pub(super) output_tokens: i64,
    /// 输出令牌分类明细。
    #[serde(default)]
    pub(super) output_tokens_details: Option<OutputTokensDetailsWire>,
    /// 推理执行地域；当前 Canonical 尚未建模。
    #[serde(default)]
    pub(super) inference_geo: Option<String>,
    /// 服务端工具调用计数；当前 Canonical 尚未建模。
    #[serde(default)]
    pub(super) server_tool_use: Option<ServerToolUsageWire>,
    /// 实际使用的服务等级；当前 Canonical 尚未建模。
    #[serde(default)]
    pub(super) service_tier: Option<ServiceTierWire>,
}

/// 缓存写入令牌的 TTL 明细。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CacheCreationWire {
    /// 五分钟缓存写入令牌数。
    pub(super) ephemeral_5m_input_tokens: i64,
    /// 一小时缓存写入令牌数。
    pub(super) ephemeral_1h_input_tokens: i64,
}

/// 输出令牌分类明细。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputTokensDetailsWire {
    /// 内部思考使用的输出令牌数。
    pub(super) thinking_tokens: i64,
}

/// Anthropic 服务端工具调用计数。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServerToolUsageWire {
    /// Web Fetch 请求次数。
    #[serde(default)]
    pub(super) web_fetch_requests: Option<i64>,
    /// Web Search 请求次数。
    #[serde(default)]
    pub(super) web_search_requests: Option<i64>,
}

/// Anthropic 实际使用的服务等级。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceTierWire {
    /// 标准服务等级。
    Standard,
    /// 优先服务等级。
    Priority,
    /// 批处理服务等级。
    Batch,
}

impl ServiceTierWire {
    /// 返回 Anthropic wire 字符串。
    pub(super) const fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Priority => "priority",
            Self::Batch => "batch",
        }
    }
}
