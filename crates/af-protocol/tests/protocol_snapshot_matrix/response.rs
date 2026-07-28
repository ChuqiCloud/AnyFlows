use af_protocol::{CanonicalResponse, Usage, anthropic, gemini, openai_chat, openai_responses};
use serde_json::Value;

use super::fixtures::{ProtocolKind, response};

#[test]
fn snapshots_response_conversion_matrix() {
    let mut rows = Vec::new();
    for source in ProtocolKind::ALL {
        let canonical = parse_source(source);
        for target in ProtocolKind::ALL {
            let outcome = match build_target(target, &canonical) {
                Ok(value) => {
                    let round_trip = parse_target(target, &value);
                    assert_response_semantics(&canonical, &round_trip);
                    format!("ok [{}]", top_level_keys(&value))
                }
                Err(error) => format!("error {error}"),
            };
            rows.push(format!("{} -> {}: {outcome}", source.name(), target.name()));
        }
    }

    let report = rows.join("\n");
    insta::assert_snapshot!(report.as_str(), @r"
    openai_chat -> openai_chat: ok [choices,created,id,model,object,usage]
    openai_chat -> anthropic: ok [container,content,id,model,role,stop_details,stop_reason,stop_sequence,type,usage]
    openai_chat -> gemini: ok [candidates,modelVersion,responseId,usageMetadata]
    openai_chat -> openai_responses: error UnsupportedOperation
    anthropic -> openai_chat: error InvalidValue
    anthropic -> anthropic: ok [container,content,id,model,role,stop_details,stop_reason,stop_sequence,type,usage]
    anthropic -> gemini: ok [candidates,modelVersion,responseId,usageMetadata]
    anthropic -> openai_responses: error UnsupportedOperation
    gemini -> openai_chat: error InvalidValue
    gemini -> anthropic: ok [container,content,id,model,role,stop_details,stop_reason,stop_sequence,type,usage]
    gemini -> gemini: ok [candidates,modelVersion,responseId,usageMetadata]
    gemini -> openai_responses: error UnsupportedOperation
    openai_responses -> openai_chat: error UnsupportedOperation
    openai_responses -> anthropic: error UnsupportedOperation
    openai_responses -> gemini: error UnsupportedOperation
    openai_responses -> openai_responses: ok [created_at,error,id,incomplete_details,model,object,output,status,usage]
    ");
}

fn parse_source(protocol: ProtocolKind) -> CanonicalResponse {
    let body = serde_json::to_vec(&response(protocol)).expect("响应样本必须可序列化");
    match protocol {
        ProtocolKind::OpenAiChat => {
            openai_chat::parse_response(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Anthropic => {
            anthropic::parse_response(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Gemini => gemini::parse_response(&body).map_err(|error| error.to_string()),
        ProtocolKind::OpenAiResponses => {
            openai_responses::parse_response(&body).map_err(|error| error.to_string())
        }
    }
    .unwrap_or_else(|error| panic!("{} 响应样本解析失败：{error}", protocol.name()))
}

fn build_target(protocol: ProtocolKind, response: &CanonicalResponse) -> Result<Value, String> {
    match protocol {
        ProtocolKind::OpenAiChat => openai_chat::build_response(response).map_err(debug_error),
        ProtocolKind::Anthropic => anthropic::build_response(response).map_err(debug_error),
        ProtocolKind::Gemini => gemini::build_response(response).map_err(debug_error),
        ProtocolKind::OpenAiResponses => {
            openai_responses::build_response(response).map_err(debug_error)
        }
    }
}

fn parse_target(protocol: ProtocolKind, value: &Value) -> CanonicalResponse {
    let body = serde_json::to_vec(value).expect("目标响应必须可序列化");
    match protocol {
        ProtocolKind::OpenAiChat => {
            openai_chat::parse_response(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Anthropic => {
            anthropic::parse_response(&body).map_err(|error| error.to_string())
        }
        ProtocolKind::Gemini => gemini::parse_response(&body).map_err(|error| error.to_string()),
        ProtocolKind::OpenAiResponses => {
            openai_responses::parse_response(&body).map_err(|error| error.to_string())
        }
    }
    .unwrap_or_else(|error| panic!("{} 目标响应回环失败：{error}", protocol.name()))
}

fn assert_response_semantics(expected: &CanonicalResponse, actual: &CanonicalResponse) {
    assert_eq!(actual.operation, expected.operation);
    assert_eq!(actual.id, expected.id);
    assert_eq!(actual.model, expected.model);
    assert_eq!(actual.choices, expected.choices);
    assert_usage_semantics(expected.usage.as_ref(), actual.usage.as_ref());
}

fn assert_usage_semantics(expected: Option<&Usage>, actual: Option<&Usage>) {
    match (expected, actual) {
        (Some(expected), Some(actual)) => {
            assert_eq!(
                actual.checked_input_tokens().unwrap(),
                expected.checked_input_tokens().unwrap()
            );
            assert_eq!(actual.output_tokens(), expected.output_tokens());
            assert_eq!(actual.details(), expected.details());
        }
        (None, None) => {}
        _ => panic!("响应 usage 缺失状态发生变化"),
    }
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
