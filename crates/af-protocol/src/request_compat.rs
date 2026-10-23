//! Bounded ingress normalization. Native extensions remain in the source body.

use af_domain::Protocol;
use serde_json::{Map, Value, json};

mod history;
mod projection;
use history::{normalize_tool_history, opaque_text};
use projection::{has_opaque_semantics, project_request};

#[cfg(test)]
mod tests;

pub(crate) struct CompatibleRequest {
    pub(crate) body: Value,
    pub(crate) validation: Value,
    pub(crate) native_only: bool,
}

/// Normalize only protocol fields, never arbitrary JSON schemas, arguments or results.
pub(crate) fn normalize(protocol: Protocol, mut body: Value) -> Result<CompatibleRequest, ()> {
    normalize_request(protocol, &mut body)?;
    let native_only = has_opaque_semantics(protocol, &body);
    normalize_tool_history(protocol, &mut body, true)?;
    let mut validation = body.clone();
    let mut preserved_extensions = 0;
    project_request(protocol, &mut validation, &mut preserved_extensions)?;
    normalize_tool_history(protocol, &mut validation, true)?;
    Ok(CompatibleRequest {
        body,
        validation,
        native_only,
    })
}

fn object(value: &mut Value) -> Result<&mut Map<String, Value>, ()> {
    value.as_object_mut().ok_or(())
}

fn alias(map: &mut Map<String, Value>, from: &str, to: &str) -> Result<(), ()> {
    if let Some(value) = map.remove(from) {
        // Canonical spellings win when clients redundantly emit both SDK aliases.
        if map.get(to).is_none_or(Value::is_null) {
            map.insert(to.to_owned(), value);
        }
    }
    Ok(())
}

fn optional(map: &mut Map<String, Value>, keys: &[&str]) {
    for key in keys {
        if map.get(*key).is_some_and(Value::is_null) {
            map.remove(*key);
        }
    }
}

fn scalars(map: &mut Map<String, Value>, bools: &[&str], numbers: &[&str]) {
    for key in bools {
        let mut invalid = false;
        if let Some(value) = map.get_mut(*key) {
            if let Some(text) = value.as_str() {
                match text.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" => *value = Value::Bool(true),
                    "false" | "0" => *value = Value::Bool(false),
                    _ => invalid = true,
                }
            } else if value.as_i64() == Some(1) {
                *value = Value::Bool(true);
            } else if value.as_i64() == Some(0) {
                *value = Value::Bool(false);
            } else if !value.is_boolean() {
                invalid = true;
            }
        }
        if invalid {
            map.remove(*key);
        }
    }
    for key in numbers {
        let mut invalid = false;
        if let Some(value) = map.get_mut(*key) {
            if let Some(text) = value.as_str() {
                match serde_json::from_str::<serde_json::Number>(text.trim()) {
                    Ok(number) => *value = Value::Number(number),
                    Err(_) => invalid = true,
                }
            } else if !value.is_number() {
                invalid = true;
            }
        }
        if invalid {
            map.remove(*key);
        }
    }
}

fn lift_anthropic_system(root: &mut Map<String, Value>) -> Result<(), ()> {
    scalars(root, &[], &["max_tokens"]);
    let messages = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .ok_or(())?;
    let mut system = Vec::new();
    messages.retain(|message| {
        if message
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| matches!(role, "system" | "developer"))
        {
            if let Some(content) = message.get("content") {
                if let Some(parts) = content.as_array() {
                    system.extend(parts.iter().cloned());
                } else {
                    system.push(json!({"type":"text","text":content}));
                }
            }
            false
        } else {
            true
        }
    });
    if !system.is_empty() {
        if let Some(original) = root.remove("system") {
            let mut existing = if let Some(parts) = original.as_array() {
                parts.clone()
            } else {
                vec![json!({"type":"text","text":original})]
            };
            existing.extend(system);
            system = existing;
        }
        root.insert("system".to_owned(), Value::Array(system));
    }
    Ok(())
}

fn schema(value: &mut Value) {
    if value.is_null() {
        *value = json!({"type":"object","properties":{}});
    }
    if let Some(map) = value.as_object_mut()
        && !map.contains_key("type")
        && !map.contains_key("$ref")
        && !["oneOf", "anyOf", "allOf", "not"]
            .iter()
            .any(|k| map.contains_key(*k))
    {
        map.insert("type".to_owned(), json!("object"));
    }
}

