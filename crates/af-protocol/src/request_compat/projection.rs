use super::*;

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;

fn retain(map: &mut Map<String, Value>, keys: &[&str], extensions: &mut usize) {
    map.retain(|key, _| {
        let keep = keys.contains(&key.as_str());
        if !keep {
            *extensions += 1;
        }
        keep
    });
}

pub(super) fn project_request(
    protocol: Protocol,
    value: &mut Value,
    extensions: &mut usize,
) -> Result<(), ()> {
    let root = object(value)?;
    let keys: &[&str] = match protocol {
        Protocol::OpenAiChat => &[
            "model",
            "messages",
            "tools",
            "tool_choice",
            "reasoning_effort",
            "temperature",
            "top_p",
            "max_tokens",
            "max_completion_tokens",
            "stop",
            "user",
            "stream",
            "stream_options",
            "frequency_penalty",
            "logit_bias",
            "logprobs",
            "metadata",
            "n",
            "parallel_tool_calls",
            "presence_penalty",
            "prompt_cache_key",
            "prompt_cache_retention",
            "safety_identifier",
            "seed",
            "service_tier",
            "store",
            "top_logprobs",
            "verbosity",
        ],
        Protocol::OpenAiResponses => &[
            "model",
            "input",
            "instructions",
            "conversation",
            "previous_response_id",
            "prompt_cache_key",
            "tools",
            "tool_choice",
            "reasoning",
            "temperature",
            "top_p",
            "max_output_tokens",
            "user",
            "stream",
            "context_management",
            "include",
            "parallel_tool_calls",
            "prompt_cache_retention",
            "safety_identifier",
            "service_tier",
            "store",
            "truncation",
        ],
        Protocol::Anthropic => &[
            "model",
            "messages",
            "system",
            "max_tokens",
            "tools",
            "tool_choice",
            "thinking",
            "output_config",
            "temperature",
            "top_p",
            "stop_sequences",
            "metadata",
            "stream",
            "cache_control",
            "compaction",
            "context_management",
        ],
        Protocol::Gemini => &[
            "contents",
            "systemInstruction",
            "tools",
            "toolConfig",
            "generationConfig",
        ],
        _ => return Err(()),
    };
    retain(root, keys, extensions);
    if protocol == Protocol::Anthropic && !root.contains_key("max_tokens") {
        let budget = root
            .get("thinking")
            .and_then(|thinking| thinking.get("budget_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        // Required only by the strict validation view; the gateway injects its real default.
        root.insert(
            "max_tokens".to_owned(),
            json!(4096_i64.max(budget.saturating_add(1))),
        );
    }
    if protocol == Protocol::OpenAiChat {
        if let Some(metadata) = root.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.retain(|_, value| value.is_string());
        }
        // Native-only controls remain in body, not in the single-choice projection.
        for key in ["n", "logprobs", "top_logprobs", "logit_bias"] {
            root.remove(key);
        }
    }
    if protocol == Protocol::OpenAiResponses {
        // Generic metadata has no model semantics and is kept verbatim in body.
        if let Some(metadata) = root.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.retain(|_, value| value.is_string());
        }
    }
    if protocol == Protocol::Gemini {
        project_gemini(root, extensions)?;
    } else {
        let key = if protocol == Protocol::OpenAiResponses {
            "input"
        } else {
            "messages"
        };
        if let Some(messages) = root.get_mut(key).and_then(Value::as_array_mut) {
            for message in messages {
                let map = object(message)?;
                let kind = map
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("message")
                    .to_owned();
                match kind.as_str() {
                    "function_call" => {
                        retain(
                            map,
                            &["type", "id", "status", "call_id", "name", "arguments"],
                            extensions,
                        );
                    }
                    "function_call_output" => {
                        retain(
                            map,
                            &["type", "id", "status", "call_id", "output"],
                            extensions,
                        );
                        if let Some(output) = map.get_mut("output") {
                            project_content(output, protocol, extensions)?;
                        }
                    }
                    "reasoning" => {
                        retain(
                            map,
                            &["type", "id", "status", "encrypted_content", "summary"],
                            extensions,
                        );
                        if let Some(parts) = map.get_mut("summary").and_then(Value::as_array_mut) {
                            for part in parts {
                                retain(object(part)?, &["type", "text"], extensions);
                            }
                        }
                    }
                    "compaction" => {
                        retain(map, &["type", "id", "encrypted_content"], extensions);
                    }
                    "message" => {
                        let allowed: &[&str] = if protocol == Protocol::OpenAiResponses {
                            &["type", "id", "status", "phase", "role", "content"]
                        } else if protocol == Protocol::OpenAiChat {
                            &[
                                "role",
                                "content",
                                "tool_calls",
                                "tool_call_id",
                                "reasoning_content",
                                "reasoning",
                            ]
                        } else {
                            &["role", "content"]
                        };
                        retain(map, allowed, extensions);
                        if protocol == Protocol::OpenAiResponses {
                            if map
                                .get("status")
                                .and_then(Value::as_str)
                                .is_some_and(|value| {
                                    !matches!(value, "in_progress" | "completed" | "incomplete")
                                })
                            {
                                map.remove("status");
                            }
                            if map.get("role").and_then(Value::as_str) != Some("assistant")
                                || map
                                    .get("phase")
                                    .and_then(Value::as_str)
                                    .is_some_and(|value| {
                                        !matches!(value, "commentary" | "final_answer")
                                    })
                            {
                                map.remove("phase");
                            }
                        }
                        if let Some(content) = map.get_mut("content") {
                            project_content(content, protocol, extensions)?;
                        }
                        if let Some(calls) = map.get_mut("tool_calls").and_then(Value::as_array_mut)
                        {
                            for call in calls {
                                let call = object(call)?;
                                retain(call, &["type", "id", "index", "function"], extensions);
                                if let Some(function) = call.get_mut("function") {
                                    retain(object(function)?, &["name", "arguments"], extensions);
                                }
                            }
                        }
                    }
                    _ => {
                        *extensions += 1;
                        let text = opaque_text(message);
                        *message = json!({"role":"assistant","content":text});
                    }
                }
            }
        }
        if let Some(system) = root.get_mut("system") {
            project_content(system, protocol, extensions)?;
        }
    }
    if let Some(metadata) = root
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .filter(|_| protocol == Protocol::Anthropic)
    {
        retain(metadata, &["user_id"], extensions);
    }
    for (key, allowed) in [
        ("stream_options", &["include_usage"][..]),
        ("reasoning", &["effort", "summary"][..]),
        ("thinking", &["type", "budget_tokens", "display"][..]),
        ("output_config", &["effort"][..]),
    ] {
        if let Some(map) = root.get_mut(key).and_then(Value::as_object_mut) {
            retain(map, allowed, extensions);
        }
    }
    if root
        .get("output_config")
        .and_then(Value::as_object)
        .is_some_and(|v| !v.contains_key("effort"))
    {
        root.remove("output_config");
    }
    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        let mut native_hints = Vec::new();
        for tool in tools.iter_mut() {
            if !tool_has_name(protocol, tool) {
                native_hints.push(opaque_text(tool));
            }
            project_tool(tool, protocol, extensions)?;
        }
        tools.retain(|tool| !tool.is_null());
        if tools.is_empty() {
            root.remove("tools");
            root.remove("tool_choice");
        }
        if !native_hints.is_empty() {
            let text = native_hints.join("\n");
            let key = if protocol == Protocol::OpenAiResponses {
                "input"
            } else if protocol == Protocol::Gemini {
                "contents"
            } else {
                "messages"
            };
            let message = if protocol == Protocol::Gemini {
                json!({"role":"user","parts":[{"text":text}]})
            } else {
                json!({"role":"user","content":text})
            };
            match root.get_mut(key) {
                Some(Value::Array(messages)) => messages.push(message),
                Some(Value::String(input)) if protocol == Protocol::OpenAiResponses => {
                    let first = json!({"role":"user","content":input});
                    root.insert(key.to_owned(), json!([first, message]));
                }
                None if protocol == Protocol::OpenAiResponses => {
                    root.insert(key.to_owned(), json!([message]));
                }
                _ => {}
            }
        }
    }
    if let Some(choice) = root.get_mut("tool_choice").and_then(Value::as_object_mut) {
        retain(
            choice,
            if protocol == Protocol::Anthropic {
                &["type", "name", "disable_parallel_tool_use"]
            } else if protocol == Protocol::OpenAiResponses {
                &["type", "name"]
            } else {
                &["type", "function"]
            },
            extensions,
        );
        if let Some(function) = choice.get_mut("function").and_then(Value::as_object_mut) {
            retain(function, &["name"], extensions);
        }
        if protocol == Protocol::Anthropic {
            choice.remove("disable_parallel_tool_use");
        }
    }
    if let Some(cache) = root.get_mut("cache_control").and_then(Value::as_object_mut) {
        retain(cache, &["type", "ttl"], extensions);
    }
    Ok(())
}

