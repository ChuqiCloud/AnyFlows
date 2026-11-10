use af_adapter::{Bytes, UpstreamBody};
use af_domain::{Operation, Protocol, Role};
use af_protocol::{
    AnthropicCompactionItem, CanonicalRequest, CanonicalStreamEvent as Event, ContentBlock,
    FinishReason, Message, Sampling, TokenCount, anthropic,
};
use serde_json::{Value, json};

use super::build_response;
use crate::{
    AnthropicMessagesStream, OpenAiChatStream, anthropic_chat_bridge,
    chat_stream_encoder::ChatStreamEncoder, conversation_aggregate::Aggregate,
    conversation_decoder::ConversationDecoder, estimate_openai_request_upper_bound,
    openai_chat_usage::OpenAiChatUsageEstimator,
};

fn block(content: Value) -> Value {
    json!({"type":"compaction","content":content,"signature":"native-signature",
        "encrypted_content":"opaque-state","tool_changes":[{"type":"tool_removal","name":"old"}],
        "future_extension":null})
}

fn wire_usage() -> Value {
    json!({"input_tokens":100,"output_tokens":4,"iterations":[
        {"type":"message","input_tokens":100,"output_tokens":4},
        {"type":"compaction","input_tokens":12,"output_tokens":3}
    ],"service_tier":"standard"})
}

fn wire_response(content: Value, compact: bool) -> Value {
    json!({"id":"msg_private","type":"message","role":"assistant","model":"private-model",
        "content":[if compact {block(content)} else {json!({"type":"text","text":"answer"})}],
        "stop_reason":if compact {"compaction"} else {"end_turn"},"usage":wire_usage()})
}

fn wire_stream(content: Value, compact: bool) -> Bytes {
    let response = wire_response(content, compact);
    let frames = [
        json!({"type":"message_start","message":{"id":"msg_private","type":"message","role":"assistant","model":"private-model","content":[],"stop_reason":null,"usage":{"input_tokens":120,"output_tokens":0,"service_tier":"standard"}}}),
        json!({"type":"content_block_start","index":0,"content_block":response["content"][0]}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":response["stop_reason"],"stop_sequence":null},"usage":wire_usage()}),
        json!({"type":"message_stop"}),
    ];
    Bytes::from(
        frames
            .iter()
            .map(|frame| {
                format!(
                    "event: {}\ndata: {frame}\n\n",
                    frame["type"].as_str().unwrap()
                )
            })
            .collect::<String>(),
    )
}

fn request() -> CanonicalRequest {
    let mut request = CanonicalRequest::new(
        Operation::Chat,
        "public-model".into(),
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text("continue".into())],
        )],
        true,
    );
    request.sampling =
        Sampling::new(None, None, Some(TokenCount::new(128).unwrap()), vec![]).unwrap();
    request
}

fn assert_public_response(body: &[u8], expected_block: &Value) {
    let value: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(value["id"], "msg_public");
    assert_eq!(value["model"], "public-model");
    assert_eq!(&value["content"][0], expected_block);
    assert_eq!(value["usage"]["input_tokens"], 100);
    assert_eq!(value["usage"]["output_tokens"], 4);
    assert_eq!(value["usage"]["iterations"], wire_usage()["iterations"]);
    let parsed = anthropic::parse_response(body).unwrap();
    let billed = anthropic::billing_usage(&parsed).unwrap().unwrap();
    assert_eq!(billed.input_tokens().get(), 112);
    assert_eq!(billed.output_tokens().get(), 7);
}

#[tokio::test]
async fn full_and_collected_native_compaction_preserve_state_and_distinct_billing() {
    for content in [json!("摘要\nkeep constraints"), Value::Null] {
        for streaming in [false, true] {
            let response = if streaming {
                crate::collect_conversation_response(
                    Protocol::Anthropic,
                    UpstreamBody::Full(wire_stream(content.clone(), true)),
                )
                .await
                .unwrap()
            } else {
                anthropic::parse_response(
                    &serde_json::to_vec(&wire_response(content.clone(), true)).unwrap(),
                )
                .unwrap()
            };
            let billed = anthropic::billing_usage(&response).unwrap().unwrap();
            let body = build_response(
                Protocol::Anthropic,
                response,
                billed,
                "public",
                "public-model",
                1_700_000_000,
            )
            .unwrap();
            assert_public_response(&body, &block(content.clone()));
        }
    }
}

