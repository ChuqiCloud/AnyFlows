use std::{error::Error, fmt};

use serde_json::Value;

/// 绑定 Anthropic Messages 的原生压缩状态。
///
/// 保留原始字段及 null，避免重建时改变已签名内容。它不能冒充其他协议的
/// 压缩状态；只有调用方明确选择普通文本摘要时，才可使用可读的 content。
#[derive(Clone, Eq, PartialEq)]
pub struct AnthropicCompactionItem(Value);

impl AnthropicCompactionItem {
    /// 校验原生压缩块并保留所有扩展字段。
    pub fn from_value(value: Value) -> Result<Self, AnthropicCompactionError> {
        let object = value.as_object().ok_or(AnthropicCompactionError)?;
        crate::bounded_json::validate_object(
            object,
            crate::anthropic::REQUEST_JSON_LIMITS,
            crate::anthropic::MAX_BODY_BYTES,
        )
        .map_err(|_| AnthropicCompactionError)?;
        let valid = value.as_object().is_some_and(|block| {
            block.get("type").and_then(Value::as_str) == Some("compaction")
                && block
                    .get("content")
                    .is_none_or(|v| v.is_null() || v.is_string())
                && ["signature", "encrypted_content"]
                    .iter()
                    .all(|key| block.get(*key).is_none_or(|v| v.is_null() || v.is_string()))
        });
        if valid {
            Ok(Self(value))
        } else {
            Err(AnthropicCompactionError)
        }
    }

    /// 返回完整的原生块，不补写上游未提供的字段。
    #[must_use]
    pub const fn as_value(&self) -> &Value {
        &self.0
    }

    /// 返回可读摘要；null 表示没有生成有效摘要。
    #[must_use]
    pub fn content(&self) -> Option<&str> {
        self.0.get("content").and_then(Value::as_str)
    }

    /// 返回可读摘要。
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.content()
    }

    /// 合并流式摘要增量；只供构造上游正在返回的块使用。
    #[must_use]
    pub fn with_content(mut self, content: Option<String>) -> Self {
        self.0["content"] = content.map_or(Value::Null, Value::String);
        self
    }
}

impl fmt::Debug for AnthropicCompactionItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AnthropicCompactionItem(<已脱敏>)")
    }
}

/// 原生 Anthropic 压缩块的结构不合法。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnthropicCompactionError;

impl fmt::Display for AnthropicCompactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Anthropic compaction block")
    }
}

impl Error for AnthropicCompactionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_blocks_preserve_absent_and_null_content_and_bound_extension_shape() {
        for value in [
            json!({"type":"compaction"}),
            json!({"type":"compaction","content":null,"future":null}),
        ] {
            assert_eq!(
                AnthropicCompactionItem::from_value(value.clone())
                    .unwrap()
                    .as_value(),
                &value
            );
        }
        let mut value = json!({"type":"compaction"});
        let object = value.as_object_mut().unwrap();
        for index in 0..1023 {
            object.insert(format!("extension_{index}"), Value::Null);
        }
        assert!(AnthropicCompactionItem::from_value(value.clone()).is_ok());
        value["one_too_many"] = Value::Null;
        assert!(AnthropicCompactionItem::from_value(value).is_err());
    }
}
