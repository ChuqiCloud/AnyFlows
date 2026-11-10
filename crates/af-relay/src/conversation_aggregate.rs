use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use af_adapter::{MAX_UPSTREAM_RESPONSE_BODY_BYTES, UpstreamBody};
use af_domain::{Operation, Role, UpstreamError};
use af_protocol::{
    CanonicalResponse, CanonicalStreamEvent as Event, ContentBlock, ContentDelta, FinishReason,
    MediaSource, Message, ResponseChoice, Usage,
};

use crate::{conversation_decoder::ConversationDecoder, error::map_adaptor_error};

pub async fn collect_conversation_response(
    protocol: af_domain::Protocol,
    body: UpstreamBody,
) -> Result<CanonicalResponse, UpstreamError> {
    collect_conversation(body, ConversationDecoder::new(protocol)?).await
}

/// 完整响应复用生产流解码器，逐块检查容量并在逻辑终点取消上游读取。
pub(crate) async fn collect_conversation(
    body: UpstreamBody,
    mut decoder: ConversationDecoder,
) -> Result<CanonicalResponse, UpstreamError> {
    // Responses 原生终态已完整保留不透明压缩 Item，不能再次按通用内容重建。
    let mut aggregate =
        (!matches!(decoder, ConversationDecoder::Responses(_))).then(Aggregate::default);
    let mut done = false;
    let mut bytes = 0_usize;
    match body {
        UpstreamBody::Full(body) => {
            if body.len() > MAX_UPSTREAM_RESPONSE_BODY_BYTES {
                return Err(UpstreamError::ProtocolError);
            }
            push_collected_events(&mut aggregate, &mut done, decoder.push(&body)?)?;
        }
        UpstreamBody::Stream(mut source) => {
            while let Some(chunk) = source.next_chunk().await {
                let chunk = chunk.map_err(map_adaptor_error)?;
                bytes = bytes
                    .checked_add(chunk.len())
                    .filter(|size| *size <= MAX_UPSTREAM_RESPONSE_BODY_BYTES)
                    .ok_or(UpstreamError::ProtocolError)?;
                push_collected_events(&mut aggregate, &mut done, decoder.push(&chunk)?)?;
                if done {
                    break;
                }
            }
        }
    }
    push_collected_events(&mut aggregate, &mut done, decoder.finish()?)?;
    match aggregate {
        Some(aggregate) => aggregate.finish(),
        None => decoder
            .take_terminal_response()
            .ok_or(UpstreamError::ProtocolError),
    }
}

