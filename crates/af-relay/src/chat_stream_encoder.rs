use af_domain::{AfError, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalStreamEvent, Usage, anthropic::AnthropicMessagesStreamEncoder,
    gemini::GeminiGenerateContentStreamEncoder, openai_chat::OpenAiChatStreamEncoder,
    openai_responses::OpenAiResponsesStreamEncoder,
};

use crate::anthropic_chat_bridge::{CrossProtocolStreamBridge, adapt_usage};
use crate::openai_chat_usage::{OpenAiChatUsageEstimator, UsageResolutionError};

/// OpenAI Chat 上游流对应的闭合下游协议编码器。
pub(crate) enum ChatStreamEncoder {
    Responses {
        encoder: Box<OpenAiResponsesStreamEncoder>,
        bridge: Option<CrossProtocolStreamBridge>,
    },
    OpenAi {
        encoder: OpenAiChatStreamEncoder,
        include_usage: bool,
        bridge: Option<CrossProtocolStreamBridge>,
    },
    Anthropic {
        encoder: Box<AnthropicMessagesStreamEncoder>,
        usage_estimator: OpenAiChatUsageEstimator,
        bridge: Option<CrossProtocolStreamBridge>,
    },
    Gemini {
        encoder: Box<GeminiGenerateContentStreamEncoder>,
        bridge: Option<CrossProtocolStreamBridge>,
    },
}

impl ChatStreamEncoder {
    /// 在发送上游请求前完成客户端协议元数据和初始 usage 校验。
    pub(crate) fn new(
        protocol: Protocol,
        request_id: &str,
        client_model: &str,
        created_at: i64,
        request: &CanonicalRequest,
        include_usage: bool,
    ) -> Result<Self, AfError> {
        match protocol {
            Protocol::OpenAiResponses => Ok(Self::Responses {
                bridge: None,
                encoder: Box::new(
                    OpenAiResponsesStreamEncoder::new(
                        format!("resp_{request_id}"),
                        client_model,
                        created_at,
                    )
                    .map_err(|_| AfError::Internal)?,
                ),
            }),
            Protocol::OpenAiChat => {
                let encoder = OpenAiChatStreamEncoder::new(
                    format!("chatcmpl-{request_id}"),
                    client_model,
                    created_at,
                )
                .map_err(|_| AfError::Internal)?
                .with_usage_null_fields(include_usage);
                Ok(Self::OpenAi {
                    encoder,
                    include_usage,
                    bridge: None,
                })
            }
            Protocol::Anthropic => {
                let usage_estimator = OpenAiChatUsageEstimator::new(request);
                let initial_usage = usage_estimator
                    .initial_usage()
                    .map_err(map_initial_usage_error)?;
                let encoder = AnthropicMessagesStreamEncoder::new(
                    format!("msg_{request_id}"),
                    client_model,
                    initial_usage,
                )
                .map_err(|_| AfError::Internal)?
                .with_estimated_input_correction();
                Ok(Self::Anthropic {
                    encoder: Box::new(encoder),
                    usage_estimator,
                    bridge: None,
                })
            }
            Protocol::Gemini => {
                let encoder = GeminiGenerateContentStreamEncoder::new(
                    format!("response-{request_id}"),
                    client_model,
                )
                .map_err(|_| AfError::Internal)?;
                Ok(Self::Gemini {
                    encoder: Box::new(encoder),
                    bridge: None,
                })
            }
            Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages
            | Protocol::OpenAiAudio
            | Protocol::OpenAiSpeech
            | Protocol::JinaRerank
            | Protocol::CohereRerank
            | Protocol::XaiVideo => Err(AfError::Internal),
        }
    }

    /// 同协议直接保留签名，跨协议统一适配明文、元数据与工具事件。
    pub(crate) fn with_upstream_protocol(mut self, upstream: Protocol) -> Self {
        let (downstream, bridge) = match &mut self {
            Self::Responses { bridge, .. } => (Protocol::OpenAiResponses, bridge),
            Self::OpenAi { bridge, .. } => (Protocol::OpenAiChat, bridge),
            Self::Anthropic { bridge, .. } => (Protocol::Anthropic, bridge),
            Self::Gemini { bridge, .. } => (Protocol::Gemini, bridge),
        };
        *bridge = (upstream != downstream).then(|| CrossProtocolStreamBridge::new(downstream));
        self
    }

