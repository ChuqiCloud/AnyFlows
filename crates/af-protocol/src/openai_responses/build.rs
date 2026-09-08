use std::{collections::HashSet, error::Error, fmt};

use af_domain::{Operation, Protocol};
use serde_json::{Map, Value};

use super::{
    MAX_OUTPUT_TOKENS, ParseRequestError,
    build_input::build_input,
    convert::{
        MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOLS, validate_model, validate_raw_fields,
        validate_schema, validate_tool_name,
    },
};
use crate::{
    CanonicalRequest, ReasoningEffort, ToolChoice, ToolDef, UnsupportedCapability,
    bounded_json::validate_object, validate_request_capabilities,
};

const MAX_USER_ID_BYTES: usize = 512;
const MAX_CONTINUATION_ID_BYTES: usize = 512;
const MAX_PROMPT_CACHE_KEY_BYTES: usize = 256;

/// 将 Canonical 请求构造为 OpenAI Responses JSON。
///
/// Canonical 可由调用方直接构造，因此本函数会重新校验协议预算、工具关联、
/// continuation 互斥关系和同源 raw，未建模能力一律失败关闭。
pub fn build_request(request: &CanonicalRequest) -> Result<Value, BuildRequestError> {
    if request.operation != Operation::Responses {
        return Err(BuildRequestError::UnsupportedOperation);
    }
    validate_request_capabilities(Protocol::OpenAiResponses, request)
        .map_err(BuildRequestError::UnsupportedCapability)?;
    validate_model(&request.model).map_err(map_request_validation_error)?;
    if request.stream_options.include_usage()
        || !request.attachments.is_empty()
        || request.metadata.session_id().is_some()
    {
        return Err(BuildRequestError::UnsupportedFeature);
    }

    let input = build_input(&request.messages, &request.continuation)?;
    let (tools, tool_names) = build_tools(&request.tools)?;
    let tool_choice = build_tool_choice(&request.tool_choice, &tool_names)?;

    let mut root = Map::new();
    root.insert("model".to_owned(), Value::String(request.model.clone()));
    root.insert("input".to_owned(), Value::Array(input));
    root.insert("stream".to_owned(), Value::Bool(request.stream));
    if !tools.is_empty() {
        root.insert("tools".to_owned(), Value::Array(tools));
    }
    root.insert("tool_choice".to_owned(), tool_choice);
    insert_reasoning(&mut root, request)?;
    insert_sampling(&mut root, request)?;
    insert_metadata(&mut root, request)?;
    insert_continuation(&mut root, request)?;
    merge_raw(&mut root, request)?;
    validate_final_request(&root)?;
    Ok(Value::Object(root))
}

fn validate_final_request(root: &Map<String, Value>) -> Result<(), BuildRequestError> {
    validate_object(root, super::REQUEST_JSON_LIMITS, super::MAX_BODY_BYTES)
        .map_err(|_| BuildRequestError::StructureLimitExceeded)
}

fn build_tools(tools: &[ToolDef]) -> Result<(Vec<Value>, HashSet<String>), BuildRequestError> {
    if tools.len() > MAX_TOOLS {
        return Err(BuildRequestError::StructureLimitExceeded);
    }

    let mut encoded = Vec::with_capacity(tools.len());
    let mut names = HashSet::with_capacity(tools.len());
    let mut total_schema_bytes = 0_usize;
    for tool in tools {
        validate_tool_name(&tool.name).map_err(map_request_validation_error)?;
        if !names.insert(tool.name.clone()) {
            return Err(BuildRequestError::InvalidValue);
        }
        if tool
            .description
            .as_ref()
            .is_some_and(|description| description.len() > MAX_TOOL_DESCRIPTION_BYTES)
        {
            return Err(BuildRequestError::StructureLimitExceeded);
        }
        validate_schema(&tool.input_schema, &mut total_schema_bytes)
            .map_err(map_request_validation_error)?;

        let mut function = Map::new();
        function.insert("type".to_owned(), Value::String("function".to_owned()));
        function.insert("name".to_owned(), Value::String(tool.name.clone()));
        if let Some(description) = &tool.description {
            function.insert("description".to_owned(), Value::String(description.clone()));
        }
        function.insert("parameters".to_owned(), tool.input_schema.clone());
        // Responses 的函数工具要求布尔 strict；来源未声明时保持非严格语义。
        function.insert(
            "strict".to_owned(),
            Value::Bool(tool.strict.unwrap_or(false)),
        );
        encoded.push(Value::Object(function));
    }
    Ok((encoded, names))
}

