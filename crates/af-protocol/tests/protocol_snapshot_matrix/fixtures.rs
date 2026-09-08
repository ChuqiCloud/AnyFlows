use serde_json::{Value, json};

/// 快照矩阵覆盖的协议集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolKind {
    OpenAiChat,
    Anthropic,
    Gemini,
    OpenAiResponses,
}

impl ProtocolKind {
    pub const ALL: [Self; 4] = [
        Self::OpenAiChat,
        Self::Anthropic,
        Self::Gemini,
        Self::OpenAiResponses,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenAiChat => "openai_chat",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenAiResponses => "openai_responses",
        }
    }
}

pub const MODEL: &str = "matrix-model";
pub const CREATED_AT: i64 = 1_700_000_000;

pub fn request(protocol: ProtocolKind) -> Value {
    match protocol {
        ProtocolKind::OpenAiChat => json!({
            "model": MODEL,
            "messages": [{"role": "user", "content": "matrix-input"}],
            "max_completion_tokens": 32
        }),
        ProtocolKind::Anthropic => json!({
            "model": MODEL,
            "max_tokens": 32,
            "messages": [{"role": "user", "content": "matrix-input"}]
        }),
        ProtocolKind::Gemini => json!({
            "contents": [{"role": "user", "parts": [{"text": "matrix-input"}]}],
            "generationConfig": {"maxOutputTokens": 32}
        }),
        ProtocolKind::OpenAiResponses => json!({
            "model": MODEL,
            "input": "matrix-input",
            "max_output_tokens": 32
        }),
    }
}

pub fn response(protocol: ProtocolKind) -> Value {
    match protocol {
        ProtocolKind::OpenAiChat => json!({
            "id": "chatcmpl-matrix",
            "object": "chat.completion",
            "created": CREATED_AT,
            "model": MODEL,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "matrix-output"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 3,
                "total_tokens": 11
            }
        }),
        ProtocolKind::Anthropic => json!({
            "id": "msg-matrix",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "matrix-output", "citations": null}],
            "model": MODEL,
            "container": null,
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "stop_details": null,
            "usage": {
                "input_tokens": 8,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "cache_creation": null,
                "output_tokens": 3,
                "output_tokens_details": null,
                "inference_geo": null,
                "server_tool_use": null,
                "service_tier": null
            }
        }),
        ProtocolKind::Gemini => json!({
            "responseId": "gemini-matrix",
            "modelVersion": MODEL,
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "matrix-output"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 3,
                "totalTokenCount": 11
            }
        }),
        ProtocolKind::OpenAiResponses => json!({
            "id": "resp-matrix",
            "object": "response",
            "created_at": CREATED_AT,
            "status": "completed",
            "error": null,
            "incomplete_details": null,
            "model": MODEL,
            "output": [{
                "id": "msg-matrix",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "matrix-output",
                    "annotations": [],
                    "logprobs": []
                }]
            }],
            "usage": {
                "input_tokens": 8,
                "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                "output_tokens": 3,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 11
            }
        }),
    }
}

pub fn stream(protocol: ProtocolKind) -> Vec<u8> {
    match protocol {
        ProtocolKind::OpenAiChat => openai_chat_stream(),
        ProtocolKind::Anthropic => anthropic_stream(),
        ProtocolKind::Gemini => gemini_stream(),
        ProtocolKind::OpenAiResponses => openai_responses_stream(),
    }
}

fn openai_chat_stream() -> Vec<u8> {
    let chunk = |choices: Value, usage: Option<Value>| {
        let mut value = json!({
            "id": "chatcmpl-matrix-stream",
            "object": "chat.completion.chunk",
            "created": CREATED_AT,
            "model": MODEL,
            "choices": choices
        });
        if let Some(usage) = usage {
            value["usage"] = usage;
        }
        sse_data(&value)
    };
    [
        chunk(
            json!([{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]),
            None,
        ),
        chunk(
            json!([{"index": 0, "delta": {"content": "matrix-output"}, "finish_reason": null}]),
            None,
        ),
        chunk(
            json!([{"index": 0, "delta": {}, "finish_reason": "stop"}]),
            None,
        ),
        chunk(
            json!([]),
            Some(json!({
                "prompt_tokens": 8,
                "completion_tokens": 3,
                "total_tokens": 11
            })),
        ),
        b"data: [DONE]\n\n".to_vec(),
    ]
    .concat()
}

fn anthropic_stream() -> Vec<u8> {
    [
        named_event(
            "message_start",
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg-matrix-stream",
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": MODEL,
                    "container": null,
                    "stop_reason": null,
                    "stop_sequence": null,
                    "stop_details": null,
                    "usage": {
                        "input_tokens": 8,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0,
                        "cache_creation": null,
                        "output_tokens": 0,
                        "output_tokens_details": {"thinking_tokens": 0},
                        "inference_geo": null,
                        "server_tool_use": null,
                        "service_tier": null
                    }
                }
            }),
        ),
        named_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": "", "citations": null}
            }),
        ),
        named_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "matrix-output"}
            }),
        ),
        named_event(
            "content_block_stop",
            json!({"type": "content_block_stop", "index": 0}),
        ),
        named_event(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {
                    "container": null,
                    "stop_details": null,
                    "stop_reason": "end_turn",
                    "stop_sequence": null
                },
                "usage": {
                    "input_tokens": 8,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0,
                    "output_tokens": 3,
                    "output_tokens_details": {"thinking_tokens": 0},
                    "server_tool_use": null
                }
            }),
        ),
        named_event("message_stop", json!({"type": "message_stop"})),
    ]
    .concat()
}

