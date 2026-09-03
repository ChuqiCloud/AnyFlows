use std::{fmt, marker::PhantomData, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};

/// 领域枚举解析错误；只保留类型名，不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParseEnumError {
    enum_name: &'static str,
}

impl ParseEnumError {
    const fn new(enum_name: &'static str) -> Self {
        Self { enum_name }
    }

    /// 返回解析失败的枚举类型名。
    #[must_use]
    pub const fn enum_name(self) -> &'static str {
        self.enum_name
    }
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} 值无效", self.enum_name)
    }
}

impl std::error::Error for ParseEnumError {}

trait StringEnum {
    const NAME: &'static str;
}

struct StringEnumVisitor<T>(PhantomData<fn() -> T>);

impl<'de, T> Visitor<'de> for StringEnumVisitor<T>
where
    T: FromStr<Err = ParseEnumError> + StringEnum,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} 的稳定字符串标识", T::NAME)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        value.parse().map_err(E::custom)
    }
}

fn deserialize_string_enum<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr<Err = ParseEnumError> + StringEnum,
{
    deserializer
        .deserialize_str(StringEnumVisitor(PhantomData))
        .map_err(|_| serde::de::Error::custom(ParseEnumError::new(T::NAME)))
}

macro_rules! string_enum {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// 返回当前契约定义的全部枚举值。
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// 返回用于配置、存储和 API 的稳定字符串标识。
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl StringEnum for $name {
            const NAME: &'static str = stringify!($name);
        }

        impl FromStr for $name {
            type Err = ParseEnumError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(ParseEnumError::new(stringify!($name))),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserialize_string_enum(deserializer)
            }
        }
    };
}

string_enum! {
    /// 客户端与上游使用的协议族。
    pub enum Protocol {
        /// OpenAI Chat Completions 协议。
        OpenAiChat => "openai_chat",
        /// OpenAI Responses 协议。
        OpenAiResponses => "openai_responses",
        /// OpenAI Embeddings 协议。
        OpenAiEmbeddings => "openai_embeddings",
        /// OpenAI Images 非流式生成协议。
        OpenAiImages => "openai_images",
        /// OpenAI Audio 非流式文件转录协议。
        OpenAiAudio => "openai_audio",
        /// OpenAI Audio 非 SSE 文本转语音协议。
        OpenAiSpeech => "openai_speech",
        /// Jina 原生 Rerank 协议。
        JinaRerank => "jina_rerank",
        /// Cohere v2 原生 Rerank 协议。
        CohereRerank => "cohere_rerank",
        /// xAI Grok Imagine Video 异步任务协议。
        XaiVideo => "xai_video",
        /// Anthropic Messages 协议。
        Anthropic => "anthropic",
        /// Gemini generateContent 协议。
        Gemini => "gemini",
    }
}

impl CredentialQuotaDimension {
    /// 根据已归一化的 Canonical 模型选择固定额度维度。
    #[must_use]
    pub fn for_canonical_model(model: &str) -> Self {
        if model == "gpt-5.3-codex-spark" {
            Self::Spark
        } else {
            Self::Global
        }
    }
}

string_enum! {
    /// 一次 Canonical 请求执行的操作类型。
    pub enum Operation {
        /// 对话补全。
        Chat => "chat",
        /// OpenAI Responses 风格响应。
        Responses => "responses",
        /// OpenAI Responses 独立上下文压缩。
        ResponsesCompact => "responses_compact",
        /// 文本向量化。
        Embedding => "embedding",
        /// 图像生成或处理。
        Image => "image",
        /// 语音合成或转录。
        Audio => "audio",
        /// 结果重排序。
        Rerank => "rerank",
        /// 视频生成或处理。
        Video => "video",
        /// 只计算输入 token 数量。
        CountTokens => "count_tokens",
    }
}

string_enum! {
    /// OpenAI Responses Compact 渠道能力的显式策略。
    pub enum ResponsesCompactMode {
        /// 由后续探测结果决定是否使用 Compact。
        Auto => "auto",
        /// 仅允许在明确支持 Compact 的原生 Responses 渠道上使用。
        ForceOn => "force_on",
        /// 即使探测到支持，也不为该渠道使用 Compact。
        ForceOff => "force_off",
    }
}

string_enum! {
    /// OpenAI Responses Compact 端点的最近一次确定性探测结论。
    pub enum ResponsesCompactProbeResult {
        /// 尚未完成有效探测，不能据此参与 Compact 调度。
        Unknown => "unknown",
        /// 上游返回并通过 `response.compaction` 完整结构校验。
        Supported => "supported",
        /// 上游明确不提供该端点，或成功响应不符合 Compact 协议。
        Unsupported => "unsupported",
    }
}

string_enum! {
    /// 出站请求可显式选择的版本化客户端仿真档案。
    ///
    /// 关闭状态由配置缺失表达，避免把默认关闭误序列化为已选择档案。
    pub enum ClientSimulationProfile {
        /// 只模拟 Anthropic CLI 的稳定 HTTP 身份 Header，不改变正文或协议能力。
        AnthropicCliHeadersV1 => "anthropic_cli_headers_v1",
    }
}

string_enum! {
    /// 出站请求可显式选择的版本化客户端仿真正文档案。
    ///
    /// 关闭状态由配置缺失表达；正文档案不得携带管理员自定义文本或模板。
    pub enum ClientSimulationBodyProfile {
        /// 在受控 Anthropic CLI 仿真请求开头插入冻结的 UTC 日期指令。
        AnthropicCliSystemDateV1 => "anthropic_cli_system_date_v1",
    }
}

