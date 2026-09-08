use std::{error::Error, fmt};

use serde_json::Value;

use crate::{canonical::MediaSource, usage::TokenCount};

/// 客户端声明的函数工具。
#[derive(Clone, PartialEq)]
pub struct ToolDef {
    /// 跨消息引用的工具名称。
    pub name: String,
    /// 提供给模型的工具说明。
    pub description: Option<String>,
    /// 工具参数的 JSON Schema。
    pub input_schema: Value,
    /// 是否要求上游严格遵循工具参数 Schema；`None` 表示来源协议未声明。
    pub strict: Option<bool>,
}

impl fmt::Debug for ToolDef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToolDef")
            .field("name", &"<已脱敏>")
            .field("has_description", &self.description.is_some())
            .field("input_schema", &"<已脱敏>")
            .field("strict", &self.strict)
            .finish()
    }
}

/// 模型选择工具的协议无关策略。
#[derive(Clone, Eq, PartialEq)]
pub enum ToolChoice {
    /// 由模型决定是否调用工具。
    Auto,
    /// 禁止调用工具。
    None,
    /// 必须调用任意一个已声明工具。
    Required,
    /// 必须调用指定工具。
    Named {
        /// 已声明的工具名称。
        name: String,
    },
}

impl fmt::Debug for ToolChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => formatter.write_str("Auto"),
            Self::None => formatter.write_str("None"),
            Self::Required => formatter.write_str("Required"),
            Self::Named { .. } => formatter.write_str("Named(<已脱敏>)"),
        }
    }
}

/// 客户端对流式响应的协议无关偏好。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamOptions {
    include_usage: bool,
}

impl StreamOptions {
    /// 未请求任何可选流式数据的显式空配置。
    pub const EMPTY: Self = Self {
        include_usage: false,
    };

    /// 构造流式响应偏好。
    #[must_use]
    pub const fn new(include_usage: bool) -> Self {
        Self { include_usage }
    }

    /// 返回客户端是否要求在流末接收完整 usage。
    #[must_use]
    pub const fn include_usage(&self) -> bool {
        self.include_usage
    }
}

/// 跨协议归一化的推理强度。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReasoningEffort {
    /// 显式关闭推理。
    None,
    /// 最低推理强度。
    Minimal,
    /// 低推理强度。
    Low,
    /// 中等推理强度。
    Medium,
    /// 高推理强度。
    High,
    /// 超高推理强度，对应部分协议的 `xhigh`。
    ExtraHigh,
    /// 厂商允许的最大推理强度。
    Max,
}

/// 协议无关的推理参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReasoningConfig {
    effort: Option<ReasoningEffort>,
    budget_tokens: Option<TokenCount>,
    include_thinking: bool,
}

impl ReasoningConfig {
    /// 校验并构造推理强度、预算与思考内容输出策略。
    pub const fn new(
        effort: Option<ReasoningEffort>,
        budget_tokens: Option<TokenCount>,
        include_thinking: bool,
    ) -> Result<Self, ReasoningConfigError> {
        if matches!(effort, Some(ReasoningEffort::None))
            && (budget_tokens.is_some() || include_thinking)
        {
            return Err(ReasoningConfigError::DisabledWithActiveOptions);
        }
        Ok(Self {
            effort,
            budget_tokens,
            include_thinking,
        })
    }

    /// 返回归一化推理强度。
    #[must_use]
    pub const fn effort(&self) -> Option<ReasoningEffort> {
        self.effort
    }

    /// 返回推理令牌预算。
    #[must_use]
    pub const fn budget_tokens(&self) -> Option<TokenCount> {
        self.budget_tokens
    }

    /// 返回是否要求上游输出思考内容。
    #[must_use]
    pub const fn include_thinking(&self) -> bool {
        self.include_thinking
    }
}

/// 推理配置校验错误，不保留外部参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReasoningConfigError {
    /// 显式关闭推理时仍携带预算或要求输出思考内容。
    DisabledWithActiveOptions,
}

impl fmt::Display for ReasoningConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisabledWithActiveOptions => {
                formatter.write_str("关闭推理时不得设置预算或输出思考内容")
            }
        }
    }
}

impl Error for ReasoningConfigError {}

/// 协议无关的采样参数。
#[derive(Clone, PartialEq)]
pub struct Sampling {
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_output_tokens: Option<TokenCount>,
    stop_sequences: Vec<String>,
}

impl Sampling {
    /// 未指定任何采样覆盖的显式空配置。
    pub const EMPTY: Self = Self {
        temperature: None,
        top_p: None,
        max_output_tokens: None,
        stop_sequences: Vec::new(),
    };