fn build_tool_choice(
    choice: &ToolChoice,
    tool_names: &HashSet<String>,
) -> Result<Value, BuildRequestError> {
    if tool_names.is_empty() && matches!(choice, ToolChoice::Required | ToolChoice::Named { .. }) {
        return Err(BuildRequestError::InvalidValue);
    }

    match choice {
        ToolChoice::Auto => Ok(Value::String("auto".to_owned())),
        ToolChoice::None => Ok(Value::String("none".to_owned())),
        ToolChoice::Required => Ok(Value::String("required".to_owned())),
        ToolChoice::Named { name } => {
            validate_tool_name(name).map_err(map_request_validation_error)?;
            if !tool_names.contains(name) {
                return Err(BuildRequestError::InvalidValue);
            }
            Ok(Value::Object(Map::from_iter([
                ("type".to_owned(), Value::String("function".to_owned())),
                ("name".to_owned(), Value::String(name.clone())),
            ])))
        }
    }
}

fn insert_reasoning(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(reasoning) = request.reasoning else {
        return Ok(());
    };
    if reasoning.budget_tokens().is_some() || reasoning.include_thinking() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    let effort = match reasoning.effort() {
        Some(ReasoningEffort::None) => "none",
        Some(ReasoningEffort::Minimal) => "minimal",
        Some(ReasoningEffort::Low) => "low",
        Some(ReasoningEffort::Medium) => "medium",
        Some(ReasoningEffort::High) => "high",
        Some(ReasoningEffort::ExtraHigh) => "xhigh",
        Some(ReasoningEffort::Max) => "max",
        None => return Err(BuildRequestError::InvalidValue),
    };
    root.insert(
        "reasoning".to_owned(),
        Value::Object(Map::from_iter([(
            "effort".to_owned(),
            Value::String(effort.to_owned()),
        )])),
    );
    Ok(())
}

fn insert_sampling(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    if let Some(temperature) = request.sampling.temperature() {
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "temperature".to_owned(),
            number_value(temperature).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(top_p) = request.sampling.top_p() {
        if !top_p.is_finite() || !(0.0..=1.0).contains(&top_p) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "top_p".to_owned(),
            number_value(top_p).ok_or(BuildRequestError::InvalidValue)?,
        );
    }
    if let Some(max_output_tokens) = request.sampling.max_output_tokens() {
        let max_output_tokens = max_output_tokens.get();
        if !(1..=MAX_OUTPUT_TOKENS).contains(&max_output_tokens) {
            return Err(BuildRequestError::InvalidValue);
        }
        root.insert(
            "max_output_tokens".to_owned(),
            Value::Number(max_output_tokens.into()),
        );
    }
    if !request.sampling.stop_sequences().is_empty() {
        return Err(BuildRequestError::UnsupportedFeature);
    }
    Ok(())
}

fn insert_metadata(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(user_id) = request.metadata.user_id() else {
        return Ok(());
    };
    if user_id.is_empty()
        || user_id.len() > MAX_USER_ID_BYTES
        || user_id.chars().any(char::is_control)
    {
        return Err(BuildRequestError::InvalidValue);
    }
    root.insert("user".to_owned(), Value::String(user_id.to_owned()));
    Ok(())
}

