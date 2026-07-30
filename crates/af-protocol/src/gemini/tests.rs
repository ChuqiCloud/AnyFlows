use std::error::Error as _;

use af_domain::Role;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use serde_json::{Value, json};

use super::{MAX_BODY_BYTES, ParseRequestError, parse_request, parse_stream_request_envelope};
use crate::{ContentBlock, MediaSource, ReasoningEffort, ToolChoice};

fn parse(value: Value) -> Result<crate::CanonicalRequest, ParseRequestError> {
    parse_request(
        "models/gemini-2.5-pro",
        &serde_json::to_vec(&value).unwrap(),
    )
}

fn minimal_body() -> Value {
    json!({
        "contents": [{
            "role": "user",
            "parts": [{"text": "hello"}]
        }]
    })
}

#[test]
fn parses_minimal_request_and_uses_path_model() {
    let request = parse(minimal_body()).unwrap();

    assert_eq!(request.model, "gemini-2.5-pro");
    assert!(!request.stream);
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::User);
    assert!(matches!(
        request.messages[0].content.as_slice(),
        [ContentBlock::Text(text)] if text == "hello"
    ));
    assert_eq!(request.tool_choice, ToolChoice::None);
    assert!(request.reasoning.is_none());
}

#[test]
fn stream_path_sets_canonical_mode_without_losing_source_proof() {
    let body = Bytes::from(serde_json::to_vec(&minimal_body()).unwrap());
    let envelope = parse_stream_request_envelope("models/gemini-2.5-pro", body.clone()).unwrap();

    assert!(envelope.canonical().stream);
    assert_eq!(
        envelope.source_protocol(),
        Some(af_domain::Protocol::Gemini)
    );
    let crate::SameProtocolDecision::Passthrough(source) =
        envelope.into_same_protocol(af_domain::Protocol::Gemini)
    else {
        panic!("Gemini 流式模式位于 URL，正文仍应保持同协议直通资格");
    };
    assert_eq!(source.body(), &body);
    assert!(source.canonical().stream);
}

