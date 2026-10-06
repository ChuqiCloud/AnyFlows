use serde::Deserialize;
use serde_json::Value;

use super::wire::ToolCallWire;

/// OpenAI Chat Completions 非流式响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChatResponseWire {
    /// 上游响应标识。
    pub(super) id: String,
    /// NewAPI 兼容字段；Canonical 响应使用 `id` 作为稳定标识。
    #[serde(default)]
    pub(super) request_id: Option<String>,
    /// 固定的响应对象类型。
    pub(super) object: ChatCompletionObjectWire,
    /// 响应创建时间的 Unix 秒时间戳。
    pub(super) created: i64,
    /// 实际生成响应的模型名。
    pub(super) model: String,
    /// 上游返回的候选结果。
    pub(super) choices: Vec<ResponseChoiceWire>,
    /// 可选的服务等级结果。
    #[serde(default)]
    pub(super) service_tier: Option<ServiceTierWire>,
    /// 可选的后端配置指纹。
    #[serde(default)]
    pub(super) system_fingerprint: Option<String>,
    /// 可选的令牌用量。
    #[serde(default)]
    pub(super) usage: Option<CompletionUsageWire>,
    /// 当前 Canonical 尚未建模的审核结果。
    #[serde(default)]
    pub(super) moderation: Option<Value>,
}

/// 固定为 `chat.completion` 的对象类型。
#[derive(Deserialize)]
pub(super) enum ChatCompletionObjectWire {
    /// 非流式 Chat Completions 响应。
    #[serde(rename = "chat.completion")]
    ChatCompletion,
}

/// 非流式响应中的单个候选结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseChoiceWire {
    /// 上游提供的候选索引。
    pub(super) index: u32,
    /// 候选生成的助手消息。
    pub(super) message: ResponseMessageWire,
    /// 候选结束原因。
    pub(super) finish_reason: FinishReasonWire,
    /// 当前 Canonical 尚未建模的 token logprobs。
    #[serde(default)]
    pub(super) logprobs: Option<Value>,
}

/// OpenAI Chat 的候选结束原因。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FinishReasonWire {
    /// 模型自然结束或命中停止序列。
    Stop,
    /// 输出达到长度限制。
    Length,
    /// 模型生成了工具调用。
    ToolCalls,
    /// 内容被安全策略过滤。
    ContentFilter,
    /// 已弃用的旧式函数调用结束原因。
    FunctionCall,
}

/// 模型生成的助手消息。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResponseMessageWire {
    /// 文本内容；工具调用响应通常为 `null`。
    #[serde(default)]
    pub(super) content: Option<String>,
    /// NewAPI 返回的非流式推理文本；当前 Canonical Chat 响应不单独暴露该字段。
    #[serde(default)]
    pub(super) reasoning_content: Option<String>,
    /// 部分 NewAPI 版本使用的推理文本别名。
    #[serde(default)]
    pub(super) reasoning: Option<String>,
    /// 固定的助手角色。
    pub(super) role: AssistantRoleWire,
    /// 当前 Canonical 尚未建模的拒绝内容。
    #[serde(default)]
    pub(super) refusal: Option<String>,
    /// 当前 Canonical 尚未建模的引用标注。
    #[serde(default)]
    pub(super) annotations: Option<Vec<Value>>,
    /// 当前 Canonical 尚未建模的音频输出。
    #[serde(default)]
    pub(super) audio: Option<Value>,
    /// 已弃用的旧式函数调用。
    #[serde(default)]
    pub(super) function_call: Option<Value>,
    /// 模型生成的函数工具调用。
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<ToolCallWire>>,
}

/// 固定为 `assistant` 的响应消息角色。
#[derive(Deserialize)]
pub(super) enum AssistantRoleWire {
    /// 助手输出消息。
    #[serde(rename = "assistant")]
    Assistant,
}

/// 上游实际使用的服务等级。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceTierWire {
    /// 由项目配置自动选择。
    Auto,
    /// 标准处理等级。
    Default,
    /// 弹性处理等级。
    Flex,
    /// Scale 处理等级。
    Scale,
    /// Priority 处理等级。
    Priority,
}

impl ServiceTierWire {
    /// 返回 OpenAI wire 字符串。
    pub(super) const fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Default => "default",
            Self::Flex => "flex",
            Self::Scale => "scale",
            Self::Priority => "priority",
        }
    }
}

/// OpenAI Chat 返回的令牌用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompletionUsageWire {
    /// 输入提示令牌总量。
    pub(super) prompt_tokens: i64,
    /// 输出补全令牌总量。
    pub(super) completion_tokens: i64,
    /// 可选的冗余总令牌数，用于一致性校验。
    #[serde(default)]
    pub(super) total_tokens: Option<i64>,
    /// 可选的输入令牌明细。
    #[serde(default)]
    pub(super) prompt_tokens_details: Option<PromptTokensDetailsWire>,
    /// 可选的输出令牌明细。
    #[serde(default)]
    pub(super) completion_tokens_details: Option<CompletionTokensDetailsWire>,
    /// DeepSeek 兼容接口返回的缓存命中输入令牌数。
    #[serde(default)]
    pub(super) prompt_cache_hit_tokens: Option<i64>,
    /// DeepSeek 兼容接口返回的缓存未命中输入令牌数。
    #[serde(default)]
    pub(super) prompt_cache_miss_tokens: Option<i64>,
}

/// OpenAI 输入提示令牌明细。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PromptTokensDetailsWire {
    /// 缓存命中的输入令牌数。
    #[serde(default)]
    pub(super) cached_tokens: Option<i64>,
    /// 音频输入令牌数。
    #[serde(default)]
    pub(super) audio_tokens: Option<i64>,
    /// 未区分缓存时长的写入令牌数。
    #[serde(default)]
    pub(super) cache_write_tokens: Option<i64>,
}

/// OpenAI 输出补全令牌明细。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompletionTokensDetailsWire {
    /// 推理令牌数。
    #[serde(default)]
    pub(super) reasoning_tokens: Option<i64>,
    /// 音频输出令牌数。
    #[serde(default)]
    pub(super) audio_tokens: Option<i64>,
    /// 命中预测输出的令牌数。
    #[serde(default)]
    pub(super) accepted_prediction_tokens: Option<i64>,
    /// 未命中预测输出的令牌数。
    #[serde(default)]
    pub(super) rejected_prediction_tokens: Option<i64>,
}
