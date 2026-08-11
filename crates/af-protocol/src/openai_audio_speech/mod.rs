//! OpenAI Audio Speech 非 SSE 协议转换。
//!
//! 首切片按 `POST /v1/audio/speech` endpoint reference 处理 JSON 请求和完整二进制响应。
//! `stream_format: "sse"` 仍未进入当前生产范围；公开路由、调度与计费由上层独立服务
//! 闭合。官方 TTS 指南的 custom voice 示例还使用了当前 endpoint reference 未声明的
//! `language`/`format` 别名；这里继续按未知字段失败关闭，避免在契约未稳定前静默改写语义。

use std::{error::Error, fmt, str::FromStr};

use bytes::Bytes;
use serde_json::{Map, Number, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    AudioSpeechOptions, AudioSpeechOutputFormat, AudioSpeechSpeed, AudioSpeechStreamFormat,
    AudioSpeechVoice, AudioSpeechVoiceId, AudioSpeechVoiceName, CanonicalAudioSpeechRequest,
    CanonicalAudioSpeechRequestError, CanonicalAudioSpeechResponse, GeneratedSpeechAudio,
    MAX_GENERATED_SPEECH_BYTES,
};

mod wire;

use wire::{
    AudioSpeechOutputFormatWire, AudioSpeechRequestWire, AudioSpeechStreamFormatWire,
    AudioSpeechVoiceWire, Field,
};

/// OpenAI Audio Speech JSON 请求正文上限。
pub const MAX_REQUEST_BODY_BYTES: usize = 128 * 1_024;
/// OpenAI Audio Speech 完整二进制响应正文上限。
pub const MAX_RESPONSE_BODY_BYTES: usize = MAX_GENERATED_SPEECH_BYTES;

const REQUEST_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 4,
    max_nodes: 32,
    max_object_entries: 12,
    max_array_items: 0,
    // 正文上限已经约束全部字符串；这里保留同等累计预算，避免合法边界值叠加键名后被误拒。
    max_string_bytes: MAX_REQUEST_BODY_BYTES,
    max_key_bytes: 128,
};

/// 解析 OpenAI `POST /v1/audio/speech` 非 SSE 请求。
///
/// 该函数拒绝重复键、未知字段、显式空值、SSE 输出和超过结构预算的 JSON；命名声音
/// 保持供应商兼容，custom voice 必须使用只含 `id` 的闭合对象。
pub fn parse_request(
    body: &[u8],
) -> Result<CanonicalAudioSpeechRequest, ParseAudioSpeechRequestError> {
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return Err(ParseAudioSpeechRequestError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, REQUEST_JSON_LIMITS).map_err(map_request_json_error)?;
    let wire =
        serde_json::from_value(value).map_err(|_| ParseAudioSpeechRequestError::InvalidValue)?;
    convert_request(wire)
}

/// 将 Canonical Audio Speech 请求构造成 OpenAI wire JSON。
///
/// 只输出调用方显式提供的可选字段，避免给兼容上游注入默认参数；结果重新经过入站
/// 解析器，防止程序化请求绕过字符数、语速和结构预算。
pub fn build_request(
    request: &CanonicalAudioSpeechRequest,
) -> Result<Value, BuildAudioSpeechRequestError> {
    let mut root = Map::from_iter([
        (
            "model".to_owned(),
            Value::String(request.model().to_owned()),
        ),
        (
            "input".to_owned(),
            Value::String(request.input().to_owned()),
        ),
        ("voice".to_owned(), build_voice(request.options().voice())),
    ]);
    let options = request.options();
    if let Some(instructions) = options.instructions() {
        root.insert(
            "instructions".to_owned(),
            Value::String(instructions.to_owned()),
        );
    }
    if let Some(format) = options.output_format() {
        root.insert(
            "response_format".to_owned(),
            Value::String(format.as_str().to_owned()),
        );
    }
    if let Some(speed) = options.speed() {
        let number = Number::from_str(&speed.to_string())
            .map_err(|_| BuildAudioSpeechRequestError::InvalidValue)?;
        root.insert("speed".to_owned(), Value::Number(number));
    }
    if let Some(stream_format) = options.stream_format() {
        root.insert(
            "stream_format".to_owned(),
            Value::String(stream_format.as_str().to_owned()),
        );
    }
    let value = Value::Object(root);
    revalidate_request(&value)?;
    Ok(value)
}

