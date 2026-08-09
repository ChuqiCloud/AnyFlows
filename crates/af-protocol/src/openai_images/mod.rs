//! OpenAI Images 非流式生成协议转换。
//!
//! 首切片只处理 GPT Image 的 JSON 生成入口和 Base64 完整响应。编辑、变体、URL
//! 响应、partial image 流式事件、生产调度与计费由后续独立切片接入。

use std::{error::Error, fmt};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use serde_json::{Map, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    CanonicalImageGenerationRequest, CanonicalImageGenerationRequestError,
    CanonicalImageGenerationResponse, GeneratedImage, ImageBackground, ImageCompression,
    ImageCount, ImageGenerationOptions, ImageGenerationUsage, ImageModeration, ImageOutputFormat,
    ImageQuality, ImageSize, ImageTokenBreakdown, MAX_GENERATED_IMAGE_BYTES,
    MAX_IMAGE_GENERATION_COUNT, MAX_TOTAL_GENERATED_IMAGE_BYTES, TokenCount,
};

mod wire;

use wire::{
    Field, ImageBackgroundWire, ImageDataWire, ImageGenerationRequestWire,
    ImageGenerationResponseWire, ImageGenerationUsageWire, ImageModerationWire,
    ImageOutputFormatWire, ImageQualityWire, ImageResponseBackgroundWire, ImageResponseQualityWire,
    ImageTokenDetailsWire,
};

/// OpenAI Images 请求正文上限。
pub const MAX_REQUEST_BODY_BYTES: usize = 256 * 1_024;
/// OpenAI Images 非流式响应正文上限。
pub const MAX_RESPONSE_BODY_BYTES: usize = 192 * 1_024 * 1_024;

const MAX_ENCODED_IMAGE_BYTES: usize = MAX_GENERATED_IMAGE_BYTES.div_ceil(3) * 4;

const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 4,
    max_nodes: 64,
    max_object_entries: 16,
    max_array_items: MAX_IMAGE_GENERATION_COUNT,
    max_string_bytes: MAX_REQUEST_BODY_BYTES,
    max_key_bytes: 128,
};

const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 6,
    max_nodes: 256,
    max_object_entries: 16,
    max_array_items: MAX_IMAGE_GENERATION_COUNT,
    max_string_bytes: MAX_RESPONSE_BODY_BYTES,
    max_key_bytes: 128,
};

/// 解析 OpenAI `POST /v1/images/generations` 非流式请求。
///
/// 该函数拒绝重复键、未知字段、显式空值、DALL·E 私有参数和流式特性；提示词、模型名
/// 与所有用户可控乘数在进入 Canonical 前完成预算校验。
pub fn parse_request(
    body: &[u8],
) -> Result<CanonicalImageGenerationRequest, ParseImageGenerationRequestError> {
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return Err(ParseImageGenerationRequestError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_request_json_error)?;
    let wire = serde_json::from_value(value)
        .map_err(|_| ParseImageGenerationRequestError::InvalidValue)?;
    convert_request(wire)
}

/// 将 Canonical 图片生成请求构造成 OpenAI wire JSON。
///
/// 仅输出调用方显式提供的可选参数，避免给兼容上游注入其不认识的默认字段；构造结果会
/// 重新通过入站解析器，防止程序化请求绕过关联和大小预算。
pub fn build_request(
    request: &CanonicalImageGenerationRequest,
) -> Result<Value, BuildImageGenerationRequestError> {
    let mut root = Map::from_iter([
        (
            "model".to_owned(),
            Value::String(request.model().to_owned()),
        ),
        (
            "prompt".to_owned(),
            Value::String(request.prompt().to_owned()),
        ),
    ]);
    let options = request.options();
    if let Some(count) = options.count() {
        root.insert("n".to_owned(), Value::Number(count.get().into()));
    }
    if let Some(size) = options.size() {
        root.insert("size".to_owned(), Value::String(size.to_string()));
    }
    if let Some(quality) = options.quality() {
        root.insert(
            "quality".to_owned(),
            Value::String(quality.as_str().to_owned()),
        );
    }
    if let Some(background) = options.background() {
        root.insert(
            "background".to_owned(),
            Value::String(background.as_str().to_owned()),
        );
    }
    if let Some(moderation) = options.moderation() {
        root.insert(
            "moderation".to_owned(),
            Value::String(moderation.as_str().to_owned()),
        );
    }
    if let Some(format) = options.output_format() {
        root.insert(
            "output_format".to_owned(),
            Value::String(format.as_str().to_owned()),
        );
    }
    if let Some(compression) = options.compression() {
        root.insert(
            "output_compression".to_owned(),
            Value::Number(compression.get().into()),
        );
    }

    let value = Value::Object(root);
    revalidate_request(&value)?;
    Ok(value)
}

