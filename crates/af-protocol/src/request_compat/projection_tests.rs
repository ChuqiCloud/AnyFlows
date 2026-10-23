use af_domain::Protocol;
use bytes::Bytes;
use serde_json::{Value, json};

use crate::{CanonicalRequestEnvelope, Sampling, TokenCount, ToolChoice, gemini};

fn envelope(body: &Value, stream: bool) -> CanonicalRequestEnvelope {
    let bytes = Bytes::from(serde_json::to_vec(body).unwrap());
    if stream {
        gemini::parse_stream_request_envelope("models/client-model", bytes).unwrap()
    } else {
        gemini::parse_request_envelope("models/client-model", bytes).unwrap()
    }
}

fn assert_native_only(request: CanonicalRequestEnvelope, original: &Value) {
    for target in [
        Protocol::OpenAiChat,
        Protocol::OpenAiResponses,
        Protocol::Anthropic,
    ] {
        assert!(
            request.clone().plan_conversation(target).is_none(),
            "{target:?}"
        );
    }
    assert_native_preserved(request, original);
}

fn assert_native_preserved(request: CanonicalRequestEnvelope, original: &Value) {
    let routed = request_with_route(request.clone());
    let plan = request.clone().plan_conversation(Protocol::Gemini).unwrap();
    let native: Value = serde_json::from_slice(plan.body()).unwrap();
    assert_eq!(&native, original);
    let (request, body) = plan.into_parts();
    assert_eq!(
        request.plan_conversation(Protocol::Gemini).unwrap().body(),
        &body
    );
    let plan = routed.plan_conversation(Protocol::Gemini).unwrap();
    let routed: Value = serde_json::from_slice(plan.body()).unwrap();
    assert_eq!(routed["contents"], original["contents"]);
    assert_eq!(routed["tools"], original["tools"]);
    assert_eq!(routed["toolConfig"], original["toolConfig"]);
    assert_eq!(routed["x_vendor"], original["x_vendor"]);
}

fn request_with_route(request: CanonicalRequestEnvelope) -> CanonicalRequestEnvelope {
    request.with_routed_parameters(
        "upstream-model".to_owned(),
        Sampling::new(
            Some(0.25),
            None,
            Some(TokenCount::new(128).unwrap()),
            vec![],
        )
        .unwrap(),
    )
}

#[test]
fn validated_function_mode_preserves_native_body_and_converts_the_allowed_tools() {
    assert_tool_config_projection("VALIDATED", ToolChoice::Auto);
}

#[test]
fn required_function_mode_preserves_native_body_and_converts_the_allowed_tools() {
    assert_tool_config_projection("ANY", ToolChoice::Required);
}

#[test]
fn auto_function_mode_preserves_native_body_and_converts_the_allowed_tools() {
    assert_tool_config_projection("AUTO", ToolChoice::Auto);
}

fn assert_tool_config_projection(mode: &str, choice: ToolChoice) {
    for stream in [false, true] {
        for allowed in [
            None,
            Some(json!([])),
            Some(json!(["lookup"])),
            Some(json!(["lookup", "other"])),
        ] {
            let mut body = json!({
                "contents":[{"role":"user","parts":[{"text":"hello"}]}],
                "tools":[
                    {"functionDeclarations":[{"name":"lookup","parameters":{"type":"object"}}]},
                    {"functionDeclarations":[{"name":"other","parameters":{"type":"object"}}]},
                    {"functionDeclarations":[{"name":"unused","parameters":{"type":"object"}}]}
                ],
                "toolConfig":{"functionCallingConfig":{"mode":mode,"x_option":true}},
                "x_vendor":{"keep":[1,2]}
            });
            let expected = match allowed.as_ref().and_then(Value::as_array).map(Vec::len) {
                Some(1) => vec!["lookup"],
                Some(2) => vec!["lookup", "other"],
                _ => vec!["lookup", "other", "unused"],
            };
            if let Some(allowed) = allowed {
                body["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"] = allowed;
            }
            let request = envelope(&body, stream);
            assert_eq!(request.canonical().stream, stream);
            assert_eq!(request.canonical().tool_choice, choice);
            assert_eq!(
                request
                    .canonical()
                    .tools
                    .iter()
                    .map(|tool| tool.name.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            for target in [
                Protocol::OpenAiChat,
                Protocol::OpenAiResponses,
                Protocol::Anthropic,
                Protocol::Gemini,
            ] {
                let plan = request.clone().plan_conversation(target).unwrap();
                assert_eq!(plan.request().canonical().stream, stream);
                assert_eq!(plan.request().canonical().tool_choice, choice);
                if target != Protocol::Gemini {
                    let converted: Value = serde_json::from_slice(plan.body()).unwrap();
                    assert_eq!(
                        converted["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|tool| {
                                if target == Protocol::OpenAiChat {
                                    tool["function"]["name"].as_str().unwrap()
                                } else {
                                    tool["name"].as_str().unwrap()
                                }
                            })
                            .collect::<Vec<_>>(),
                        expected,
                        "{target:?}"
                    );
                }
            }
            assert_native_preserved(request, &body);
        }
    }
}

#[test]
fn inline_pdf_and_video_preserve_native_payload_in_both_response_modes() {
    for stream in [false, true] {
        for (mime, data) in [("application/pdf", "JVBERi0="), ("video/mp4", "AAAAAA==")] {
            let body = json!({
                "contents":[{"role":"user","parts":[
                    {"text":"Inspect the attachment"},
                    {"inlineData":{"mimeType":mime,"data":data,"x_blob":{"keep":true}},"x_part":7}
                ]}],
                "x_vendor":{"keep":[1,2]}
            });
            let request = envelope(&body, stream);
            assert_eq!(request.canonical().stream, stream);
            let visible = format!("{:?}", request.canonical());
            assert!(!visible.contains(data));
            assert_native_only(request, &body);
        }
    }
}

#[test]
fn modeled_inline_image_remains_convertible() {
    let body = json!({
        "contents":[{"role":"user","parts":[
            {"inlineData":{"mimeType":"image/png","data":"AAAAAA=="}}
        ]}]
    });
    assert!(
        envelope(&body, false)
            .plan_conversation(Protocol::OpenAiChat)
            .is_some()
    );
}
