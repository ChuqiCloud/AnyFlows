use af_domain::Protocol;
use af_protocol::{
    CanonicalRequest, CanonicalRequestEnvelope, SameProtocolDecision, anthropic, gemini,
    openai_chat, openai_responses,
};
use bytes::Bytes;
use serde_json::Value;

use super::fixtures::{MODEL, ProtocolKind, request};

#[test]
fn snapshots_request_conversion_matrix() {
    let mut rows = Vec::new();
    for source in ProtocolKind::ALL {
        let canonical = parse_source(source);
        for target in ProtocolKind::ALL {
            let outcome = match build_target(target, &canonical) {
                Ok(value) => {
                    let round_trip = parse_target(target, &value);
                    assert_eq!(round_trip, canonical);
                    format!("ok [{}]", top_level_keys(&value))
                }
                Err(error) => format!("error {error}"),
            };
            rows.push(format!("{} -> {}: {outcome}", source.name(), target.name()));
        }
    }

    let report = rows.join("\n");
    insta::assert_snapshot!(report.as_str(), @r"
    openai_chat -> openai_chat: ok [max_completion_tokens,messages,model,stream,tool_choice]
    openai_chat -> anthropic: ok [max_tokens,messages,model]
    openai_chat -> gemini: ok [contents,generationConfig,toolConfig]
    openai_chat -> openai_responses: error UnsupportedOperation
    anthropic -> openai_chat: ok [max_completion_tokens,messages,model,stream,tool_choice]
    anthropic -> anthropic: ok [max_tokens,messages,model]
    anthropic -> gemini: ok [contents,generationConfig,toolConfig]
    anthropic -> openai_responses: error UnsupportedOperation
    gemini -> openai_chat: ok [max_completion_tokens,messages,model,stream,tool_choice]
    gemini -> anthropic: ok [max_tokens,messages,model]
    gemini -> gemini: ok [contents,generationConfig,toolConfig]
    gemini -> openai_responses: error UnsupportedOperation
    openai_responses -> openai_chat: error UnsupportedOperation
    openai_responses -> anthropic: error UnsupportedOperation
    openai_responses -> gemini: error UnsupportedOperation
    openai_responses -> openai_responses: ok [input,max_output_tokens,model,stream,tool_choice]
    ");
}

#[test]
fn only_same_protocol_can_consume_the_validated_source_body() {
    for source in ProtocolKind::ALL {
        let original =
            Bytes::from(serde_json::to_vec_pretty(&request(source)).expect("请求样本必须可序列化"));
        let envelope = parse_source_envelope(source, original.clone());
        assert_eq!(envelope.source_protocol(), Some(domain_protocol(source)));

        for target in ProtocolKind::ALL {
            let decision = envelope.clone().into_same_protocol(domain_protocol(target));
            if source == target {
                let SameProtocolDecision::Passthrough(passthrough) = decision else {
                    panic!("同协议请求必须具备直通资格");
                };
                assert_eq!(passthrough.protocol(), domain_protocol(source));
                assert_eq!(passthrough.body(), &original);
                assert_eq!(passthrough.canonical(), envelope.canonical());
            } else {
                let SameProtocolDecision::Rebuild(rejected) = decision else {
                    panic!("跨协议请求不得取得原始正文");
                };
                assert_eq!(rejected.source_protocol(), Some(domain_protocol(source)));
            }
        }
    }
}

#[test]
fn canonical_only_request_and_debug_output_do_not_expose_source_body() {
    let original = Bytes::from_static(
        br#"{"model":"matrix-model","messages":[{"role":"user","content":"body-secret"}]}"#,
    );
    let parsed = openai_chat::parse_request_envelope(original).unwrap();
    let debug = format!("{parsed:?}");
    assert!(debug.contains("source_body_bytes"));
    assert!(!debug.contains("body-secret"));

    let canonical_only = CanonicalRequestEnvelope::from_canonical(parsed.into_canonical());
    assert_eq!(canonical_only.source_protocol(), None);
    assert!(matches!(
        canonical_only.into_same_protocol(Protocol::OpenAiChat),
        SameProtocolDecision::Rebuild(_)
    ));
}

fn parse_source(protocol: ProtocolKind) -> CanonicalRequest {
    let body = serde_json::to_vec(&request(protocol)).expect("请求样本必须可序列化");
    match protocol {
        ProtocolKind::OpenAiChat => {
            openai_chat::parse_request(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Anthropic => {
            anthropic::parse_request(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Gemini => gemini::parse_request(&format!("models/{MODEL}"), &body)
            .map_err(|error| error.to_string()),
        ProtocolKind::OpenAiResponses => {
            openai_responses::parse_request(&body).map_err(|error| error.to_string())
        }
    }
    .unwrap_or_else(|error| panic!("{} 请求样本解析失败：{error}", protocol.name()))
}

fn parse_source_envelope(protocol: ProtocolKind, body: Bytes) -> CanonicalRequestEnvelope {
    match protocol {
        ProtocolKind::OpenAiChat => {
            openai_chat::parse_request_envelope(body).map_err(|error| error.to_string())
        }
        ProtocolKind::Anthropic => {
            anthropic::parse_request_envelope(body).map_err(|error| error.to_string())
        }
        ProtocolKind::Gemini => gemini::parse_request_envelope(&format!("models/{MODEL}"), body)
            .map_err(|error| error.to_string()),
        ProtocolKind::OpenAiResponses => {
            openai_responses::parse_request_envelope(body).map_err(|error| error.to_string())
        }
    }
    .unwrap_or_else(|error| panic!("{} 请求信封解析失败：{error}", protocol.name()))
}

const fn domain_protocol(protocol: ProtocolKind) -> Protocol {
    match protocol {
        ProtocolKind::OpenAiChat => Protocol::OpenAiChat,
        ProtocolKind::Anthropic => Protocol::Anthropic,
        ProtocolKind::Gemini => Protocol::Gemini,
        ProtocolKind::OpenAiResponses => Protocol::OpenAiResponses,
    }
}

fn build_target(protocol: ProtocolKind, request: &CanonicalRequest) -> Result<Value, String> {
    match protocol {
        ProtocolKind::OpenAiChat => openai_chat::build_request(request).map_err(debug_error),
        ProtocolKind::Anthropic => anthropic::build_request(request).map_err(debug_error),
        ProtocolKind::Gemini => gemini::build_request(request).map_err(debug_error),
        ProtocolKind::OpenAiResponses => {
            openai_responses::build_request(request).map_err(debug_error)
        }
    }
}

fn parse_target(protocol: ProtocolKind, value: &Value) -> CanonicalRequest {
    let body = serde_json::to_vec(value).expect("目标请求必须可序列化");
    match protocol {
        ProtocolKind::OpenAiChat => {
            openai_chat::parse_request(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Anthropic => {
            anthropic::parse_request(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Gemini => gemini::parse_request(&format!("models/{MODEL}"), &body)
            .map_err(|error| error.to_string()),
        ProtocolKind::OpenAiResponses => {
            openai_responses::parse_request(&body).map_err(|error| error.to_string())
        }
    }
    .unwrap_or_else(|error| panic!("{} 目标请求回环失败：{error}", protocol.name()))
}

fn top_level_keys(value: &Value) -> String {
    value
        .as_object()
        .expect("协议构造器必须返回对象")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
