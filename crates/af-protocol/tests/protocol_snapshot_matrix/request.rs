use af_protocol::{CanonicalRequest, anthropic, gemini, openai_chat, openai_responses};
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
    insta::assert_snapshot!("request_4x4", report.as_str(), @r"
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
