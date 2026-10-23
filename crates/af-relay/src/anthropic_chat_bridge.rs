use std::collections::BTreeMap;

use af_domain::{AfError, Operation, Protocol, UpstreamError};
use af_protocol::{
    CanonicalResponse, CanonicalStreamEvent, ContentBlock, ContentDelta, TokenCount, Usage,
    UsageDetails, UsageSemantics,
};

/// 跨协议保留明文和工具调用；不把来源签名冒充目标供应商的签名。
pub(crate) fn adapt_response(
    upstream: Protocol,
    downstream: Protocol,
    response: &mut CanonicalResponse,
) -> Result<(), UpstreamError> {
    if upstream == downstream {
        return Ok(());
    }
    for choice in &mut response.choices {
        if downstream != Protocol::Anthropic {
            choice.stop_sequence = None;
        }
        if downstream != Protocol::OpenAiChat {
            choice.logprobs = None;
        }
        let mut blocks = Vec::with_capacity(choice.message.content.len());
        for block in std::mem::take(&mut choice.message.content) {
            match block {
                ContentBlock::Thinking { text, .. } if downstream == Protocol::Anthropic => {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::Text(text));
                    }
                }
                ContentBlock::Thinking { text, .. } => {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::Thinking {
                            text,
                            signature: None,
                        });
                    }
                }
                ContentBlock::ToolUse {
                    id, name, input, ..
                } => {
                    blocks.push(ContentBlock::ToolUse {
                        id,
                        name,
                        input,
                        signature: None,
                    });
                }
                ContentBlock::CacheControl(_) | ContentBlock::Compaction(_) => {}
                block => blocks.push(block),
            }
        }
        choice.message.content =
            if matches!(downstream, Protocol::OpenAiChat | Protocol::OpenAiResponses) {
                ordered_openai_blocks(blocks, downstream)
            } else {
                blocks
            };
    }
    let operation = if downstream == Protocol::OpenAiResponses {
        Operation::Responses
    } else {
        Operation::Chat
    };
    let usage = response
        .usage
        .map(|usage| adapt_usage(downstream, usage))
        .transpose()?;
    // 协议私有响应字段只属于来源，跨协议新建信封避免泄漏或错误合并。
    *response = CanonicalResponse::new(
        operation,
        response.id.clone(),
        response.model.clone(),
        response.created_at,
        std::mem::take(&mut response.choices),
        usage,
    );
    Ok(())
}

fn ordered_openai_blocks(blocks: Vec<ContentBlock>, downstream: Protocol) -> Vec<ContentBlock> {
    let mut reasoning = String::new();
    let mut content = Vec::new();
    let mut tools = Vec::new();
    let mut body_started = false;
    for block in blocks {
        match block {
            ContentBlock::Thinking { text, .. } if !body_started => reasoning.push_str(&text),
            ContentBlock::Thinking { text, .. } => content.push(ContentBlock::Text(text)),
            block @ ContentBlock::ToolUse { .. } => {
                body_started = true;
                tools.push(block);
            }
            block => {
                body_started = true;
                content.push(block);
            }
        }
    }
    let mut output = Vec::with_capacity(content.len() + tools.len() + 1);
    if !reasoning.is_empty() {
        output.push(ContentBlock::Thinking {
            text: reasoning,
            signature: None,
        });
    }
    if downstream == Protocol::OpenAiChat {
        let mut text = String::new();
        let mut has_text = false;
        for block in content {
            if let ContentBlock::Text(part) = block {
                has_text = true;
                text.push_str(&part);
            } else {
                output.push(block);
            }
        }
        if has_text {
            output.push(ContentBlock::Text(text));
        }
    } else {
        output.extend(content);
    }
    output.extend(tools);
    output
}