fn normalize_request(protocol: Protocol, value: &mut Value) -> Result<(), ()> {
    let root = object(value)?;
    optional(
        root,
        &[
            "tools",
            "tool_choice",
            "functions",
            "function_call",
            "reasoning",
            "reasoning_effort",
            "temperature",
            "top_p",
            "max_tokens",
            "max_completion_tokens",
            "max_output_tokens",
            "user",
            "stream",
            "stream_options",
            "stop",
            "stop_sequences",
            "metadata",
            "system",
            "thinking",
            "output_config",
            "cache_control",
            "instructions",
            "conversation",
            "previous_response_id",
            "prompt_cache_key",
            "context_management",
            "include",
            "store",
            "parallel_tool_calls",
        ],
    );
    scalars(
        root,
        &["stream", "store", "parallel_tool_calls", "logprobs"],
        &[
            "temperature",
            "top_p",
            "max_tokens",
            "max_completion_tokens",
            "max_output_tokens",
            "seed",
            "n",
            "frequency_penalty",
            "presence_penalty",
            "top_logprobs",
        ],
    );
    if let Some(options) = root
        .get_mut("stream_options")
        .and_then(Value::as_object_mut)
    {
        optional(options, &["include_usage", "include_obfuscation"]);
        scalars(options, &["include_usage", "include_obfuscation"], &[]);
    }
    match protocol {
        Protocol::OpenAiChat => {
            legacy_chat(root)?;
            normalize_messages(root.get_mut("messages").ok_or(())?, protocol)?;
        }
        Protocol::OpenAiResponses => {
            if let Some(input) = root.get_mut("input") {
                if input.is_object() {
                    *input = Value::Array(vec![input.take()]);
                }
                if let Some(items) = input.as_array_mut() {
                    for item in items {
                        if item.is_string() {
                            *item = json!({"role":"user","content":item.take()});
                        }
                        let map = object(item)?;
                        optional(map, &["id", "status", "encrypted_content", "summary"]);
                        let kind = map.get("type").and_then(Value::as_str).unwrap_or("message");
                        match kind {
                            "message" => normalize_message(map, protocol)?,
                            "function_call" => stringify_field(map, "arguments", false),
                            "function_call_output" => normalize_result(map, "output", protocol)?,
                            _ => {}
                        }
                    }
                }
            }
            if let Some(reasoning) = root.get_mut("reasoning").and_then(Value::as_object_mut) {
                optional(reasoning, &["effort", "summary"]);
            }
        }
        Protocol::Anthropic => {
            alias(root, "max_tokens_to_sample", "max_tokens")?;
            normalize_messages(root.get_mut("messages").ok_or(())?, protocol)?;
            lift_anthropic_system(root)?;
            if let Some(system) = root.get_mut("system") {
                normalize_content(system, protocol, false)?;
            }
            if let Some(thinking) = root.get_mut("thinking").and_then(Value::as_object_mut) {
                optional(thinking, &["display"]);
                scalars(thinking, &[], &["budget_tokens"]);
            }
        }
        Protocol::Gemini => normalize_gemini(root)?,
        _ => return Err(()),
    }
    if let Some(tools) = root.get_mut("tools") {
        if tools.is_object() {
            *tools = Value::Array(vec![tools.take()]);
        }
        for tool in tools.as_array_mut().ok_or(())? {
            let map = object(tool)?;
            if protocol == Protocol::Gemini {
                alias(map, "function_declarations", "functionDeclarations")?;
                if let Some(definitions) = map.get_mut("functionDeclarations") {
                    if definitions.is_object() {
                        *definitions = Value::Array(vec![definitions.take()]);
                    }
                    for definition in definitions.as_array_mut().ok_or(())? {
                        normalize_function(object(definition)?, "parameters")?;
                    }
                }
            } else if protocol == Protocol::OpenAiChat {
                if !map.contains_key("type") {
                    map.insert("type".to_owned(), json!("function"));
                }
                if map.get("type").and_then(Value::as_str) == Some("function") {
                    if !map.contains_key("function") {
                        let definition = Value::Object(std::mem::take(map));
                        map.insert("type".to_owned(), json!("function"));
                        let mut definition = definition;
                        object(&mut definition)?.remove("type");
                        map.insert("function".to_owned(), definition);
                    }
                    normalize_function(object(map.get_mut("function").ok_or(())?)?, "parameters")?;
                }
            } else {
                if protocol == Protocol::OpenAiResponses
                    && map.contains_key("name")
                    && !map.contains_key("type")
                {
                    map.insert("type".to_owned(), json!("function"));
                }
                if map.contains_key("name") {
                    normalize_function(
                        map,
                        if protocol == Protocol::Anthropic {
                            "input_schema"
                        } else {
                            "parameters"
                        },
                    )?;
                }
            }
        }
    }
    normalize_optional_parameters(protocol, root);
    Ok(())
}

