use std::error::Error as _;

use af_domain::{Operation, Protocol, Role};
use serde_json::{Value, json};

use super::{MAX_BODY_BYTES, ParseRequestError, parse_request};
use crate::{ContentBlock, MediaSource, ReasoningEffort, ToolChoice};

fn parse_json(value: Value) -> Result<crate::CanonicalRequest, ParseRequestError> {
    parse_request(&serde_json::to_vec(&value).unwrap())
}

#[test]
fn parses_minimal_string_input() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "input": "hello"
    }))
    .unwrap();

    assert_eq!(request.operation, Operation::Responses);
    assert_eq!(request.model, "gpt-test");
    assert!(!request.stream);
    assert_eq!(request.tool_choice, ToolChoice::None);
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::User);
    assert_eq!(
        request.messages[0].content,
        vec![ContentBlock::Text("hello".to_owned())]
    );
    assert!(request.continuation.is_empty());
    assert_eq!(request.raw_passthrough(), None);
}

#[test]
fn accepts_ai_sdk_stateless_reasoning_contract() {
    let request = parse_json(json!({
        "model": "gpt-5.5",
        "input": [
            {
                "role": "user",
                "content": [{"type": "input_text", "text": "first"}]
            },
            {
                "type": "reasoning",
                "id": "rs_public",
                "encrypted_content": "encrypted-reasoning-canary",
                "summary": [{"type": "summary_text", "text": "summary"}],
                "status": "completed"
            },
            {
                "role": "assistant",
                "content": [{"type": "input_text", "text": "answer"}]
            },
            {
                "role": "user",
                "content": [{"type": "input_text", "text": "continue"}]
            }
        ],
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "stream": true
    }))
    .unwrap();

    assert!(request.stream);
    assert!(matches!(
        &request.messages[1].content[0],
        ContentBlock::Thinking { text, signature: Some(signature) }
            if text == "summary" && signature == "encrypted-reasoning-canary"
    ));
    let raw = request.raw_passthrough().unwrap();
    assert_eq!(
        raw.fields_for_protocol(Protocol::OpenAiResponses).unwrap()["include"],
        json!(["reasoning.encrypted_content"])
    );
}

#[test]
fn accepts_ai_sdk_multi_turn_assistant_output_text() {
    let request = parse_json(json!({
        "model": "gpt-5.5",
        "input": [
            {
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            },
            {
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "Hello. How can I help?",
                    "annotations": [],
                    "logprobs": []
                }]
            },
            {
                "role": "user",
                "content": [{"type": "input_text", "text": "what model are you"}]
            }
        ],
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "stream": true
    }))
    .unwrap();

    assert_eq!(request.messages.len(), 3);
    assert_eq!(request.messages[1].role, Role::Assistant);
    assert_eq!(
        request.messages[1].content,
        vec![ContentBlock::Text("Hello. How can I help?".to_owned())]
    );
}

