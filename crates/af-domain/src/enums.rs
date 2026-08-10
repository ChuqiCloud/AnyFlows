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
        /// Anthropic Messages 协议。
        Anthropic => "anthropic",
        /// Gemini generateContent 协议。
        Gemini => "gemini",
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
