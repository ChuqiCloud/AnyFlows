use af_domain::{AfError, Protocol, UpstreamError};
use af_protocol::{CanonicalResponse, CanonicalStreamEvent, ContentBlock, ContentDelta};

/// Chat 没有 Anthropic 原生思考签名；以可直接回放的文本保留其推理内容。
pub(crate) fn adapt_response(
    upstream: Protocol,
    downstream: Protocol,
    response: &mut CanonicalResponse,
) -> Result<(), UpstreamError> {
    if downstream != Protocol::Anthropic || upstream == Protocol::Anthropic {
        return Ok(());
    }
    for choice in &mut response.choices {
        for block in &mut choice.message.content {
            match block {
                ContentBlock::Thinking {
                    text,
                    signature: None,
                } => {
                    if upstream != Protocol::OpenAiChat {
                        return Err(UpstreamError::ProtocolError);
                    }
                    *block = ContentBlock::Text(std::mem::take(text));
                }
                ContentBlock::Thinking {
                    signature: Some(_), ..
                }
                | ContentBlock::ToolUse {
                    signature: Some(_), ..
                } => {
                    return Err(UpstreamError::ProtocolError);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// 每个连续片段使用独立下游块，避免推理与正文交错时重开已关闭的块。
#[derive(Default)]
pub(crate) struct ChatThinkingTextBridge {
    active: Option<(ContentKey, u32)>,
    next_index: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ContentKey {
    Text(u32),
    Thinking(u32),
}

impl ChatThinkingTextBridge {
    pub(crate) fn adapt(
        &mut self,
        event: CanonicalStreamEvent,
    ) -> Result<CanonicalStreamEvent, AfError> {
        Ok(match event {
            CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                signature: None,
            } => CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index: self.content_index(ContentKey::Thinking(content_index))?,
                delta: ContentDelta::Text(text),
            },
            CanonicalStreamEvent::ReasoningDelta {
                signature: Some(_), ..
            }
            | CanonicalStreamEvent::ToolCallSignature { .. } => return Err(AfError::Internal),
            CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta,
            } => CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index: self.content_index(ContentKey::Text(content_index))?,
                delta,
            },
            event @ (CanonicalStreamEvent::ToolCallStart { .. }
            | CanonicalStreamEvent::ToolCallEnd { .. }
            | CanonicalStreamEvent::Finish { .. }) => {
                self.active = None;
                event
            }
            event => event,
        })
    }

    fn content_index(&mut self, key: ContentKey) -> Result<u32, AfError> {
        if let Some((active, index)) = self.active
            && active == key
        {
            return Ok(index);
        }
        let index = self.next_index;
        self.next_index = self.next_index.checked_add(1).ok_or(AfError::Internal)?;
        self.active = Some((key, index));
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{Operation, Role};
    use af_protocol::{CanonicalRequest, FinishReason, Message, anthropic, openai_chat};
    use serde_json::json;

    use super::*;
    use crate::chat_stream_encoder::ChatStreamEncoder;
    use crate::conversation_decoder::ConversationDecoder;

    fn response() -> CanonicalResponse {
        openai_chat::parse_response(&serde_json::to_vec(&json!({
            "id":"upstream", "object":"chat.completion", "created":1700000000,
            "model":"upstream", "choices":[{
                "index":0, "message":{"role":"assistant","reasoning_content":"plan","content":"answer"},
                "finish_reason":"stop"
            }], "usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}
        })).unwrap()).unwrap()
    }

    #[test]
    fn chat_full_thinking_becomes_replayable_text_without_changing_usage() {
        let mut response = response();
        let usage = response.usage;
        adapt_response(Protocol::OpenAiChat, Protocol::Anthropic, &mut response).unwrap();
        assert_eq!(response.usage, usage);
        assert_eq!(
            response.choices[0].message.content,
            vec![
                ContentBlock::Text("plan".to_owned()),
                ContentBlock::Text("answer".to_owned()),
            ]
        );
        let body = crate::conversation::build_response(
            Protocol::Anthropic,
            response,
            usage.unwrap(),
            "bridge",
            "public-model",
            1_700_000_000,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["content"][0]["type"], "text");
        assert_eq!(body["content"][0]["text"], "plan");
        let replay = json!({
            "model":"public-model","max_tokens":2048,
            "messages":[{"role":"user","content":"question"},
                {"role":"assistant","content":[{"type":"text","text":body["content"][0]["text"]},{"type":"text","text":body["content"][1]["text"]}]},
                {"role":"user","content":"continue"}]
        });
        anthropic::parse_request(&serde_json::to_vec(&replay).unwrap()).unwrap();
    }

    #[test]
    fn native_signatures_are_preserved_and_foreign_signatures_are_rejected() {
        let mut response = response();
        if let ContentBlock::Thinking { signature, .. } =
            &mut response.choices[0].message.content[0]
        {
            *signature = Some("source-signature".to_owned());
        }
        let original = response.choices[0].message.content.clone();
        adapt_response(Protocol::Anthropic, Protocol::Anthropic, &mut response).unwrap();
        assert_eq!(response.choices[0].message.content, original);
        for source in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Gemini,
        ] {
            assert_eq!(
                adapt_response(source, Protocol::Anthropic, &mut response.clone()),
                Err(UpstreamError::ProtocolError)
            );
        }
    }

    #[test]
    fn streaming_chat_interleaving_keeps_every_text_fragment_and_tool_lifecycle() {
        let request = CanonicalRequest::new(
            Operation::Chat,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("question".to_owned())],
            )],
            true,
        );
        let mut encoder = ChatStreamEncoder::new(
            Protocol::Anthropic,
            "bridge",
            "public-model",
            1_700_000_000,
            &request,
            false,
        )
        .unwrap()
        .with_upstream_protocol(Protocol::OpenAiChat);
        let mut bytes = Vec::new();
        let mut expected_usage = crate::openai_chat_usage::OpenAiChatUsageEstimator::new(&request);
        for event in [
            CanonicalStreamEvent::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: "plan".to_owned(),
                signature: None,
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: " more".to_owned(),
                signature: None,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("answer".to_owned()),
            },
            CanonicalStreamEvent::ReasoningDelta {
                choice_index: 0,
                content_index: 0,
                text: "reconsider".to_owned(),
                signature: None,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("revised".to_owned()),
            },
            CanonicalStreamEvent::ToolCallStart {
                choice_index: 0,
                tool_index: 0,
                id: "call_lookup".to_owned(),
                name: "lookup".to_owned(),
            },
            CanonicalStreamEvent::ToolCallArgsDelta {
                choice_index: 0,
                tool_index: 0,
                partial_json: "{}".to_owned(),
            },
            CanonicalStreamEvent::ToolCallEnd {
                choice_index: 0,
                tool_index: 0,
            },
            CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("after tool".to_owned()),
            },
            CanonicalStreamEvent::Finish {
                choice_index: 0,
                reason: FinishReason::ToolCalls,
                stop_sequence: None,
            },
        ] {
            expected_usage.observe(&event);
            bytes.extend(encoder.encode_event(event).unwrap());
        }
        bytes.extend(encoder.finish(Ok(response().usage.unwrap()), true).unwrap());
        let mut decoder = ConversationDecoder::new(Protocol::Anthropic).unwrap();
        let mut events = Vec::new();
        for chunk in bytes.chunks(7) {
            events.extend(decoder.push(chunk).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        let expected_usage = expected_usage.resolve_estimated().unwrap();
        let actual_usage = events
            .iter()
            .find_map(|event| match event {
                CanonicalStreamEvent::Usage(usage) => Some(*usage),
                _ => None,
            })
            .unwrap();
        assert_eq!(actual_usage.input_tokens(), expected_usage.input_tokens());
        assert_eq!(actual_usage.output_tokens(), expected_usage.output_tokens());
        assert_eq!(actual_usage.details(), expected_usage.details());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CanonicalStreamEvent::ReasoningDelta { .. }))
        );
        let texts = events
            .iter()
            .filter_map(|event| match event {
                CanonicalStreamEvent::ContentDelta {
                    delta: ContentDelta::Text(text),
                    ..
                } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "plan",
                " more",
                "answer",
                "reconsider",
                "revised",
                "after tool"
            ]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::ToolCallStart { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::ToolCallEnd { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::Usage(_)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CanonicalStreamEvent::StreamEnd))
                .count(),
            1
        );
    }

    #[test]
    fn streaming_bridge_rejects_unexpected_source_signatures() {
        assert!(
            ChatThinkingTextBridge::default()
                .adapt(CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 0,
                    text: "plan".to_owned(),
                    signature: Some("foreign".to_owned()),
                })
                .is_err()
        );
        assert!(
            ChatThinkingTextBridge::default()
                .adapt(CanonicalStreamEvent::ToolCallSignature {
                    choice_index: 0,
                    tool_index: 0,
                    signature: "foreign".to_owned(),
                })
                .is_err()
        );
    }

    #[test]
    fn foreign_streaming_thinking_cannot_be_relabelled_as_anthropic_signature() {
        let request = CanonicalRequest::new(
            Operation::Chat,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("question".to_owned())],
            )],
            true,
        );
        for source in [Protocol::OpenAiResponses, Protocol::Gemini] {
            let mut encoder = ChatStreamEncoder::new(
                Protocol::Anthropic,
                "foreign",
                "public-model",
                1_700_000_000,
                &request,
                false,
            )
            .unwrap()
            .with_upstream_protocol(source);
            encoder
                .encode_event(CanonicalStreamEvent::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                })
                .unwrap();
            assert!(
                encoder
                    .encode_event(CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: 0,
                        text: "plan".to_owned(),
                        signature: Some("foreign".to_owned()),
                    })
                    .is_err()
            );
        }
    }
}