fn clamp_number(map: &mut Map<String, Value>, key: &str, maximum: f64) {
    if let Some(number) = map.get(key).and_then(Value::as_f64) {
        let clamped = number.clamp(0.0, maximum);
        if clamped != number {
            map.insert(key.to_owned(), json!(clamped));
        }
    }
}

fn output_limit(map: &mut Map<String, Value>, key: &str) {
    if let Some(number) = map.get(key).and_then(Value::as_f64) {
        if number < 1.0 {
            map.remove(key);
        } else if map.get(key).and_then(Value::as_i64).is_none() || number > 1_000_000.0 {
            map.insert(
                key.to_owned(),
                json!(number.min(1_000_000.0).floor() as i64),
            );
        }
    }
}

fn normalize_optional_parameters(protocol: Protocol, root: &mut Map<String, Value>) {
    for key in ["stop", "stop_sequences"] {
        if root.get(key).is_some_and(|value| {
            value.as_array().is_some_and(Vec::is_empty) || value.as_str().is_some_and(str::is_empty)
        }) {
            root.remove(key);
        }
    }
    if protocol == Protocol::Gemini {
        if let Some(config) = root
            .get_mut("generationConfig")
            .and_then(Value::as_object_mut)
        {
            if config.get("stopSequences").is_some_and(|value| {
                value.as_array().is_some_and(Vec::is_empty)
                    || value.as_str().is_some_and(str::is_empty)
            }) {
                config.remove("stopSequences");
            }
            clamp_number(config, "temperature", 2.0);
            clamp_number(config, "topP", 1.0);
            output_limit(config, "maxOutputTokens");
            if let Some(thinking) = config
                .get_mut("thinkingConfig")
                .and_then(Value::as_object_mut)
                && let Some(budget) = thinking.get("thinkingBudget").and_then(Value::as_i64)
            {
                // -1 is Gemini's native dynamic thinking budget.
                if budget < -1 {
                    thinking.remove("thinkingBudget");
                } else if budget > 1_000_000 {
                    thinking.insert("thinkingBudget".to_owned(), json!(1_000_000));
                }
            }
        }
        if let Some(config) = root
            .get_mut("toolConfig")
            .and_then(Value::as_object_mut)
            .and_then(|config| config.get_mut("functionCallingConfig"))
            .and_then(Value::as_object_mut)
            && config.get("mode").is_some_and(|mode| {
                !mode
                    .as_str()
                    .is_some_and(|mode| matches!(mode, "AUTO" | "ANY" | "NONE" | "VALIDATED"))
            })
        {
            config.insert("mode".to_owned(), json!("AUTO"));
        }
        return;
    }
    clamp_number(
        root,
        "temperature",
        if protocol == Protocol::Anthropic {
            1.0
        } else {
            2.0
        },
    );
    clamp_number(root, "top_p", 1.0);
    for key in ["max_tokens", "max_completion_tokens", "max_output_tokens"] {
        output_limit(root, key);
    }
    if protocol == Protocol::Anthropic {
        let maximum = root
            .get("max_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(1_000_000);
        if let Some(thinking) = root.get_mut("thinking").and_then(Value::as_object_mut)
            && thinking.get("type").and_then(Value::as_str) == Some("enabled")
        {
            if maximum > 1024
                && let Some(budget) = thinking.get("budget_tokens").and_then(Value::as_i64)
            {
                thinking.insert(
                    "budget_tokens".to_owned(),
                    json!(budget.clamp(1024, maximum - 1)),
                );
            } else {
                // Adaptive thinking is valid without an explicit budget.
                thinking.insert("type".to_owned(), json!("adaptive"));
                thinking.remove("budget_tokens");
            }
        }
    }
    let names: std::collections::HashSet<String> = root
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            if protocol == Protocol::OpenAiChat {
                tool.get("function")?.get("name")?.as_str()
            } else {
                tool.get("name")?.as_str()
            }
        })
        .map(str::to_owned)
        .collect();
    let Some(choice) = root.get("tool_choice") else {
        return;
    };
    let valid = if let Some(mode) = choice.as_str() {
        protocol != Protocol::Anthropic && matches!(mode, "auto" | "none")
            || protocol != Protocol::Anthropic && mode == "required" && !names.is_empty()
    } else if protocol == Protocol::Anthropic {
        match choice.get("type").and_then(Value::as_str) {
            Some("auto" | "none") => true,
            Some("any") => !names.is_empty(),
            Some("tool") => choice
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| names.contains(name)),
            _ => false,
        }
    } else {
        let name = if protocol == Protocol::OpenAiChat {
            choice
                .get("function")
                .and_then(|function| function.get("name"))
        } else {
            choice.get("name")
        };
        choice.get("type").and_then(Value::as_str) == Some("function")
            && name
                .and_then(Value::as_str)
                .is_some_and(|name| names.contains(name))
    };
    if !valid {
        root.insert(
            "tool_choice".to_owned(),
            if protocol == Protocol::Anthropic {
                json!({"type":"auto"})
            } else {
                json!("auto")
            },
        );
    }
}

