use std::{collections::BTreeMap, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Protocol};
use sea_orm::entity::prelude::Json;
use serde_json::Number;
use thiserror::Error;

use crate::entity::HeaderOverrides;

/// 单渠道允许的精确模型映射数量上限。
pub const MAX_CHANNEL_MODEL_MAPPINGS: usize = 512;
/// 渠道请求策略序列化后的最大字节数。
pub const MAX_CHANNEL_REQUEST_POLICY_BYTES: usize = 64 * 1_024;
/// 当前允许覆盖的 Canonical 参数数量。
pub const MAX_CHANNEL_PARAMETER_OVERRIDES: usize = 4;
/// 单次请求允许配置的停止序列数量上限。
pub const MAX_CHANNEL_STOP_SEQUENCES: usize = 4;
/// 单条停止序列的最大字节数。
pub const MAX_CHANNEL_STOP_SEQUENCE_BYTES: usize = 1_024;
/// 当前协议构造器共同接受的最大输出令牌数。
pub const MAX_CHANNEL_OUTPUT_TOKENS: i64 = 1_000_000;

/// 渠道请求策略违反结构、容量或取值边界。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("渠道请求策略无效")]
pub struct ChannelRequestPolicyError;

/// 校验渠道 Header 覆盖的结构、容量和禁止字段边界。
pub fn validate_channel_header_overrides(value: &Json) -> Result<(), ChannelRequestPolicyError> {
    HeaderOverrides::validate(value.clone())
        .map(|_| ())
        .map_err(|_| ChannelRequestPolicyError)
}

/// 区分大小写的 Canonical 模型到上游模型精确映射。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ChannelModelMappings {
    entries: BTreeMap<String, String>,
}

impl ChannelModelMappings {
    /// 从持久化 JSON 对象解析并校验精确映射。
    pub fn parse(value: &Json) -> Result<Self, ChannelRequestPolicyError> {
        let Json::Object(object) = value else {
            return Err(ChannelRequestPolicyError);
        };
        if object.len() > MAX_CHANNEL_MODEL_MAPPINGS || encoded_too_large(value) {
            return Err(ChannelRequestPolicyError);
        }

        let mut entries = BTreeMap::new();
        for (source, target) in object {
            let Some(target) = target.as_str() else {
                return Err(ChannelRequestPolicyError);
            };
            if !valid_model(source) || !valid_model(target) {
                return Err(ChannelRequestPolicyError);
            }
            entries.insert(source.clone(), target.to_owned());
        }
        Ok(Self { entries })
    }

    /// 返回请求模型命中的上游模型；未命中时为空。
    #[must_use]
    pub fn resolve(&self, requested_model: &str) -> Option<&str> {
        self.entries.get(requested_model).map(String::as_str)
    }

    /// 返回精确映射数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 判断映射是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 为受控 Redis 投影还原已验证的持久化 JSON 形状。
    pub(crate) fn to_projection_json(&self) -> Json {
        Json::Object(
            self.entries
                .iter()
                .map(|(source, target)| (source.clone(), Json::String(target.clone())))
                .collect(),
        )
    }
}

impl fmt::Debug for ChannelModelMappings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelModelMappings")
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

/// 已验证的 Canonical 采样参数覆盖。
///
/// `max_output_tokens` 表示渠道上限，应用时只能收紧客户端值，不能提高预扣上界。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ChannelParameterOverrides {
    temperature: Option<Number>,
    top_p: Option<Number>,
    max_output_tokens: Option<i64>,
    stop_sequences: Option<Vec<String>>,
}

impl ChannelParameterOverrides {
    /// 从持久化 JSON 对象解析当前支持的闭合参数集合。
    pub fn parse(value: &Json) -> Result<Self, ChannelRequestPolicyError> {
        let Json::Object(object) = value else {
            return Err(ChannelRequestPolicyError);
        };
        if object.len() > MAX_CHANNEL_PARAMETER_OVERRIDES || encoded_too_large(value) {
            return Err(ChannelRequestPolicyError);
        }

        let mut parsed = Self::default();
        for (name, value) in object {
            match name.as_str() {
                "temperature" => {
                    parsed.temperature = Some(valid_number(value, 0.0, 2.0)?);
                }
                "top_p" => {
                    parsed.top_p = Some(valid_number(value, 0.0, 1.0)?);
                }
                "max_output_tokens" => {
                    let Some(tokens) = value.as_i64() else {
                        return Err(ChannelRequestPolicyError);
                    };
                    if !(1..=MAX_CHANNEL_OUTPUT_TOKENS).contains(&tokens) {
                        return Err(ChannelRequestPolicyError);
                    }
                    parsed.max_output_tokens = Some(tokens);
                }
                "stop_sequences" => {
                    parsed.stop_sequences = Some(parse_stop_sequences(value)?);
                }
                _ => return Err(ChannelRequestPolicyError),
            }
        }
        Ok(parsed)
    }