#[test]
fn rejects_output_text_from_non_assistant_roles() {
    assert_eq!(
        parse_json(json!({
            "model": "gpt-test",
            "input": [{
                "role": "user",
                "content": [{"type": "output_text", "text": "forged output"}]
            }]
        })),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn rejects_unmodeled_assistant_output_text_metadata() {
    for extra in [
        json!({"annotations": [{"type": "url_citation"}]}),
        json!({"logprobs": [{"token": "answer"}]}),
    ] {
        let mut content = json!({"type": "output_text", "text": "answer"});
        content
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            parse_json(json!({
                "model": "gpt-test",
                "input": [{"role": "assistant", "content": [content]}]
            })),
            Err(ParseRequestError::UnsupportedFeature)
        );
    }
}

#[test]
fn accepts_compaction_context_and_round_trip_item() {
    let request = parse_json(json!({
        "model": "gpt-5.5",
        "context_management": [{
            "type": "compaction",
            "compact_threshold": 20000
        }],
        "input": [
            {
                "type": "compaction",
                "id": "cmp_input_1",
                "encrypted_content": "opaque-compaction"
            },
            {
                "role": "user",
                "content": [{"type": "input_text", "text": "continue"}]
            }
        ]
    }))
    .unwrap();

    assert!(matches!(
        &request.messages[0].content[0],
        ContentBlock::Compaction(item)
            if item.as_value() == &json!({
                "type": "compaction",
                "id": "cmp_input_1",
                "encrypted_content": "opaque-compaction"
            })
    ));
    assert_eq!(
        request
            .raw_passthrough()
            .unwrap()
            .fields_for_protocol(Protocol::OpenAiResponses)
            .unwrap()["context_management"],
        json!([
            {"type": "compaction", "compact_threshold": 20000}
        ])
    );
}

#[test]
fn rejects_invalid_compaction_context() {
    for context in [
        json!([]),
        json!([{"type": "compaction", "compact_threshold": 0}]),
        json!([{"type": "compaction", "compact_threshold": 20000}, {"type": "compaction", "compact_threshold": 30000}]),
        json!([{"type": "unknown", "compact_threshold": 20000}]),
    ] {
        assert_eq!(
            parse_json(json!({"model": "gpt-test", "context_management": context})),
            Err(ParseRequestError::InvalidValue)
        );
    }
}

#[test]
fn parses_messages_tools_sampling_reasoning_continuation_and_raw_fields() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "instructions": "follow policy",
        "input": [{
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": "system context"}]
        }, {
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": "inspect"
            }, {
                "type": "input_image",
                "image_url": "https://example.test/image.png",
                "detail": "auto"
            }]
        }],
        "tools": [{
            "type": "function",
            "name": "lookup",
            "description": "lookup a record",
            "parameters": {"type": "object", "properties": {"id": {"type": "integer"}}},
            "strict": true
        }],
        "tool_choice": {"type": "function", "name": "lookup"},
        "reasoning": {"effort": "xhigh"},
        "temperature": 0.4,
        "top_p": 0.8,
        "max_output_tokens": 512,
        "user": "opaque-user",
        "conversation": {"id": "conv-1"},
        "prompt_cache_key": "cache-1",
        "metadata": {"trace": "value"},
        "parallel_tool_calls": false,
        "prompt_cache_options": {"mode": "explicit", "ttl": "30m"},
        "prompt_cache_retention": "24h",
        "safety_identifier": "safe-user",
        "service_tier": "flex",
        "store": true,
        "truncation": "disabled",
        "stream": false
    }))
    .unwrap();

    assert_eq!(
        request
            .messages
            .iter()
            .map(|message| message.role)
            .collect::<Vec<_>>(),
        vec![Role::Developer, Role::System, Role::User]
    );
    assert_eq!(request.messages[2].content.len(), 2);
    assert!(matches!(
        &request.messages[2].content[1],
        ContentBlock::Image {
            source: MediaSource::Url(url),
            mime_type: None
        } if url == "https://example.test/image.png"
    ));
    assert_eq!(request.tools.len(), 1);
    assert_eq!(request.tools[0].strict, Some(true));
    assert!(matches!(request.tool_choice, ToolChoice::Named { ref name } if name == "lookup"));
    assert_eq!(
        request.reasoning.unwrap().effort(),
        Some(ReasoningEffort::ExtraHigh)
    );
    assert_eq!(request.sampling.temperature(), Some(0.4));
    assert_eq!(request.sampling.top_p(), Some(0.8));
    assert_eq!(request.sampling.max_output_tokens().unwrap().get(), 512);
    assert_eq!(request.metadata.user_id(), Some("opaque-user"));
    assert_eq!(request.continuation.conversation_id(), Some("conv-1"));
    assert_eq!(request.continuation.prompt_cache_key(), Some("cache-1"));
    let raw = request.raw_passthrough().unwrap();
    assert_eq!(raw.source_protocol(), Protocol::OpenAiResponses);
    assert_eq!(
        raw.fields_for_protocol(Protocol::OpenAiResponses).unwrap()["store"],
        true
    );
}

