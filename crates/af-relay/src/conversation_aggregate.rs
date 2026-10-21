use std::collections::{BTreeMap, btree_map::Entry};

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
    let mut aggregate = Aggregate::default();
    let mut bytes = 0_usize;
    match body {
        UpstreamBody::Full(body) => {
            if body.len() > MAX_UPSTREAM_RESPONSE_BODY_BYTES {
                return Err(UpstreamError::ProtocolError);
            }
            aggregate.push(decoder.push(&body)?)?;
        }
        UpstreamBody::Stream(mut source) => {
            while let Some(chunk) = source.next_chunk().await {
                let chunk = chunk.map_err(map_adaptor_error)?;
                bytes = bytes
                    .checked_add(chunk.len())
                    .filter(|size| *size <= MAX_UPSTREAM_RESPONSE_BODY_BYTES)
                    .ok_or(UpstreamError::ProtocolError)?;
                aggregate.push(decoder.push(&chunk)?)?;
                if aggregate.done {
                    break;
                }
            }
        }
    }
    aggregate.push(decoder.finish()?)?;
    aggregate.finish()
}

#[derive(Default)]
pub(crate) struct Aggregate {
    choices: BTreeMap<u32, Choice>,
    usage: Option<Usage>,
    logprobs_bytes: usize,
    done: bool,
}

#[derive(Default)]
struct Choice {
    content: BTreeMap<u32, ContentBlock>,
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
        Ok(CanonicalResponse::new(
            Operation::Chat,
            "aggregated".to_owned(),
            "aggregated".to_owned(),
            None,
            choices,
            self.usage,
        ))
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
    use super::*;

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
