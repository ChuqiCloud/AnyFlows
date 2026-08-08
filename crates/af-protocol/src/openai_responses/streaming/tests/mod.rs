mod decode;
mod encode;

use serde_json::{Value, json};

use crate::sse::SseParser;
use crate::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};

pub(super) const RESPONSE_ID: &str = "resp_stream_1";
pub(super) const MODEL: &str = "gpt-5.5";
pub(super) const CREATED_AT: i64 = 1_700_000_000;

pub(super) fn frame(kind: &str, sequence: u64, fields: Value) -> Vec<u8> {
    let mut fields = fields.as_object().cloned().expect("测试事件必须是对象");
    fields.insert("type".to_owned(), Value::String(kind.to_owned()));
    fields.insert("sequence_number".to_owned(), Value::Number(sequence.into()));
    let data = serde_json::to_vec(&Value::Object(fields)).expect("测试事件必须可序列化");
    let mut output = Vec::new();
    output.extend_from_slice(b"event: ");
    output.extend_from_slice(kind.as_bytes());
    output.extend_from_slice(b"\ndata: ");
    output.extend_from_slice(&data);
    output.extend_from_slice(b"\n\n");
    output
}

pub(super) fn frame_without_sequence(kind: &str, fields: Value) -> Vec<u8> {
    let mut fields = fields.as_object().cloned().expect("测试事件必须是对象");
    fields.insert("type".to_owned(), Value::String(kind.to_owned()));
    let data = serde_json::to_vec(&Value::Object(fields)).expect("测试事件必须可序列化");
    let mut output = Vec::new();
    output.extend_from_slice(b"event: ");
    output.extend_from_slice(kind.as_bytes());
    output.extend_from_slice(b"\ndata: ");
    output.extend_from_slice(&data);
    output.extend_from_slice(b"\n\n");
    output
}

pub(super) fn initial_response() -> Value {
    response("in_progress", Vec::new(), None, None, None)
}

pub(super) fn response(
    status: &str,
    output: Vec<Value>,
    usage: Option<Value>,
    incomplete_reason: Option<&str>,
    error: Option<Value>,
) -> Value {
    json!({
        "id": RESPONSE_ID,
        "object": "response",
        "created_at": CREATED_AT,
        "status": status,
        "error": error,
        "incomplete_details": incomplete_reason.map(|reason| json!({"reason": reason})),
        "model": MODEL,
        "output": output,
        "usage": usage,
    })
}

pub(super) fn output_text(text: &str) -> Value {
    json!({
        "type": "output_text",
        "text": text,
        "annotations": [],
        "logprobs": [],
    })
}

pub(super) fn message_item(id: &str, status: &str, text: &str) -> Value {
    json!({
        "id": id,
        "type": "message",
        "status": status,
        "role": "assistant",
        "content": [output_text(text)],
    })
}

pub(super) fn usage_value(input: i64, output: i64, cached: i64, reasoning: i64) -> Value {
    json!({
        "input_tokens": input,
        "input_tokens_details": {
            "cached_tokens": cached,
            "cache_write_tokens": 0,
        },
        "output_tokens": output,
        "output_tokens_details": {"reasoning_tokens": reasoning},
        "total_tokens": input + output,
    })
}

pub(super) fn usage(input: i64, output: i64, cached: i64, reasoning: i64) -> Usage {
    Usage::new(
        TokenCount::new(input).unwrap(),
        TokenCount::new(output).unwrap(),
        UsageDetails::new(
            TokenCount::new(cached).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::new(reasoning).unwrap(),
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}

pub(super) fn join(frames: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    frames.into_iter().flatten().collect()
}

pub(super) fn parse_encoded_events(bytes: &[u8]) -> Vec<(String, Value)> {
    let mut parser = SseParser::new();
    let frames = parser.push(bytes).expect("编码结果必须是合法 SSE");
    parser.finish().expect("编码结果必须位于事件边界");
    frames
        .into_iter()
        .filter(|frame| !frame.is_comment())
        .map(|frame| {
            let name = frame.event_name().expect("Responses 事件必须显式命名");
            let value = serde_json::from_slice(frame.data()).expect("data 必须是 JSON");
            (name.to_owned(), value)
        })
        .collect()
}