#[test]
fn parses_system_media_tools_structured_results_and_thinking() {
    let thought_signature = STANDARD.encode("thought-signature");
    let tool_signature = STANDARD.encode("tool-signature");
    let body = json!({
        "systemInstruction": {
            "parts": [{"text": "Follow policy"}]
        },
        "contents": [
            {
                "role": "user",
                "parts": [
                    {"text": "Look at this"},
                    {"inlineData": {"mimeType": "IMAGE/PNG", "data": "aGk="}}
                ]
            },
            {
                "role": "model",
                "parts": [
                    {
                        "text": "I should inspect it",
                        "thought": true,
                        "thoughtSignature": thought_signature
                    },
                    {
                        "functionCall": {
                            "id": "call-1",
                            "name": "lookup",
                            "args": {"query": "status"}
                        },
                        "thoughtSignature": tool_signature
                    }
                ]
            },
            {
                "role": "user",
                "parts": [
                    {
                        "functionResponse": {
                            "id": "call-1",
                            "name": "lookup",
                            "response": {
                                "result": {"ok": true, "items": [1, 2]},
                                "nullable": null
                            }
                        }
                    },
                    {"text": "Continue"}
                ]
            }
        ],
        "tools": [{
            "functionDeclarations": [{
                "name": "lookup",
                "description": "Look up a status",
                "parametersJsonSchema": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"]
                }
            }]
        }],
        "toolConfig": {
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": ["lookup"]
            }
        },
        "generationConfig": {
            "temperature": 1.5,
            "topP": 0.9,
            "maxOutputTokens": 1024,
            "stopSequences": ["END"],
            "candidateCount": 1,
            "thinkingConfig": {
                "includeThoughts": true,
                "thinkingLevel": "HIGH"
            }
        }
    });

    let request = parse(body).unwrap();
    assert_eq!(request.messages.len(), 5);
    assert_eq!(request.messages[0].role, Role::System);
    assert_eq!(request.messages[1].role, Role::User);
    assert!(matches!(
        &request.messages[1].content[1],
        ContentBlock::Image {
            source: MediaSource::Base64(data),
            mime_type: Some(mime_type),
        } if data == "aGk=" && mime_type == "image/png"
    ));
    assert_eq!(request.messages[2].role, Role::Assistant);
    assert!(matches!(
        &request.messages[2].content[0],
        ContentBlock::Thinking { text, signature }
            if text == "I should inspect it"
                && signature.as_deref() == Some(thought_signature.as_str())
    ));
    assert!(matches!(
        &request.messages[2].content[1],
        ContentBlock::ToolUse {
            id,
            name,
            input,
            signature,
        } if id == "call-1"
            && name == "lookup"
            && input == &json!({"query": "status"})
            && signature.as_deref() == Some(tool_signature.as_str())
    ));
    assert_eq!(request.messages[3].role, Role::Tool);
    assert!(matches!(
        &request.messages[3].content[0],
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            structured_content: Some(result),
            is_error: false,
        } if tool_use_id == "call-1"
            && content.is_empty()
            && result == &json!({
                "result": {"ok": true, "items": [1, 2]},
                "nullable": null
            })
    ));
    assert_eq!(request.messages[4].role, Role::User);
    assert_eq!(request.tools.len(), 1);
    assert!(matches!(
        &request.tool_choice,
        ToolChoice::Named { name } if name == "lookup"
    ));
    assert_eq!(request.sampling.temperature(), Some(1.5));
    assert_eq!(request.sampling.top_p(), Some(0.9));
    assert_eq!(request.sampling.max_output_tokens().unwrap().get(), 1024);
    assert_eq!(request.sampling.stop_sequences(), ["END"]);
    let reasoning = request.reasoning.unwrap();
    assert_eq!(reasoning.effort(), Some(ReasoningEffort::High));
    assert!(reasoning.include_thinking());
}

#[test]
fn preserves_function_error_object_without_stringifying_it() {
    let body = json!({
        "contents": [
            {
                "role": "model",
                "parts": [{
                    "functionCall": {"id": "call-1", "name": "lookup", "args": {}}
                }]
            },
            {
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "id": "call-1",
                        "name": "lookup",
                        "response": {"error": {"code": 7, "retryable": false}}
                    }
                }]
            }
        ]
    });

    let request = parse(body).unwrap();
    assert!(matches!(
        &request.messages[1].content[0],
        ContentBlock::ToolResult {
            content,
            structured_content: Some(value),
            is_error: true,
            ..
        } if content.is_empty()
            && value == &json!({"error": {"code": 7, "retryable": false}})
    ));
}

#[test]
fn rejects_missing_function_ids_instead_of_inventing_them() {
    let call_without_id = json!({
        "contents": [{
            "role": "model",
            "parts": [{"functionCall": {"name": "lookup", "args": {}}}]
        }]
    });
    assert_eq!(
        parse(call_without_id),
        Err(ParseRequestError::UnsupportedFeature)
    );

    let response_without_id = json!({
        "contents": [
            {
                "role": "model",
                "parts": [{
                    "functionCall": {"id": "call-1", "name": "lookup", "args": {}}
                }]
            },
            {
                "role": "user",
                "parts": [{
                    "functionResponse": {"name": "lookup", "response": {"ok": true}}
                }]
            }
        ]
    });
    assert_eq!(
        parse(response_without_id),
        Err(ParseRequestError::UnsupportedFeature)
    );
}

