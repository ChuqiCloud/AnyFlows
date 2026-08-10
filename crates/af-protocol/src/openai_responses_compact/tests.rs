use af_domain::Operation;
use serde_json::json;

use super::*;

#[test]
fn parses_and_rebuilds_ordered_context_without_leaking_secrets() {
    let body = json!({
        "model": "gpt-private-compact",
        "instructions": "private instructions",
        "input": [
            {
                "type": "message",
                "id": "msg_1",
                "role": "user",
                "content": [{"type":"input_text","text":"private user text"}]
            },
            {
                "type": "compaction",
                "id": "cmp_1",
                "encrypted_content": "encrypted-private-canary"
            },
            {
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call_1",
                "name": "lookup",
                "arguments": "{\"city\":\"private\"}",
                "status": "completed"
            },
            {
                "type": "function_call_output",
                "id": "fco_1",
                "call_id": "call_1",
                "output": "private tool output",
                "status": "completed"
            }
        ]
    });
    let request = parse_request(&serde_json::to_vec(&body).unwrap()).unwrap();

    assert_eq!(request.operation(), Operation::ResponsesCompact);
    assert_eq!(request.model(), "gpt-private-compact");
    assert_eq!(request.instructions(), Some("private instructions"));
    let items = request.input().unwrap().items().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].as_value()["type"], "message");
    assert_eq!(items[1].as_value()["type"], "compaction");
    assert_eq!(items[2].as_value()["type"], "function_call");
    assert_eq!(items[3].as_value()["type"], "function_call_output");

    let debug = format!("{request:?}");
    for canary in [
        "gpt-private-compact",
        "private instructions",
        "private user text",
        "encrypted-private-canary",
        "private tool output",
    ] {
        assert!(!debug.contains(canary));
    }

    let rebuilt = build_request(&request).unwrap();
    assert_eq!(rebuilt, body);
    assert_eq!(
        parse_request(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        request
    );
}

#[test]
fn accepts_previous_response_reference_without_input() {
    let request =
        parse_request(br#"{"model":"gpt-5.6","previous_response_id":"resp_private_1"}"#).unwrap();

    assert_eq!(request.input(), None);
    assert_eq!(request.previous_response_id(), Some("resp_private_1"));
    assert!(!format!("{request:?}").contains("resp_private_1"));
}

#[test]
fn parses_and_rebuilds_response_compaction_window_and_full_usage() {
    let body = json!({
        "id": "resp_private_compaction",
        "created_at": 1764967971,
        "object": "response.compaction",
        "output": [
            {
                "id": "msg_000",
                "type": "message",
                "status": "completed",
                "content": [{"type":"input_text","text":"private retained text"}],
                "role": "user"
            },
            {
                "id": "cmp_001",
                "type": "compaction",
                "encrypted_content": "encrypted-private-summary"
            }
        ],
        "usage": {
            "input_tokens": 139,
            "input_tokens_details": {
                "cached_tokens": 20,
                "cache_write_tokens": 30
            },
            "output_tokens": 438,
            "output_tokens_details": {
                "reasoning_tokens": 64
            },
            "total_tokens": 577
        }
    });
    let response = parse_response(&serde_json::to_vec(&body).unwrap()).unwrap();

    assert_eq!(response.operation(), Operation::ResponsesCompact);
    assert_eq!(response.id(), "resp_private_compaction");
    assert_eq!(response.output().len(), 2);
    assert_eq!(response.usage().input_tokens().get(), 139);
    assert_eq!(response.usage().cached_tokens().get(), 20);
    assert_eq!(response.usage().cache_write_tokens().get(), 30);
    assert_eq!(response.usage().output_tokens().get(), 438);
    assert_eq!(response.usage().reasoning_tokens().get(), 64);
    assert_eq!(response.usage().total_tokens().get(), 577);

    let debug = format!("{response:?}");
    for canary in [
        "resp_private_compaction",
        "private retained text",
        "encrypted-private-summary",
    ] {
        assert!(!debug.contains(canary));
    }
    assert_eq!(build_response(&response).unwrap(), body);
}

#[test]
fn rejects_unknown_duplicate_null_and_missing_context_request_fields() {
    assert_eq!(
        parse_request(br#"{"model":"first","model":"second","input":"x"}"#),
        Err(ParseResponsesCompactionRequestError::DuplicateKey)
    );
    for body in [
        br#"{"model":"m"}"#.as_slice(),
        br#"{"model":"m","input":null}"#.as_slice(),
        br#"{"model":"m","input":"x","unknown":true}"#.as_slice(),
        br#"{"model":"m","input":[{"type":"future_item"}]}"#.as_slice(),
        br#"{"model":"m","input":[{"type":"compaction","encrypted_content":""}]}"#.as_slice(),
    ] {
        assert!(
            parse_request(body).is_err(),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
    assert_eq!(
        parse_request(br#"{"model":"m","input":[{"type":"future_item"}]}"#),
        Err(ParseResponsesCompactionRequestError::UnsupportedFeature)
    );
}

#[test]
fn response_requires_compaction_item_and_consistent_usage() {
    let base = json!({
        "id":"resp_1",
        "created_at":1,
        "object":"response.compaction",
        "output":[{"type":"compaction","encrypted_content":"encrypted"}],
        "usage":{
            "input_tokens":10,
            "input_tokens_details":{"cached_tokens":2,"cache_write_tokens":3},
            "output_tokens":5,
            "output_tokens_details":{"reasoning_tokens":4},
            "total_tokens":15
        }
    });
    assert!(parse_response(&serde_json::to_vec(&base).unwrap()).is_ok());

    let mut no_compaction = base.clone();
    no_compaction["output"] = json!([{
        "type":"message",
        "role":"user",
        "content":"retained"
    }]);
    assert_eq!(
        parse_response(&serde_json::to_vec(&no_compaction).unwrap()),
        Err(ParseResponsesCompactionResponseError::InvalidValue)
    );

    for (pointer, value) in [
        ("/usage/total_tokens", json!(14)),
        ("/usage/input_tokens_details/cached_tokens", json!(8)),
        ("/usage/output_tokens_details/reasoning_tokens", json!(6)),
    ] {
        let mut invalid = base.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            parse_response(&serde_json::to_vec(&invalid).unwrap()),
            Err(ParseResponsesCompactionResponseError::InvalidUsage),
            "pointer={pointer}"
        );
    }
}

#[test]
fn usage_constructor_checks_negative_values_and_overflow() {
    assert_eq!(
        ResponsesCompactionUsage::new(-1, 0, 0, 0, 0, 0),
        Err(ResponsesCompactionUsageError::NegativeTokenCount)
    );
    assert_eq!(
        ResponsesCompactionUsage::new(i64::MAX, 0, 0, 1, 0, i64::MAX),
        Err(ResponsesCompactionUsageError::Overflow)
    );
}