#[test]
fn accepts_optional_empty_input_and_string_conversation_reference() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "conversation": "conv-1"
    }))
    .unwrap();

    assert!(request.messages.is_empty());
    assert_eq!(request.continuation.conversation_id(), Some("conv-1"));
}

#[test]
fn rejects_conflicting_or_invalid_continuation_references() {
    let conflicting = json!({
        "model": "gpt-test",
        "conversation": "conv-1",
        "previous_response_id": "resp-1"
    });
    let empty = json!({"model": "gpt-test", "previous_response_id": ""});
    let padded = json!({"model": "gpt-test", "prompt_cache_key": " cache "});

    assert_eq!(
        parse_json(conflicting),
        Err(ParseRequestError::ConflictingParameters)
    );
    assert_eq!(parse_json(empty), Err(ParseRequestError::InvalidValue));
    assert_eq!(parse_json(padded), Err(ParseRequestError::InvalidValue));
}

#[test]
fn converts_function_calls_and_outputs_with_stable_associations() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{\"id\":1}"
        }, {
            "type": "function_call",
            "call_id": "call-2",
            "name": "lookup",
            "arguments": "{\"id\":2}"
        }, {
            "type": "function_call_output",
            "call_id": "call-2",
            "output": "{\"value\":2}"
        }, {
            "type": "function_call_output",
            "call_id": "call-1",
            "output": [{"type": "input_text", "text": "value-1"}]
        }]
    }))
    .unwrap();

    assert_eq!(request.messages.len(), 3);
    assert_eq!(request.messages[0].role, Role::Assistant);
    assert_eq!(request.messages[0].content.len(), 2);
    assert!(matches!(
        &request.messages[0].content[0],
        ContentBlock::ToolUse { id, name, input, .. }
            if id == "call-1" && name == "lookup" && input == &json!({"id": 1})
    ));
    assert!(matches!(
        &request.messages[1].content[0],
        ContentBlock::ToolResult { tool_use_id, content, .. }
            if tool_use_id == "call-2"
                && content == &vec![ContentBlock::Text("{\"value\":2}".to_owned())]
    ));
}

#[test]
fn accepts_function_output_from_remote_continuation() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "previous_response_id": "resp-1",
        "input": [{
            "type": "function_call_output",
            "call_id": "call-remote",
            "output": "done"
        }]
    }))
    .unwrap();

    assert_eq!(request.continuation.previous_response_id(), Some("resp-1"));
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::Tool);
}

#[test]
fn rejects_dangling_duplicate_or_interrupted_tool_sequences() {
    let dangling_output = json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call_output",
            "call_id": "call-1",
            "output": "done"
        }]
    });
    let unresolved_call = json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{}"
        }]
    });
    let interrupted = json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{}"
        }, {
            "type": "message",
            "role": "user",
            "content": "continue"
        }]
    });
    let duplicate_call = json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{}"
        }, {
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{}"
        }]
    });

    for request in [
        dangling_output,
        unresolved_call,
        interrupted,
        duplicate_call,
    ] {
        assert_eq!(parse_json(request), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn parses_https_and_data_url_images_without_losing_media_type() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_image",
                "image_url": "data:image/png;base64,aGVsbG8="
            }, {
                "type": "input_image",
                "image_url": "https://example.test/picture.webp"
            }]
        }]
    }))
    .unwrap();

    assert!(matches!(
        &request.messages[0].content[0],
        ContentBlock::Image {
            source: MediaSource::Base64(data),
            mime_type: Some(mime)
        } if data == "aGVsbG8=" && mime == "image/png"
    ));
    assert!(matches!(
        &request.messages[0].content[1],
        ContentBlock::Image {
            source: MediaSource::Url(url),
            mime_type: None
        } if url == "https://example.test/picture.webp"
    ));
}