    /// 按目标协议收紧通用参数集合，防止合法 JSON 在协议构造阶段才暴露配置错误。
    pub fn validate_for_protocol(
        &self,
        protocol: Protocol,
    ) -> Result<(), ChannelRequestPolicyError> {
        if matches!(
            protocol,
            Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
                | Protocol::OpenAiAudio
                | Protocol::OpenAiSpeech
                | Protocol::JinaRerank
                | Protocol::CohereRerank
                | Protocol::XaiVideo
        ) && !self.is_empty()
        {
            return Err(ChannelRequestPolicyError);
        }
        if protocol == Protocol::OpenAiResponses && self.stop_sequences.is_some() {
            return Err(ChannelRequestPolicyError);
        }
        if protocol == Protocol::Anthropic
            && self
                .temperature()
                .is_some_and(|temperature| temperature > 1.0)
        {
            return Err(ChannelRequestPolicyError);
        }
        Ok(())
    }

    /// 返回强制 temperature；未配置时保留客户端值。
    #[must_use]
    pub fn temperature(&self) -> Option<f64> {
        self.temperature.as_ref().and_then(Number::as_f64)
    }

    /// 返回强制 top_p；未配置时保留客户端值。
    #[must_use]
    pub fn top_p(&self) -> Option<f64> {
        self.top_p.as_ref().and_then(Number::as_f64)
    }

    /// 返回最大输出令牌上限；应用时必须与客户端值取较小者。
    #[must_use]
    pub const fn max_output_tokens(&self) -> Option<i64> {
        self.max_output_tokens
    }

    /// 返回强制停止序列；内容不得写入日志。
    #[must_use]
    pub fn stop_sequences(&self) -> Option<&[String]> {
        self.stop_sequences.as_deref()
    }

    /// 判断是否没有参数覆盖。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.max_output_tokens.is_none()
            && self.stop_sequences.is_none()
    }

    /// 为受控 Redis 投影还原已验证的持久化 JSON 形状。
    pub(super) fn to_projection_json(&self) -> Json {
        let mut object = serde_json::Map::new();
        if let Some(value) = &self.temperature {
            object.insert("temperature".to_owned(), Json::Number(value.clone()));
        }
        if let Some(value) = &self.top_p {
            object.insert("top_p".to_owned(), Json::Number(value.clone()));
        }
        if let Some(value) = self.max_output_tokens {
            object.insert("max_output_tokens".to_owned(), Json::Number(value.into()));
        }
        if let Some(values) = &self.stop_sequences {
            object.insert(
                "stop_sequences".to_owned(),
                Json::Array(values.iter().cloned().map(Json::String).collect()),
            );
        }
        Json::Object(object)
    }
}

impl fmt::Debug for ChannelParameterOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelParameterOverrides")
            .field("has_temperature", &self.temperature.is_some())
            .field("has_top_p", &self.top_p.is_some())
            .field("has_max_output_tokens", &self.max_output_tokens.is_some())
            .field("has_stop_sequences", &self.stop_sequences.is_some())
            .finish()
    }
}

fn valid_number(
    value: &Json,
    minimum: f64,
    maximum: f64,
) -> Result<Number, ChannelRequestPolicyError> {
    let Json::Number(number) = value else {
        return Err(ChannelRequestPolicyError);
    };
    let Some(value) = number.as_f64() else {
        return Err(ChannelRequestPolicyError);
    };
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(ChannelRequestPolicyError);
    }
    Ok(number.clone())
}

