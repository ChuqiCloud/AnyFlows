use serde::Deserialize;
use serde_json::Value;

use super::wire::{ContentWire, Field, deserialize_field};

/// Gemini `generateContent` 非流式响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerateContentResponseWire {
    /// 模型生成的候选结果。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) candidates: Field<Vec<CandidateWire>>,
    /// 请求提示的安全反馈。
    #[serde(
        rename = "promptFeedback",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) prompt_feedback: Field<PromptFeedbackWire>,
    /// 实际模型版本。
    #[serde(
        rename = "modelVersion",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) model_version: Field<String>,
    /// 响应稳定标识。
    #[serde(rename = "responseId", default, deserialize_with = "deserialize_field")]
    pub(super) response_id: Field<String>,
    /// 令牌用量元数据。
    #[serde(
        rename = "usageMetadata",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) usage_metadata: Field<UsageMetadataWire>,
    /// 当前模型生命周期状态。
    #[serde(
        rename = "modelStatus",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) model_status: Field<ModelStatusWire>,
}

/// Gemini 响应中的单个候选结果。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CandidateWire {
    /// 候选索引。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) index: Field<i64>,
    /// 模型生成的内容。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) content: Field<ContentWire>,
    /// 模型结束生成的原因。
    #[serde(
        rename = "finishReason",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) finish_reason: Field<FinishReasonWire>,
    /// 结束原因的可选说明。
    #[serde(
        rename = "finishMessage",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) finish_message: Field<String>,
    /// 候选安全评级。
    #[serde(
        rename = "safetyRatings",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) safety_ratings: Field<Vec<SafetyRatingWire>>,
    /// 当前候选的令牌数。
    #[serde(rename = "tokenCount", default, deserialize_with = "deserialize_field")]
    pub(super) token_count: Field<i64>,
    /// Grounding 元数据，等待 Canonical 建模。
    #[serde(
        rename = "groundingMetadata",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) grounding_metadata: Field<Value>,
    /// token logprobs，等待 Canonical 建模。
    #[serde(
        rename = "logprobsResult",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) logprobs_result: Field<Value>,
    /// 平均 logprob，等待 Canonical 建模。
    #[serde(
        rename = "avgLogprobs",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) avg_logprobs: Field<f64>,
    /// URL Context 元数据，等待 Canonical 建模。
    #[serde(
        rename = "urlContextMetadata",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) url_context_metadata: Field<Value>,
    /// 旧 Grounding 归因，等待 Canonical 建模。
    #[serde(
        rename = "groundingAttributions",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) grounding_attributions: Field<Vec<Value>>,
    /// 引用元数据，等待 Canonical 建模。
    #[serde(
        rename = "citationMetadata",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) citation_metadata: Field<Value>,
}

/// Gemini 候选结束原因。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(super) enum FinishReasonWire {
    /// 未指定原因，非流式终态不得使用。
    #[serde(rename = "FINISH_REASON_UNSPECIFIED")]
    Unspecified,
    /// 自然结束或命中停止词。
    #[serde(rename = "STOP")]
    Stop,
    /// 达到最大输出令牌数。
    #[serde(rename = "MAX_TOKENS")]
    MaxTokens,
    /// 安全策略拦截。
    #[serde(rename = "SAFETY")]
    Safety,
    /// 复述策略拦截。
    #[serde(rename = "RECITATION")]
    Recitation,
    /// 不支持的语言。
    #[serde(rename = "LANGUAGE")]
    Language,
    /// 其他原因。
    #[serde(rename = "OTHER")]
    Other,
    /// 命中术语阻止列表。
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    /// 可能包含禁止内容。
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
    /// 可能包含敏感个人信息。
    #[serde(rename = "SPII")]
    Spii,
    /// 函数调用格式无效。
    #[serde(rename = "MALFORMED_FUNCTION_CALL")]
    MalformedFunctionCall,
    /// 生成图片触发安全策略。
    #[serde(rename = "IMAGE_SAFETY")]
    ImageSafety,
    /// 生成图片包含禁止内容。
    #[serde(rename = "IMAGE_PROHIBITED_CONTENT")]
    ImageProhibitedContent,
    /// 图片生成因其他原因停止。
    #[serde(rename = "IMAGE_OTHER")]
    ImageOther,
    /// 预期图片但未生成。
    #[serde(rename = "NO_IMAGE")]
    NoImage,
    /// 图片生成命中复述策略。
    #[serde(rename = "IMAGE_RECITATION")]
    ImageRecitation,
    /// 未启用工具却生成了工具调用。
    #[serde(rename = "UNEXPECTED_TOOL_CALL")]
    UnexpectedToolCall,
    /// 连续工具调用次数过多。
    #[serde(rename = "TOO_MANY_TOOL_CALLS")]
    TooManyToolCalls,
    /// 请求缺少思考签名。
    #[serde(rename = "MISSING_THOUGHT_SIGNATURE")]
    MissingThoughtSignature,
    /// 响应结构无效。
    #[serde(rename = "MALFORMED_RESPONSE")]
    MalformedResponse,
    /// 升级规则过滤。
    #[serde(rename = "ESCALATION")]
    Escalation,
}