/// 保持计费总量和可表达明细，避免供应商特有细分使整个响应失败。
pub(crate) fn adapt_usage(downstream: Protocol, usage: Usage) -> Result<Usage, UpstreamError> {
    if !matches!(
        downstream,
        Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic | Protocol::Gemini
    ) {
        return Err(UpstreamError::ProtocolError);
    }
    let cache_creation = downstream == Protocol::Anthropic;
    let audio = matches!(downstream, Protocol::OpenAiChat | Protocol::Gemini);
    let details = usage.details();
    if (cache_creation
        || (details.cache_creation_5m() == TokenCount::ZERO
            && details.cache_creation_1h() == TokenCount::ZERO))
        && (audio
            || (details.audio_input() == TokenCount::ZERO
                && details.audio_output() == TokenCount::ZERO))
    {
        return Ok(usage);
    }
    Usage::new(
        usage
            .checked_input_tokens()
            .map_err(|_| UpstreamError::ProtocolError)?,
        usage.output_tokens(),
        UsageDetails::new(
            details.cache_read(),
            if cache_creation {
                details.cache_creation_5m()
            } else {
                TokenCount::ZERO
            },
            if cache_creation {
                details.cache_creation_1h()
            } else {
                TokenCount::ZERO
            },
            details.reasoning(),
            if audio {
                details.audio_input()
            } else {
                TokenCount::ZERO
            },
            if audio {
                details.audio_output()
            } else {
                TokenCount::ZERO
            },
        ),
        usage.source(),
        UsageSemantics::Inclusive,
    )
    .map_err(|_| UpstreamError::ProtocolError)
}

/// 每个连续片段使用独立下游块，避免推理与正文交错时重开已关闭的块。
pub(crate) struct CrossProtocolStreamBridge {
    downstream: Protocol,
    active: Option<(ContentKey, u32)>,
    next_index: u32,
    responses_body_started: bool,
    deferred_tools: BTreeMap<(u32, u32), Vec<CanonicalStreamEvent>>,
    deferred_order: Vec<(u32, u32)>,
    deferred_bytes: usize,
    deferred_events: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ContentKey {
    Text(u32),
    Thinking(u32),
}

impl CrossProtocolStreamBridge {
    pub(crate) fn new(downstream: Protocol) -> Self {
        Self {
            downstream,
            active: None,
            next_index: 0,
            responses_body_started: false,
            deferred_tools: BTreeMap::new(),
            deferred_order: Vec::new(),
            deferred_bytes: 0,
            deferred_events: 0,
        }
    }