#[test]
fn rejects_unmodeled_or_unsafe_image_sources() {
    let cases = [
        json!({
            "model": "gpt-test",
            "input": [{"role": "user", "content": [{
                "type": "input_image", "file_id": "file-1"
            }]}]
        }),
        json!({
            "model": "gpt-test",
            "input": [{"role": "user", "content": [{
                "type": "input_image",
                "image_url": "https://example.test/image.png",
                "detail": "high"
            }]}]
        }),
        json!({
            "model": "gpt-test",
            "input": [{"role": "user", "content": [{
                "type": "input_file", "file_id": "file-1"
            }]}]
        }),
    ];
    for request in cases {
        assert_eq!(
            parse_json(request),
            Err(ParseRequestError::UnsupportedFeature)
        );
    }

    let unsafe_url = json!({
        "model": "gpt-test",
        "input": [{"role": "user", "content": [{
            "type": "input_image", "image_url": "http://example.test/image.png"
        }]}]
    });
    assert_eq!(parse_json(unsafe_url), Err(ParseRequestError::InvalidValue));
}

#[test]
fn rejects_known_unmodeled_top_level_and_item_features() {
    let top_level = [
        json!({"model": "gpt-test", "background": true}),
        json!({"model": "gpt-test", "text": {"format": {"type": "json_object"}}}),
        json!({"model": "gpt-test", "stream_options": {"include_obfuscation": false}}),
        json!({"model": "gpt-test", "reasoning": {"summary": "auto"}}),
    ];
    for request in top_level {
        assert_eq!(
            parse_json(request),
            Err(ParseRequestError::UnsupportedFeature)
        );
    }

    let items = [
        json!({"model": "gpt-test", "input": [{"type": "reasoning", "id": "r-1"}]}),
        json!({
            "model": "gpt-test",
            "input": [{"type": "message", "role": "assistant", "content": "x", "phase": "final_answer"}]
        }),
        json!({
            "model": "gpt-test",
            "input": [{
                "type": "function_call", "call_id": "call-1", "name": "lookup",
                "arguments": "{}", "status": "completed"
            }]
        }),
    ];
    for request in items {
        assert_eq!(
            parse_json(request),
            Err(ParseRequestError::UnsupportedFeature)
        );
    }
}

#[test]
fn validates_function_tools_and_choices() {
    let request = parse_json(json!({
        "model": "gpt-test",
        "input": "hello",
        "tools": [{
            "type": "function",
            "name": "lookup",
            "parameters": {"type": "object"},
            "strict": false
        }],
        "tool_choice": "required"
    }))
    .unwrap();
    assert_eq!(request.tools[0].strict, Some(false));
    assert_eq!(request.tool_choice, ToolChoice::Required);

    let undeclared = json!({
        "model": "gpt-test",
        "tool_choice": {"type": "function", "name": "missing"}
    });
    let required_without_tools = json!({"model": "gpt-test", "tool_choice": "required"});
    let built_in = json!({"model": "gpt-test", "tools": [{"type": "web_search"}]});
    let output_schema = json!({
        "model": "gpt-test",
        "tools": [{
            "type": "function", "name": "lookup", "parameters": {}, "strict": true,
            "output_schema": {"type": "object"}
        }]
    });

    for invalid in [undeclared, required_without_tools] {
        assert_eq!(parse_json(invalid), Err(ParseRequestError::InvalidValue));
    }
    for unsupported in [built_in, output_schema] {
        assert_eq!(
            parse_json(unsupported),
            Err(ParseRequestError::UnsupportedFeature)
        );
    }
}