/// 解析已完整读取的 OpenAI Images 非流式响应。
///
/// Base64 会立即解码并校验文件签名和内存预算；URL、改写提示词、混合格式及不闭合的
/// token 用量均在生成 Canonical 前拒绝。
pub fn parse_response(
    body: &[u8],
) -> Result<CanonicalImageGenerationResponse, ParseImageGenerationResponseError> {
    if body.len() > MAX_RESPONSE_BODY_BYTES {
        return Err(ParseImageGenerationResponseError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_response_json_error)?;
    let wire = serde_json::from_value(value)
        .map_err(|_| ParseImageGenerationResponseError::InvalidValue)?;
    convert_response(wire)
}

/// 将 Canonical 图片生成响应构造成 OpenAI wire JSON。
///
/// 图片字节统一编码为标准 Base64，绝不生成临时 URL；输出重新经过响应解析器校验。
pub fn build_response(
    response: &CanonicalImageGenerationResponse,
) -> Result<Value, BuildImageGenerationResponseError> {
    let data = response
        .images()
        .iter()
        .map(|image| {
            Value::Object(Map::from_iter([(
                "b64_json".to_owned(),
                Value::String(STANDARD.encode(image.bytes())),
            )]))
        })
        .collect::<Vec<_>>();
    let mut root = Map::from_iter([
        (
            "created".to_owned(),
            Value::Number(response.created().into()),
        ),
        ("data".to_owned(), Value::Array(data)),
        (
            "output_format".to_owned(),
            Value::String(response.output_format().as_str().to_owned()),
        ),
    ]);
    if let Some(background) = response.background() {
        root.insert(
            "background".to_owned(),
            Value::String(background.as_str().to_owned()),
        );
    }
    if let Some(quality) = response.quality() {
        root.insert(
            "quality".to_owned(),
            Value::String(quality.as_str().to_owned()),
        );
    }
    if let Some(size) = response.size() {
        root.insert("size".to_owned(), Value::String(size.to_string()));
    }
    if let Some(usage) = response.usage() {
        root.insert("usage".to_owned(), build_usage(usage)?);
    }

    let value = Value::Object(root);
    revalidate_response(&value)?;
    Ok(value)
}

fn convert_request(
    wire: ImageGenerationRequestWire,
) -> Result<CanonicalImageGenerationRequest, ParseImageGenerationRequestError> {
    if matches!(wire.stream, Field::Value(true))
        || !matches!(wire.partial_images, Field::Missing)
        || !matches!(wire.response_format, Field::Missing)
        || !matches!(wire.style, Field::Missing)
        || !matches!(wire.user, Field::Missing)
    {
        return Err(ParseImageGenerationRequestError::UnsupportedFeature);
    }

    let count = match wire.n {
        Field::Missing => None,
        Field::Value(value) => Some(
            ImageCount::new(value).map_err(|_| ParseImageGenerationRequestError::InvalidValue)?,
        ),
    };
    let size = match wire.size {
        Field::Missing => None,
        Field::Value(value) => Some(
            ImageSize::parse(&value).map_err(|_| ParseImageGenerationRequestError::InvalidValue)?,
        ),
    };
    let quality = match wire.quality {
        Field::Missing => None,
        Field::Value(value) => Some(map_quality(value)),
    };
    let background = match wire.background {
        Field::Missing => None,
        Field::Value(value) => Some(map_background(value)),
    };
    let moderation = match wire.moderation {
        Field::Missing => None,
        Field::Value(value) => Some(map_moderation(value)),
    };
    let output_format = match wire.output_format {
        Field::Missing => None,
        Field::Value(value) => Some(map_output_format(value)),
    };
    let compression = match wire.output_compression {
        Field::Missing => None,
        Field::Value(value) => Some(
            ImageCompression::new(value)
                .map_err(|_| ParseImageGenerationRequestError::InvalidValue)?,
        ),
    };
    let options = ImageGenerationOptions::new(
        count,
        size,
        quality,
        background,
        moderation,
        output_format,
        compression,
    )
    .map_err(map_request_error)?;
    CanonicalImageGenerationRequest::new(wire.model, wire.prompt, options)
        .map_err(map_request_error)
}

fn convert_response(
    wire: ImageGenerationResponseWire,
) -> Result<CanonicalImageGenerationResponse, ParseImageGenerationResponseError> {
    let created =
        u64::try_from(wire.created).map_err(|_| ParseImageGenerationResponseError::InvalidValue)?;
    let mut images = Vec::with_capacity(wire.data.len());
    let mut decoded_bytes = 0_usize;
    for item in wire.data {
        let image = convert_image(item)?;
        decoded_bytes = decoded_bytes
            .checked_add(image.bytes().len())
            .ok_or(ParseImageGenerationResponseError::StructureLimitExceeded)?;
        if decoded_bytes > MAX_TOTAL_GENERATED_IMAGE_BYTES {
            return Err(ParseImageGenerationResponseError::StructureLimitExceeded);
        }
        images.push(image);
    }

    if let Field::Value(format) = wire.output_format {
        let format = map_output_format(format);
        if images.iter().any(|image| image.format() != format) {
            return Err(ParseImageGenerationResponseError::InvalidValue);
        }
    }
    let background = match wire.background {
        Field::Missing => None,
        Field::Value(ImageResponseBackgroundWire::Opaque) => Some(ImageBackground::Opaque),
        Field::Value(ImageResponseBackgroundWire::Transparent) => {
            Some(ImageBackground::Transparent)
        }
    };
    let quality = match wire.quality {
        Field::Missing => None,
        Field::Value(ImageResponseQualityWire::Low) => Some(ImageQuality::Low),
        Field::Value(ImageResponseQualityWire::Medium) => Some(ImageQuality::Medium),
        Field::Value(ImageResponseQualityWire::High) => Some(ImageQuality::High),
    };
    let size = match wire.size {
        Field::Missing => None,
        Field::Value(value) => match ImageSize::parse(&value)
            .map_err(|_| ParseImageGenerationResponseError::InvalidValue)?
        {
            ImageSize::Auto => return Err(ParseImageGenerationResponseError::InvalidValue),
            ImageSize::Exact(dimensions) => Some(dimensions),
        },
    };
    let usage = match wire.usage {
        Field::Missing => None,
        Field::Value(usage) => Some(convert_usage(usage)?),
    };

    CanonicalImageGenerationResponse::new(created, images, background, quality, size, usage)
        .map_err(|_| ParseImageGenerationResponseError::InvalidValue)
}

fn convert_image(item: ImageDataWire) -> Result<GeneratedImage, ParseImageGenerationResponseError> {
    if !matches!(item.url, Field::Missing) || !matches!(item.revised_prompt, Field::Missing) {
        return Err(ParseImageGenerationResponseError::UnsupportedFeature);
    }
    let Field::Value(encoded) = item.b64_json else {
        return Err(ParseImageGenerationResponseError::InvalidValue);
    };
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_IMAGE_BYTES {
        return Err(ParseImageGenerationResponseError::StructureLimitExceeded);
    }
    let bytes = STANDARD
        .decode(&encoded)
        .or_else(|_| STANDARD_NO_PAD.decode(&encoded))
        .map_err(|_| ParseImageGenerationResponseError::InvalidValue)?;
    GeneratedImage::new(bytes).map_err(|_| ParseImageGenerationResponseError::InvalidValue)
}

fn convert_usage(
    usage: ImageGenerationUsageWire,
) -> Result<ImageGenerationUsage, ParseImageGenerationResponseError> {
    let input_tokens = parse_token_count(usage.input_tokens)?;
    let output_tokens = parse_token_count(usage.output_tokens)?;
    let total_tokens = parse_token_count(usage.total_tokens)?;
    let input_details = convert_token_details(usage.input_tokens_details)?;
    let output_details = match usage.output_tokens_details {
        Field::Missing => None,
        Field::Value(details) => Some(convert_token_details(details)?),
    };
    let canonical =
        ImageGenerationUsage::new(input_tokens, input_details, output_tokens, output_details)
            .map_err(|_| ParseImageGenerationResponseError::InvalidValue)?;
    if canonical
        .checked_total_tokens()
        .map_err(|_| ParseImageGenerationResponseError::InvalidValue)?
        != total_tokens
    {
        return Err(ParseImageGenerationResponseError::InvalidValue);
    }
    Ok(canonical)
}

fn convert_token_details(
    details: ImageTokenDetailsWire,
) -> Result<ImageTokenBreakdown, ParseImageGenerationResponseError> {
    Ok(ImageTokenBreakdown::new(
        parse_token_count(details.text_tokens)?,
        parse_token_count(details.image_tokens)?,
    ))
}

fn parse_token_count(value: i64) -> Result<TokenCount, ParseImageGenerationResponseError> {
    TokenCount::new(value).map_err(|_| ParseImageGenerationResponseError::InvalidValue)
}

fn build_usage(usage: ImageGenerationUsage) -> Result<Value, BuildImageGenerationResponseError> {
    let total = usage
        .checked_total_tokens()
        .map_err(|_| BuildImageGenerationResponseError::InvalidValue)?;
    let mut object = Map::from_iter([
        (
            "input_tokens".to_owned(),
            Value::Number(usage.input_tokens().get().into()),
        ),
        (
            "input_tokens_details".to_owned(),
            build_token_details(usage.input_details()),
        ),
        (
            "output_tokens".to_owned(),
            Value::Number(usage.output_tokens().get().into()),
        ),
        ("total_tokens".to_owned(), Value::Number(total.get().into())),
    ]);
    if let Some(details) = usage.output_details() {
        object.insert(
            "output_tokens_details".to_owned(),
            build_token_details(details),
        );
    }
    Ok(Value::Object(object))
}

fn build_token_details(details: ImageTokenBreakdown) -> Value {
    Value::Object(Map::from_iter([
        (
            "image_tokens".to_owned(),
            Value::Number(details.image_tokens().get().into()),
        ),
        (
            "text_tokens".to_owned(),
            Value::Number(details.text_tokens().get().into()),
        ),
    ]))
}

fn map_background(value: ImageBackgroundWire) -> ImageBackground {
    match value {
        ImageBackgroundWire::Auto => ImageBackground::Auto,
        ImageBackgroundWire::Opaque => ImageBackground::Opaque,
        ImageBackgroundWire::Transparent => ImageBackground::Transparent,
    }
}

fn map_moderation(value: ImageModerationWire) -> ImageModeration {
    match value {
        ImageModerationWire::Auto => ImageModeration::Auto,
        ImageModerationWire::Low => ImageModeration::Low,
    }
}

fn map_output_format(value: ImageOutputFormatWire) -> ImageOutputFormat {
    match value {
        ImageOutputFormatWire::Png => ImageOutputFormat::Png,
        ImageOutputFormatWire::Jpeg => ImageOutputFormat::Jpeg,
        ImageOutputFormatWire::Webp => ImageOutputFormat::Webp,
    }
}

fn map_quality(value: ImageQualityWire) -> ImageQuality {
    match value {
        ImageQualityWire::Auto => ImageQuality::Auto,
        ImageQualityWire::Low => ImageQuality::Low,
        ImageQualityWire::Medium => ImageQuality::Medium,
        ImageQualityWire::High => ImageQuality::High,
    }
}

fn revalidate_request(value: &Value) -> Result<(), BuildImageGenerationRequestError> {
    let body =
        serde_json::to_vec(value).map_err(|_| BuildImageGenerationRequestError::InvalidValue)?;
    parse_request(&body).map_err(|_| BuildImageGenerationRequestError::InvalidValue)?;
    Ok(())
}

fn revalidate_response(value: &Value) -> Result<(), BuildImageGenerationResponseError> {
    let body =
        serde_json::to_vec(value).map_err(|_| BuildImageGenerationResponseError::InvalidValue)?;
    parse_response(&body).map_err(|_| BuildImageGenerationResponseError::InvalidValue)?;
    Ok(())
}

fn map_request_json_error(error: BoundedJsonError) -> ParseImageGenerationRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseImageGenerationRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseImageGenerationRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseImageGenerationRequestError::StructureLimitExceeded,
    }
}