fn normalize_function(map: &mut Map<String, Value>, key: &str) -> Result<(), ()> {
    optional(
        map,
        &[
            "description",
            "strict",
            "eager_input_streaming",
            "cache_control",
        ],
    );
    scalars(map, &["strict", "eager_input_streaming"], &[]);
    if key == "parameters" {
        alias(map, "input_schema", "parameters")?;
    }
    if !map.contains_key(key) && !map.contains_key("parametersJsonSchema") {
        map.insert(key.to_owned(), json!({"type":"object","properties":{}}));
    }
    if let Some(value) = map.get_mut(key) {
        schema(value);
    }
    Ok(())
}

fn stringify_field(map: &mut Map<String, Value>, key: &str, include_null: bool) {
    if let Some(value) = map.get_mut(key)
        && !value.is_string()
        && (include_null || !value.is_null())
    {
        *value = Value::String(value.to_string());
    }
}

fn normalize_messages(value: &mut Value, protocol: Protocol) -> Result<(), ()> {
    if value.is_object() {
        *value = Value::Array(vec![value.take()]);
    }
    for message in value.as_array_mut().ok_or(())? {
        if message.is_string() {
            *message = json!({"role":"user","content":message.take()});
        }
        normalize_message(object(message)?, protocol)?;
    }
    Ok(())
}

fn normalize_message(map: &mut Map<String, Value>, protocol: Protocol) -> Result<(), ()> {
    alias(map, "text", "content")?;
    if let Some(role) = map.get("role").and_then(Value::as_str).map(str::to_owned) {
        let role = match role.trim().to_ascii_lowercase().as_str() {
            "human" | "user" => "user",
            "ai" | "model" | "assistant" => "assistant",
            "system" => "system",
            "developer" => "developer",
            "tool" => "tool",
            _ => return Err(()),
        };
        map.insert("role".to_owned(), json!(role));
    }
    let tool = map.get("role").and_then(Value::as_str) == Some("tool");
    let assistant = map.get("role").and_then(Value::as_str) == Some("assistant");
    if tool {
        normalize_result(map, "content", protocol)?;
    } else if let Some(content) = map.get_mut("content") {
        if content.is_null() && !(protocol == Protocol::OpenAiChat && assistant) {
            *content = json!("");
        }
        normalize_content(content, protocol, false)?;
    }
    optional(map, &["tool_calls", "reasoning_content", "reasoning"]);
    if let Some(calls) = map.get_mut("tool_calls").and_then(Value::as_array_mut) {
        for call in calls {
            let call = object(call)?;
            call.entry("type").or_insert(json!("function"));
            if let Some(function) = call.get_mut("function").and_then(Value::as_object_mut) {
                stringify_field(function, "arguments", false);
            }
        }
    }
    Ok(())
}