/// 解析已完整读取的 OpenAI Audio Speech 二进制响应。
///
/// 调用方传入原请求的有效输出格式；该边界不会信任 MIME 来猜测格式，而是按请求格式
/// 校验文件签名、PCM 采样帧和完整响应字节预算。
pub fn parse_response(
    format: AudioSpeechOutputFormat,
    body: Bytes,
) -> Result<CanonicalAudioSpeechResponse, ParseAudioSpeechResponseError> {
    if body.len() > MAX_RESPONSE_BODY_BYTES {
        return Err(ParseAudioSpeechResponseError::BodyTooLarge);
    }
    let audio = GeneratedSpeechAudio::new(format, body)
        .map_err(|_| ParseAudioSpeechResponseError::InvalidAudio)?;
    Ok(CanonicalAudioSpeechResponse::new(audio))
}

/// 将 Canonical Audio Speech 响应构造成完整二进制正文。
///
/// `Bytes` 克隆只增加引用计数；返回前仍重新通过响应解析边界，保证调用方不能构造出
/// 与格式或预算不一致的下游正文。
pub fn build_response(
    response: &CanonicalAudioSpeechResponse,
) -> Result<Bytes, BuildAudioSpeechResponseError> {
    let body = response.audio().bytes().clone();
    parse_response(response.output_format(), body.clone())
        .map_err(|_| BuildAudioSpeechResponseError::InvalidValue)?;
    Ok(body)
}

fn convert_request(
    wire: AudioSpeechRequestWire,
) -> Result<CanonicalAudioSpeechRequest, ParseAudioSpeechRequestError> {
    let voice = match wire.voice {
        AudioSpeechVoiceWire::Named(name) => AudioSpeechVoice::Named(
            AudioSpeechVoiceName::new(name)
                .map_err(|_| ParseAudioSpeechRequestError::InvalidValue)?,
        ),
        AudioSpeechVoiceWire::Custom(custom) => AudioSpeechVoice::Custom(
            AudioSpeechVoiceId::new(custom.id)
                .map_err(|_| ParseAudioSpeechRequestError::InvalidValue)?,
        ),
    };
    let instructions = match wire.instructions {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    };
    let output_format = match wire.response_format {
        Field::Missing => None,
        Field::Value(format) => Some(convert_output_format(format)),
    };
    let speed = match wire.speed {
        Field::Missing => None,
        Field::Value(value) => Some(
            AudioSpeechSpeed::parse(&value.to_string())
                .map_err(|_| ParseAudioSpeechRequestError::InvalidValue)?,
        ),
    };
    let stream_format = match wire.stream_format {
        Field::Missing => None,
        Field::Value(AudioSpeechStreamFormatWire::Audio) => Some(AudioSpeechStreamFormat::Audio),
        Field::Value(AudioSpeechStreamFormatWire::Sse) => {
            return Err(ParseAudioSpeechRequestError::UnsupportedFeature);
        }
    };
    let options = AudioSpeechOptions::new(voice, instructions, output_format, speed, stream_format)
        .map_err(|_| ParseAudioSpeechRequestError::InvalidValue)?;
    CanonicalAudioSpeechRequest::new(wire.model, wire.input, options).map_err(map_request_error)
}