fn map_response_json_error(error: BoundedJsonError) -> ParseImageGenerationResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseImageGenerationResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseImageGenerationResponseError::DuplicateKey,
        BoundedJsonError::LimitExceeded => {
            ParseImageGenerationResponseError::StructureLimitExceeded
        }
    }
}

fn map_request_error(
    error: CanonicalImageGenerationRequestError,
) -> ParseImageGenerationRequestError {
    match error {
        CanonicalImageGenerationRequestError::InvalidModel
        | CanonicalImageGenerationRequestError::InvalidPrompt
        | CanonicalImageGenerationRequestError::InvalidParameterCombination => {
            ParseImageGenerationRequestError::InvalidValue
        }
    }
}

/// OpenAI Images 请求解析错误，不保留模型名、提示词或外部字段值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseImageGenerationRequestError {
    /// 请求体超过协议层正文预算。
    BodyTooLarge,
    /// 请求体不是单个合法 JSON 值。
    InvalidJson,
    /// JSON 对象出现重复键。
    DuplicateKey,
    /// JSON 或业务结构超过受限预算。
    StructureLimitExceeded,
    /// 请求字段类型、取值或关联关系无效。
    InvalidValue,
    /// 请求使用了首切片尚未建模的特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseImageGenerationRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Images 请求体超过大小限制",
            Self::InvalidJson => "Images 请求体不是有效 JSON",
            Self::DuplicateKey => "Images 请求包含重复字段",
            Self::StructureLimitExceeded => "Images 请求结构超过限制",
            Self::InvalidValue => "Images 请求字段无效",
            Self::UnsupportedFeature => "Images 请求包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseImageGenerationRequestError {}

