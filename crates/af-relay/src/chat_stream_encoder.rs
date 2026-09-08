use af_domain::{AfError, Protocol};
use af_protocol::{
    CanonicalRequest, CanonicalStreamEvent, Usage, anthropic::AnthropicMessagesStreamEncoder,
    gemini::GeminiGenerateContentStreamEncoder, openai_chat::OpenAiChatStreamEncoder,
};

use crate::openai_chat_usage::{OpenAiChatUsageEstimator, UsageResolutionError};

/// OpenAI Chat 上游流对应的闭合下游协议编码器。
pub(crate) enum ChatStreamEncoder {
    OpenAi {
        encoder: OpenAiChatStreamEncoder,
        include_usage: bool,
    },
    Anthropic {
        encoder: Box<AnthropicMessagesStreamEncoder>,
        usage_estimator: OpenAiChatUsageEstimator,
    },
    Gemini {
        encoder: Box<GeminiGenerateContentStreamEncoder>,
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
                .map_err(|_| AfError::Internal)?;
                Ok(Self::Anthropic {
                    encoder: Box::new(encoder),
                    usage_estimator,
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
                })
            }
            Protocol::OpenAiResponses
            | Protocol::OpenAiEmbeddings
            | Protocol::OpenAiImages
            | Protocol::OpenAiAudio
            | Protocol::OpenAiSpeech
            | Protocol::JinaRerank
            | Protocol::CohereRerank
            | Protocol::XaiVideo => Err(AfError::Internal),
        }
    }

    /// 编码一个非终点事件；Anthropic 始终忽略上游 usage，避免初始与最终口径漂移。
    pub(crate) fn encode_event(&mut self, event: CanonicalStreamEvent) -> Result<Vec<u8>, AfError> {
        match self {
            Self::OpenAi {
                encoder,
                include_usage,
            } => {
                if matches!(event, CanonicalStreamEvent::Usage(_)) && !*include_usage {
                    return Ok(Vec::new());
                }
                encoder.encode(event).map_err(|_| AfError::Internal)
            }
            Self::Anthropic {
                encoder,
                usage_estimator,
            } => {
                if matches!(event, CanonicalStreamEvent::Usage(_)) {
                    return Ok(Vec::new());
                }
                usage_estimator.observe(&event);
                encoder.encode(event).map_err(|_| AfError::Internal)
            }
            Self::Gemini { encoder } => encoder.encode(event).map_err(|_| AfError::Internal),
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
            Self::OpenAi {
                encoder,
                include_usage,
            } => {
                if *include_usage && !upstream_usage_seen {
                    let usage = billing_usage.map_err(|_| AfError::Internal)?;
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
            Self::Anthropic {
                encoder,
                usage_estimator,
            } => {
                let usage = usage_estimator
                    .resolve_estimated()
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
            Self::Gemini { encoder } => {
                if !upstream_usage_seen {
                    let usage = billing_usage.map_err(|_| AfError::Internal)?;
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
