use serde::Deserialize;
use serde_json::{Map, Value};

/// OpenAI Responses 非流式响应。
#[derive(Deserialize)]
pub(super) struct ResponsesResponseWire {
    /// 响应稳定标识。
    pub(super) id: String,
    /// 固定的响应对象类型。
    pub(super) object: ResponseObjectWire,
    /// 响应创建时间的 Unix 秒时间戳。
    pub(super) created_at: i64,
    /// 响应终态。
    pub(super) status: ResponseStatusWire,
    /// 失败响应的错误详情；成功边界只接受空值。
    #[serde(default)]
    pub(super) error: Option<Value>,
    /// 未完整结束的结构化原因。
    #[serde(default)]
    pub(super) incomplete_details: Option<IncompleteDetailsWire>,
    /// 实际生成响应的模型名。
    pub(super) model: String,
    /// 模型按顺序生成的输出 Item。
    pub(super) output: Vec<ResponseOutputItemWire>,
    /// SDK 聚合文本便利字段；API wire 出现非空值时无法无损归一。
    #[serde(default)]
    pub(super) output_text: Option<String>,
    /// 可选审核结果，等待 Canonical 建模。
    #[serde(default)]
    pub(super) moderation: Option<Value>,
    /// 可选的令牌用量。
    #[serde(default)]
    pub(super) usage: Option<ResponseUsageWire>,
    /// Codex 内部访问能力元数据，不属于公开 Responses 语义。
    #[serde(default, rename = "access_programs")]
    pub(super) _access_programs: Option<Value>,
    /// 等待响应 raw 白名单校验的官方回显字段。
    #[serde(flatten)]
    pub(super) extra: Map<String, Value>,
}

/// 固定为 `response` 的对象类型。
#[derive(Deserialize)]
pub(super) enum ResponseObjectWire {
    /// 普通 Responses 响应。
    #[serde(rename = "response")]
    Response,
}

/// Responses 响应生命周期状态。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResponseStatusWire {
    /// 响应已完整结束。
    Completed,
    /// 响应生成失败。
    Failed,
    /// 响应仍在生成。
    InProgress,
    /// 响应已取消。
    Cancelled,
    /// 响应仍在排队。
    Queued,
    /// 响应因长度或内容策略提前结束。
    Incomplete,
}

impl ResponseStatusWire {
    /// 返回官方 wire 字符串。
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::InProgress => "in_progress",
            Self::Cancelled => "cancelled",
            Self::Queued => "queued",
            Self::Incomplete => "incomplete",
        }
    }
}

/// 未完整结束的原因。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncompleteDetailsWire {
    /// 长度或内容策略原因。
    #[serde(default)]
    pub(super) reason: Option<IncompleteReasonWire>,
}

/// OpenAI 当前公开的未完整结束原因。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum IncompleteReasonWire {
    /// 达到最大输出令牌数。
    MaxOutputTokens,
    /// 输出被内容策略拦截。
    ContentFilter,
}

/// 当前 Canonical 可以无损表达的输出 Item。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ResponseOutputItemWire {
    /// 助手消息。
    #[serde(rename = "message")]
    Message(OutputMessageWire),
    /// 函数工具调用。
    #[serde(rename = "function_call")]
    FunctionCall(FunctionCallWire),
    /// 推理摘要与加密连续上下文。
    #[serde(rename = "reasoning")]
    Reasoning(ReasoningItemWire),
    /// 服务端返回的不透明上下文压缩 Item。
    #[serde(rename = "compaction")]
    Compaction(CompactionItemWire),
}

/// Responses 输出中的不透明压缩 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompactionItemWire {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) encrypted_content: String,
}

/// 助手输出消息 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputMessageWire {
    /// OpenAI 分配的 Item 标识。
    pub(super) id: String,
    /// 有序文本或拒绝内容。
    pub(super) content: Vec<OutputContentWire>,
    /// 固定的助手角色。
    pub(super) role: AssistantRoleWire,
    /// Item 的终态。
    pub(super) status: ItemStatusWire,
    /// Codex 类模型的输出阶段，等待 Canonical 建模。
    #[serde(default)]
    pub(super) phase: Option<MessagePhaseWire>,
}

/// 固定为 `assistant` 的响应角色。
#[derive(Deserialize)]
pub(super) enum AssistantRoleWire {
    /// 助手输出。
    #[serde(rename = "assistant")]
    Assistant,
}

/// 输出 Item 的生命周期状态。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ItemStatusWire {
    /// Item 仍在生成。
    InProgress,
    /// Item 已完整结束。
    Completed,
    /// Item 未完整结束。
    Incomplete,
}

/// 助手消息输出阶段。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum MessagePhaseWire {
    /// 中间说明。
    Commentary,
    /// 最终回答。
    FinalAnswer,
}

