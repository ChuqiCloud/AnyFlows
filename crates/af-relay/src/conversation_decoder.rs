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