impl FinishReasonWire {
    /// 返回官方 wire 字符串。
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "FINISH_REASON_UNSPECIFIED",
            Self::Stop => "STOP",
            Self::MaxTokens => "MAX_TOKENS",
            Self::Safety => "SAFETY",
            Self::Recitation => "RECITATION",
            Self::Language => "LANGUAGE",
            Self::Other => "OTHER",
            Self::Blocklist => "BLOCKLIST",
            Self::ProhibitedContent => "PROHIBITED_CONTENT",
            Self::Spii => "SPII",
            Self::MalformedFunctionCall => "MALFORMED_FUNCTION_CALL",
            Self::ImageSafety => "IMAGE_SAFETY",
            Self::ImageProhibitedContent => "IMAGE_PROHIBITED_CONTENT",
            Self::ImageOther => "IMAGE_OTHER",
            Self::NoImage => "NO_IMAGE",
            Self::ImageRecitation => "IMAGE_RECITATION",
            Self::UnexpectedToolCall => "UNEXPECTED_TOOL_CALL",
            Self::TooManyToolCalls => "TOO_MANY_TOOL_CALLS",
            Self::MissingThoughtSignature => "MISSING_THOUGHT_SIGNATURE",
            Self::MalformedResponse => "MALFORMED_RESPONSE",
            Self::Escalation => "ESCALATION",
        }
    }

    /// 返回该原因是否归入 Canonical 内容过滤终态。
    pub(super) const fn is_content_filter(self) -> bool {
        !matches!(self, Self::Unspecified | Self::Stop | Self::MaxTokens)
    }
}

/// 请求提示的安全反馈。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PromptFeedbackWire {
    /// 提示安全评级。
    #[serde(
        rename = "safetyRatings",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) safety_ratings: Field<Vec<SafetyRatingWire>>,
    /// 提示被整体阻止的原因。
    #[serde(
        rename = "blockReason",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) block_reason: Field<BlockReasonWire>,
}

/// Gemini 提示阻止原因。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(super) enum BlockReasonWire {
    /// 未指定原因，显式返回时无有效语义。
    #[serde(rename = "BLOCK_REASON_UNSPECIFIED")]
    Unspecified,
    /// 安全策略拦截。
    #[serde(rename = "SAFETY")]
    Safety,
    /// 未知原因。
    #[serde(rename = "OTHER")]
    Other,
    /// 命中术语阻止列表。
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    /// 命中禁止内容策略。
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
    /// 图片生成安全策略拦截。
    #[serde(rename = "IMAGE_SAFETY")]
    ImageSafety,
}

/// 单项安全评级。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SafetyRatingWire {
    /// 危害类别。
    pub(super) category: SafetyCategoryWire,
    /// 危害概率。
    pub(super) probability: HarmProbabilityWire,
    /// 当前评级是否导致内容被阻止。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) blocked: Field<bool>,
}

