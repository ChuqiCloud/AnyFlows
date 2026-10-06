use serde_json::{Map, Value};

use super::MAX_BODY_BYTES;
use super::error::{
    BuildResponsesCompactionRequestError, BuildResponsesCompactionResponseError,
    ParseResponsesCompactionRequestError, ParseResponsesCompactionResponseError,
};
use super::model::{
    CanonicalResponsesCompactionRequest, CanonicalResponsesCompactionResponse,
    ResponsesCompactionInput,
};
use super::validation::{
    JSON_LIMITS, ensure_keys, item_kind, map_item_error, map_item_error_response, map_usage_error,
    optional_string, parse_input, parse_items, parse_usage, required_string, validate_model,
    validate_opaque_id, validate_text,
};
use crate::bounded_json::{self, BoundedJsonError};

/// 解析 OpenAI `/v1/responses/compact` 请求。
pub fn parse_request(
    body: &[u8],
) -> Result<CanonicalResponsesCompactionRequest, ParseResponsesCompactionRequestError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseResponsesCompactionRequestError::BodyTooLarge);
    }
    let value = bounded_json::parse_value(body, JSON_LIMITS).map_err(map_json_error)?;
    parse_request_value(value)
}

/// 将 Canonical 压缩请求构造成 OpenAI wire JSON，并重新经过入站校验。
pub fn build_request(
    request: &CanonicalResponsesCompactionRequest,
) -> Result<Value, BuildResponsesCompactionRequestError> {
    let mut root = Map::new();
    root.insert("model".to_owned(), Value::String(request.model.clone()));
    if let Some(input) = &request.input {
        let value = match input {
            ResponsesCompactionInput::Text(text) => Value::String(text.clone()),
            ResponsesCompactionInput::Items(items) => {
                Value::Array(items.iter().map(|item| item.0.clone()).collect())
            }
        };
        root.insert("input".to_owned(), value);
    }
    if let Some(instructions) = &request.instructions {
        root.insert(
            "instructions".to_owned(),
            Value::String(instructions.clone()),
        );
    }
    if let Some(id) = &request.previous_response_id {
        root.insert("previous_response_id".to_owned(), Value::String(id.clone()));
    }
    let value = Value::Object(root);
    let body = serde_json::to_vec(&value)
        .map_err(|_| BuildResponsesCompactionRequestError::InvalidValue)?;
    parse_request(&body).map_err(|_| BuildResponsesCompactionRequestError::InvalidValue)?;
    Ok(value)
}

/// 解析 OpenAI `/v1/responses/compact` 非流式响应。
pub fn parse_response(
    body: &[u8],
) -> Result<CanonicalResponsesCompactionResponse, ParseResponsesCompactionResponseError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ParseResponsesCompactionResponseError::BodyTooLarge);
    }
    let value = bounded_json::parse_value(body, JSON_LIMITS).map_err(map_response_json_error)?;
    parse_response_value(value)
}

/// 将 Canonical 压缩响应构造成完整 wire JSON，并重新校验窗口和 usage。
pub fn build_response(
    response: &CanonicalResponsesCompactionResponse,
) -> Result<Value, BuildResponsesCompactionResponseError> {
    let usage = response.usage;
    let input_details = Map::from_iter([
        (
            "cached_tokens".to_owned(),
            Value::Number(usage.cached_tokens().get().into()),
        ),
        (
            "cache_write_tokens".to_owned(),
            Value::Number(usage.cache_write_tokens().get().into()),
        ),
    ]);
    let output_details = Map::from_iter([(
        "reasoning_tokens".to_owned(),
        Value::Number(usage.reasoning_tokens().get().into()),
    )]);
    let value = Value::Object(Map::from_iter([
        ("id".to_owned(), Value::String(response.id.clone())),
        (
            "created_at".to_owned(),
            Value::Number(response.created_at.into()),
        ),
        (
            "object".to_owned(),
            Value::String("response.compaction".to_owned()),
        ),
        (
            "output".to_owned(),
            Value::Array(response.output.iter().map(|item| item.0.clone()).collect()),
        ),
        (
            "usage".to_owned(),
            Value::Object(Map::from_iter([
                (
                    "input_tokens".to_owned(),
                    Value::Number(usage.input_tokens().get().into()),
                ),
                (
                    "input_tokens_details".to_owned(),
                    Value::Object(input_details),
                ),
                (
                    "output_tokens".to_owned(),
                    Value::Number(usage.output_tokens().get().into()),
                ),
                (
                    "output_tokens_details".to_owned(),
                    Value::Object(output_details),
                ),
                (
                    "total_tokens".to_owned(),
                    Value::Number(usage.total_tokens().get().into()),
                ),
            ])),
        ),
    ]));
    let body = serde_json::to_vec(&value)
        .map_err(|_| BuildResponsesCompactionResponseError::InvalidValue)?;
    parse_response(&body).map_err(|_| BuildResponsesCompactionResponseError::InvalidValue)?;
    Ok(value)
}