    pub(crate) fn adapt(
        &mut self,
        event: CanonicalStreamEvent,
    ) -> Result<Vec<CanonicalStreamEvent>, AfError> {
        if matches!(
            self.downstream,
            Protocol::OpenAiResponses | Protocol::Anthropic | Protocol::Gemini
        ) {
            let tool = match &event {
                CanonicalStreamEvent::ToolCallStart {
                    choice_index,
                    tool_index,
                    id,
                    name,
                    ..
                } => Some((
                    (*choice_index, *tool_index),
                    id.len().saturating_add(name.len()),
                )),
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index,
                    tool_index,
                    partial_json,
                    ..
                } => Some(((*choice_index, *tool_index), partial_json.len())),
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index,
                    tool_index,
                } => Some(((*choice_index, *tool_index), 0)),
                _ => None,
            };
            if let Some((tool_index, bytes)) = tool {
                if matches!(event, CanonicalStreamEvent::ToolCallStart { .. }) {
                    if self.deferred_tools.contains_key(&tool_index) {
                        return Err(AfError::Internal);
                    }
                    self.deferred_order.push(tool_index);
                } else if !self.deferred_tools.contains_key(&tool_index) {
                    return Err(AfError::Internal);
                }
                self.responses_body_started = true;
                self.deferred_events = self
                    .deferred_events
                    .checked_add(1)
                    .ok_or(AfError::Internal)?;
                self.deferred_bytes = self
                    .deferred_bytes
                    .checked_add(bytes)
                    .ok_or(AfError::Internal)?;
                if self.deferred_bytes > 16 * 1024 * 1024
                    || self.deferred_events > 65_536
                    || self.deferred_tools.len() >= 1024
                        && !self.deferred_tools.contains_key(&tool_index)
                {
                    return Err(AfError::Internal);
                }
                self.deferred_tools
                    .entry(tool_index)
                    .or_default()
                    .push(event);
                return Ok(Vec::new());
            }
        }
        let event = match event {
            CanonicalStreamEvent::ReasoningDelta {
                choice_index,
                content_index,
                text,
                ..
            } => {
                if text.is_empty() {
                    return Ok(Vec::new());
                }
                if self.downstream == Protocol::Anthropic
                    || self.downstream == Protocol::OpenAiResponses && self.responses_body_started
                {
                    CanonicalStreamEvent::ContentDelta {
                        choice_index,
                        content_index: self.content_index(ContentKey::Thinking(content_index))?,
                        delta: ContentDelta::Text(text),
                    }
                } else {
                    let content_index = if self.downstream == Protocol::OpenAiChat {
                        0
                    } else {
                        self.content_index(ContentKey::Thinking(
                            if self.downstream == Protocol::OpenAiResponses {
                                0
                            } else {
                                content_index
                            },
                        ))?
                    };
                    CanonicalStreamEvent::ReasoningDelta {
                        choice_index,
                        content_index,
                        text,
                        signature: None,
                    }
                }
            }
            CanonicalStreamEvent::ToolCallSignature { .. } => return Ok(Vec::new()),
            CanonicalStreamEvent::LogprobsDelta { .. }
                if self.downstream != Protocol::OpenAiChat =>
            {
                return Ok(Vec::new());
            }
            CanonicalStreamEvent::CompactionStart { .. }
            | CanonicalStreamEvent::CompactionEnd { .. } => return Ok(Vec::new()),
            CanonicalStreamEvent::ContentDelta {
                choice_index,
                content_index,
                delta,
            } => {
                self.responses_body_started = true;
                CanonicalStreamEvent::ContentDelta {
                    choice_index,
                    content_index: if self.downstream == Protocol::OpenAiChat {
                        1
                    } else {
                        self.content_index(ContentKey::Text(content_index))?
                    },
                    delta,
                }
            }
            event @ (CanonicalStreamEvent::ToolCallStart { .. }
            | CanonicalStreamEvent::ToolCallEnd { .. }) => {
                self.active = None;
                event
            }
            CanonicalStreamEvent::Finish {
                choice_index,
                reason,
                stop_sequence,
            } => {
                self.active = None;
                let finish = CanonicalStreamEvent::Finish {
                    choice_index,
                    reason,
                    stop_sequence: if self.downstream == Protocol::Anthropic {
                        stop_sequence
                    } else {
                        None
                    },
                };
                let mut output = Vec::new();
                let mut tools = Vec::new();
                self.deferred_order.retain(|key| {
                    if key.0 == choice_index {
                        tools.push(*key);
                        false
                    } else {
                        true
                    }
                });
                for (index, key) in tools.into_iter().enumerate() {
                    let events = self
                        .deferred_tools
                        .remove(&key)
                        .expect("selected tool is still buffered");
                    let index = u32::try_from(index).map_err(|_| AfError::Internal)?;
                    for mut event in events {
                        match &mut event {
                            CanonicalStreamEvent::ToolCallStart { tool_index, .. }
                            | CanonicalStreamEvent::ToolCallArgsDelta { tool_index, .. }
                            | CanonicalStreamEvent::ToolCallEnd { tool_index, .. } => {
                                *tool_index = index
                            }
                            _ => unreachable!("only tool lifecycle events are deferred"),
                        }
                        output.push(event);
                    }
                }
                output.push(finish);
                return Ok(output);
            }
            CanonicalStreamEvent::Usage(usage) => CanonicalStreamEvent::Usage(
                adapt_usage(self.downstream, usage).map_err(|_| AfError::Internal)?,
            ),
            event => event,
        };
        Ok(vec![event])
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

    const PROTOCOLS: [Protocol; 4] = [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
        Protocol::Gemini,
    ];

    fn request() -> CanonicalRequest {
        CanonicalRequest::new(
            Operation::Chat,
            "public-model".to_owned(),
            vec![Message::new(
                Role::User,
                vec![ContentBlock::Text("question".to_owned())],
            )],
            true,
        )
    }