fn normalize_content(value: &mut Value, protocol: Protocol, result: bool) -> Result<(), ()> {
    if value.as_array().is_some_and(Vec::is_empty) {
        *value = json!("");
    }
    if value.is_object() {
        *value = Value::Array(vec![value.take()]);
    }
    if value.is_number() || value.is_boolean() {
        *value = Value::String(value.to_string());
    }
    if let Some(parts) = value.as_array_mut() {
        for part in parts {
            let text_type = if protocol == Protocol::OpenAiResponses {
                "input_text"
            } else {
                "text"
            };
            if part.is_string() {
                *part = json!({"type":text_type,"text":part.take()});
            }
            let map = object(part)?;
            if !map.contains_key("type") && map.contains_key("text") {
                map.insert("type".to_owned(), json!(text_type));
            }
            let kind = map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if matches!(kind.as_str(), "text" | "input_text" | "output_text") {
                if protocol != Protocol::OpenAiResponses {
                    map.insert("type".to_owned(), json!("text"));
                } else if kind == "text" {
                    map.insert("type".to_owned(), json!("input_text"));
                }
                if let Some(text) = map.get_mut("text")
                    && (text.is_number() || text.is_boolean())
                {
                    *text = Value::String(text.to_string());
                }
            }
            if kind == "tool_result" {
                normalize_result(map, "content", protocol)?;
                scalars(map, &["is_error"], &[]);
            }
            if kind == "tool_use"
                && let Some(Value::String(raw)) = map.get("input")
                && let Ok(parsed) = serde_json::from_str::<Value>(raw)
            {
                map.insert("input".to_owned(), parsed);
            }
            optional(map, &["cache_control", "annotations", "logprobs"]);
            if let Some(image) = map.get_mut("image_url")
                && protocol == Protocol::OpenAiChat
                && image.is_string()
            {
                *image = json!({"url":image.take()});
            }
        }
    } else if result && !value.is_string() {
        *value = Value::String(value.to_string());
    }
    Ok(())
}

fn normalize_result(map: &mut Map<String, Value>, key: &str, protocol: Protocol) -> Result<(), ()> {
    if let Some(value) = map.get_mut(key) {
        let blocks = value.as_array().is_some_and(|items| {
            items
                .iter()
                .all(|v| v.as_object().is_some_and(|m| m.contains_key("type")))
        });
        if value.is_object()
            || value.is_array() && !blocks
            || value.is_null()
            || value.is_number()
            || value.is_boolean()
        {
            *value = Value::String(value.to_string());
        } else {
            normalize_content(value, protocol, true)?;
        }
    }
    Ok(())
}

fn legacy_chat(root: &mut Map<String, Value>) -> Result<(), ()> {
    if let Some(functions) = root.remove("functions") {
        let legacy = functions.as_array().ok_or(())?;
        let tools = root.entry("tools").or_insert_with(|| json!([]));
        if tools.is_object() {
            *tools = Value::Array(vec![tools.take()]);
        }
        let tools = tools.as_array_mut().ok_or(())?;
        for function in legacy {
            let name = function.get("name").and_then(Value::as_str);
            if name.is_some()
                && !tools.iter().any(|tool| {
                    tool.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        == name
                })
            {
                tools.push(json!({"type":"function","function":function}));
            }
        }
    }
    if let Some(choice) = root.remove("function_call")
        && root.get("tool_choice").is_none_or(Value::is_null)
    {
        let choice = if choice.is_object() {
            json!({"type":"function","function":choice})
        } else {
            choice
        };
        root.insert("tool_choice".to_owned(), choice);
    }
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let mut reserved = std::collections::HashSet::new();
    for message in messages.iter() {
        if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    reserved.insert(id.to_owned());
                }
            }
        }
    }
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut sequence = 0;
    for message in messages {
        let map = object(message)?;
        if let Some(function) = map.remove("function_call").filter(|v| !v.is_null()) {
            if map.get("tool_calls").is_some_and(|calls| !calls.is_null()) {
                // A modern tool-call list is authoritative over its legacy mirror.
                if let Some(calls) = map.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        if let (Some(name), Some(id)) = (
                            call.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(Value::as_str),
                            call.get("id").and_then(Value::as_str),
                        ) {
                            pending.push((name.to_owned(), id.to_owned()));
                        }
                    }
                }
                continue;
            }
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or(())?
                .to_owned();
            let id = loop {
                let id = format!("legacy_call_{sequence}");
                sequence += 1;
                if reserved.insert(id.clone()) {
                    break id;
                }
            };
            pending.push((name, id.clone()));
            map.insert(
                "tool_calls".to_owned(),
                json!([{"id":id,"type":"function","function":function}]),
            );
        }
        if map.get("role").and_then(Value::as_str) == Some("function") {
            let name = map.get("name").and_then(Value::as_str).ok_or(())?;
            let id = pending
                .iter()
                .position(|(n, _)| n == name)
                .map(|index| pending.remove(index).1);
            let name = name.to_owned();
            map.remove("name");
            map.insert("role".to_owned(), json!("tool"));
            if let Some(id) = id {
                map.insert("tool_call_id".to_owned(), json!(id));
            } else {
                // Preserve the name so the common matcher can resolve or render an orphan.
                map.insert("name".to_owned(), json!(name));
            }
        }
    }
    Ok(())
}