    /// 校验并构造通用采样参数。
    pub fn new(
        temperature: Option<f64>,
        top_p: Option<f64>,
        max_output_tokens: Option<TokenCount>,
        stop_sequences: Vec<String>,
    ) -> Result<Self, SamplingError> {
        if temperature.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(SamplingError::InvalidTemperature);
        }
        if top_p.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
            return Err(SamplingError::InvalidTopP);
        }
        Ok(Self {
            temperature,
            top_p,
            max_output_tokens,
            stop_sequences,
        })
    }

    /// 返回 temperature 覆盖值。
    #[must_use]
    pub const fn temperature(&self) -> Option<f64> {
        self.temperature
    }

    /// 返回 top_p 覆盖值。
    #[must_use]
    pub const fn top_p(&self) -> Option<f64> {
        self.top_p
    }

    /// 返回最大输出令牌数。
    #[must_use]
    pub const fn max_output_tokens(&self) -> Option<TokenCount> {
        self.max_output_tokens
    }

    /// 返回停止序列；内容不得写入日志。
    #[must_use]
    pub fn stop_sequences(&self) -> &[String] {
        &self.stop_sequences
    }
}

impl fmt::Debug for Sampling {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sampling")
            .field("temperature", &self.temperature)
            .field("top_p", &self.top_p)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("stop_sequence_count", &self.stop_sequences.len())
            .finish()
    }
}

/// 采样参数校验错误，不保留外部数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplingError {
    /// temperature 不是有限非负数。
    InvalidTemperature,
    /// top_p 不在闭区间 0 到 1 内。
    InvalidTopP,
}

impl fmt::Display for SamplingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTemperature => formatter.write_str("temperature 必须是有限非负数"),
            Self::InvalidTopP => formatter.write_str("top_p 必须在 0 到 1 之间"),
        }
    }
}

impl Error for SamplingError {}

/// 未内联到消息内容中的多模态附件。
#[derive(Clone, PartialEq)]
pub struct Attachment {
    /// 附件来源。
    pub source: MediaSource,
    /// 已校验的可选媒体类型。
    pub mime_type: Option<String>,
    /// 提供给模型的可选文件名。
    pub filename: Option<String>,
}

/// 跨请求延续模型上下文所需的协议无关引用。
#[derive(Clone, Eq, PartialEq)]
pub struct RequestContinuation {
    previous_response_id: Option<String>,
    conversation_id: Option<String>,
    prompt_cache_key: Option<String>,
}

impl RequestContinuation {
    /// 未声明任何延续引用的显式空值。
    pub const EMPTY: Self = Self {
        previous_response_id: None,
        conversation_id: None,
        prompt_cache_key: None,
    };

    /// 构造已由协议边界校验的响应、会话与提示缓存引用。
    #[must_use]
    pub const fn new(
        previous_response_id: Option<String>,
        conversation_id: Option<String>,
        prompt_cache_key: Option<String>,
    ) -> Self {
        Self {
            previous_response_id,
            conversation_id,
            prompt_cache_key,
        }
    }

    /// 返回需要接续的上一响应标识。
    #[must_use]
    pub fn previous_response_id(&self) -> Option<&str> {
        self.previous_response_id.as_deref()
    }

    /// 返回需要接续的持久会话标识。
    #[must_use]
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    /// 返回用于上游提示缓存亲和的稳定键。
    #[must_use]
    pub fn prompt_cache_key(&self) -> Option<&str> {
        self.prompt_cache_key.as_deref()
    }

    /// 判断请求是否没有任何延续引用。
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.previous_response_id.is_none()
            && self.conversation_id.is_none()
            && self.prompt_cache_key.is_none()
    }
}

impl fmt::Debug for RequestContinuation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestContinuation")
            .field(
                "has_previous_response_id",
                &self.previous_response_id.is_some(),
            )
            .field("has_conversation_id", &self.conversation_id.is_some())
            .field("has_prompt_cache_key", &self.prompt_cache_key.is_some())
            .finish()
    }
}

impl fmt::Debug for Attachment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Attachment")
            .field("source", &"<已脱敏>")
            .field("has_mime_type", &self.mime_type.is_some())
            .field("has_filename", &self.filename.is_some())
            .finish()
    }
}

/// 网关用于粘性会话等能力的请求元数据。
#[derive(Clone, Eq, PartialEq)]
pub struct RequestMetadata {
    user_id: Option<String>,
    session_id: Option<String>,
}

impl RequestMetadata {
    /// 未携带任何网关元数据的显式空值。
    pub const EMPTY: Self = Self {
        user_id: None,
        session_id: None,
    };

    /// 构造已由协议边界校验的用户与会话标识。
    #[must_use]
    pub const fn new(user_id: Option<String>, session_id: Option<String>) -> Self {
        Self {
            user_id,
            session_id,
        }
    }

    /// 返回协议提供的用户标识。
    #[must_use]
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// 返回协议提供的会话标识。
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}

impl fmt::Debug for RequestMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestMetadata")
            .field("has_user_id", &self.user_id.is_some())
            .field("has_session_id", &self.session_id.is_some())
            .finish()
    }
}