impl MessagePhaseWire {
    /// 返回官方 wire 字符串。
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Commentary => "commentary",
            Self::FinalAnswer => "final_answer",
        }
    }
}

/// 助手消息中的输出内容。
#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum OutputContentWire {
    /// 普通输出文本。
    #[serde(rename = "output_text")]
    Text(OutputTextWire),
    /// 模型拒绝内容，等待 Canonical 独立建模。
    #[serde(rename = "refusal")]
    Refusal(OutputRefusalWire),
}

/// 普通输出文本块。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputTextWire {
    /// 模型输出文本。
    pub(super) text: String,
    /// 引用标注；非空时无法无损归一。
    #[serde(default)]
    pub(super) annotations: Option<Vec<Value>>,
    /// token logprobs；非空时无法无损归一。
    #[serde(default)]
    pub(super) logprobs: Option<Vec<Value>>,
}

/// 模型拒绝内容。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputRefusalWire {
    /// 面向调用方的拒绝说明。
    pub(super) refusal: String,
}

/// 函数工具调用 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunctionCallWire {
    /// JSON 编码的函数参数对象。
    pub(super) arguments: String,
    /// 工具结果关联使用的真实调用标识。
    pub(super) call_id: String,
    /// 函数名称。
    pub(super) name: String,
    /// OpenAI 分配的可选 Item 标识。
    #[serde(default)]
    pub(super) id: Option<String>,
    /// 程序化调用来源，等待 Canonical 建模。
    #[serde(default)]
    pub(super) caller: Option<Value>,
    /// 函数命名空间，等待 Canonical 建模。
    #[serde(default)]
    pub(super) namespace: Option<String>,
    /// 可选的 Item 生命周期状态。
    #[serde(default)]
    pub(super) status: Option<ItemStatusWire>,
}

/// 推理输出 Item。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasoningItemWire {
    /// OpenAI 分配的 Item 标识。
    pub(super) id: String,
    /// 有序推理摘要；当前只允许零项或一项。
    pub(super) summary: Vec<SummaryTextWire>,
    /// 原始推理文本，等待 Canonical 区分摘要与正文。
    #[serde(default)]
    pub(super) content: Option<Vec<ReasoningTextWire>>,
    /// 无状态续传使用的加密推理上下文。
    #[serde(default)]
    pub(super) encrypted_content: Option<String>,
    /// 可选的 Item 生命周期状态。
    #[serde(default)]
    pub(super) status: Option<ItemStatusWire>,
}

/// 单个推理摘要文本。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SummaryTextWire {
    /// 固定的摘要内容类型。
    #[serde(rename = "type")]
    pub(super) _kind: SummaryTextTypeWire,
    /// 推理摘要文本。
    pub(super) text: String,
}

/// 固定为 `summary_text` 的摘要类型。
#[derive(Deserialize)]
pub(super) enum SummaryTextTypeWire {
    /// 推理摘要文本。
    #[serde(rename = "summary_text")]
    SummaryText,
}

/// 当前尚未进入 Canonical 的原始推理正文。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReasoningTextWire {
    /// 固定的推理正文类型。
    #[serde(rename = "type")]
    pub(super) _kind: ReasoningTextTypeWire,
    /// 原始推理文本。
    #[serde(rename = "text")]
    pub(super) _text: String,
}

/// 固定为 `reasoning_text` 的正文类型。
#[derive(Deserialize)]
pub(super) enum ReasoningTextTypeWire {
    /// 原始推理正文。
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}

/// Responses 返回的令牌用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseUsageWire {
    /// 输入令牌总量。
    pub(super) input_tokens: i64,
    /// 输入令牌细分。
    #[serde(default)]
    pub(super) input_tokens_details: Option<InputTokensDetailsWire>,
    /// 部分兼容上游返回的 Chat 风格输入令牌细分别名。
    #[serde(default)]
    pub(super) prompt_tokens_details: Option<InputTokensDetailsWire>,
    /// 输出令牌总量。
    pub(super) output_tokens: i64,
    /// 输出令牌细分。
    #[serde(default)]
    pub(super) output_tokens_details: Option<OutputTokensDetailsWire>,
    /// 输入与输出令牌冗余总量。
    pub(super) total_tokens: i64,
    /// Codex 内部归因信息，不参与 AnyFlows 计费语义。
    #[serde(default, rename = "attribution")]
    pub(super) _attribution: Option<Value>,
}

/// Responses 输入令牌细分。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputTokensDetailsWire {
    /// 缓存写入令牌数，当前缺少 TTL 拆分。
    #[serde(default)]
    pub(super) cache_write_tokens: Option<i64>,
    /// 缓存命中的输入令牌数。
    #[serde(default)]
    pub(super) cached_tokens: Option<i64>,
}

/// Responses 输出令牌细分。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputTokensDetailsWire {
    /// 内部推理令牌数。
    #[serde(default)]
    pub(super) reasoning_tokens: Option<i64>,
}
