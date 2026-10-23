use super::*;

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

struct HistoricalResult {
    block: Value,
    source_message: usize,
    // Share the wrapper across parallel results without copying the content
    // array or large vendor metadata once per tool result.
    envelope: Option<std::sync::Arc<Map<String, Value>>>,
}

fn result_content_key(protocol: Protocol) -> Option<&'static str> {
    match protocol {
        Protocol::Anthropic => Some("content"),
        Protocol::Gemini => Some("parts"),
        _ => None,
    }
}

fn result_message(protocol: Protocol, result: &HistoricalResult, blocks: Vec<Value>) -> Value {
    let mut wrapper = result.envelope.as_deref().cloned().unwrap_or_default();
    wrapper.insert("role".to_owned(), json!("user"));
    wrapper.insert(
        result_content_key(protocol)
            .expect("only block protocols have a result wrapper")
            .to_owned(),
        Value::Array(blocks),
    );
    Value::Object(wrapper)
}

fn append_result_messages(
    protocol: Protocol,
    output: &mut Vec<Value>,
    answers: Vec<HistoricalResult>,
) {
    if !matches!(protocol, Protocol::Anthropic | Protocol::Gemini) {
        output.extend(answers.into_iter().map(|answer| answer.block));
        return;
    }
    let key = result_content_key(protocol).expect("block protocol");
    let mut active: Option<(usize, Value)> = None;
    for answer in answers {
        if let Some((source, message)) = &mut active
            && *source == answer.source_message
        {
            message[key]
                .as_array_mut()
                .expect("result content is an array")
                .push(answer.block);
        } else {
            if let Some((_, message)) = active.take() {
                output.push(message);
            }
            let message = result_message(protocol, &answer, vec![answer.block.clone()]);
            active = Some((answer.source_message, message));
        }
    }
    if let Some((_, message)) = active {
        output.push(message);
    }
}

pub(super) fn opaque_text(value: &Value) -> String {
    fn readable(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let kind = object.get("type").and_then(Value::as_str).unwrap_or("");
                if matches!(
                    kind,
                    "image"
                        | "image_url"
                        | "input_image"
                        | "input_audio"
                        | "audio"
                        | "input_file"
                        | "file"
                        | "document"
                        | "video"
                ) || [
                    "inlineData",
                    "fileData",
                    "image_url",
                    "audio",
                    "videoMetadata",
                ]
                .iter()
                .any(|key| object.contains_key(*key))
                {
                    return json!({"type": "native_media", "note": "native content retained by source protocol"});
                }
                Value::Object(
                    object
                        .iter()
                        .filter_map(|(key, value)| {
                            let normalized = key.replace('_', "").to_ascii_lowercase();
                            if ["signature", "thoughtsignature", "encryptedcontent"]
                                .contains(&normalized.as_str())
                            {
                                None
                            } else {
                                Some((key.clone(), readable(value)))
                            }
                        })
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.iter().map(readable).collect()),
            _ => value.clone(),
        }
    }
    let encoded = readable(value).to_string();
    if encoded.len() <= 16_384 {
        return format!("[preserved native content] {encoded}");
    }
    let preview: String = encoded.chars().take(4_096).collect();
    format!(
        "[preserved native content: {} bytes; preview] {preview}",
        encoded.len()
    )
}

fn result_blocks(protocol: Protocol, message: &Value) -> Vec<Value> {
    match protocol {
        Protocol::OpenAiChat if message.get("role").and_then(Value::as_str) == Some("tool") => {
            vec![message.clone()]
        }
        Protocol::OpenAiResponses
            if message.get("type").and_then(Value::as_str) == Some("function_call_output") =>
        {
            vec![message.clone()]
        }
        Protocol::Anthropic => message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_result"))
            .cloned()
            .collect(),
        Protocol::Gemini => message
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|part| part.get("functionResponse").is_some())
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

fn call_key(protocol: Protocol) -> &'static str {
    if protocol == Protocol::OpenAiResponses {
        "call_id"
    } else {
        "id"
    }
}
fn result_key(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::OpenAiChat => "tool_call_id",
        Protocol::Anthropic => "tool_use_id",
        _ => "call_id",
    }
}
fn call_name(protocol: Protocol, call: &Value) -> Option<&str> {
    if protocol == Protocol::OpenAiChat {
        call.get("function")?.get("name")?.as_str()
    } else {
        call.get("name")?.as_str()
    }
}
fn calls_mut(protocol: Protocol, message: &mut Value) -> Vec<&mut Value> {
    match protocol {
        Protocol::OpenAiChat => message
            .get_mut("tool_calls")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .collect(),
        Protocol::OpenAiResponses
            if message.get("type").and_then(Value::as_str) == Some("function_call") =>
        {
            vec![message]
        }
        Protocol::Anthropic => message
            .get_mut("content")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_use"))
            .collect(),
        Protocol::Gemini => message
            .get_mut("parts")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter_map(|part| part.get_mut("functionCall"))
            .collect(),
        _ => Vec::new(),
    }
}

