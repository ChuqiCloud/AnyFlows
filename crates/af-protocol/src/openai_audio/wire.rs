use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Number, Value};

/// 区分字段缺失与显式提供；显式 `null` 在协议边界拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 上游未提供字段。
    #[default]
    Missing,
    /// 上游提供了非空字段。
    Value(T),
}

pub(super) fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// OpenAI Audio 基础 JSON 转录响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TranscriptionResponseWire {
    /// 完整转录文本。
    pub(super) text: String,
    /// `gpt-transcribe` 返回的可选检测语言。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) languages: Field<Vec<TranscriptionLanguageWire>>,
    /// logprobs 尚未进入 Canonical，出现时必须失败关闭。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) logprobs: Field<Value>,
    /// token 或时长联合用量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) usage: Field<TranscriptionUsageWire>,
}

/// OpenAI 返回的检测语言对象。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TranscriptionLanguageWire {
    pub(super) code: String,
}

/// OpenAI Audio token 或时长用量。
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum TranscriptionUsageWire {
    /// 按 token 计费的模型用量。
    #[serde(rename = "tokens")]
    Tokens {
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        #[serde(default, deserialize_with = "deserialize_field")]
        input_token_details: Field<AudioInputTokenDetailsWire>,
    },
    /// 按输入音频时长计费的模型用量。
    #[serde(rename = "duration")]
    Duration { seconds: Number },
}

/// OpenAI Audio 可选输入 token 明细。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AudioInputTokenDetailsWire {
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) audio_tokens: Field<i64>,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) text_tokens: Field<i64>,
}