#[tokio::test]
async fn dedicated_and_generic_native_streams_bill_iterations_without_inflating_wire_usage() {
    for content in [json!("summary"), Value::Null] {
        for generic in [false, true] {
            let input = UpstreamBody::Full(wire_stream(content.clone(), true));
            let (output, billed) = if generic {
                let encoder = ChatStreamEncoder::new(
                    Protocol::Anthropic,
                    "public",
                    "public-model",
                    1_700_000_000,
                    &request(),
                    false,
                )
                .unwrap()
                .with_upstream_protocol(Protocol::Anthropic);
                let (stream, usage) = OpenAiChatStream::new(
                    input,
                    encoder,
                    OpenAiChatUsageEstimator::new(&request()),
                );
                let mut stream =
                    stream.with_decoder(ConversationDecoder::new(Protocol::Anthropic).unwrap());
                let mut output = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    output.extend(chunk.unwrap());
                }
                (output, usage.resolve().await.unwrap())
            } else {
                let (mut stream, usage) =
                    AnthropicMessagesStream::new(input, "msg_public", "public-model");
                let mut output = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    output.extend(chunk.unwrap());
                }
                (output, usage.resolve().await.unwrap())
            };
            assert_eq!(billed.input_tokens().get(), 112);
            assert_eq!(billed.output_tokens().get(), 7);
            let response = crate::collect_conversation_response(
                Protocol::Anthropic,
                UpstreamBody::Full(Bytes::from(output)),
            )
            .await
            .unwrap();
            let body = build_response(
                Protocol::Anthropic,
                response,
                billed,
                "public",
                "public-model",
                1_700_000_000,
            )
            .unwrap();
            assert_public_response(&body, &block(content.clone()));
        }
    }
}

#[tokio::test]
async fn ordinary_anthropic_metadata_converts_and_compaction_iterations_are_counted_once() {
    for target in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Gemini,
    ] {
        let encoder = ChatStreamEncoder::new(
            target,
            "public",
            "public-model",
            1_700_000_000,
            &request(),
            true,
        )
        .unwrap()
        .with_upstream_protocol(Protocol::Anthropic);
        let (stream, usage) = OpenAiChatStream::new(
            UpstreamBody::Full(wire_stream(Value::Null, false)),
            encoder,
            OpenAiChatUsageEstimator::new(&request()),
        );
        let mut stream =
            stream.with_decoder(ConversationDecoder::new(Protocol::Anthropic).unwrap());
        let mut output = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            output.extend(chunk.unwrap());
        }
        let billed = usage.resolve().await.unwrap();
        assert_eq!(billed.input_tokens().get(), 112, "{target:?}");
        assert_eq!(billed.output_tokens().get(), 7, "{target:?}");
        let response =
            crate::collect_conversation_response(target, UpstreamBody::Full(Bytes::from(output)))
                .await
                .unwrap();
        assert_eq!(
            response
                .usage
                .unwrap()
                .checked_input_tokens()
                .unwrap()
                .get(),
            112,
            "{target:?}"
        );
        assert_eq!(
            response.usage.unwrap().output_tokens().get(),
            7,
            "{target:?}"
        );
    }
}

#[test]
fn native_replay_is_precharged_and_opaque_responses_cannot_be_dropped_by_conversion() {
    let item = AnthropicCompactionItem::from_value(block(json!("summary"))).unwrap();
    let mut replay = request();
    replay.messages.insert(
        0,
        Message::new(
            Role::Assistant,
            vec![ContentBlock::AnthropicCompaction(item.clone())],
        ),
    );
    assert!(
        estimate_openai_request_upper_bound(&replay)
            .unwrap()
            .input_tokens()
            > TokenCount::ZERO
    );
    let response = anthropic::parse_response(
        &serde_json::to_vec(&wire_response(json!("summary"), true)).unwrap(),
    )
    .unwrap();
    for target in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Gemini,
    ] {
        assert!(
            anthropic_chat_bridge::adapt_response(
                Protocol::Anthropic,
                target,
                &mut response.clone()
            )
            .is_err()
        );
        let mut bridge = anthropic_chat_bridge::CrossProtocolStreamBridge::new(target);
        assert!(
            bridge
                .adapt(Event::AnthropicCompactionStart {
                    choice_index: 0,
                    content_index: 0,
                    item: item.clone()
                })
                .is_err()
        );
        assert!(
            bridge
                .adapt(Event::Finish {
                    choice_index: 0,
                    reason: FinishReason::Compaction,
                    stop_sequence: None
                })
                .is_err()
        );
    }
    let mut aggregate = Aggregate::default();
    aggregate
        .push(vec![
            Event::MessageStart {
                choice_index: 0,
                role: Role::Assistant,
            },
            Event::AnthropicCompactionStart {
                choice_index: 0,
                content_index: 0,
                item,
            },
            Event::Finish {
                choice_index: 0,
                reason: FinishReason::Compaction,
                stop_sequence: None,
            },
            Event::StreamEnd,
        ])
        .unwrap();
    assert!(aggregate.finish().is_err());
}