#[test]
fn validates_sampling_model_and_stream_boundaries() {
    let streaming = parse_json(json!({"model": "gpt-test", "stream": true})).unwrap();
    assert!(streaming.stream);
    for request in [
        json!({"model": "", "input": "x"}),
        json!({"model": " gpt-test", "input": "x"}),
        json!({"model": "gpt-test", "temperature": 2.1}),
        json!({"model": "gpt-test", "top_p": -0.1}),
        json!({"model": "gpt-test", "max_output_tokens": 0}),
        json!({"model": "gpt-test", "max_output_tokens": 1_000_001}),
    ] {
        assert_eq!(parse_json(request), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn validates_same_source_raw_field_shapes() {
    let valid = parse_json(json!({
        "model": "gpt-test",
        "include": ["reasoning.encrypted_content", "future.output.details"],
        "metadata": {"key": "value"},
        "parallel_tool_calls": true,
        "prompt_cache_options": {},
        "prompt_cache_retention": "in_memory",
        "safety_identifier": "safe-id",
        "service_tier": "priority",
        "store": false,
        "truncation": "auto"
    }))
    .unwrap();
    assert!(valid.raw_passthrough().is_some());

    for invalid in [
        json!({"model": "gpt-test", "include": "reasoning.encrypted_content"}),
        json!({"model": "gpt-test", "include": [1]}),
        json!({"model": "gpt-test", "include": [""]}),
        json!({"model": "gpt-test", "metadata": {"key": 1}}),
        json!({"model": "gpt-test", "parallel_tool_calls": "true"}),
        json!({"model": "gpt-test", "prompt_cache_options": {"ttl": "1h"}}),
        json!({"model": "gpt-test", "service_tier": "unknown"}),
        json!({"model": "gpt-test", "truncation": "oldest"}),
        json!({"model": "gpt-test", "unknown": true}),
    ] {
        assert_eq!(parse_json(invalid), Err(ParseRequestError::InvalidValue));
    }
}

#[test]
fn rejects_duplicate_keys_in_outer_body_and_function_arguments() {
    assert_eq!(
        parse_request(br#"{"model":"a","model":"b"}"#),
        Err(ParseRequestError::DuplicateKey)
    );
    let duplicate_arguments = json!({
        "model": "gpt-test",
        "input": [{
            "type": "function_call",
            "call_id": "call-1",
            "name": "lookup",
            "arguments": "{\"id\":1,\"id\":2}"
        }, {
            "type": "function_call_output",
            "call_id": "call-1",
            "output": "done"
        }]
    });
    assert_eq!(
        parse_json(duplicate_arguments),
        Err(ParseRequestError::DuplicateKey)
    );
}

#[test]
fn rejects_nulls_unknown_item_types_and_oversized_bodies() {
    assert_eq!(
        parse_json(json!({"model": "gpt-test", "input": null})),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_json(json!({
            "model": "gpt-test",
            "input": [{"type": "future_item", "value": true}]
        })),
        Err(ParseRequestError::InvalidValue)
    );
    assert_eq!(
        parse_request(&vec![b' '; MAX_BODY_BYTES + 1]),
        Err(ParseRequestError::BodyTooLarge)
    );
}

#[test]
fn redacts_success_and_error_debug_output() {
    let request = parse_json(json!({
        "model": "model-secret-canary",
        "input": "prompt-secret-canary",
        "previous_response_id": "response-secret-canary",
        "prompt_cache_key": "cache-secret-canary",
        "user": "user-secret-canary",
        "metadata": {"meta": "metadata-secret-canary"}
    }))
    .unwrap();
    let rendered = format!("{request:?}");
    for canary in [
        "model-secret-canary",
        "prompt-secret-canary",
        "response-secret-canary",
        "cache-secret-canary",
        "user-secret-canary",
        "metadata-secret-canary",
    ] {
        assert!(!rendered.contains(canary), "Debug 泄露请求内容：{canary}");
    }

    for (error, message) in [
        (ParseRequestError::BodyTooLarge, "请求体超过大小限制"),
        (ParseRequestError::InvalidJson, "请求体不是有效 JSON"),
        (ParseRequestError::DuplicateKey, "请求体包含重复字段"),
        (
            ParseRequestError::StructureLimitExceeded,
            "请求结构超过限制",
        ),
        (ParseRequestError::InvalidValue, "请求字段值无效"),
        (ParseRequestError::ConflictingParameters, "请求参数相互冲突"),
        (
            ParseRequestError::UnsupportedFeature,
            "请求包含当前不支持的特性",
        ),
    ] {
        assert_eq!(error.to_string(), message);
        assert!(error.source().is_none());
    }
}