    fn decode_stream(target: Protocol, bytes: &[u8]) -> Vec<CanonicalStreamEvent> {
        let mut decoder = ConversationDecoder::new(target).unwrap();
        let mut events = Vec::new();
        for chunk in bytes.chunks(7) {
            events.extend(decoder.push(chunk).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        events
    }

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
    fn native_full_responses_keep_all_source_content_and_metadata() {
        for source in PROTOCOLS {
            let mut response = response();
            response.choices[0]
                .message
                .content
                .push(ContentBlock::Thinking {
                    text: String::new(),
                    signature: Some("native-signature".to_owned()),
                });
            response.choices[0].logprobs = Some(json!({"content":[]}));
            let original = response.clone();
            adapt_response(source, source, &mut response).unwrap();
            assert_eq!(response, original);
        }
    }

    #[test]
    fn full_cross_protocol_matrix_keeps_text_and_tools_without_foreign_signatures() {
        for source in PROTOCOLS {
            for target in PROTOCOLS {
                if source == target {
                    continue;
                }
                let mut response = response();
                response.choices[0].message.content = vec![
                    ContentBlock::Thinking {
                        text: "plan".to_owned(),
                        signature: Some("foreign-signature-canary".to_owned()),
                    },
                    ContentBlock::Text("answer".to_owned()),
                    ContentBlock::ToolUse {
                        id: "call_a".to_owned(),
                        name: "lookup".to_owned(),
                        input: json!({"key":"a"}),
                        signature: Some("foreign-tool-canary".to_owned()),
                    },
                    ContentBlock::Thinking {
                        text: "reconsider".to_owned(),
                        signature: Some("foreign-signature-canary".to_owned()),
                    },
                    ContentBlock::Text("after tool".to_owned()),
                ];
                response.choices[0].finish_reason = FinishReason::ToolCalls;
                let usage = response.usage.unwrap();
                adapt_response(source, target, &mut response).unwrap();
                let bytes = crate::conversation::build_response(
                    target,
                    response,
                    usage,
                    "matrix",
                    "public-model",
                    1_700_000_000,
                )
                .unwrap();
                let wire = std::str::from_utf8(&bytes).unwrap();
                assert!(
                    !wire.contains("foreign-signature-canary")
                        && !wire.contains("foreign-tool-canary")
                );
                let decoded = crate::conversation::parse_response(target, &bytes).unwrap();
                let visible = decoded.choices[0]
                    .message
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(text) | ContentBlock::Thinking { text, .. } => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<String>();
                assert_eq!(
                    visible, "plananswerreconsiderafter tool",
                    "{source:?} -> {target:?}"
                );
                assert!(decoded.choices[0].message.content.iter().any(|block| matches!(block, ContentBlock::ToolUse { id, name, input, signature: None } if id == "call_a" && name == "lookup" && input == &json!({"key":"a"}))));
                assert_eq!(
                    decoded.usage.unwrap().checked_total_tokens().unwrap(),
                    usage.checked_total_tokens().unwrap()
                );
            }
        }
    }

    #[test]
    fn usage_projection_keeps_billing_totals_and_supported_details() {
        let count = |value| TokenCount::new(value).unwrap();
        let usage = Usage::new(
            count(10),
            count(9),
            UsageDetails::new(count(4), count(5), count(6), count(3), count(1), count(2)),
            af_protocol::UsageSource::Upstream,
            UsageSemantics::CacheSeparated,
        )
        .unwrap();
        for target in PROTOCOLS {
            let adapted = adapt_usage(target, usage).unwrap();
            assert_eq!(
                adapted.checked_input_tokens().unwrap(),
                usage.checked_input_tokens().unwrap()
            );
            assert_eq!(
                adapted.checked_total_tokens().unwrap(),
                usage.checked_total_tokens().unwrap()
            );
            assert_eq!(adapted.output_tokens(), usage.output_tokens());
            assert_eq!(adapted.details().cache_read(), count(4));
            assert_eq!(adapted.details().reasoning(), count(3));
            assert_eq!(adapted.source(), usage.source());
            let mut response = response();
            let source = if target == Protocol::OpenAiChat {
                Protocol::Gemini
            } else {
                Protocol::OpenAiChat
            };
            adapt_response(source, target, &mut response).unwrap();
            let bytes = crate::conversation::build_response(
                target,
                response,
                usage,
                "usage",
                "public-model",
                1_700_000_000,
            )
            .unwrap();
            let parsed = crate::conversation::parse_response(target, &bytes).unwrap();
            assert_eq!(
                parsed.usage.unwrap().checked_total_tokens().unwrap(),
                usage.checked_total_tokens().unwrap()
            );
        }
    }

    #[test]
    fn streaming_four_by_four_preserves_plaintext_lifecycle_and_native_signatures() {
        const SIGNATURE: &str = "c2lnbmF0dXJlLWNhbmFyeQ==";
        for source in PROTOCOLS {
            for target in PROTOCOLS {
                let mut encoder = ChatStreamEncoder::new(
                    target,
                    "matrix",
                    "public-model",
                    1_700_000_000,
                    &request(),
                    true,
                )
                .unwrap()
                .with_upstream_protocol(source);
                let signature = (source != Protocol::OpenAiChat).then(|| SIGNATURE.to_owned());
                let mut events = vec![
                    CanonicalStreamEvent::MessageStart {
                        choice_index: 0,
                        role: Role::Assistant,
                    },
                    CanonicalStreamEvent::ReasoningDelta {
                        choice_index: 0,
                        content_index: 0,
                        text: "plan".to_owned(),
                        signature: signature.clone(),
                    },
                    CanonicalStreamEvent::ContentDelta {
                        choice_index: 0,
                        content_index: 1,
                        delta: ContentDelta::Text("answer".to_owned()),
                    },
                    CanonicalStreamEvent::ToolCallStart {
                        choice_index: 0,
                        tool_index: 0,
                        id: "call_lookup".to_owned(),
                        name: "lookup".to_owned(),
                    },
                ];
                if source == Protocol::Gemini {
                    events.push(CanonicalStreamEvent::ToolCallSignature {
                        choice_index: 0,
                        tool_index: 0,
                        signature: SIGNATURE.to_owned(),
                    });
                }
                events.extend([
                    CanonicalStreamEvent::ToolCallArgsDelta {
                        choice_index: 0,
                        tool_index: 0,
                        partial_json: "{}".to_owned(),
                    },
                    CanonicalStreamEvent::ToolCallEnd {
                        choice_index: 0,
                        tool_index: 0,
                    },
                    CanonicalStreamEvent::Finish {
                        choice_index: 0,
                        reason: FinishReason::ToolCalls,
                        stop_sequence: None,
                    },
                    CanonicalStreamEvent::Usage(response().usage.unwrap()),
                ]);
                let mut bytes = Vec::new();
                for event in events {
                    bytes.extend(encoder.encode_event(event).unwrap());
                }
                bytes.extend(encoder.finish(Ok(response().usage.unwrap()), true).unwrap());
                let wire = std::str::from_utf8(&bytes).unwrap();
                if source != target {
                    assert!(!wire.contains(SIGNATURE), "{source:?} -> {target:?}");
                } else if signature.is_some() {
                    assert!(wire.contains(SIGNATURE));
                }
                let decoded = decode_stream(target, &bytes);
                let visible = decoded
                    .iter()
                    .filter_map(|event| match event {
                        CanonicalStreamEvent::ReasoningDelta { text, .. }
                        | CanonicalStreamEvent::ContentDelta {
                            delta: ContentDelta::Text(text),
                            ..
                        } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                assert_eq!(visible, "plananswer", "{source:?} -> {target:?}");
                assert_eq!(decoded.iter().filter(|event| matches!(event, CanonicalStreamEvent::ToolCallStart { id, name, .. } if id == "call_lookup" && name == "lookup")).count(), 1);
                assert_eq!(
                    decoded
                        .iter()
                        .filter(|event| matches!(event, CanonicalStreamEvent::ToolCallEnd { .. }))
                        .count(),
                    1
                );
                assert_eq!(
                    decoded
                        .iter()
                        .filter(|event| matches!(event, CanonicalStreamEvent::Usage(_)))
                        .count(),
                    1
                );
                assert_eq!(
                    decoded
                        .iter()
                        .filter(|event| matches!(event, CanonicalStreamEvent::StreamEnd))
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn foreign_parallel_tools_and_interleaved_text_encode_for_every_target() {
        for target in PROTOCOLS {
            let source = if target == Protocol::Gemini {
                Protocol::OpenAiChat
            } else {
                Protocol::Gemini
            };
            let mut encoder = ChatStreamEncoder::new(
                target,
                "parallel",
                "public-model",
                1_700_000_000,
                &request(),
                true,
            )
            .unwrap()
            .with_upstream_protocol(source);
            let mut bytes = Vec::new();
            for event in [
                CanonicalStreamEvent::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                },
                CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 7,
                    text: "plan".to_owned(),
                    signature: Some("foreign".to_owned()),
                },
                CanonicalStreamEvent::ContentDelta {
                    choice_index: 0,
                    content_index: 8,
                    delta: ContentDelta::Text("answer".to_owned()),
                },
                CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: 0,
                    id: "call_a".to_owned(),
                    name: "lookup".to_owned(),
                },
                CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: 1,
                    id: "call_b".to_owned(),
                    name: "lookup".to_owned(),
                },
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 1,
                    partial_json: "{\"key\":\"b\"}".to_owned(),
                },
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 0,
                    partial_json: "{\"key\":\"a\"}".to_owned(),
                },
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 1,
                },
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 0,
                },
                CanonicalStreamEvent::ContentDelta {
                    choice_index: 0,
                    content_index: 8,
                    delta: ContentDelta::Text("after tools".to_owned()),
                },
                CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 9,
                    text: "reconsider".to_owned(),
                    signature: Some("foreign".to_owned()),
                },
                CanonicalStreamEvent::Finish {
                    choice_index: 0,
                    reason: FinishReason::ToolCalls,
                    stop_sequence: None,
                },
            ] {
                bytes.extend(encoder.encode_event(event).unwrap());
            }
            bytes.extend(
                encoder
                    .finish(Ok(response().usage.unwrap()), false)
                    .unwrap(),
            );
            let decoded = decode_stream(target, &bytes);
            let visible = decoded
                .iter()
                .filter_map(|event| match event {
                    CanonicalStreamEvent::ReasoningDelta { text, .. }
                    | CanonicalStreamEvent::ContentDelta {
                        delta: ContentDelta::Text(text),
                        ..
                    } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<String>();
            assert_eq!(visible, "plananswerafter toolsreconsider", "{target:?}");
            let ids = decoded
                .iter()
                .filter_map(|event| match event {
                    CanonicalStreamEvent::ToolCallStart { tool_index, id, .. } => {
                        Some((*tool_index, id.as_str()))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>();
            assert_eq!(ids.len(), 2, "{target:?}: parallel call identities");
            let indices: BTreeMap<_, _> = ids.into_iter().map(|(index, id)| (id, index)).collect();
            assert_eq!(
                indices.keys().copied().collect::<Vec<_>>(),
                ["call_a", "call_b"]
            );
            let mut arguments = BTreeMap::<u32, String>::new();
            for event in &decoded {
                if let CanonicalStreamEvent::ToolCallArgsDelta {
                    tool_index,
                    partial_json,
                    ..
                } = event
                {
                    arguments
                        .entry(*tool_index)
                        .or_default()
                        .push_str(partial_json);
                }
            }
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&arguments[&indices["call_a"]]).unwrap(),
                json!({"key":"a"})
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&arguments[&indices["call_b"]]).unwrap(),
                json!({"key":"b"})
            );
            assert_eq!(
                decoded
                    .iter()
                    .filter(|event| matches!(event, CanonicalStreamEvent::ToolCallEnd { .. }))
                    .count(),
                2
            );
        }
    }

    #[test]
    fn deferred_tools_preserve_start_order_independently_of_indices_and_completion() {
        for target in [
            Protocol::Anthropic,
            Protocol::OpenAiResponses,
            Protocol::Gemini,
        ] {
            let mut bridge = CrossProtocolStreamBridge::new(target);
            for event in [
                CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: 9,
                    id: "first".to_owned(),
                    name: "lookup".to_owned(),
                },
                CanonicalStreamEvent::ToolCallStart {
                    choice_index: 0,
                    tool_index: 3,
                    id: "second".to_owned(),
                    name: "lookup".to_owned(),
                },
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 3,
                    partial_json: "{}".to_owned(),
                },
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 3,
                },
                CanonicalStreamEvent::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 9,
                    partial_json: "{}".to_owned(),
                },
                CanonicalStreamEvent::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 9,
                },
            ] {
                assert!(bridge.adapt(event).unwrap().is_empty());
            }
            let output = bridge
                .adapt(CanonicalStreamEvent::Finish {
                    choice_index: 0,
                    reason: FinishReason::ToolCalls,
                    stop_sequence: None,
                })
                .unwrap();
            let starts = output
                .iter()
                .filter_map(|event| match event {
                    CanonicalStreamEvent::ToolCallStart { id, tool_index, .. } => {
                        Some((id.as_str(), *tool_index))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(starts, [("first", 0), ("second", 1)]);
            assert!(matches!(
                &output[..],
                [
                    CanonicalStreamEvent::ToolCallStart { tool_index: 0, .. },
                    CanonicalStreamEvent::ToolCallArgsDelta { tool_index: 0, .. },
                    CanonicalStreamEvent::ToolCallEnd { tool_index: 0, .. },
                    CanonicalStreamEvent::ToolCallStart { tool_index: 1, .. },
                    CanonicalStreamEvent::ToolCallArgsDelta { tool_index: 1, .. },
                    CanonicalStreamEvent::ToolCallEnd { tool_index: 1, .. },
                    CanonicalStreamEvent::Finish { .. }
                ]
            ));
        }
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
    fn native_signatures_are_preserved_and_foreign_signatures_become_replayable_text() {
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
            let mut adapted = response.clone();
            adapt_response(source, Protocol::Anthropic, &mut adapted).unwrap();
            assert_eq!(
                adapted.choices[0].message.content,
                vec![
                    ContentBlock::Text("plan".to_owned()),
                    ContentBlock::Text("answer".to_owned())
                ]
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
            bytes.extend(encoder.encode_event(event).unwrap());
        }
        bytes.extend(encoder.finish(Ok(response().usage.unwrap()), true).unwrap());
        let mut decoder = ConversationDecoder::new(Protocol::Anthropic).unwrap();
        let mut events = Vec::new();
        for chunk in bytes.chunks(7) {
            events.extend(decoder.push(chunk).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        let expected_usage = response().usage.unwrap();
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
    fn streaming_bridge_keeps_foreign_text_and_suppresses_signature_only_events() {
        let mut bridge = CrossProtocolStreamBridge::new(Protocol::Anthropic);
        assert_eq!(
            bridge
                .adapt(CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 0,
                    text: "plan".to_owned(),
                    signature: Some("foreign".to_owned()),
                })
                .unwrap(),
            vec![CanonicalStreamEvent::ContentDelta {
                choice_index: 0,
                content_index: 0,
                delta: ContentDelta::Text("plan".to_owned())
            }]
        );
        assert!(
            bridge
                .adapt(CanonicalStreamEvent::ToolCallSignature {
                    choice_index: 0,
                    tool_index: 0,
                    signature: "foreign".to_owned(),
                })
                .unwrap()
                .is_empty()
        );
        assert!(
            bridge
                .adapt(CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 0,
                    text: String::new(),
                    signature: Some("foreign".to_owned())
                })
                .unwrap()
                .is_empty()
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
            let bytes = encoder
                .encode_event(CanonicalStreamEvent::ReasoningDelta {
                    choice_index: 0,
                    content_index: 0,
                    text: "plan".to_owned(),
                    signature: Some("foreign".to_owned()),
                })
                .unwrap();
            let wire = String::from_utf8(bytes).unwrap();
            assert!(wire.contains("plan"));
            assert!(
                !wire.contains("foreign")
                    && !wire.contains("signature_delta")
                    && !wire.contains("thinking_delta")
            );
        }
    }
}