/// OpenAI Images 请求构造错误，不保留提示词或模型名。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildImageGenerationRequestError {
    /// Canonical 请求无法重新满足 OpenAI wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildImageGenerationRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Images 请求")
    }
}

impl Error for BuildImageGenerationRequestError {}

/// OpenAI Images 响应解析错误，不保留图片、Base64 或用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseImageGenerationResponseError {
    /// 响应体超过协议层正文预算。
    BodyTooLarge,
    /// 响应体不是单个合法 JSON 值。
    InvalidJson,
    /// JSON 对象出现重复键。
    DuplicateKey,
    /// JSON、Base64 或解码图片超过受限预算。
    StructureLimitExceeded,
    /// 响应字段、图片签名、元数据或用量无效。
    InvalidValue,
    /// 响应使用了 URL、改写提示词等未建模特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseImageGenerationResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Images 响应体超过大小限制",
            Self::InvalidJson => "Images 响应体不是有效 JSON",
            Self::DuplicateKey => "Images 响应包含重复字段",
            Self::StructureLimitExceeded => "Images 响应结构超过限制",
            Self::InvalidValue => "Images 响应字段无效",
            Self::UnsupportedFeature => "Images 响应包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseImageGenerationResponseError {}

/// OpenAI Images 响应构造错误，不保留图片正文或上游数据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildImageGenerationResponseError {
    /// Canonical 响应无法重新满足 OpenAI wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildImageGenerationResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Images 响应")
    }
}

impl Error for BuildImageGenerationResponseError {}

#[cfg(test)]
mod tests;