    /// 编码一个非终点事件；Anthropic 始终忽略上游 usage，避免初始与最终口径漂移。
    pub(crate) fn encode_event(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, AfError> {
        if let Self::Anthropic {
            usage_estimator, ..
        } = self
            && !matches!(event, CanonicalStreamEvent::Usage(_))
        {
            usage_estimator.observe(&event);
        }
        let bridge = match self {
            Self::Responses { bridge, .. }
            | Self::OpenAi { bridge, .. }
            | Self::Anthropic { bridge, .. }
            | Self::Gemini { bridge, .. } => bridge,
        };
        let events = match bridge {
            Some(bridge) => bridge.adapt(event)?,
            None => vec![event],
        };
        let mut bytes = Vec::new();
        for event in events {
            bytes.extend(self.encode_adapted_event(event)?);
        }
        Ok(bytes)
    }

    fn encode_adapted_event(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, AfError> {
        match self {
            Self::Responses { encoder, .. } => encoder.encode(event).map_err(|_| AfError::Internal),
            Self::OpenAi {
                encoder,
                include_usage,
                ..
            } => {
                if matches!(event, CanonicalStreamEvent::Usage(_)) && !*include_usage {
                    return Ok(Vec::new());
                }
                encoder.encode(event).map_err(|_| AfError::Internal)
            }
            Self::Anthropic { encoder, .. } => {
                if matches!(event, CanonicalStreamEvent::Usage(_)) {
                    return Ok(Vec::new());
                }
                encoder.encode(event).map_err(|_| AfError::Internal)
            }
            Self::Gemini { encoder, .. } => encoder.encode(event).map_err(|_| AfError::Internal),
        }
    }

    /// 以客户端协议的 usage 口径编码逻辑终点。
    pub(crate) fn finish(
        &mut self,
        billing_usage: Result<Usage, UsageResolutionError>,
        upstream_usage_seen: bool,
    ) -> Result<Vec<u8>, AfError> {
        let mut output = Vec::new();
        match self {
            Self::Responses { encoder, .. } => {
                if !upstream_usage_seen {
                    output.extend(
                        encoder
                            .encode(CanonicalStreamEvent::Usage(
                                adapt_usage(
                                    Protocol::OpenAiResponses,
                                    billing_usage.map_err(|_| AfError::Internal)?,
                                )
                                .map_err(|_| AfError::Internal)?,
                            ))
                            .map_err(|_| AfError::Internal)?,
                    );
                }
                output.extend(
                    encoder
                        .encode(CanonicalStreamEvent::StreamEnd)
                        .map_err(|_| AfError::Internal)?,
                );
            }
            Self::OpenAi {
                encoder,
                include_usage,
                ..
            } => {
                if *include_usage && !upstream_usage_seen {
                    let usage = adapt_usage(
                        Protocol::OpenAiChat,
                        billing_usage.map_err(|_| AfError::Internal)?,
                    )
                    .map_err(|_| AfError::Internal)?;
                    output.extend(
                        encoder
                            .encode(CanonicalStreamEvent::Usage(usage))
                            .map_err(|_| AfError::Internal)?,
                    );
                }
                output.extend(
                    encoder
                        .encode(CanonicalStreamEvent::StreamEnd)
                        .map_err(|_| AfError::Internal)?,
                );
            }
            Self::Anthropic { encoder, .. } => {
                let usage = adapt_usage(
                    Protocol::Anthropic,
                    billing_usage.map_err(|_| AfError::Internal)?,
                )
                .map_err(|_| AfError::Internal)?;
                output.extend(
                    encoder
                        .encode(CanonicalStreamEvent::Usage(usage))
                        .map_err(|_| AfError::Internal)?,
                );
                output.extend(
                    encoder
                        .encode(CanonicalStreamEvent::StreamEnd)
                        .map_err(|_| AfError::Internal)?,
                );
            }
            Self::Gemini { encoder, .. } => {
                if !upstream_usage_seen {
                    let usage = adapt_usage(
                        Protocol::Gemini,
                        billing_usage.map_err(|_| AfError::Internal)?,
                    )
                    .map_err(|_| AfError::Internal)?;
                    output.extend(
                        encoder
                            .encode(CanonicalStreamEvent::Usage(usage))
                            .map_err(|_| AfError::Internal)?,
                    );
                }
                output.extend(
                    encoder
                        .encode(CanonicalStreamEvent::StreamEnd)
                        .map_err(|_| AfError::Internal)?,
                );
            }
        }
        Ok(output)
    }
}

fn map_initial_usage_error(error: UsageResolutionError) -> AfError {
    match error {
        UsageResolutionError::UnsupportedContent
        | UsageResolutionError::StatefulContextUnsupported => AfError::InvalidRequest,
        UsageResolutionError::Interrupted
        | UsageResolutionError::InvalidSequence
        | UsageResolutionError::Overflow
        | UsageResolutionError::EstimationFailed => AfError::Internal,
    }
}