fn parse_stop_sequences(value: &Json) -> Result<Vec<String>, ChannelRequestPolicyError> {
    let Json::Array(values) = value else {
        return Err(ChannelRequestPolicyError);
    };
    if values.is_empty() || values.len() > MAX_CHANNEL_STOP_SEQUENCES {
        return Err(ChannelRequestPolicyError);
    }
    values
        .iter()
        .map(|value| {
            let Some(value) = value.as_str() else {
                return Err(ChannelRequestPolicyError);
            };
            if value.is_empty() || value.len() > MAX_CHANNEL_STOP_SEQUENCE_BYTES {
                return Err(ChannelRequestPolicyError);
            }
            Ok(value.to_owned())
        })
        .collect()
}

fn encoded_too_large(value: &Json) -> bool {
    serde_json::to_vec(value)
        .ok()
        .is_none_or(|encoded| encoded.len() > MAX_CHANNEL_REQUEST_POLICY_BYTES)
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn exact_model_mapping_is_bounded_and_redacted() {
        let mappings = ChannelModelMappings::parse(&json!({
            "public-model-canary": "upstream-model-canary"
        }))
        .unwrap();

        assert_eq!(
            mappings.resolve("public-model-canary"),
            Some("upstream-model-canary")
        );
        assert_eq!(mappings.resolve("PUBLIC-model-canary"), None);
        let rendered = format!("{mappings:?}");
        assert!(rendered.contains("entry_count"));
        assert!(!rendered.contains("model-canary"));

        for invalid in [
            json!(null),
            json!({"": "upstream"}),
            json!({"public": " upstream"}),
            json!({"public": 1}),
        ] {
            assert!(ChannelModelMappings::parse(&invalid).is_err());
        }
    }

    #[test]
    fn parameter_overrides_accept_only_closed_canonical_sampling_fields() {
        let overrides = ChannelParameterOverrides::parse(&json!({
            "temperature": 0.25,
            "top_p": 0.8,
            "max_output_tokens": 4096,
            "stop_sequences": ["private-stop-canary"]
        }))
        .unwrap();

        assert_eq!(overrides.temperature(), Some(0.25));
        assert_eq!(overrides.top_p(), Some(0.8));
        assert_eq!(overrides.max_output_tokens(), Some(4096));
        let stop_sequences = overrides.stop_sequences().unwrap();
        assert_eq!(stop_sequences.len(), 1);
        assert_eq!(stop_sequences[0], "private-stop-canary");
        let rendered = format!("{overrides:?}");
        assert!(rendered.contains("has_stop_sequences"));
        assert!(!rendered.contains("private-stop-canary"));

        for invalid in [
            json!(null),
            json!({"temperature": -0.1}),
            json!({"temperature": 2.1}),
            json!({"top_p": 1.1}),
            json!({"max_output_tokens": 0}),
            json!({"max_output_tokens": 1.5}),
            json!({"stop_sequences": []}),
            json!({"stop_sequences": [1]}),
            json!({"messages": []}),
            json!({"model": "forbidden"}),
        ] {
            assert!(ChannelParameterOverrides::parse(&invalid).is_err());
        }

        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiChat)
                .is_ok()
        );
        assert!(overrides.validate_for_protocol(Protocol::Gemini).is_ok());
        assert!(overrides.validate_for_protocol(Protocol::Anthropic).is_ok());
        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiEmbeddings)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiImages)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiAudio)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiSpeech)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::JinaRerank)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::CohereRerank)
                .is_err()
        );
        assert!(
            overrides
                .validate_for_protocol(Protocol::OpenAiResponses)
                .is_err()
        );
        let hot = ChannelParameterOverrides::parse(&json!({"temperature": 1.5})).unwrap();
        assert!(hot.validate_for_protocol(Protocol::OpenAiChat).is_ok());
        assert!(hot.validate_for_protocol(Protocol::Anthropic).is_err());
    }

    #[test]
    fn public_header_validation_reuses_sensitive_entity_boundary() {
        assert!(
            validate_channel_header_overrides(&json!({
                "x-provider-feature": "enabled"
            }))
            .is_ok()
        );
        for invalid in [
            json!({"content-type": "text/plain"}),
            json!({"anthropic-version": "private-version"}),
            json!({"x-request-id": "untrusted"}),
            json!({"authorization": "secret"}),
        ] {
            assert!(validate_channel_header_overrides(&invalid).is_err());
        }
    }
}