fn repair_unmatched_calls(
    protocol: Protocol,
    message: &mut Value,
    matched: &std::collections::HashMap<String, HistoricalResult>,
) -> Result<(), ()> {
    if protocol == Protocol::OpenAiResponses {
        if message.get("type").and_then(Value::as_str) == Some("function_call")
            && !message
                .get("call_id")
                .and_then(Value::as_str)
                .is_some_and(|id| matched.contains_key(id))
        {
            *message = json!({"role":"assistant","content":opaque_text(message)});
        }
        return Ok(());
    }
    let key = match protocol {
        Protocol::OpenAiChat => "tool_calls",
        Protocol::Anthropic => "content",
        Protocol::Gemini => "parts",
        _ => return Err(()),
    };
    let mut descriptions = Vec::new();
    if let Some(blocks) = message.get_mut(key).and_then(Value::as_array_mut) {
        for block in blocks.iter_mut() {
            let call = match protocol {
                Protocol::OpenAiChat => Some(&*block),
                Protocol::Anthropic
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") =>
                {
                    Some(&*block)
                }
                Protocol::Gemini => block.get("functionCall"),
                _ => None,
            };
            if call.is_some_and(|call| {
                !call
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| matched.contains_key(id))
            }) {
                let text = opaque_text(block);
                if protocol == Protocol::OpenAiChat {
                    descriptions.push(text);
                    *block = Value::Null;
                } else if protocol == Protocol::Gemini {
                    *block = json!({"text":text});
                } else {
                    *block = json!({"type":"text","text":text});
                }
            }
        }
        if protocol == Protocol::OpenAiChat {
            blocks.retain(|block| !block.is_null());
        }
    }
    if !descriptions.is_empty() {
        let map = object(message)?;
        if map
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            map.remove("tool_calls");
        }
        let text = descriptions.join("\n");
        match map.entry("content").or_insert(Value::Null) {
            Value::String(content) => {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&text);
            }
            Value::Array(parts) => parts.push(json!({"type":"text","text":text})),
            content => *content = Value::String(text),
        }
    }
    Ok(())
}