fn parse_request_value(
    value: Value,
) -> Result<CanonicalResponsesCompactionRequest, ParseResponsesCompactionRequestError> {
    let object = value
        .as_object()
        .ok_or(ParseResponsesCompactionRequestError::InvalidValue)?;
    ensure_keys(
        object,
        &["model", "input", "instructions", "previous_response_id"],
    )
    .map_err(map_item_error)?;
    let model = required_string(object, "model").map_err(map_item_error)?;
    validate_model(model).map_err(map_item_error)?;
    let input = object
        .get("input")
        .map(parse_input)
        .transpose()
        .map_err(map_item_error)?;
    let instructions = optional_string(object, "instructions")
        .map_err(map_item_error)?
        .map(str::to_owned);
    if let Some(value) = instructions.as_deref() {
        validate_text(value).map_err(map_item_error)?;
    }
    let previous_response_id = optional_string(object, "previous_response_id")
        .map_err(map_item_error)?
        .map(str::to_owned);
    if let Some(value) = previous_response_id.as_deref() {
        validate_opaque_id(value).map_err(map_item_error)?;
    }
    CanonicalResponsesCompactionRequest::new(
        model.to_owned(),
        input,
        instructions,
        previous_response_id,
    )
    .map_err(|_| ParseResponsesCompactionRequestError::InvalidValue)
}

fn parse_response_value(
    value: Value,
) -> Result<CanonicalResponsesCompactionResponse, ParseResponsesCompactionResponseError> {
    let object = value
        .as_object()
        .ok_or(ParseResponsesCompactionResponseError::InvalidValue)?;
    ensure_keys(object, &["id", "created_at", "object", "output", "usage"])
        .map_err(map_item_error_response)?;
    let id = required_string(object, "id").map_err(map_item_error_response)?;
    validate_opaque_id(id).map_err(map_item_error_response)?;
    let created_at = object
        .get("created_at")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .ok_or(ParseResponsesCompactionResponseError::InvalidValue)?;
    if object.get("object").and_then(Value::as_str) != Some("response.compaction") {
        return Err(ParseResponsesCompactionResponseError::InvalidValue);
    }
    let output = object
        .get("output")
        .and_then(Value::as_array)
        .ok_or(ParseResponsesCompactionResponseError::InvalidValue)?;
    let output = parse_items(output).map_err(map_item_error_response)?;
    if output.is_empty()
        || !output
            .iter()
            .any(|item| item_kind(item.as_value()) == Some("compaction"))
    {
        return Err(ParseResponsesCompactionResponseError::InvalidValue);
    }
    let usage = parse_usage(object.get("usage")).map_err(map_usage_error)?;
    Ok(CanonicalResponsesCompactionResponse {
        id: id.to_owned(),
        created_at,
        output,
        usage,
    })
}

fn map_json_error(error: BoundedJsonError) -> ParseResponsesCompactionRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseResponsesCompactionRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseResponsesCompactionRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => {
            ParseResponsesCompactionRequestError::StructureLimitExceeded
        }
    }
}

fn map_response_json_error(error: BoundedJsonError) -> ParseResponsesCompactionResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseResponsesCompactionResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseResponsesCompactionResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => {
            ParseResponsesCompactionResponseError::StructureLimitExceeded
        }
    }
}
