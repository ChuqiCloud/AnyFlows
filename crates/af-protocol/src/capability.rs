use std::{error::Error, fmt};

use af_domain::Protocol;

mod request;
mod response;
mod stream;

pub use request::{RequestCapability, validate_request_capabilities};
pub use response::{ResponseCapability, validate_response_capabilities};
pub use stream::{StreamCapability, validate_stream_event_capabilities};

/// 单个协议对请求、非流式响应和流式事件的闭合能力声明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolCapabilities {
    protocol: Protocol,
    request: &'static [RequestCapability],
    response: &'static [ResponseCapability],
    stream: &'static [StreamCapability],
}

impl ProtocolCapabilities {
    /// 返回能力声明所属协议。
    #[must_use]
    pub const fn protocol(self) -> Protocol {
        self.protocol
    }

    /// 返回协议可表达的请求能力。
    #[must_use]
    pub const fn request(self) -> &'static [RequestCapability] {
        self.request
    }

    /// 返回协议可表达的非流式响应能力。
    #[must_use]
    pub const fn response(self) -> &'static [ResponseCapability] {
        self.response
    }

    /// 返回协议可表达的流式事件能力。
    #[must_use]
    pub const fn stream(self) -> &'static [StreamCapability] {
        self.stream
    }
}

const OPENAI_CHAT_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiChat,
    request: request::OPENAI_CHAT,
    response: response::OPENAI_CHAT,
    stream: stream::OPENAI_CHAT,
};
const OPENAI_RESPONSES_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiResponses,
    request: request::OPENAI_RESPONSES,
    response: response::OPENAI_RESPONSES,
    stream: stream::OPENAI_RESPONSES,
};
const OPENAI_EMBEDDINGS_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiEmbeddings,
    request: &[],
    response: &[],
    stream: &[],
};
const OPENAI_IMAGES_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiImages,
    request: &[],
    response: &[],
    stream: &[],
};
const OPENAI_AUDIO_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiAudio,
    request: &[],
    response: &[],
    stream: &[],
};
const OPENAI_SPEECH_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::OpenAiSpeech,
    request: &[],
    response: &[],
    stream: &[],
};
const JINA_RERANK_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::JinaRerank,
    request: &[],
    response: &[],
    stream: &[],
};
const COHERE_RERANK_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::CohereRerank,
    request: &[],
    response: &[],
    stream: &[],
};
const XAI_VIDEO_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::XaiVideo,
    request: &[],
    response: &[],
    stream: &[],
};
const ANTHROPIC_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::Anthropic,
    request: request::ANTHROPIC,
    response: response::ANTHROPIC,
    stream: stream::ANTHROPIC,
};
const GEMINI_CAPABILITIES: ProtocolCapabilities = ProtocolCapabilities {
    protocol: Protocol::Gemini,
    request: request::GEMINI,
    response: response::GEMINI,
    stream: stream::GEMINI,
};

/// 查询指定目标协议的完整能力声明。
#[must_use]
pub const fn protocol_capabilities(protocol: Protocol) -> &'static ProtocolCapabilities {
    match protocol {
        Protocol::OpenAiChat => &OPENAI_CHAT_CAPABILITIES,
        Protocol::OpenAiResponses => &OPENAI_RESPONSES_CAPABILITIES,
        Protocol::OpenAiEmbeddings => &OPENAI_EMBEDDINGS_CAPABILITIES,
        Protocol::OpenAiImages => &OPENAI_IMAGES_CAPABILITIES,
        Protocol::OpenAiAudio => &OPENAI_AUDIO_CAPABILITIES,
        Protocol::OpenAiSpeech => &OPENAI_SPEECH_CAPABILITIES,
        Protocol::JinaRerank => &JINA_RERANK_CAPABILITIES,
        Protocol::CohereRerank => &COHERE_RERANK_CAPABILITIES,
        Protocol::XaiVideo => &XAI_VIDEO_CAPABILITIES,
        Protocol::Anthropic => &ANTHROPIC_CAPABILITIES,
        Protocol::Gemini => &GEMINI_CAPABILITIES,
    }
}

/// 判断目标协议是否支持指定请求能力。
#[must_use]
pub fn supports_request_capability(protocol: Protocol, capability: RequestCapability) -> bool {
    protocol_capabilities(protocol)
        .request
        .contains(&capability)
}

/// 判断目标协议是否支持指定非流式响应能力。
#[must_use]
pub fn supports_response_capability(protocol: Protocol, capability: ResponseCapability) -> bool {
    protocol_capabilities(protocol)
        .response
        .contains(&capability)
}

/// 判断目标协议是否支持指定流式事件能力。
#[must_use]
pub fn supports_stream_capability(protocol: Protocol, capability: StreamCapability) -> bool {
    protocol_capabilities(protocol).stream.contains(&capability)
}

/// 一项带所属阶段的协议能力。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtocolCapability {
    /// 请求构造能力。
    Request(RequestCapability),
    /// 非流式响应构造能力。
    Response(ResponseCapability),
    /// 流式事件编码能力。
    Stream(StreamCapability),
}

impl fmt::Display for ProtocolCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(capability) => write!(formatter, "请求能力 `{capability}`"),
            Self::Response(capability) => write!(formatter, "响应能力 `{capability}`"),
            Self::Stream(capability) => write!(formatter, "流式能力 `{capability}`"),
        }
    }
}

/// 目标协议缺少单项可表达能力的结构化错误。
///
/// 错误只保存闭合枚举，不携带模型名、消息正文、工具参数或上游元数据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedCapability {
    target_protocol: Protocol,
    capability: ProtocolCapability,
}

impl UnsupportedCapability {
    pub(crate) const fn request(target_protocol: Protocol, capability: RequestCapability) -> Self {
        Self {
            target_protocol,
            capability: ProtocolCapability::Request(capability),
        }
    }

    pub(super) const fn response(
        target_protocol: Protocol,
        capability: ResponseCapability,
    ) -> Self {
        Self {
            target_protocol,
            capability: ProtocolCapability::Response(capability),
        }
    }

    pub(super) const fn stream(target_protocol: Protocol, capability: StreamCapability) -> Self {
        Self {
            target_protocol,
            capability: ProtocolCapability::Stream(capability),
        }
    }

    /// 返回无法表达该能力的目标协议。
    #[must_use]
    pub const fn target_protocol(self) -> Protocol {
        self.target_protocol
    }

    /// 返回缺失的闭合能力。
    #[must_use]
    pub const fn capability(self) -> ProtocolCapability {
        self.capability
    }
}

impl fmt::Display for UnsupportedCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "目标协议 {} 不支持 {}",
            self.target_protocol, self.capability
        )
    }
}

impl Error for UnsupportedCapability {}

#[cfg(test)]
mod tests;