/// Match real historical results; missing execution results are never synthesized.
pub(super) fn normalize_tool_history(
    protocol: Protocol,
    body: &mut Value,
    projection: bool,
) -> Result<bool, ()> {
    let key = match protocol {
        Protocol::OpenAiResponses => "input",
        Protocol::Gemini => "contents",
        _ => "messages",
    };
    let Some(messages) = body.get_mut(key).and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let mut results = Vec::new();
    for (source_message, message) in messages.iter().enumerate() {
        let blocks = result_blocks(protocol, message);
        if blocks.is_empty() {
            continue;
        }
        let envelope = result_content_key(protocol).and_then(|key| {
            message.as_object().map(|message| {
                std::sync::Arc::new(
                    message
                        .iter()
                        .filter(|(field, _)| field.as_str() != key)
                        .map(|(field, value)| (field.clone(), value.clone()))
                        .collect::<Map<_, _>>(),
                )
            })
        });
        results.extend(blocks.into_iter().map(|block| HistoricalResult {
            block,
            source_message,
            envelope: envelope.clone(),
        }));
    }
    let mut reserved = std::collections::HashSet::new();
    for message in messages.iter_mut() {
        for call in calls_mut(protocol, message) {
            if let Some(id) = call.get(call_key(protocol)).and_then(Value::as_str) {
                reserved.insert(id.to_owned());
            }
        }
    }
    let mut sequence = 0;
    let mut seen = std::collections::HashSet::new();
    let mut aliases: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut calls = Vec::new();
    for message in messages.iter_mut() {
        for call in calls_mut(protocol, message) {
            let call = object(call)?;
            let original = call
                .get(call_key(protocol))
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned);
            if original.as_ref().is_none_or(|id| !seen.insert(id.clone())) {
                let id = loop {
                    let id = format!("compat_call_{sequence}");
                    sequence += 1;
                    if reserved.insert(id.clone()) {
                        break id;
                    }
                };
                call.insert(call_key(protocol).to_owned(), json!(id));
            }
            let id = call
                .get(call_key(protocol))
                .and_then(Value::as_str)
                .ok_or(())?
                .to_owned();
            if let Some(original) = original {
                aliases.entry(original).or_default().push(id.clone());
            }
            calls.push((
                id,
                call_name(protocol, &Value::Object(call.clone()))
                    .unwrap_or("")
                    .to_owned(),
            ));
        }
    }
    let mut matched = std::collections::HashMap::new();
    let mut orphan = Vec::new();
    for mut result in results {
        let raw = if protocol == Protocol::Gemini {
            result.block.get_mut("functionResponse").ok_or(())?
        } else {
            &mut result.block
        };
        let raw = object(raw)?;
        let idkey = if protocol == Protocol::Gemini {
            "id"
        } else {
            result_key(protocol)
        };
        let explicit = raw
            .get(idkey)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let name = raw.get("name").and_then(Value::as_str);
        let found = if let Some(id) = explicit {
            calls.iter().find(|(call, _)| {
                (call == id || aliases.get(id).is_some_and(|ids| ids.contains(call)))
                    && !matched.contains_key(call)
            })
        } else {
            calls
                .iter()
                .find(|(id, n)| !matched.contains_key(id) && name.is_none_or(|name| name == n))
        };
        if let Some((id, _)) = found {
            raw.insert(idkey.to_owned(), json!(id));
            matched.insert(id.clone(), result);
        } else {
            orphan.push(result);
        }
    }
    let incomplete = calls.iter().any(|(id, _)| !matched.contains_key(id)) || !orphan.is_empty();
    let mut output = Vec::new();
    let mut deferred_responses = Vec::new();
    let mut entries = std::mem::take(messages).into_iter().peekable();
    while let Some(mut message) = entries.next() {
        let standalone = !result_blocks(protocol, &message).is_empty()
            && matches!(protocol, Protocol::OpenAiChat | Protocol::OpenAiResponses);
        if standalone {
            continue;
        }
        if projection {
            repair_unmatched_calls(protocol, &mut message, &matched)?;
        }
        let ids = calls_mut(protocol, &mut message)
            .into_iter()
            .filter_map(|call| {
                call.get(call_key(protocol))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        if matches!(protocol, Protocol::Anthropic | Protocol::Gemini) {
            let block_key = if protocol == Protocol::Gemini {
                "parts"
            } else {
                "content"
            };
            if let Some(parts) = message.get_mut(block_key).and_then(Value::as_array_mut) {
                parts.retain(|part| {
                    if protocol == Protocol::Gemini {
                        part.get("functionResponse").is_none()
                    } else {
                        part.get("type").and_then(Value::as_str) != Some("tool_result")
                    }
                });
                if parts.is_empty() {
                    continue;
                }
            }
        }
        if protocol == Protocol::OpenAiResponses
            && message.get("type").and_then(Value::as_str) != Some("function_call")
        {
            output.append(&mut deferred_responses);
        }
        output.push(message);
        // Responses represents a parallel call batch with consecutive items.
        // Delay their results until the last call rather than splitting it.
        let answers = ids
            .iter()
            .filter_map(|id| matched.remove(id))
            .collect::<Vec<_>>();
        if !answers.is_empty() {
            match protocol {
                Protocol::Anthropic | Protocol::Gemini => {
                    append_result_messages(protocol, &mut output, answers)
                }
                Protocol::OpenAiResponses => {
                    deferred_responses.extend(answers.into_iter().map(|answer| answer.block));
                    if !entries.peek().is_some_and(|next| {
                        next.get("type").and_then(Value::as_str) == Some("function_call")
                    }) {
                        output.append(&mut deferred_responses);
                    }
                }
                _ => output.extend(answers.into_iter().map(|answer| answer.block)),
            }
        }
    }
    output.append(&mut deferred_responses);
    for result in orphan {
        if projection {
            let text = opaque_text(&result.block);
            output.push(if protocol == Protocol::Gemini {
                result_message(protocol, &result, vec![json!({"text":text})])
            } else if protocol == Protocol::Anthropic {
                result_message(protocol, &result, vec![json!({"type":"text","text":text})])
            } else {
                json!({"role":"user","content":text})
            });
        } else {
            match protocol {
                Protocol::Anthropic | Protocol::Gemini => {
                    let message = result_message(protocol, &result, vec![result.block.clone()]);
                    output.push(message);
                }
                _ => output.push(result.block),
            }
        }
    }
    *messages = output;
    Ok(incomplete)
}