fn project_content(
    value: &mut Value,
    protocol: Protocol,
    extensions: &mut usize,
) -> Result<(), ()> {
    let Some(parts) = value.as_array_mut() else {
        return Ok(());
    };
    for part in parts {
        let map = object(part)?;
        let kind = map
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if protocol == Protocol::Anthropic
            && kind == "thinking"
            && !map
                .get("signature")
                .and_then(Value::as_str)
                .is_some_and(|signature| !signature.is_empty())
        {
            let text = map
                .get("thinking")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            *part = json!({"type":"text","text":text});
            continue;
        }
        if protocol == Protocol::Anthropic && kind == "compaction" {
            // Keep vendor-signed fields and explicit nulls intact for the
            // native compaction decoder; the block remains native-only.
            continue;
        }
        let keys: Option<&[&str]> = match (protocol, kind.as_str()) {
            (Protocol::OpenAiChat, "text") => Some(&["type", "text"]),
            (Protocol::OpenAiChat, "image_url") => Some(&["type", "image_url"]),
            (Protocol::OpenAiChat, "input_audio") => Some(&["type", "input_audio"]),
            (Protocol::OpenAiResponses, "input_text" | "output_text") => Some(&["type", "text"]),
            (Protocol::OpenAiResponses, "input_image") if map.contains_key("image_url") => {
                Some(&["type", "image_url", "detail"])
            }
            (Protocol::Anthropic, "text") => Some(&["type", "text", "cache_control"]),
            (Protocol::Anthropic, "image") => Some(&["type", "source", "cache_control"]),
            (Protocol::Anthropic, "thinking") => Some(&["type", "thinking", "signature"]),
            (Protocol::Anthropic, "tool_use") => {
                Some(&["type", "id", "name", "input", "cache_control"])
            }
            (Protocol::Anthropic, "tool_result") => Some(&[
                "type",
                "tool_use_id",
                "content",
                "is_error",
                "cache_control",
            ]),
            _ => None,
        };
        if let Some(keys) = keys {
            retain(map, keys, extensions);
            if kind == "tool_result"
                && let Some(content) = map.get_mut("content")
            {
                project_content(content, protocol, extensions)?;
            }
            for (key, keys) in [
                ("image_url", &["url", "detail"][..]),
                ("input_audio", &["data", "format"][..]),
                ("source", &["type", "media_type", "data", "url"][..]),
                ("cache_control", &["type", "ttl"][..]),
            ] {
                if let Some(nested) = map.get_mut(key).and_then(Value::as_object_mut) {
                    retain(nested, keys, extensions);
                }
            }
        } else {
            *extensions += 1;
            let text = opaque_text(part);
            *part = json!({"type":if protocol==Protocol::OpenAiResponses{"input_text"}else{"text"},"text":text});
        }
    }
    Ok(())
}

