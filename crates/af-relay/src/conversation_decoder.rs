use af_domain::{Protocol, UpstreamError};
use af_protocol::{
    CanonicalStreamEvent, anthropic::AnthropicMessagesStreamDecoder,
    gemini::GeminiGenerateContentStreamDecoder, openai_chat::OpenAiChatStreamDecoder,
    openai_responses::OpenAiResponsesStreamDecoder,
};

/// 上游协议只影响解码，计费、下游编码和完成回调共用同一个生命周期。
pub(crate) enum ConversationDecoder {
    Chat(OpenAiChatStreamDecoder),
    Responses(Box<OpenAiResponsesStreamDecoder>),
    Anthropic(AnthropicMessagesStreamDecoder),
    Gemini(GeminiGenerateContentStreamDecoder),
}

impl ConversationDecoder {
    /// Responses 的终态快照已由原生解码器与完整 Item 生命周期交叉校验。
    pub(crate) fn take_terminal_response(&mut self) -> Option<af_protocol::CanonicalResponse> {
        match self {
            Self::Responses(decoder) => decoder.take_terminal_response(),
            _ => None,
        }
    }

    pub(crate) fn billing_usage(&self) -> Option<af_protocol::Usage> {
        match self {
            Self::Anthropic(decoder) => decoder.billing_usage(),
            _ => None,
        }
    }

    /// Anthropic 顶层 usage 不包括压缩迭代；用协议解码器算出的计费用量替代它。
    pub(crate) fn with_billing_usage(&self, event: CanonicalStreamEvent) -> CanonicalStreamEvent {
        match event {
            CanonicalStreamEvent::Usage(usage) => {
                CanonicalStreamEvent::Usage(self.billing_usage().unwrap_or(usage))
            }
            event => event,
        }
    }

    pub(crate) fn new(protocol: Protocol) -> Result<Self, UpstreamError> {
        match protocol {
            Protocol::OpenAiChat => Ok(Self::Chat(OpenAiChatStreamDecoder::new())),
            Protocol::OpenAiResponses => Ok(Self::Responses(Box::new(
                OpenAiResponsesStreamDecoder::new(),
            ))),
            Protocol::Anthropic => Ok(Self::Anthropic(AnthropicMessagesStreamDecoder::new())),
            Protocol::Gemini => Ok(Self::Gemini(GeminiGenerateContentStreamDecoder::new())),
            _ => Err(UpstreamError::ProtocolError),
        }
    }

    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Vec<CanonicalStreamEvent>, UpstreamError> {
        match self {
            Self::Chat(decoder) => decoder
                .push(bytes)
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Responses(decoder) => decoder
                .push(bytes)
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Anthropic(decoder) => decoder
                .push(bytes)
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Gemini(decoder) => decoder
                .push(bytes)
                .map_err(|_| UpstreamError::ProtocolError),
        }
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<CanonicalStreamEvent>, UpstreamError> {
        match self {
            Self::Chat(decoder) => decoder
                .finish()
                .map(|()| Vec::new())
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Responses(decoder) => decoder
                .finish()
                .map(|()| Vec::new())
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Anthropic(decoder) => decoder
                .finish()
                .map(|()| Vec::new())
                .map_err(|_| UpstreamError::ProtocolError),
            Self::Gemini(decoder) => decoder.finish().map_err(|_| UpstreamError::ProtocolError),
        }
    }
}