fn convert_output_format(format: AudioSpeechOutputFormatWire) -> AudioSpeechOutputFormat {
    match format {
        AudioSpeechOutputFormatWire::Mp3 => AudioSpeechOutputFormat::Mp3,
        AudioSpeechOutputFormatWire::Opus => AudioSpeechOutputFormat::Opus,
        AudioSpeechOutputFormatWire::Aac => AudioSpeechOutputFormat::Aac,
        AudioSpeechOutputFormatWire::Flac => AudioSpeechOutputFormat::Flac,
        AudioSpeechOutputFormatWire::Wav => AudioSpeechOutputFormat::Wav,
        AudioSpeechOutputFormatWire::Pcm => AudioSpeechOutputFormat::Pcm,
    }
}

fn build_voice(voice: &AudioSpeechVoice) -> Value {
    match voice {
        AudioSpeechVoice::Named(name) => Value::String(name.as_str().to_owned()),
        AudioSpeechVoice::Custom(id) => Value::Object(Map::from_iter([(
            "id".to_owned(),
            Value::String(id.as_str().to_owned()),
        )])),
    }
}

fn revalidate_request(value: &Value) -> Result<(), BuildAudioSpeechRequestError> {
    let body = serde_json::to_vec(value).map_err(|_| BuildAudioSpeechRequestError::InvalidValue)?;
    parse_request(&body).map_err(|_| BuildAudioSpeechRequestError::InvalidValue)?;
    Ok(())
}

fn map_request_json_error(error: BoundedJsonError) -> ParseAudioSpeechRequestError {
    match error {
        BoundedJsonError::InvalidJson => ParseAudioSpeechRequestError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseAudioSpeechRequestError::DuplicateKey,
        BoundedJsonError::LimitExceeded => ParseAudioSpeechRequestError::StructureLimitExceeded,
    }
}

fn map_request_error(error: CanonicalAudioSpeechRequestError) -> ParseAudioSpeechRequestError {
    match error {
        CanonicalAudioSpeechRequestError::InvalidModel
        | CanonicalAudioSpeechRequestError::InvalidInput => {
            ParseAudioSpeechRequestError::InvalidValue
        }
    }
}

/// OpenAI Audio Speech 请求解析错误，不保留模型、文本、声音或指令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseAudioSpeechRequestError {
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
    /// 请求使用了首切片尚未建模的 SSE 特性。
    UnsupportedFeature,
}

impl fmt::Display for ParseAudioSpeechRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Audio Speech 请求体超过大小限制",
            Self::InvalidJson => "Audio Speech 请求体不是有效 JSON",
            Self::DuplicateKey => "Audio Speech 请求包含重复字段",
            Self::StructureLimitExceeded => "Audio Speech 请求结构超过限制",
            Self::InvalidValue => "Audio Speech 请求字段无效",
            Self::UnsupportedFeature => "Audio Speech 请求包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseAudioSpeechRequestError {}

/// OpenAI Audio Speech 请求构造错误，不保留请求内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildAudioSpeechRequestError {
    /// Canonical 请求无法重新满足 OpenAI wire 边界。
    InvalidValue,
}

impl fmt::Display for BuildAudioSpeechRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Audio Speech 请求")
    }
}

impl Error for BuildAudioSpeechRequestError {}

/// OpenAI Audio Speech 二进制响应解析错误，不保留音频正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseAudioSpeechResponseError {
    /// 完整响应体超过协议层字节预算。
    BodyTooLarge,
    /// 响应为空、文件签名错误或 PCM 采样帧未对齐。
    InvalidAudio,
}

impl fmt::Display for ParseAudioSpeechResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Audio Speech 响应体超过大小限制",
            Self::InvalidAudio => "Audio Speech 响应音频无效",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseAudioSpeechResponseError {}

/// OpenAI Audio Speech 二进制响应构造错误，不保留音频正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildAudioSpeechResponseError {
    /// Canonical 响应无法重新满足二进制格式与预算边界。
    InvalidValue,
}

impl fmt::Display for BuildAudioSpeechResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Audio Speech 响应")
    }
}

impl Error for BuildAudioSpeechResponseError {}

#[cfg(test)]
mod tests;