fn normalize_gemini(root: &mut Map<String, Value>) -> Result<(), ()> {
    for (a, b) in [
        ("system_instruction", "systemInstruction"),
        ("generation_config", "generationConfig"),
        ("tool_config", "toolConfig"),
    ] {
        alias(root, a, b)?;
    }
    optional(
        root,
        &["generationConfig", "toolConfig", "systemInstruction"],
    );
    if let Some(config) = root
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    {
        for (a, b) in [
            ("max_output_tokens", "maxOutputTokens"),
            ("top_p", "topP"),
            ("stop_sequences", "stopSequences"),
            ("candidate_count", "candidateCount"),
            ("thinking_config", "thinkingConfig"),
        ] {
            alias(config, a, b)?;
        }
        optional(
            config,
            &[
                "temperature",
                "topP",
                "maxOutputTokens",
                "stopSequences",
                "candidateCount",
                "thinkingConfig",
            ],
        );
        scalars(
            config,
            &[],
            &["temperature", "topP", "maxOutputTokens", "candidateCount"],
        );
        if let Some(thinking) = config
            .get_mut("thinkingConfig")
            .and_then(Value::as_object_mut)
        {
            for (a, b) in [
                ("include_thoughts", "includeThoughts"),
                ("thinking_budget", "thinkingBudget"),
                ("thinking_level", "thinkingLevel"),
            ] {
                alias(thinking, a, b)?;
            }
            optional(
                thinking,
                &["includeThoughts", "thinkingBudget", "thinkingLevel"],
            );
            scalars(thinking, &["includeThoughts"], &["thinkingBudget"]);
            if let Some(level) = thinking
                .get("thinkingLevel")
                .and_then(Value::as_str)
                .map(str::to_ascii_uppercase)
            {
                thinking.insert("thinkingLevel".to_owned(), json!(level));
            }
        }
    }
    for key in ["contents", "systemInstruction"] {
        if let Some(value) = root.get_mut(key) {
            if key == "contents" && value.is_object() {
                *value = Value::Array(vec![value.take()]);
            }
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
                if content.is_string() {
                    *content = json!({"parts":[{"text":content.take()}]});
                }
                let map = object(content)?;
                optional(map, &["role"]);
                if let Some(role) = map
                    .get("role")
                    .and_then(Value::as_str)
                    .map(str::to_ascii_lowercase)
                {
                    map.insert(
                        "role".to_owned(),
                        json!(if role == "assistant" || role == "ai" {
                            "model"
                        } else if role == "human" {
                            "user"
                        } else {
                            &role
                        }),
                    );
                }
                let parts = map.get_mut("parts").ok_or(())?;
                if parts.is_string() {
                    *parts = json!([{"text":parts.take()}]);
                } else if parts.is_object() {
                    *parts = Value::Array(vec![parts.take()]);
                }
                for part in parts.as_array_mut().ok_or(())? {
                    if part.is_string() {
                        *part = json!({"text":part.take()});
                    }
                    let part = object(part)?;
                    for (a, b) in [
                        ("inline_data", "inlineData"),
                        ("function_call", "functionCall"),
                        ("function_response", "functionResponse"),
                        ("thought_signature", "thoughtSignature"),
                    ] {
                        alias(part, a, b)?;
                    }
                    optional(
                        part,
                        &[
                            "thought",
                            "thoughtSignature",
                            "inlineData",
                            "functionCall",
                            "functionResponse",
                        ],
                    );
                    scalars(part, &["thought"], &[]);
                    if let Some(blob) = part.get_mut("inlineData").and_then(Value::as_object_mut) {
                        alias(blob, "mime_type", "mimeType")?;
                    }
                    if let Some(call) = part.get_mut("functionCall").and_then(Value::as_object_mut)
                    {
                        optional(call, &["id", "args"]);
                        if let Some(Value::String(raw)) = call.get("args")
                            && let Ok(parsed) = serde_json::from_str::<Value>(raw)
                        {
                            call.insert("args".to_owned(), parsed);
                        }
                    }
                    if let Some(result) = part
                        .get_mut("functionResponse")
                        .and_then(Value::as_object_mut)
                    {
                        optional(result, &["id"]);
                        if let Some(response) = result.get_mut("response")
                            && !response.is_object()
                        {
                            *response = json!({"result":response.take()});
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
