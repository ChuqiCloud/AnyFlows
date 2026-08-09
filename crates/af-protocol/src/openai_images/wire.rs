use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;

/// 区分字段缺失与显式提供；显式 `null` 在协议边界拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 客户端或上游未提供字段。
    #[default]
    Missing,
    /// 客户端或上游提供了非空字段。
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

/// OpenAI Images 非流式生成请求。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageGenerationRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 文本提示词。
    pub(super) prompt: String,
    /// 可选输出背景。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) background: Field<ImageBackgroundWire>,
    /// 可选内容审核强度。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) moderation: Field<ImageModerationWire>,
    /// 可选生成张数。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) n: Field<u32>,
    /// 可选 JPEG/WebP 压缩率。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_compression: Field<u32>,
    /// 可选输出格式。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_format: Field<ImageOutputFormatWire>,
    /// 流式 partial image 数；首切片拒绝。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) partial_images: Field<u32>,
    /// 可选输出质量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) quality: Field<ImageQualityWire>,
    /// DALL·E 响应载体；GPT Image 首切片拒绝。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) response_format: Field<Value>,
    /// 可选输出尺寸。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) size: Field<String>,
    /// 非流式首切片只接受缺失或 `false`。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream: Field<bool>,
    /// DALL·E 风格参数；首切片拒绝。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) style: Field<Value>,
    /// 上游终端用户标识；首切片拒绝转发。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) user: Field<Value>,
}

/// GPT Image 背景策略 wire。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageBackgroundWire {
    Auto,
    Opaque,
    Transparent,
}

/// GPT Image 审核强度 wire。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageModerationWire {
    Auto,
    Low,
}

/// GPT Image 输出格式 wire。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageOutputFormatWire {
    Png,
    Jpeg,
    Webp,
}

/// GPT Image 输出质量 wire。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageQualityWire {
    Auto,
    Low,
    Medium,
    High,
}

/// OpenAI Images 非流式生成响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageGenerationResponseWire {
    /// Unix 秒级创建时间。
    pub(super) created: i64,
    /// 上游回显的实际背景。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) background: Field<ImageResponseBackgroundWire>,
    /// Base64 图片集合。
    pub(super) data: Vec<ImageDataWire>,
    /// 上游回显的输出格式。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_format: Field<ImageOutputFormatWire>,
    /// 上游回显的实际质量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) quality: Field<ImageResponseQualityWire>,
    /// 上游回显的实际尺寸。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) size: Field<String>,
    /// 上游明确提供的图片 token 用量。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) usage: Field<ImageGenerationUsageWire>,
}

/// 一张 OpenAI Images 响应图片。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageDataWire {
    /// GPT Image 返回的 Base64 图片。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) b64_json: Field<String>,
    /// DALL·E 临时 URL；首切片拒绝。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) url: Field<Value>,
    /// DALL·E 改写提示词；首切片拒绝。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) revised_prompt: Field<Value>,
}

/// Images 响应只允许回显确定背景，不接受 `auto`。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageResponseBackgroundWire {
    Opaque,
    Transparent,
}

/// Images 响应只允许回显确定质量，不接受 `auto`。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ImageResponseQualityWire {
    Low,
    Medium,
    High,
}

/// OpenAI Images token 用量。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageGenerationUsageWire {
    pub(super) input_tokens: i64,
    pub(super) input_tokens_details: ImageTokenDetailsWire,
    pub(super) output_tokens: i64,
    pub(super) total_tokens: i64,
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) output_tokens_details: Field<ImageTokenDetailsWire>,
}

/// OpenAI Images 文本与图片 token 明细。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageTokenDetailsWire {
    pub(super) image_tokens: i64,
    pub(super) text_tokens: i64,
}