/// Gemini 官方危害类别。
#[derive(Clone, Copy, Deserialize, Eq, Hash, PartialEq)]
pub(super) enum SafetyCategoryWire {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "HARM_CATEGORY_DEROGATORY")]
    Derogatory,
    #[serde(rename = "HARM_CATEGORY_TOXICITY")]
    Toxicity,
    #[serde(rename = "HARM_CATEGORY_VIOLENCE")]
    Violence,
    #[serde(rename = "HARM_CATEGORY_SEXUAL")]
    Sexual,
    #[serde(rename = "HARM_CATEGORY_MEDICAL")]
    Medical,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS")]
    Dangerous,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    Harassment,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HateSpeech,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    SexuallyExplicit,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    DangerousContent,
    #[serde(rename = "HARM_CATEGORY_CIVIC_INTEGRITY")]
    CivicIntegrity,
    #[serde(rename = "HARM_CATEGORY_JAILBREAK")]
    Jailbreak,
}

/// Gemini 官方危害概率。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(super) enum HarmProbabilityWire {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "NEGLIGIBLE")]
    Negligible,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}

/// Gemini 用量元数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UsageMetadataWire {
    #[serde(
        rename = "promptTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) prompt_token_count: Field<i64>,
    #[serde(
        rename = "candidatesTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) candidates_token_count: Field<i64>,
    #[serde(
        rename = "totalTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) total_token_count: Field<i64>,
    #[serde(
        rename = "cachedContentTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) cached_content_token_count: Field<i64>,
    #[serde(
        rename = "thoughtsTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) thoughts_token_count: Field<i64>,
    #[serde(
        rename = "toolUsePromptTokenCount",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) tool_use_prompt_token_count: Field<i64>,
    #[serde(
        rename = "promptTokensDetails",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) prompt_tokens_details: Field<Vec<ModalityTokenCountWire>>,
    #[serde(
        rename = "cacheTokensDetails",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) cache_tokens_details: Field<Vec<ModalityTokenCountWire>>,
    #[serde(
        rename = "candidatesTokensDetails",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) candidates_tokens_details: Field<Vec<ModalityTokenCountWire>>,
    #[serde(
        rename = "toolUsePromptTokensDetails",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) tool_use_prompt_tokens_details: Field<Vec<ModalityTokenCountWire>>,
    #[serde(
        rename = "serviceTier",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) service_tier: Field<ServiceTierWire>,
}

/// 单一模态的令牌计数。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModalityTokenCountWire {
    /// 令牌所属模态。
    pub(super) modality: ModalityWire,
    /// 当前模态令牌数。
    #[serde(rename = "tokenCount")]
    pub(super) token_count: i64,
}

/// Gemini 官方模态类型。
#[derive(Clone, Copy, Deserialize, Eq, Hash, PartialEq)]
pub(super) enum ModalityWire {
    #[serde(rename = "MODALITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "TEXT")]
    Text,
    #[serde(rename = "IMAGE")]
    Image,
    #[serde(rename = "VIDEO")]
    Video,
    #[serde(rename = "AUDIO")]
    Audio,
    #[serde(rename = "DOCUMENT")]
    Document,
}

/// Gemini 实际使用的服务等级。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(super) enum ServiceTierWire {
    #[serde(rename = "unspecified")]
    Unspecified,
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "flex")]
    Flex,
    #[serde(rename = "priority")]
    Priority,
}

/// 当前模型生命周期状态。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelStatusWire {
    /// 模型发布阶段。
    #[serde(rename = "modelStage", default, deserialize_with = "deserialize_field")]
    pub(super) model_stage: Field<ModelStageWire>,
    /// 计划退役时间。
    #[serde(
        rename = "retirementTime",
        default,
        deserialize_with = "deserialize_field"
    )]
    pub(super) retirement_time: Field<String>,
    /// 生命周期说明。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) message: Field<String>,
}

/// Gemini 模型生命周期阶段。
#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
pub(super) enum ModelStageWire {
    #[serde(rename = "MODEL_STAGE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "UNSTABLE_EXPERIMENTAL")]
    UnstableExperimental,
    #[serde(rename = "EXPERIMENTAL")]
    Experimental,
    #[serde(rename = "PREVIEW")]
    Preview,
    #[serde(rename = "STABLE")]
    Stable,
    #[serde(rename = "LEGACY")]
    Legacy,
    #[serde(rename = "DEPRECATED")]
    Deprecated,
    #[serde(rename = "RETIRED")]
    Retired,
}
