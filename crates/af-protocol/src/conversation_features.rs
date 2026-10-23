//! Render shared optional features from the retained source context.

use af_domain::Protocol;
use bytes::Bytes;
use serde_json::{Value, json};

use crate::{CanonicalRequestEnvelope, ConversationConversionError};

pub(crate) fn apply(
    request: &CanonicalRequestEnvelope,
    target: Protocol,
    body: Bytes,
) -> Result<Bytes, ConversationConversionError> {
    let Some((source, context)) = request.source_context() else {
        return Ok(body);
    };
    let context: Value =
        serde_json::from_slice(context).map_err(|_| ConversationConversionError::InvalidRequest)?;
    let mut value: Value =
        serde_json::from_slice(&body).map_err(|_| ConversationConversionError::InvalidRequest)?;
    let Some(root) = value.as_object_mut() else {
        return Err(ConversationConversionError::InvalidRequest);
    };
    if let Some(format) = output_format(source, &context) {
        match target {
            Protocol::OpenAiChat => {
                root.insert("response_format".to_owned(), if format.schema.is_null() {
                    json!({"type":"json_object"})
                } else {
                    json!({"type":"json_schema","json_schema":{"name":format.name,"schema":format.schema,"strict":format.strict}})
                });
            }
            Protocol::OpenAiResponses => {
                root.insert("text".to_owned(), json!({"format":if format.schema.is_null() {
                    json!({"type":"json_object"})
                } else { json!({"type":"json_schema","name":format.name,"schema":format.schema,"strict":format.strict}) }}));
            }
            Protocol::Anthropic => {
                let config = root
                    .entry("output_config")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or(ConversationConversionError::InvalidRequest)?;
                config.insert("format".to_owned(), json!({"type":"json_schema","schema":if format.schema.is_null() {json!({"type":"object","additionalProperties":true})} else {format.schema}}));
            }
            Protocol::Gemini => {
                let config = root
                    .entry("generationConfig")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or(ConversationConversionError::InvalidRequest)?;
                config.insert("responseMimeType".to_owned(), json!("application/json"));
                if !format.schema.is_null() {
                    config.insert("responseJsonSchema".to_owned(), format.schema);
                }
            }
            _ => return Err(ConversationConversionError::UnsupportedOperation),
        }
    }
    if matches!(source, Protocol::OpenAiChat | Protocol::OpenAiResponses)
        && matches!(target, Protocol::OpenAiChat | Protocol::OpenAiResponses)
        && let Some(parallel) = context
            .get("parallel_tool_calls")
            .filter(|v| v.is_boolean())
    {
        root.insert("parallel_tool_calls".to_owned(), parallel.clone());
    }
    for (chat, gemini) in [
        ("seed", "seed"),
        ("frequency_penalty", "frequencyPenalty"),
        ("presence_penalty", "presencePenalty"),
    ] {
        let shared = if source == Protocol::OpenAiChat {
            context.get(chat)
        } else if source == Protocol::Gemini {
            context.get("generationConfig").and_then(|c| c.get(gemini))
        } else {
            None
        };
        if let Some(number) = shared.filter(|v| v.is_number()) {
            if target == Protocol::OpenAiChat {
                root.insert(chat.to_owned(), number.clone());
            } else if target == Protocol::Gemini {
                let config = root
                    .entry("generationConfig")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or(ConversationConversionError::InvalidRequest)?;
                config.insert(gemini.to_owned(), number.clone());
            }
        }
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| ConversationConversionError::InvalidRequest)
}

struct OutputFormat {
    schema: Value,
    name: String,
    strict: bool,
}

fn output_format(source: Protocol, context: &Value) -> Option<OutputFormat> {
    let format = match source {
        Protocol::OpenAiChat => {
            let format = context.get("response_format")?;
            if format.get("type")?.as_str()? == "json_schema" {
                format.get("json_schema")?
            } else if format.get("type")?.as_str()? == "json_object" {
                format
            } else {
                return None;
            }
        }
        Protocol::OpenAiResponses => {
            let format = context.get("text")?.get("format")?;
            if !matches!(format.get("type")?.as_str()?, "json_schema" | "json_object") {
                return None;
            }
            format
        }
        Protocol::Anthropic => context
            .get("output_config")
            .and_then(|config| config.get("format"))
            .or_else(|| context.get("output_format"))?,
        Protocol::Gemini => {
            let config = context.get("generationConfig")?;
            if config.get("responseMimeType")?.as_str()? != "application/json" {
                return None;
            }
            return Some(OutputFormat {
                schema: config
                    .get("responseJsonSchema")
                    .or_else(|| config.get("responseSchema"))
                    .cloned()
                    .unwrap_or(Value::Null),
                name: "response".to_owned(),
                strict: false,
            });
        }
        _ => return None,
    };
    Some(OutputFormat {
        schema: format.get("schema").cloned().unwrap_or(Value::Null),
        name: format
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("response")
            .to_owned(),
        strict: format
            .get("strict")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anthropic, gemini, openai_chat, openai_responses};

    #[test]
    fn json_schema_formats_cross_all_sixteen_routes() {
        let protocols = [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
            Protocol::Gemini,
        ];
        let schema = json!({"type":"object","properties":{"answer":{"type":"string"}},"required":["answer"],"additionalProperties":false});
        for source in protocols {
            let wire = match source {
                Protocol::OpenAiChat => {
                    json!({"model":"model","messages":[{"role":"user","content":"question"}],"max_completion_tokens":4096,"response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":schema,"strict":true}}})
                }
                Protocol::OpenAiResponses => {
                    json!({"model":"model","input":"question","max_output_tokens":4096,"text":{"format":{"type":"json_schema","name":"answer","schema":schema,"strict":true}}})
                }
                Protocol::Anthropic => {
                    json!({"model":"model","messages":[{"role":"user","content":"question"}],"max_tokens":4096,"output_config":{"format":{"type":"json_schema","schema":schema}}})
                }
                Protocol::Gemini => {
                    json!({"contents":[{"role":"user","parts":[{"text":"question"}]}],"generationConfig":{"maxOutputTokens":4096,"responseMimeType":"application/json","responseJsonSchema":schema}})
                }
                _ => unreachable!(),
            };
            let bytes = Bytes::from(serde_json::to_vec(&wire).unwrap());
            let request = match source {
                Protocol::OpenAiChat => openai_chat::parse_request_envelope(bytes).unwrap(),
                Protocol::OpenAiResponses => {
                    openai_responses::parse_request_envelope(bytes).unwrap()
                }
                Protocol::Anthropic => anthropic::parse_request_envelope(bytes).unwrap(),
                Protocol::Gemini => gemini::parse_request_envelope("models/model", bytes).unwrap(),
                _ => unreachable!(),
            };
            for target in protocols {
                let plan = request
                    .clone()
                    .plan_conversation(target)
                    .unwrap_or_else(|| panic!("{source:?}->{target:?}"));
                let value: Value = serde_json::from_slice(plan.body()).unwrap();
                let actual = match target {
                    Protocol::OpenAiChat => &value["response_format"]["json_schema"]["schema"],
                    Protocol::OpenAiResponses => &value["text"]["format"]["schema"],
                    Protocol::Anthropic => &value["output_config"]["format"]["schema"],
                    Protocol::Gemini => &value["generationConfig"]["responseJsonSchema"],
                    _ => unreachable!(),
                };
                assert_eq!(actual, &schema, "{source:?}->{target:?}");
            }
        }
    }
}
