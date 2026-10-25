use serde_json::Value;

use super::ParseRequestError;
use crate::AnthropicCompactionItem;

pub(super) fn parse_item(mut value: Value) -> Result<AnthropicCompactionItem, ParseRequestError> {
    value
        .as_object_mut()
        .ok_or(ParseRequestError::InvalidValue)?
        .insert("type".to_owned(), Value::String("compaction".to_owned()));
    AnthropicCompactionItem::from_value(value).map_err(|_| ParseRequestError::InvalidValue)
}

pub(super) fn validate_controls(
    compaction: Option<&Value>,
    context_management: Option<&Value>,
) -> Result<(), ParseRequestError> {
    if compaction.is_some() && context_management.is_some() {
        return Err(ParseRequestError::InvalidValue);
    }
    if let Some(value) = compaction {
        let object = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
        if object.get("type").and_then(Value::as_str) != Some("summarize")
            || object
                .get("instructions")
                .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(ParseRequestError::InvalidValue);
        }
    }
    if let Some(value) = context_management {
        let object = value.as_object().ok_or(ParseRequestError::InvalidValue)?;
        if let Some(edits) = object.get("edits").filter(|v| !v.is_null()) {
            let edits = edits.as_array().ok_or(ParseRequestError::InvalidValue)?;
            for edit in edits {
                let edit = edit.as_object().ok_or(ParseRequestError::InvalidValue)?;
                let kind = edit
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or(ParseRequestError::InvalidValue)?;
                if kind == "compact_20260112" {
                    if edit
                        .get("instructions")
                        .is_some_and(|v| !v.is_null() && !v.is_string())
                        || edit
                            .get("pause_after_compaction")
                            .is_some_and(|v| !v.is_boolean())
                    {
                        return Err(ParseRequestError::InvalidValue);
                    }
                    if let Some(trigger) = edit.get("trigger").filter(|v| !v.is_null())
                        && (trigger.get("type").and_then(Value::as_str) != Some("input_tokens")
                            || trigger
                                .get("value")
                                .and_then(Value::as_i64)
                                .is_none_or(|v| v <= 0))
                    {
                        return Err(ParseRequestError::InvalidValue);
                    }
                }
            }
        }
    }
    Ok(())
}