#[test]
fn validates_function_call_and_response_association() {
    let mismatched_name = json!({
        "contents": [
            {
                "role": "model",
                "parts": [{
                    "functionCall": {"id": "call-1", "name": "lookup", "args": {}}
                }]
            },
            {
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "id": "call-1",
                        "name": "other",
                        "response": {"ok": true}
                    }
                }]
            }
        ]
    });
    assert_eq!(parse(mismatched_name), Err(ParseRequestError::InvalidValue));

    let unanswered_call = json!({
        "contents": [{
            "role": "model",
            "parts": [{
                "functionCall": {"id": "call-1", "name": "lookup", "args": {}}
            }]
        }]
    });
    assert_eq!(parse(unanswered_call), Err(ParseRequestError::InvalidValue));

    let duplicate_call = json!({
        "contents": [
            {
                "role": "model",
                "parts": [
                    {"functionCall": {"id": "call-1", "name": "lookup", "args": {}}},
                    {"functionCall": {"id": "call-1", "name": "lookup", "args": {}}}
                ]
            },
            {
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "id": "call-1",
                        "name": "lookup",
                        "response": {"ok": true}
                    }
                }]
            }
        ]
    });
    assert_eq!(parse(duplicate_call), Err(ParseRequestError::InvalidValue));
}

#[test]
fn rejects_duplicate_keys_at_any_depth() {
    let duplicate = br#"{
        "contents": [{
            "role": "model",
            "parts": [{
                "functionCall": {
                    "id": "call-1",
                    "name": "lookup",
                    "args": {"query": "a", "query": "b"}
                }
            }]
        }]
    }"#;
    assert_eq!(
        parse_request("models/gemini-2.5-pro", duplicate),
        Err(ParseRequestError::DuplicateKey)
    );
}

#[test]
fn validates_model_resource_and_rejects_body_model() {
    let body = serde_json::to_vec(&minimal_body()).unwrap();
    for invalid in [
        "gemini-2.5-pro",
        "models/",
        "models/a/b",
        "models/gemini\nsecret",
    ] {
        assert_eq!(
            parse_request(invalid, &body),
            Err(ParseRequestError::InvalidValue)
        );
    }

    let mut body_model = minimal_body();
    body_model["model"] = json!("models/other");
    assert_eq!(parse(body_model), Err(ParseRequestError::InvalidValue));
}

#[test]
fn distinguishes_known_unsupported_and_unknown_fields() {
    let mut safety = minimal_body();
    safety["safetySettings"] = json!([]);
    assert_eq!(parse(safety), Err(ParseRequestError::UnsupportedFeature));

    let mut top_k = minimal_body();
    top_k["generationConfig"] = json!({"topK": 20});
    assert_eq!(parse(top_k), Err(ParseRequestError::UnsupportedFeature));

    let file_data = json!({
        "contents": [{
            "parts": [{
                "fileData": {
                    "mimeType": "application/pdf",
                    "fileUri": "https://example.test/file.pdf"
                }
            }]
        }]
    });
    assert_eq!(parse(file_data), Err(ParseRequestError::UnsupportedFeature));

    let mut unknown = minimal_body();
    unknown["futureOption"] = json!(true);
    assert_eq!(parse(unknown), Err(ParseRequestError::InvalidValue));
}

#[test]
fn validates_roles_part_unions_and_signature_placement() {
    let invalid_role = json!({
        "contents": [{"role": "assistant", "parts": [{"text": "hello"}]}]
    });
    assert_eq!(parse(invalid_role), Err(ParseRequestError::InvalidValue));

    let multiple_payloads = json!({
        "contents": [{
            "parts": [{
                "text": "hello",
                "inlineData": {"mimeType": "image/png", "data": "aGk="}
            }]
        }]
    });
    assert_eq!(
        parse(multiple_payloads),
        Err(ParseRequestError::InvalidValue)
    );

    let user_thought = json!({
        "contents": [{"role": "user", "parts": [{"text": "secret", "thought": true}]}]
    });
    assert_eq!(parse(user_thought), Err(ParseRequestError::InvalidValue));

    let signature_on_plain_text = json!({
        "contents": [{
            "role": "model",
            "parts": [{"text": "hello", "thoughtSignature": "c2ln"}]
        }]
    });
    assert_eq!(
        parse(signature_on_plain_text),
        Err(ParseRequestError::UnsupportedFeature)
    );

    let system_media = json!({
        "systemInstruction": {
            "parts": [{"inlineData": {"mimeType": "image/png", "data": "aGk="}}]
        },
        "contents": [{"parts": [{"text": "hello"}]}]
    });
    assert_eq!(
        parse(system_media),
        Err(ParseRequestError::UnsupportedFeature)
    );
}