fn insert_continuation(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let previous_response_id = request.continuation.previous_response_id();
    let conversation_id = request.continuation.conversation_id();
    if previous_response_id.is_some() && conversation_id.is_some() {
        return Err(BuildRequestError::FieldConflict);
    }
    if let Some(value) = previous_response_id {
        validate_opaque_id(value, MAX_CONTINUATION_ID_BYTES)?;
        root.insert(
            "previous_response_id".to_owned(),
            Value::String(value.to_owned()),
        );
    }
    if let Some(value) = conversation_id {
        validate_opaque_id(value, MAX_CONTINUATION_ID_BYTES)?;
        root.insert("conversation".to_owned(), Value::String(value.to_owned()));
    }
    if let Some(value) = request.continuation.prompt_cache_key() {
        validate_opaque_id(value, MAX_PROMPT_CACHE_KEY_BYTES)?;
        root.insert(
            "prompt_cache_key".to_owned(),
            Value::String(value.to_owned()),
        );
    }
    Ok(())
}

fn validate_opaque_id(value: &str, max_bytes: usize) -> Result<(), BuildRequestError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(BuildRequestError::InvalidValue);
    }
    Ok(())
}

fn merge_raw(
    root: &mut Map<String, Value>,
    request: &CanonicalRequest,
) -> Result<(), BuildRequestError> {
    let Some(raw) = request.raw_passthrough() else {
        return Ok(());
    };
    let fields = raw
        .fields_for_protocol(Protocol::OpenAiResponses)
        .map_err(|_| BuildRequestError::RawProtocolMismatch)?;
    if fields.keys().any(|key| root.contains_key(key)) {
        return Err(BuildRequestError::FieldConflict);
    }
    validate_raw_fields(fields).map_err(map_request_validation_error)?;
    for (key, value) in fields {
        root.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn number_value(value: f64) -> Option<Value> {
    serde_json::Number::from_f64(value).map(Value::Number)
}

pub(super) fn map_request_validation_error(error: ParseRequestError) -> BuildRequestError {
    match error {
        ParseRequestError::StructureLimitExceeded | ParseRequestError::BodyTooLarge => {
            BuildRequestError::StructureLimitExceeded
        }
        ParseRequestError::UnsupportedFeature => BuildRequestError::UnsupportedFeature,
        ParseRequestError::ConflictingParameters => BuildRequestError::FieldConflict,
        ParseRequestError::InvalidJson
        | ParseRequestError::DuplicateKey
        | ParseRequestError::InvalidValue => BuildRequestError::InvalidValue,
    }
}

/// OpenAI Responses 请求构造错误，不保留 Canonical 中的敏感内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRequestError {
    /// Canonical 操作不是 Responses。
    UnsupportedOperation,
    /// Canonical 字段的取值或关联关系无效。
    InvalidValue,
    /// Canonical 请求超过协议结构预算。
    StructureLimitExceeded,
    /// 目标协议缺少一项已声明的请求能力。
    UnsupportedCapability(UnsupportedCapability),
    /// Canonical 请求使用了 Responses 无法无损表达的特性。
    UnsupportedFeature,
    /// raw 字段来源协议与 Responses 不一致。
    RawProtocolMismatch,
    /// continuation 或 raw 字段与已编码字段发生冲突。
    FieldConflict,
}

impl fmt::Display for BuildRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation => formatter.write_str("请求操作不是 Responses"),
            Self::InvalidValue => formatter.write_str("Canonical 请求字段值无效"),
            Self::StructureLimitExceeded => formatter.write_str("Canonical 请求结构超过限制"),
            Self::UnsupportedCapability(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedFeature => formatter.write_str("请求包含 Responses 不支持的特性"),
            Self::RawProtocolMismatch => formatter.write_str("未归一化字段不得跨协议透传"),
            Self::FieldConflict => formatter.write_str("请求字段相互冲突"),
        }
    }
}

impl Error for BuildRequestError {}