fn project_tool(tool: &mut Value, protocol: Protocol, extensions: &mut usize) -> Result<(), ()> {
    let map = object(tool)?;
    if protocol == Protocol::Gemini {
        retain(map, &["functionDeclarations"], extensions);
        if let Some(defs) = map
            .get_mut("functionDeclarations")
            .and_then(Value::as_array_mut)
        {
            for def in defs {
                retain(
                    object(def)?,
                    &["name", "description", "parameters", "parametersJsonSchema"],
                    extensions,
                );
            }
        }
        if map.is_empty() {
            *tool = Value::Null;
        }
        return Ok(());
    }
    if protocol == Protocol::OpenAiChat {
        if map.get("type").and_then(Value::as_str) != Some("function") {
            *extensions += 1;
            if let Some(name) = map.get("name").and_then(Value::as_str) {
                *tool = json!({"type":"function","function":{"name":name,"parameters":{"type":"object","properties":{}}}});
            } else {
                *tool = Value::Null;
            }
            return Ok(());
        }
        retain(map, &["type", "function"], extensions);
        retain(
            object(map.get_mut("function").ok_or(())?)?,
            &["name", "description", "parameters", "strict"],
            extensions,
        );
    } else {
        if !map.contains_key("name") {
            *extensions += 1;
            *tool = Value::Null;
            return Ok(());
        }
        if protocol == Protocol::OpenAiResponses {
            map.insert("type".to_owned(), json!("function"));
        }
        retain(
            map,
            if protocol == Protocol::Anthropic {
                &[
                    "name",
                    "description",
                    "input_schema",
                    "eager_input_streaming",
                    "cache_control",
                ]
            } else {
                &["type", "name", "description", "parameters", "strict"]
            },
            extensions,
        );
    }
    Ok(())
}