#[test]
fn validates_tool_declarations_and_choice_modes() {
    let mut both_schemas = minimal_body();
    both_schemas["tools"] = json!([{
        "functionDeclarations": [{
            "name": "lookup",
            "parameters": {"type": "object"},
            "parametersJsonSchema": {"type": "object"}
        }]
    }]);
    assert_eq!(parse(both_schemas), Err(ParseRequestError::InvalidValue));

    let mut validated = minimal_body();
    validated["tools"] = json!([{
        "functionDeclarations": [{"name": "lookup"}]
    }]);
    validated["toolConfig"] = json!({
        "functionCallingConfig": {"mode": "VALIDATED"}
    });
    assert_eq!(parse(validated), Err(ParseRequestError::UnsupportedFeature));

    let mut subset = minimal_body();
    subset["tools"] = json!([{
        "functionDeclarations": [
            {"name": "one"},
            {"name": "two"},
            {"name": "three"}
        ]
    }]);
    subset["toolConfig"] = json!({
        "functionCallingConfig": {
            "mode": "ANY",
            "allowedFunctionNames": ["one", "two"]
        }
    });
    assert_eq!(parse(subset), Err(ParseRequestError::UnsupportedFeature));

    let mut unknown_name = minimal_body();
    unknown_name["tools"] = json!([{
        "functionDeclarations": [{"name": "lookup"}]
    }]);
    unknown_name["toolConfig"] = json!({
        "functionCallingConfig": {
            "mode": "ANY",
            "allowedFunctionNames": ["missing"]
        }
    });
    assert_eq!(parse(unknown_name), Err(ParseRequestError::InvalidValue));
}

#[test]
fn validates_sampling_and_thinking_boundaries() {
    let mut candidates = minimal_body();
    candidates["generationConfig"] = json!({"candidateCount": 2});
    assert_eq!(
        parse(candidates),
        Err(ParseRequestError::UnsupportedFeature)
    );

    let mut temperature = minimal_body();
    temperature["generationConfig"] = json!({"temperature": 2.1});
    assert_eq!(parse(temperature), Err(ParseRequestError::InvalidValue));

    let mut stops = minimal_body();
    stops["generationConfig"] = json!({
        "stopSequences": ["1", "2", "3", "4", "5", "6"]
    });
    assert_eq!(parse(stops), Err(ParseRequestError::StructureLimitExceeded));

    let mut conflicting = minimal_body();
    conflicting["generationConfig"] = json!({
        "thinkingConfig": {"thinkingBudget": 128, "thinkingLevel": "LOW"}
    });
    assert_eq!(parse(conflicting), Err(ParseRequestError::InvalidValue));

    let mut disabled_with_output = minimal_body();
    disabled_with_output["generationConfig"] = json!({
        "thinkingConfig": {"thinkingBudget": 0, "includeThoughts": true}
    });
    assert_eq!(
        parse(disabled_with_output),
        Err(ParseRequestError::InvalidValue)
    );

    let mut dynamic = minimal_body();
    dynamic["generationConfig"] = json!({
        "thinkingConfig": {"thinkingBudget": -1, "includeThoughts": true}
    });
    let dynamic = parse(dynamic).unwrap().reasoning.unwrap();
    assert_eq!(dynamic.effort(), None);
    assert_eq!(dynamic.budget_tokens(), None);
    assert!(dynamic.include_thinking());
}