fn push_collected_events(
    aggregate: &mut Option<Aggregate>,
    done: &mut bool,
    events: Vec<Event>,
) -> Result<(), UpstreamError> {
    if let Some(aggregate) = aggregate {
        aggregate.push(events)?;
        *done = aggregate.done;
        return Ok(());
    }
    for event in events {
        if *done {
            return Err(UpstreamError::ProtocolError);
        }
        match event {
            Event::Error(error) => return Err(error),
            Event::StreamEnd => *done = true,
            _ => {}
        }
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct Aggregate {
    choices: BTreeMap<u32, Choice>,
    usage: Option<Usage>,
    anthropic_usage_metadata: Option<serde_json::Value>,
    logprobs_bytes: usize,
    done: bool,
}

#[derive(Default)]
struct Choice {
    content: BTreeMap<u32, ContentBlock>,
    open_compactions: BTreeSet<u32>,
    tools: BTreeMap<u32, Tool>,
    order: Vec<Part>,
    finish: Option<(FinishReason, Option<String>)>,
    logprobs: Option<serde_json::Value>,
}

enum Part {
    Content(u32),
    Tool(u32),
}

struct Tool {
    id: String,
    name: String,
    arguments: String,
    signature: Option<String>,
    ended: bool,
}

impl Aggregate {
    fn choice(&mut self, index: u32) -> Result<&mut Choice, UpstreamError> {
        self.choices
            .get_mut(&index)
            .ok_or(UpstreamError::ProtocolError)
    }

    pub(crate) fn push(&mut self, events: Vec<Event>) -> Result<(), UpstreamError> {
        for event in events {
            if self.done {
                return Err(UpstreamError::ProtocolError);
            }
            match event {
                Event::MessageStart {
                    choice_index,
                    role: Role::Assistant,
                } => {
                    if self
                        .choices
                        .insert(choice_index, Choice::default())
                        .is_some()
                    {
                        return Err(UpstreamError::ProtocolError);
                    }
                }
                Event::ContentDelta {
                    choice_index,
                    content_index,
                    delta,
                } => {
                    let choice = self.choice(choice_index)?;
                    match choice.content.entry(content_index) {
                        Entry::Vacant(entry) => {
                            entry.insert(match delta {
                                ContentDelta::Text(text) => ContentBlock::Text(text),
                                ContentDelta::Image { data, mime_type } => ContentBlock::Image {
                                    source: MediaSource::Base64(data),
                                    mime_type: Some(mime_type),
                                },
                                ContentDelta::Audio { data, mime_type } => ContentBlock::Audio {
                                    source: MediaSource::Base64(data),
                                    mime_type,
                                },
                            });
                            choice.order.push(Part::Content(content_index));
                        }
                        Entry::Occupied(mut entry) => match (entry.get_mut(), delta) {
                            (ContentBlock::Text(text), ContentDelta::Text(delta)) => {
                                text.push_str(&delta)
                            }
                            _ => return Err(UpstreamError::ProtocolError),
                        },
                    }
                }
                Event::ReasoningDelta {
                    choice_index,
                    content_index,
                    text,
                    signature,
                } => {
                    let choice = self.choice(choice_index)?;
                    if let Entry::Vacant(entry) = choice.content.entry(content_index) {
                        entry.insert(ContentBlock::Thinking {
                            text: String::new(),
                            signature: None,
                        });
                        choice.order.push(Part::Content(content_index));
                    }
                    match choice.content.get_mut(&content_index) {
                        Some(ContentBlock::Thinking {
                            text: accumulated,
                            signature: stored,
                        }) => {
                            accumulated.push_str(&text);
                            if let Some(signature) = signature {
                                stored.get_or_insert_default().push_str(&signature);
                            }
                        }
                        _ => return Err(UpstreamError::ProtocolError),
                    }
                }
                Event::AnthropicCompactionStart {
                    choice_index,
                    content_index,
                    item,
                } => {
                    let choice = self.choice(choice_index)?;
                    if choice
                        .content
                        .insert(content_index, ContentBlock::AnthropicCompaction(item))
                        .is_some()
                    {
                        return Err(UpstreamError::ProtocolError);
                    }
                    choice.order.push(Part::Content(content_index));
                    choice.open_compactions.insert(content_index);
                }
                Event::AnthropicCompactionDelta {
                    choice_index,
                    content_index,
                    ..
                } => {
                    // End carries the validated complete native block, including opaque fields.
                    if !self
                        .choice(choice_index)?
                        .open_compactions
                        .contains(&content_index)
                    {
                        return Err(UpstreamError::ProtocolError);
                    }
                }
                Event::AnthropicCompactionEnd {
                    choice_index,
                    content_index,
                    item,
                } => {
                    let choice = self.choice(choice_index)?;
                    if !choice.open_compactions.remove(&content_index) {
                        return Err(UpstreamError::ProtocolError);
                    }
                    let block = choice
                        .content
                        .get_mut(&content_index)
                        .ok_or(UpstreamError::ProtocolError)?;
                    if !matches!(block, ContentBlock::AnthropicCompaction(_)) {
                        return Err(UpstreamError::ProtocolError);
                    }
                    *block = ContentBlock::AnthropicCompaction(item);
                }
                Event::AnthropicUsageMetadata { raw } => {
                    if self.anthropic_usage_metadata.replace(raw).is_some() {
                        return Err(UpstreamError::ProtocolError);
                    }
                }
                Event::LogprobsDelta {
                    choice_index,
                    logprobs,
                } => {
                    let bytes = serde_json::to_vec(&logprobs)
                        .map_err(|_| UpstreamError::ProtocolError)?
                        .len();
                    self.logprobs_bytes = self
                        .logprobs_bytes
                        .checked_add(bytes)
                        .filter(|bytes| *bytes <= MAX_UPSTREAM_RESPONSE_BODY_BYTES)
                        .ok_or(UpstreamError::ProtocolError)?;
                    let choice = self.choice(choice_index)?;
                    merge_logprobs(&mut choice.logprobs, logprobs)?;
                }
                Event::ToolCallStart {
                    choice_index,
                    tool_index,
                    id,
                    name,
                } => {
                    let choice = self.choice(choice_index)?;
                    if choice
                        .tools
                        .insert(
                            tool_index,
                            Tool {
                                id,
                                name,
                                arguments: String::new(),
                                signature: None,
                                ended: false,
                            },
                        )
                        .is_some()
                    {
                        return Err(UpstreamError::ProtocolError);
                    }
                    choice.order.push(Part::Tool(tool_index));
                }
                Event::ToolCallArgsDelta {
                    choice_index,
                    tool_index,
                    partial_json,
                } => {
                    let tool = self
                        .choice(choice_index)?
                        .tools
                        .get_mut(&tool_index)
                        .filter(|t| !t.ended)
                        .ok_or(UpstreamError::ProtocolError)?;
                    tool.arguments.push_str(&partial_json);
                }
                Event::ToolCallSignature {
                    choice_index,
                    tool_index,
                    signature,
                } => {
                    self.choice(choice_index)?
                        .tools
                        .get_mut(&tool_index)
                        .ok_or(UpstreamError::ProtocolError)?
                        .signature = Some(signature);
                }
                Event::ToolCallEnd {
                    choice_index,
                    tool_index,
                } => {
                    self.choice(choice_index)?
                        .tools
                        .get_mut(&tool_index)
                        .filter(|t| !t.ended)
                        .ok_or(UpstreamError::ProtocolError)?
                        .ended = true;
                }
                Event::Finish {
                    choice_index,
                    reason,
                    stop_sequence,
                } => {
                    let choice = self.choice(choice_index)?;
                    if choice.finish.replace((reason, stop_sequence)).is_some() {
                        return Err(UpstreamError::ProtocolError);
                    }
                }
                Event::Usage(usage) => self.usage = Some(usage),
                Event::Ping => {}
                Event::StreamEnd => self.done = true,
                Event::Error(error) => return Err(error),
                // 不能用通用对话聚合静默移除专属压缩或内容拦截语义。
                Event::MessageStart { .. }
                | Event::CompactionStart { .. }
                | Event::CompactionEnd { .. }
                | Event::PromptBlocked => return Err(UpstreamError::ProtocolError),
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<CanonicalResponse, UpstreamError> {
        if !self.done || self.choices.is_empty() {
            return Err(UpstreamError::ProtocolError);
        }
        let choices = self
            .choices
            .into_iter()
            .map(|(index, mut choice)| {
                if !choice.open_compactions.is_empty() {
                    return Err(UpstreamError::ProtocolError);
                }
                let (reason, stop) = choice.finish.ok_or(UpstreamError::ProtocolError)?;
                let mut content = Vec::with_capacity(choice.order.len());
                for part in choice.order {
                    content.push(match part {
                        Part::Content(index) => choice
                            .content
                            .remove(&index)
                            .ok_or(UpstreamError::ProtocolError)?,
                        Part::Tool(index) => {
                            let tool = choice
                                .tools
                                .remove(&index)
                                .filter(|tool| tool.ended)
                                .ok_or(UpstreamError::ProtocolError)?;
                            let input = serde_json::from_str(if tool.arguments.is_empty() {
                                "{}"
                            } else {
                                &tool.arguments
                            })
                            .map_err(|_| UpstreamError::ProtocolError)?;
                            ContentBlock::ToolUse {
                                id: tool.id,
                                name: tool.name,
                                input,
                                signature: tool.signature,
                            }
                        }
                    });
                }
                Ok(
                    ResponseChoice::new(index, Message::new(Role::Assistant, content), reason)
                        .with_stop_sequence(stop)
                        .with_logprobs(choice.logprobs),
                )
            })
            .collect::<Result<Vec<_>, UpstreamError>>()?;
        let response = CanonicalResponse::new(
            Operation::Chat,
            "aggregated".to_owned(),
            "aggregated".to_owned(),
            None,
            choices,
            self.usage,
        );
        match self.anthropic_usage_metadata {
            Some(raw) => af_protocol::anthropic::with_usage_metadata(response, raw)
                .map_err(|_| UpstreamError::ProtocolError),
            None => Ok(response),
        }
    }
}

fn merge_logprobs(
    target: &mut Option<serde_json::Value>,
    incoming: serde_json::Value,
) -> Result<(), UpstreamError> {
    let serde_json::Value::Object(incoming) = incoming else {
        return Err(UpstreamError::ProtocolError);
    };
    let Some(current) = target else {
        *target = Some(serde_json::Value::Object(incoming));
        return Ok(());
    };
    let Some(current) = current.as_object_mut() else {
        return Err(UpstreamError::ProtocolError);
    };
    for (key, value) in incoming {
        let Some(previous) = current.get_mut(&key) else {
            current.insert(key, value);
            continue;
        };
        // Some gateways emit null placeholders in chunks before the token details arrive.
        // Treat null as an empty update or replace a previous null with the first real value.
        if value.is_null() {
            continue;
        }
        if previous.is_null() {
            *previous = value;
            continue;
        }
        match (previous, value) {
            (serde_json::Value::Array(previous), serde_json::Value::Array(next)) => {
                previous.extend(next);
            }
            (previous, next) if *previous == next => {}
            _ => return Err(UpstreamError::ProtocolError),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        pin::Pin,
        task::{Context, Poll},
    };

    use af_adapter::{AdaptorResult, Bytes, UpstreamBodyStream};
    use af_domain::Protocol;
    use futures_core::Stream;
    use serde_json::{Value, json};

    use super::*;

    struct ResponseChunks(VecDeque<AdaptorResult<Bytes>>);

    impl Stream for ResponseChunks {
        type Item = AdaptorResult<Bytes>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    fn responses_compaction_stream(complete: bool) -> (Vec<u8>, Value) {
        let item = json!({"type":"compaction", "id":"cmp_native_collect", "encrypted_content":"opaque-native-state+/="});
        let response = json!({
            "id":"resp_private", "object":"response", "created_at":1_700_000_000,
            "status":"in_progress", "error":null, "incomplete_details":null,
            "model":"private-model", "output":[], "usage":null
        });
        let mut terminal = response.clone();
        terminal["status"] = json!("completed");
        terminal["output"] = json!([item.clone()]);
        terminal["usage"] = json!({
            "input_tokens":8, "input_tokens_details":{"cached_tokens":2,"cache_write_tokens":0},
            "output_tokens":3, "output_tokens_details":{"reasoning_tokens":1}, "total_tokens":11
        });
        let mut frames = vec![
            ("response.created", json!({"response":response.clone()})),
            ("response.in_progress", json!({"response":response})),
            (
                "response.output_item.added",
                json!({"output_index":0,"item":item.clone()}),
            ),
            (
                "response.output_item.done",
                json!({"output_index":0,"item":item}),
            ),
        ];
        if complete {
            frames.push(("response.completed", json!({"response":terminal.clone()})));
        }
        let mut bytes = Vec::new();
        for (sequence, (kind, mut value)) in frames.into_iter().enumerate() {
            value["type"] = json!(kind);
            value["sequence_number"] = json!(sequence);
            bytes.extend(format!("event: {kind}\ndata: {value}\n\n").as_bytes());
        }
        (bytes, terminal)
    }

    #[tokio::test]
    async fn native_responses_compaction_sse_collects_without_losing_opaque_state() {
        for streaming_body in [false, true] {
            let (bytes, terminal) = responses_compaction_stream(true);
            let body = if streaming_body {
                let mut chunks = bytes
                    .chunks(7)
                    .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
                    .collect::<VecDeque<_>>();
                // A complete logical response must cancel the remaining transport.
                chunks.push_back(Err(af_adapter::AdaptorError::ResponseBodyTooLarge));
                UpstreamBody::Stream(UpstreamBodyStream::new(ResponseChunks(chunks)))
            } else {
                UpstreamBody::Full(Bytes::from(bytes))
            };
            let response = collect_conversation_response(Protocol::OpenAiResponses, body)
                .await
                .unwrap();
            let expected = af_protocol::openai_responses::parse_response(
                &serde_json::to_vec(&terminal).unwrap(),
            )
            .unwrap();
            assert_eq!(response.choices, expected.choices);
            assert_eq!(response.usage, expected.usage);
            for target in [Protocol::OpenAiChat, Protocol::Anthropic, Protocol::Gemini] {
                assert!(
                    crate::anthropic_chat_bridge::adapt_response(
                        Protocol::OpenAiResponses,
                        target,
                        &mut response.clone()
                    )
                    .is_err()
                );
            }
            let body = crate::conversation::build_response(
                Protocol::OpenAiResponses,
                response,
                expected.usage.unwrap(),
                "public",
                "public-model",
                1_700_000_001,
            )
            .unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["id"], "resp_public");
            assert_eq!(body["model"], "public-model");
            assert_eq!(body["output"], terminal["output"]);
            assert_eq!(body["usage"], terminal["usage"]);
        }
    }

    #[tokio::test]
    async fn native_responses_compaction_collection_requires_a_verified_terminal() {
        let (bytes, _) = responses_compaction_stream(false);
        assert!(
            collect_conversation_response(
                Protocol::OpenAiResponses,
                UpstreamBody::Full(Bytes::from(bytes))
            )
            .await
            .is_err()
        );
    }

    #[test]
    fn aggregates_stream_logprobs_arrays_without_losing_fields() {
        let mut aggregate = Aggregate::default();
        aggregate
            .push(vec![
                Event::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                },
                Event::LogprobsDelta {
                    choice_index: 0,
                    logprobs: serde_json::json!({
                        "content": null,
                        "reasoning_content": null
                    }),
                },
                Event::LogprobsDelta {
                    choice_index: 0,
                    logprobs: serde_json::json!({
                        "content": [{"token": "one", "logprob": -0.1}],
                        "reasoning_content": [{"token": "think", "logprob": -0.2}]
                    }),
                },
                Event::LogprobsDelta {
                    choice_index: 0,
                    logprobs: serde_json::json!({
                        "content": [{"token": "two", "logprob": -0.3}],
                        "reasoning_content": [{"token": "more-think", "logprob": -0.4}]
                    }),
                },
                Event::LogprobsDelta {
                    choice_index: 0,
                    logprobs: serde_json::json!({
                        "content": null,
                        "reasoning_content": null
                    }),
                },
                Event::ContentDelta {
                    choice_index: 0,
                    content_index: 0,
                    delta: ContentDelta::Text("one two".to_owned()),
                },
                Event::Finish {
                    choice_index: 0,
                    reason: FinishReason::Stop,
                    stop_sequence: None,
                },
                Event::StreamEnd,
            ])
            .unwrap();

        let response = aggregate.finish().unwrap();
        assert_eq!(
            response.choices[0].logprobs,
            Some(serde_json::json!({
                "content": [
                    {"token": "one", "logprob": -0.1},
                    {"token": "two", "logprob": -0.3}
                ],
                "reasoning_content": [
                    {"token": "think", "logprob": -0.2},
                    {"token": "more-think", "logprob": -0.4}
                ]
            }))
        );
    }

    #[test]
    fn incomplete_streams_fail_closed() {
        let mut aggregate = Aggregate::default();
        aggregate
            .push(vec![
                Event::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                },
                Event::ContentDelta {
                    choice_index: 0,
                    content_index: 0,
                    delta: ContentDelta::Text("partial".to_owned()),
                },
            ])
            .unwrap();
        assert!(aggregate.finish().is_err());
    }

    #[test]
    fn unfinished_tools_and_invalid_arguments_fail_closed() {
        for (ended, arguments) in [(false, "{}"), (true, "{")] {
            let mut aggregate = Aggregate::default();
            let mut events = vec![
                Event::MessageStart {
                    choice_index: 0,
                    role: Role::Assistant,
                },
                Event::ToolCallStart {
                    choice_index: 0,
                    tool_index: 0,
                    id: "call_1".to_owned(),
                    name: "read".to_owned(),
                },
                Event::ToolCallArgsDelta {
                    choice_index: 0,
                    tool_index: 0,
                    partial_json: arguments.to_owned(),
                },
            ];
            if ended {
                events.push(Event::ToolCallEnd {
                    choice_index: 0,
                    tool_index: 0,
                });
            }
            events.extend([
                Event::Finish {
                    choice_index: 0,
                    reason: FinishReason::ToolCalls,
                    stop_sequence: None,
                },
                Event::StreamEnd,
            ]);
            aggregate.push(events).unwrap();
            assert!(aggregate.finish().is_err());
        }
    }
}