fn tool_has_name(protocol: Protocol, tool: &Value) -> bool {
    if protocol == Protocol::Gemini {
        tool.get("functionDeclarations").is_some()
    } else {
        tool.get("name").is_some() || tool.get("function").and_then(|f| f.get("name")).is_some()
    }
}

fn project_gemini(root: &mut Map<String, Value>, extensions: &mut usize) -> Result<(), ()> {
    if let Some(config) = root
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    {
        retain(
            config,
            &[
                "temperature",
                "topP",
                "maxOutputTokens",
                "stopSequences",
                "candidateCount",
                "thinkingConfig",
            ],
            extensions,
        );
        if let Some(thinking) = config
            .get_mut("thinkingConfig")
            .and_then(Value::as_object_mut)
            && (thinking.contains_key("thinkingBudget")
                || thinking
                    .get("thinkingLevel")
                    .and_then(Value::as_str)
                    .is_some_and(|level| !matches!(level, "MINIMAL" | "LOW" | "MEDIUM" | "HIGH")))
        {
            thinking.remove("thinkingLevel");
        }
        if config
            .get("candidateCount")
            .is_some_and(|value| value.as_i64() != Some(1))
        {
            config.remove("candidateCount");
        }
        if let Some(thinking) = config
            .get_mut("thinkingConfig")
            .and_then(Value::as_object_mut)
        {
            retain(
                thinking,
                &["includeThoughts", "thinkingBudget", "thinkingLevel"],
                extensions,
            );
        }
    }
    let mut allowed_names = None;
    if let Some(config) = root.get_mut("toolConfig").and_then(Value::as_object_mut) {
        retain(config, &["functionCallingConfig"], extensions);
        if let Some(function) = config
            .get_mut("functionCallingConfig")
            .and_then(Value::as_object_mut)
        {
            retain(function, &["mode", "allowedFunctionNames"], extensions);
            // Represent allowed subsets through the declarations themselves.
            // Native requests retain their original mode and allowed names.
            let project_allowed_names = match function.get("mode").and_then(Value::as_str) {
                Some("VALIDATED") => {
                    function.insert("mode".to_owned(), json!("AUTO"));
                    true
                }
                None | Some("AUTO" | "ANY") => true,
                _ => false,
            };
            if project_allowed_names && let Some(names) = function.remove("allowedFunctionNames") {
                allowed_names = Some(
                    names
                        .as_array()
                        .ok_or(())?
                        .iter()
                        .map(|name| name.as_str().map(str::to_owned).ok_or(()))
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
        }
    }
    if let Some(names) = allowed_names
        && !names.is_empty()
        && let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut)
    {
        for tool in tools.iter_mut() {
            if let Some(declarations) = tool
                .get_mut("functionDeclarations")
                .and_then(Value::as_array_mut)
            {
                declarations.retain(|declaration| {
                    declaration
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| names.iter().any(|allowed| allowed == name))
                });
            }
        }
        tools.retain(|tool| {
            !tool
                .get("functionDeclarations")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        });
    }
    for key in ["contents", "systemInstruction"] {
        if let Some(value) = root.get_mut(key) {
            let values = if key == "contents" {
                value
                    .as_array_mut()
                    .ok_or(())?
                    .iter_mut()
                    .collect::<Vec<_>>()
            } else {
                vec![value]
            };
            for content in values {
                let content = object(content)?;
                retain(content, &["role", "parts"], extensions);
                for part in content
                    .get_mut("parts")
                    .and_then(Value::as_array_mut)
                    .ok_or(())?
                {
                    let native_inline_media = gemini_native_inline_media(part);
                    let map = object(part)?;
                    let known = map.contains_key("text")
                        || map.contains_key("inlineData")
                        || map.contains_key("functionCall")
                        || map.contains_key("functionResponse");
                    if !known || native_inline_media {
                        *extensions += 1;
                        *part = json!({"text":opaque_text(part)});
                        continue;
                    }
                    retain(
                        map,
                        &[
                            "text",
                            "inlineData",
                            "functionCall",
                            "functionResponse",
                            "thought",
                            "thoughtSignature",
                        ],
                        extensions,
                    );
                    for (key, keys) in [
                        ("inlineData", &["mimeType", "data"][..]),
                        ("functionCall", &["id", "name", "args"][..]),
                        ("functionResponse", &["id", "name", "response"][..]),
                    ] {
                        if let Some(nested) = map.get_mut(key).and_then(Value::as_object_mut) {
                            retain(nested, keys, extensions);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn has_opaque_semantics(protocol: Protocol, body: &Value) -> bool {
    let Some(root) = body.as_object() else {
        return false;
    };
    let native_fields: &[&str] = match protocol {
        Protocol::OpenAiChat => &["audio", "modalities", "web_search_options"],
        Protocol::OpenAiResponses => &["prompt", "context_management"],
        Protocol::Anthropic => &[
            "compaction",
            "container",
            "context_management",
            "mcp_servers",
            "prompt",
        ],
        Protocol::Gemini => &["cachedContent"],
        _ => return true,
    };
    if native_fields
        .iter()
        .any(|key| root.get(*key).is_some_and(|v| !v.is_null()))
    {
        return true;
    }
    let format = match protocol {
        Protocol::OpenAiChat => root.get("response_format"),
        Protocol::OpenAiResponses => root.get("text").and_then(|text| text.get("format")),
        Protocol::Anthropic => root
            .get("output_config")
            .and_then(|output| output.get("format"))
            .or_else(|| root.get("output_format")),
        _ => None,
    };
    if format.is_some_and(|format| {
        !format.is_null()
            && !format
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "json_schema" | "json_object" | "text"))
    }) {
        return true;
    }
    if protocol == Protocol::OpenAiChat && root.get("n").is_some_and(|n| n.as_i64() != Some(1)) {
        return true;
    }
    if protocol == Protocol::Gemini
        && root
            .get("generationConfig")
            .and_then(|config| config.get("responseMimeType"))
            .and_then(Value::as_str)
            .is_some_and(|mime| !matches!(mime, "application/json" | "text/plain"))
    {
        return true;
    }
    if protocol == Protocol::Gemini {
        // Known optional generation preferences may degrade on another provider;
        // output modalities, schemas and media still require native semantics.
        if root
            .get("generationConfig")
            .and_then(Value::as_object)
            .is_some_and(|config| {
                [
                    "responseModalities",
                    "speechConfig",
                    "mediaResolution",
                    "imageConfig",
                ]
                .iter()
                .any(|key| config.get(*key).is_some_and(|v| !v.is_null()))
            })
        {
            return true;
        }
        for content in root
            .get("contents")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .chain(root.get("systemInstruction"))
        {
            for part in content
                .get("parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !["text", "inlineData", "functionCall", "functionResponse"]
                    .iter()
                    .any(|key| part.get(*key).is_some())
                    || part.get("fileData").is_some()
                    || gemini_native_inline_media(part)
                {
                    return true;
                }
            }
        }
    } else {
        let key = if protocol == Protocol::OpenAiResponses {
            "input"
        } else {
            "messages"
        };
        for message in root
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if protocol == Protocol::OpenAiResponses
                && !matches!(
                    message
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("message"),
                    "message"
                        | "reasoning"
                        | "function_call"
                        | "function_call_output"
                        | "compaction"
                )
            {
                return true;
            }
            for key in ["content", "output"] {
                if content_is_opaque(protocol, message.get(key)) {
                    return true;
                }
            }
            if message.get("refusal").is_some_and(|v| !v.is_null())
                || message.get("audio").is_some_and(|v| !v.is_null())
            {
                return true;
            }
        }
        if content_is_opaque(protocol, root.get("system")) {
            return true;
        }
    }
    root.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools.iter().any(|tool| {
                if protocol == Protocol::Gemini {
                    tool.as_object().is_some_and(|map| {
                        map.keys()
                            .any(|key| key != "functionDeclarations" && key != "metadata")
                    })
                } else if protocol == Protocol::Anthropic {
                    // `custom` tools are ordinary caller-owned functions. A
                    // strict input preference does not require Anthropic's
                    // server-side tool execution or make the schema opaque.
                    tool.get("type")
                        .filter(|kind| !kind.is_null())
                        .is_some_and(|kind| {
                            kind.as_str() != Some("custom") && kind.as_str() != Some("function")
                        })
                } else {
                    // Function-loading/caller/output-schema annotations remain
                    // in the native source context. The function definition's
                    // name and input schema still convert to every target.
                    tool.get("type").and_then(Value::as_str) != Some("function")
                }
            })
        })
}

fn gemini_native_inline_media(part: &Value) -> bool {
    part.get("inlineData")
        .and_then(|data| data.get("mimeType"))
        .and_then(Value::as_str)
        .is_some_and(|mime| {
            let mime = mime.to_ascii_lowercase();
            !mime.is_empty() && !mime.starts_with("image/") && !mime.starts_with("audio/")
        })
}

fn content_is_opaque(protocol: Protocol, content: Option<&Value>) -> bool {
    content.and_then(Value::as_array).is_some_and(|parts| {
        parts.iter().any(|part| {
            let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
            let supported = match protocol {
                Protocol::OpenAiChat => matches!(kind, "text" | "image_url" | "input_audio"),
                Protocol::OpenAiResponses => {
                    matches!(kind, "input_text" | "output_text")
                        || kind == "input_image" && part.get("image_url").is_some()
                }
                Protocol::Anthropic => matches!(
                    kind,
                    "text" | "image" | "thinking" | "tool_use" | "tool_result"
                ),
                _ => false,
            };
            !supported
                || part.get("citations").is_some_and(|value| {
                    !value.is_null() && value.as_array().is_none_or(|items| !items.is_empty())
                })
                || part.get("caller").is_some_and(|value| {
                    !value.is_null() && value.get("type").and_then(Value::as_str) != Some("direct")
                })
                || (kind == "tool_result" && content_is_opaque(protocol, part.get("content")))
                || ["annotations", "logprobs"].iter().any(|key| {
                    part.get(*key)
                        .and_then(Value::as_array)
                        .is_some_and(|values| !values.is_empty())
                })
        })
    })
}