fn gemini_stream() -> Vec<u8> {
    sse_data(&json!({
        "responseId": "gemini-matrix-stream",
        "modelVersion": MODEL,
        "candidates": [{
            "index": 0,
            "content": {"role": "model", "parts": [{"text": "matrix-output"}]},
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 8,
            "candidatesTokenCount": 3,
            "totalTokenCount": 11
        }
    }))
}

fn openai_responses_stream() -> Vec<u8> {
    let item = |status: &str| {
        json!({
            "id": "msg-matrix-stream",
            "type": "message",
            "status": status,
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": "matrix-output",
                "annotations": [],
                "logprobs": []
            }]
        })
    };
    let initial = responses_state("in_progress", Vec::new(), Value::Null);
    let terminal_item = item("completed");
    [
        responses_event("response.created", 0, json!({"response": initial.clone()})),
        responses_event("response.in_progress", 1, json!({"response": initial})),
        responses_event(
            "response.output_item.added",
            2,
            json!({
                "output_index": 0,
                "item": {
                    "id": "msg-matrix-stream",
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": []
                }
            }),
        ),
        responses_event(
            "response.content_part.added",
            3,
            json!({
                "item_id": "msg-matrix-stream",
                "output_index": 0,
                "content_index": 0,
                "part": output_text("")
            }),
        ),
        responses_event(
            "response.output_text.delta",
            4,
            json!({
                "item_id": "msg-matrix-stream",
                "output_index": 0,
                "content_index": 0,
                "delta": "matrix-output",
                "logprobs": []
            }),
        ),
        responses_event(
            "response.output_text.done",
            5,
            json!({
                "item_id": "msg-matrix-stream",
                "output_index": 0,
                "content_index": 0,
                "text": "matrix-output",
                "logprobs": []
            }),
        ),
        responses_event(
            "response.content_part.done",
            6,
            json!({
                "item_id": "msg-matrix-stream",
                "output_index": 0,
                "content_index": 0,
                "part": output_text("matrix-output")
            }),
        ),
        responses_event(
            "response.output_item.done",
            7,
            json!({"output_index": 0, "item": terminal_item.clone()}),
        ),
        responses_event(
            "response.completed",
            8,
            json!({
                "response": responses_state(
                    "completed",
                    vec![terminal_item],
                    json!({
                        "input_tokens": 8,
                        "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                        "output_tokens": 3,
                        "output_tokens_details": {"reasoning_tokens": 0},
                        "total_tokens": 11
                    })
                )
            }),
        ),
    ]
    .concat()
}

fn responses_state(status: &str, output: Vec<Value>, usage: Value) -> Value {
    json!({
        "id": "resp-matrix-stream",
        "object": "response",
        "created_at": CREATED_AT,
        "status": status,
        "error": null,
        "incomplete_details": null,
        "model": MODEL,
        "output": output,
        "usage": usage
    })
}

fn output_text(text: &str) -> Value {
    json!({
        "type": "output_text",
        "text": text,
        "annotations": [],
        "logprobs": []
    })
}

fn responses_event(kind: &str, sequence: u64, fields: Value) -> Vec<u8> {
    let mut fields = fields.as_object().cloned().expect("测试事件必须是对象");
    fields.insert("type".to_owned(), Value::String(kind.to_owned()));
    fields.insert("sequence_number".to_owned(), Value::Number(sequence.into()));
    named_event(kind, Value::Object(fields))
}

fn named_event(name: &str, value: Value) -> Vec<u8> {
    format!(
        "event: {name}\ndata: {}\n\n",
        serde_json::to_string(&value).expect("测试事件必须可序列化")
    )
    .into_bytes()
}

fn sse_data(value: &Value) -> Vec<u8> {
    format!(
        "data: {}\n\n",
        serde_json::to_string(value).expect("测试事件必须可序列化")
    )
    .into_bytes()
}