string_enum! {
    /// 单个上游 Attempt 的客户端仿真应用结果。
    pub enum ClientSimulationResult {
        /// 当前候选配置了档案，但在应用前已结束。
        NotApplied => "not_applied",
        /// 白名单身份 Header 已通过统一请求预算校验。
        Applied => "applied",
        /// 档案不匹配或补丁校验失败，请求未发送。
        Failed => "failed",
    }
}

string_enum! {
    /// 请求级客户端仿真正文补丁结果。
    pub enum ClientSimulationBodyPatchResult {
        /// 正文已按闭合档案重新构建，原始正文直通资格已清除。
        Applied => "applied",
        /// 正文形状或受信时钟不满足档案约束，请求在发送前被拒绝。
        Rejected => "rejected",
    }
}

string_enum! {
    /// 渠道选择的上游适配器类型。
    pub enum ChannelType {
        /// OpenAI 及其兼容 API。
        OpenAi => "openai",
        /// Anthropic 官方或兼容 API。
        Anthropic => "anthropic",
        /// Gemini 官方或兼容 API。
        Gemini => "gemini",
        /// AWS Bedrock SigV4 API。
        Bedrock => "bedrock",
        /// Google Vertex AI。
        Vertex => "vertex",
        /// Jina AI 原生 API。
        Jina => "jina",
        /// Cohere 原生 API。
        Cohere => "cohere",
        /// xAI 原生 API。
        Xai => "xai",
        /// 配置驱动的自定义适配器。
        Custom => "custom",
    }
}

string_enum! {
    /// 凭据记录的稳定种类；不包含具体 secret 字段。
    pub enum CredentialKind {
        /// API Key 凭据。
        ApiKey => "api_key",
        /// OAuth access/refresh token 凭据。
        Oauth => "oauth",
        /// 存储为 `setup_token` 的凭据种类。
        SetupToken => "setup_token",
        /// AWS Bedrock 访问凭据。
        Bedrock => "bedrock",
        /// Service Account 凭据。
        ServiceAccount => "service_account",
        /// 存储为 `upstream` 的凭据种类。
        Upstream => "upstream",
    }
}

string_enum! {
    /// 凭据使用的固定上游额度维度。
    pub enum CredentialQuotaDimension {
        /// 普通模型共享的全局额度。
        Global => "global",
        /// GPT-5.3 Codex Spark 使用的独立额度。
        Spark => "spark",
    }
}

string_enum! {
    /// Canonical 消息角色；用户/RBAC 角色必须使用独立类型。
    pub enum Role {
        /// 系统级指令。
        System => "system",
        /// 高于终端用户输入的开发者级指令。
        Developer => "developer",
        /// 终端用户输入。
        User => "user",
        /// 模型回复。
        Assistant => "assistant",
        /// 工具执行结果。
        Tool => "tool",
    }
}

string_enum! {
    /// API 客户端可依赖的稳定公开错误码；不包含内部诊断文案。
    pub enum PublicErrorCode {
        /// 请求结构或参数无效。
        InvalidRequest => "invalid_request",
        /// API Key 无效、过期或已禁用。
        InvalidApiKey => "invalid_api_key",
        /// 可用额度不足。
        InsufficientQuota => "insufficient_quota",
        /// 请求的模型不存在或不可用。
        ModelNotFound => "model_not_found",
        /// 当前认证用户范围内不存在指定异步任务。
        TaskNotFound => "task_not_found",
        /// 相同幂等键已经绑定到不同请求载荷。
        IdempotencyConflict => "idempotency_conflict",
        /// 提交或持久化结果未知，调用方必须使用相同幂等键重试。
        RequestOutcomeUnknown => "request_outcome_unknown",
        /// 当前没有可用上游。
        UpstreamUnavailable => "upstream_unavailable",
        /// 请求受到网关或上游限流。
        RateLimited => "rate_limited",
        /// 服务内部错误。
        InternalError => "internal_error",
    }
}

/// 渠道与凭据共用的运行状态；不得用于 users 或 tokens。
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(i16)]
pub enum Status {
    /// 已验证并允许参与服务或调度。
    Enabled = 1,
    /// 管理员禁用或尚未完成验证；安全默认值。
    #[default]
    Disabled = 2,
    /// 因上游错误被系统自动禁用。
    AutoDisabled = 3,
}

impl Status {
    /// 返回当前契约定义的全部运行状态。
    pub const ALL: &'static [Self] = &[Self::Enabled, Self::Disabled, Self::AutoDisabled];

    /// 返回与数据库 CHECK 约束一致的稳定数值。
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 返回当前状态是否允许参与服务或调度。
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl TryFrom<i16> for Status {
    type Error = ParseEnumError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Disabled),
            3 => Ok(Self::AutoDisabled),
            _ => Err(ParseEnumError::new("Status")),
        }
    }
}

impl From<Status> for i16 {
    fn from(value: Status) -> Self {
        value.code()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Enabled => "启用",
            Self::Disabled => "禁用",
            Self::AutoDisabled => "自动禁用",
        };
        formatter.write_str(name)
    }
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i16(self.code())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i16::deserialize(deserializer)
            .map_err(|_| serde::de::Error::custom(ParseEnumError::new("Status")))?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}