#[test]
fn validates_inline_media_and_signature_encoding() {
    let audio = json!({
        "contents": [{
            "parts": [{"inlineData": {"mimeType": "audio/wav", "data": "aGk="}}]
        }]
    });
    let request = parse(audio).unwrap();
    assert!(matches!(
        &request.messages[0].content[0],
        ContentBlock::Audio {
            source: MediaSource::Base64(data),
            mime_type,
        } if data == "aGk=" && mime_type == "audio/wav"
    ));

    let invalid_base64 = json!({
        "contents": [{
            "parts": [{"inlineData": {"mimeType": "image/png", "data": "***"}}]
        }]
    });
    assert_eq!(parse(invalid_base64), Err(ParseRequestError::InvalidValue));

    let video = json!({
        "contents": [{
            "parts": [{"inlineData": {"mimeType": "video/mp4", "data": "aGk="}}]
        }]
    });
    assert_eq!(parse(video), Err(ParseRequestError::UnsupportedFeature));

    let invalid_signature = json!({
        "contents": [{
            "role": "model",
            "parts": [{"text": "thinking", "thought": true, "thoughtSignature": "***"}]
        }]
    });
    assert_eq!(
        parse(invalid_signature),
        Err(ParseRequestError::InvalidValue)
    );
}

#[test]
fn rejects_nulls_structure_overflow_and_oversized_body() {
    let null_config = json!({
        "contents": [{"parts": [{"text": "hello"}]}],
        "generationConfig": null
    });
    assert_eq!(parse(null_config), Err(ParseRequestError::InvalidValue));

    let empty_contents = json!({"contents": []});
    assert_eq!(
        parse(empty_contents),
        Err(ParseRequestError::StructureLimitExceeded)
    );

    let mut deep = String::from("{\"contents\":[{\"parts\":[{\"text\":\"");
    deep.push_str(&"a".repeat(8));
    deep.push_str("\"}]}],\"future\":");
    deep.push_str(&"[".repeat(40));
    deep.push_str("null");
    deep.push_str(&"]".repeat(40));
    deep.push('}');
    assert_eq!(
        parse_request("models/gemini-2.5-pro", deep.as_bytes()),
        Err(ParseRequestError::StructureLimitExceeded)
    );

    let oversized = vec![b' '; MAX_BODY_BYTES + 1];
    assert_eq!(
        parse_request("models/gemini-2.5-pro", &oversized),
        Err(ParseRequestError::BodyTooLarge)
    );
}

#[test]
fn debug_and_errors_do_not_expose_request_values() {
    let model_canary = "model-secret-canary";
    let text_canary = "text-secret-canary";
    let input_canary = "input-secret-canary";
    let result_canary = "result-secret-canary";
    let signature = STANDARD.encode("signature-secret-canary");
    let body = json!({
        "contents": [
            {"role": "user", "parts": [{"text": text_canary}]},
            {
                "role": "model",
                "parts": [{
                    "functionCall": {
                        "id": "call-secret-canary",
                        "name": "lookup",
                        "args": {"value": input_canary}
                    },
                    "thoughtSignature": signature
                }]
            },
            {
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "id": "call-secret-canary",
                        "name": "lookup",
                        "response": {"result": result_canary}
                    }
                }]
            }
        ]
    });
    let request = parse_request(
        &format!("models/{model_canary}"),
        &serde_json::to_vec(&body).unwrap(),
    )
    .unwrap();
    let debug = format!("{request:?}");
    for canary in [
        model_canary,
        text_canary,
        input_canary,
        result_canary,
        "call-secret-canary",
        signature.as_str(),
    ] {
        assert!(!debug.contains(canary));
    }

    let error = ParseRequestError::InvalidValue;
    assert_eq!(error.to_string(), "请求字段值无效");
    assert!(error.source().is_none());
}
