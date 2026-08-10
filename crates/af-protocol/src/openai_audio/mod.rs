//! OpenAI Audio 文件转录协议转换。
//!
//! 首切片只处理 `POST /v1/audio/transcriptions` 的非流式基础 JSON 响应。HTTP 层负责
//! 解析原始 multipart，协议层接收有序受限部件并拒绝重复、未知和未建模能力。

use std::{error::Error, fmt, str::FromStr};

use serde_json::{Map, Number, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    AudioDuration, AudioFile, AudioFileFormat, AudioInputTokenDetails, AudioLanguageCode,
    AudioLanguageHints, AudioTranscriptionOptions, AudioTranscriptionTokenUsage,
    AudioTranscriptionUsage, CanonicalAudioTranscriptionRequest,
    CanonicalAudioTranscriptionRequestError, CanonicalAudioTranscriptionResponse,
    MAX_AUDIO_TRANSCRIPT_TEXT_BYTES, TokenCount, TranscriptionKeyword, TranscriptionTemperature,
};

mod form;
mod wire;

pub use form::{
    MAX_TRANSCRIPTION_CONTENT_TYPE_BYTES, MAX_TRANSCRIPTION_FIELD_NAME_BYTES,
    MAX_TRANSCRIPTION_FILE_NAME_BYTES, MAX_TRANSCRIPTION_FORM_PARTS,
    MAX_TRANSCRIPTION_FORM_TEXT_BYTES, OpenAiTranscriptionFilePart, OpenAiTranscriptionForm,
    OpenAiTranscriptionFormError, OpenAiTranscriptionFormPart, OpenAiTranscriptionTextPart,
};

use wire::{AudioInputTokenDetailsWire, Field, TranscriptionResponseWire, TranscriptionUsageWire};

/// OpenAI Audio 非流式转录响应允许的最大正文大小。
pub const MAX_TRANSCRIPTION_RESPONSE_BODY_BYTES: usize = 32 * 1_024 * 1_024;

const RESPONSE_JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 8,
    max_nodes: 16_384,
    max_object_entries: 16,
    max_array_items: 4_096,
    max_string_bytes: MAX_AUDIO_TRANSCRIPT_TEXT_BYTES + 64 * 1_024,
    max_key_bytes: 128,
};

/// 解析 HTTP 层已拆分并完成基础预算约束的 OpenAI 转录表单。
///
/// 该函数要求一个 `model`、一个 `file`，支持提示词、单/多语言、关键词和有限温度；
/// 非 JSON 响应、流式、时间戳、说话人分离、logprobs 和未知字段均失败关闭。
pub fn parse_transcription_request(
    form: OpenAiTranscriptionForm,
) -> Result<CanonicalAudioTranscriptionRequest, ParseAudioTranscriptionRequestError> {
    let mut fields = TranscriptionRequestFields::default();
    for part in form.into_parts() {
        match part {
            OpenAiTranscriptionFormPart::Text(part) => {
                let (name, value) = part.into_name_value();
                fields.push_text(name, value)?;
            }
            OpenAiTranscriptionFormPart::File(part) => fields.push_file(part)?,
        }
    }
    fields.finish()
}

/// 将 Canonical 转录请求重建为可由 HTTP/适配层编码的有序 multipart 部件。
///
/// 文件名和 MIME 根据 Canonical 格式重新生成，不透传客户端原始文件名。
pub fn build_transcription_request(
    request: &CanonicalAudioTranscriptionRequest,
) -> Result<OpenAiTranscriptionForm, BuildAudioTranscriptionRequestError> {
    let mut parts = vec![text_part("model", request.model())?];
    let options = request.options();
    if let Some(prompt) = options.prompt() {
        parts.push(text_part("prompt", prompt)?);
    }
    if let Some(hints) = options.language_hints() {
        if let Some(language) = hints.as_single() {
            parts.push(text_part("language", language.as_str())?);
        } else if let Some(languages) = hints.as_multiple() {
            for language in languages {
                parts.push(text_part("languages[]", language.as_str())?);
            }
        }
    }
    for keyword in options.keywords() {
        parts.push(text_part("keywords[]", keyword.as_str())?);
    }
    if let Some(temperature) = options.temperature() {
        parts.push(text_part("temperature", &temperature.to_string())?);
    }

    let format = request.file().format();
    parts.push(
        OpenAiTranscriptionFormPart::file(
            "file".to_owned(),
            format!("audio.{}", format.extension()),
            Some(format.content_type().to_owned()),
            request.file().bytes().clone(),
        )
        .map_err(|_| BuildAudioTranscriptionRequestError::InvalidValue)?,
    );
    let form = OpenAiTranscriptionForm::new(parts)
        .map_err(|_| BuildAudioTranscriptionRequestError::InvalidValue)?;
    if parse_transcription_request(form.clone())
        .map_err(|_| BuildAudioTranscriptionRequestError::InvalidValue)?
        != *request
    {
        return Err(BuildAudioTranscriptionRequestError::InvalidValue);
    }
    Ok(form)
}

/// 解析已完整读取的 OpenAI Audio 非流式基础 JSON 响应。
pub fn parse_transcription_response(
    body: &[u8],
) -> Result<CanonicalAudioTranscriptionResponse, ParseAudioTranscriptionResponseError> {
    if body.len() > MAX_TRANSCRIPTION_RESPONSE_BODY_BYTES {
        return Err(ParseAudioTranscriptionResponseError::BodyTooLarge);
    }
    let value =
        bounded_json::parse_value(body, RESPONSE_JSON_LIMITS).map_err(map_response_json_error)?;
    let wire = serde_json::from_value(value)
        .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)?;
    convert_response(wire)
}

/// 将 Canonical 音频转录响应构造成 OpenAI 基础 JSON。
pub fn build_transcription_response(
    response: &CanonicalAudioTranscriptionResponse,
) -> Result<Value, BuildAudioTranscriptionResponseError> {
    let mut root = Map::from_iter([("text".to_owned(), Value::String(response.text().to_owned()))]);
    if let Some(languages) = response.languages() {
        root.insert(
            "languages".to_owned(),
            Value::Array(
                languages
                    .iter()
                    .map(|language| {
                        Value::Object(Map::from_iter([(
                            "code".to_owned(),
                            Value::String(language.as_str().to_owned()),
                        )]))
                    })
                    .collect(),
            ),
        );
    }
    if let Some(usage) = response.usage() {
        root.insert("usage".to_owned(), build_usage(usage)?);
    }
    let value = Value::Object(root);
    revalidate_response(&value)?;
    Ok(value)
}

#[derive(Default)]
struct TranscriptionRequestFields {
    model: Option<String>,
    file: Option<OpenAiTranscriptionFilePart>,
    prompt: Option<String>,
    language: Option<String>,
    languages: Vec<String>,
    keywords: Vec<String>,
    temperature: Option<String>,
    response_format: Option<String>,
    stream: Option<String>,
}

impl TranscriptionRequestFields {
    fn push_text(
        &mut self,
        name: String,
        value: String,
    ) -> Result<(), ParseAudioTranscriptionRequestError> {
        match name.as_str() {
            "model" => set_once(&mut self.model, value),
            "prompt" => set_once(&mut self.prompt, value),
            "language" => set_once(&mut self.language, value),
            "languages" | "languages[]" => {
                self.languages.push(value);
                Ok(())
            }
            "keywords" | "keywords[]" => {
                self.keywords.push(value);
                Ok(())
            }
            "temperature" => set_once(&mut self.temperature, value),
            "response_format" => set_once(&mut self.response_format, value),
            "stream" => set_once(&mut self.stream, value),
            "file" => Err(ParseAudioTranscriptionRequestError::InvalidValue),
            "include"
            | "include[]"
            | "timestamp_granularities"
            | "timestamp_granularities[]"
            | "chunking_strategy"
            | "known_speaker_names"
            | "known_speaker_names[]"
            | "known_speaker_references"
            | "known_speaker_references[]"
            | "logprobs" => Err(ParseAudioTranscriptionRequestError::UnsupportedFeature),
            _ => Err(ParseAudioTranscriptionRequestError::UnknownField),
        }
    }

    fn push_file(
        &mut self,
        part: OpenAiTranscriptionFilePart,
    ) -> Result<(), ParseAudioTranscriptionRequestError> {
        match part.name() {
            "file" => {
                if self.file.replace(part).is_some() {
                    Err(ParseAudioTranscriptionRequestError::DuplicateField)
                } else {
                    Ok(())
                }
            }
            "known_speaker_references" | "known_speaker_references[]" => {
                Err(ParseAudioTranscriptionRequestError::UnsupportedFeature)
            }
            _ => Err(ParseAudioTranscriptionRequestError::UnknownField),
        }
    }

    fn finish(
        self,
    ) -> Result<CanonicalAudioTranscriptionRequest, ParseAudioTranscriptionRequestError> {
        let model = self
            .model
            .ok_or(ParseAudioTranscriptionRequestError::MissingField)?;
        let file = self
            .file
            .ok_or(ParseAudioTranscriptionRequestError::MissingField)?;
        if self.language.is_some() && !self.languages.is_empty() {
            return Err(ParseAudioTranscriptionRequestError::InvalidValue);
        }
        if self
            .response_format
            .as_deref()
            .is_some_and(|value| value != "json")
        {
            return Err(ParseAudioTranscriptionRequestError::UnsupportedFeature);
        }
        match self.stream.as_deref() {
            None | Some("false") => {}
            Some("true") => return Err(ParseAudioTranscriptionRequestError::UnsupportedFeature),
            Some(_) => return Err(ParseAudioTranscriptionRequestError::InvalidValue),
        }

        let language_hints = if let Some(language) = self.language {
            Some(AudioLanguageHints::single(parse_language(language)?))
        } else if self.languages.is_empty() {
            None
        } else {
            Some(
                AudioLanguageHints::multiple(
                    self.languages
                        .into_iter()
                        .map(parse_language)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)?,
            )
        };
        let keywords = self
            .keywords
            .into_iter()
            .map(|value| {
                TranscriptionKeyword::new(value)
                    .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let temperature = self
            .temperature
            .map(|value| {
                TranscriptionTemperature::parse(&value)
                    .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)
            })
            .transpose()?;
        let options =
            AudioTranscriptionOptions::new(self.prompt, language_hints, keywords, temperature)
                .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)?;

        let (file_name, content_type, bytes) = file.into_metadata_and_bytes();
        let format = AudioFileFormat::from_file_name(&file_name)
            .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)?;
        if !format.accepts_content_type(content_type.as_deref()) {
            return Err(ParseAudioTranscriptionRequestError::InvalidValue);
        }
        let file = AudioFile::new(format, bytes)
            .map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)?;
        CanonicalAudioTranscriptionRequest::new(model, file, options).map_err(map_request_error)
    }
}

fn convert_response(
    wire: TranscriptionResponseWire,
) -> Result<CanonicalAudioTranscriptionResponse, ParseAudioTranscriptionResponseError> {
    if !matches!(wire.logprobs, Field::Missing) {
        return Err(ParseAudioTranscriptionResponseError::UnsupportedFeature);
    }
    let languages = match wire.languages {
        Field::Missing => None,
        Field::Value(languages) => Some(
            languages
                .into_iter()
                .map(|language| parse_response_language(language.code))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let usage = match wire.usage {
        Field::Missing => None,
        Field::Value(usage) => Some(convert_usage(usage)?),
    };
    CanonicalAudioTranscriptionResponse::new(wire.text, languages, usage)
        .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)
}

fn convert_usage(
    usage: TranscriptionUsageWire,
) -> Result<AudioTranscriptionUsage, ParseAudioTranscriptionResponseError> {
    match usage {
        TranscriptionUsageWire::Tokens {
            input_tokens,
            output_tokens,
            total_tokens,
            input_token_details,
        } => {
            let input_tokens = parse_token_count(input_tokens)?;
            let output_tokens = parse_token_count(output_tokens)?;
            let total_tokens = parse_token_count(total_tokens)?;
            let details = match input_token_details {
                Field::Missing => None,
                Field::Value(details) => Some(convert_token_details(details)?),
            };
            let usage = AudioTranscriptionTokenUsage::new(input_tokens, output_tokens, details)
                .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)?;
            if usage
                .checked_total_tokens()
                .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)?
                != total_tokens
            {
                return Err(ParseAudioTranscriptionResponseError::InvalidValue);
            }
            Ok(AudioTranscriptionUsage::Tokens(usage))
        }
        TranscriptionUsageWire::Duration { seconds } => {
            let duration = AudioDuration::parse_seconds(&seconds.to_string())
                .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)?;
            Ok(AudioTranscriptionUsage::Duration(duration))
        }
    }
}

fn convert_token_details(
    details: AudioInputTokenDetailsWire,
) -> Result<AudioInputTokenDetails, ParseAudioTranscriptionResponseError> {
    AudioInputTokenDetails::new(
        parse_optional_token_count(details.audio_tokens)?,
        parse_optional_token_count(details.text_tokens)?,
    )
    .map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)
}

fn build_usage(
    usage: AudioTranscriptionUsage,
) -> Result<Value, BuildAudioTranscriptionResponseError> {
    match usage {
        AudioTranscriptionUsage::Tokens(usage) => {
            let total = usage
                .checked_total_tokens()
                .map_err(|_| BuildAudioTranscriptionResponseError::InvalidValue)?;
            let mut object = Map::from_iter([
                (
                    "input_tokens".to_owned(),
                    Value::Number(usage.input_tokens().get().into()),
                ),
                (
                    "output_tokens".to_owned(),
                    Value::Number(usage.output_tokens().get().into()),
                ),
                ("total_tokens".to_owned(), Value::Number(total.get().into())),
                ("type".to_owned(), Value::String("tokens".to_owned())),
            ]);
            if let Some(details) = usage.input_details() {
                let mut detail_object = Map::new();
                if let Some(audio_tokens) = details.audio_tokens() {
                    detail_object.insert(
                        "audio_tokens".to_owned(),
                        Value::Number(audio_tokens.get().into()),
                    );
                }
                if let Some(text_tokens) = details.text_tokens() {
                    detail_object.insert(
                        "text_tokens".to_owned(),
                        Value::Number(text_tokens.get().into()),
                    );
                }
                object.insert(
                    "input_token_details".to_owned(),
                    Value::Object(detail_object),
                );
            }
            Ok(Value::Object(object))
        }
        AudioTranscriptionUsage::Duration(duration) => {
            let seconds = Number::from_str(&duration.to_string())
                .map_err(|_| BuildAudioTranscriptionResponseError::InvalidValue)?;
            Ok(Value::Object(Map::from_iter([
                ("seconds".to_owned(), Value::Number(seconds)),
                ("type".to_owned(), Value::String("duration".to_owned())),
            ])))
        }
    }
}

fn parse_language(value: String) -> Result<AudioLanguageCode, ParseAudioTranscriptionRequestError> {
    AudioLanguageCode::new(value).map_err(|_| ParseAudioTranscriptionRequestError::InvalidValue)
}

fn parse_response_language(
    value: String,
) -> Result<AudioLanguageCode, ParseAudioTranscriptionResponseError> {
    AudioLanguageCode::new(value).map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)
}

fn parse_token_count(value: i64) -> Result<TokenCount, ParseAudioTranscriptionResponseError> {
    TokenCount::new(value).map_err(|_| ParseAudioTranscriptionResponseError::InvalidValue)
}

fn parse_optional_token_count(
    field: Field<i64>,
) -> Result<Option<TokenCount>, ParseAudioTranscriptionResponseError> {
    match field {
        Field::Missing => Ok(None),
        Field::Value(value) => parse_token_count(value).map(Some),
    }
}

fn set_once(
    slot: &mut Option<String>,
    value: String,
) -> Result<(), ParseAudioTranscriptionRequestError> {
    if slot.replace(value).is_some() {
        Err(ParseAudioTranscriptionRequestError::DuplicateField)
    } else {
        Ok(())
    }
}

fn text_part(
    name: &str,
    value: &str,
) -> Result<OpenAiTranscriptionFormPart, BuildAudioTranscriptionRequestError> {
    OpenAiTranscriptionFormPart::text(name.to_owned(), value.to_owned())
        .map_err(|_| BuildAudioTranscriptionRequestError::InvalidValue)
}

fn revalidate_response(value: &Value) -> Result<(), BuildAudioTranscriptionResponseError> {
    let body = serde_json::to_vec(value)
        .map_err(|_| BuildAudioTranscriptionResponseError::InvalidValue)?;
    parse_transcription_response(&body)
        .map_err(|_| BuildAudioTranscriptionResponseError::InvalidValue)?;
    Ok(())
}

fn map_request_error(
    error: CanonicalAudioTranscriptionRequestError,
) -> ParseAudioTranscriptionRequestError {
    match error {
        CanonicalAudioTranscriptionRequestError::InvalidModel => {
            ParseAudioTranscriptionRequestError::InvalidValue
        }
    }
}

fn map_response_json_error(error: BoundedJsonError) -> ParseAudioTranscriptionResponseError {
    match error {
        BoundedJsonError::InvalidJson => ParseAudioTranscriptionResponseError::InvalidJson,
        BoundedJsonError::DuplicateKey => ParseAudioTranscriptionResponseError::DuplicateField,
        BoundedJsonError::LimitExceeded => {
            ParseAudioTranscriptionResponseError::StructureLimitExceeded
        }
    }
}

/// OpenAI Audio 转录请求解析错误，不保留字段值、文件名或文件正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseAudioTranscriptionRequestError {
    /// 必填的模型或文件字段缺失。
    MissingField,
    /// 单值字段或文件字段重复。
    DuplicateField,
    /// 字段类型、取值、文件格式或关联关系无效。
    InvalidValue,
    /// 请求使用了流式、时间戳、说话人分离等未建模能力。
    UnsupportedFeature,
    /// 请求包含当前契约未定义的字段。
    UnknownField,
}

impl fmt::Display for ParseAudioTranscriptionRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingField => "Audio 转录请求缺少必填字段",
            Self::DuplicateField => "Audio 转录请求包含重复字段",
            Self::InvalidValue => "Audio 转录请求字段无效",
            Self::UnsupportedFeature => "Audio 转录请求包含当前不支持的特性",
            Self::UnknownField => "Audio 转录请求包含未知字段",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseAudioTranscriptionRequestError {}

/// OpenAI Audio 转录请求构造错误，不保留请求内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildAudioTranscriptionRequestError {
    /// Canonical 请求无法重新满足 OpenAI multipart 边界。
    InvalidValue,
}

impl fmt::Display for BuildAudioTranscriptionRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Audio 转录请求")
    }
}

impl Error for BuildAudioTranscriptionRequestError {}

/// OpenAI Audio 转录响应解析错误，不保留转录正文或用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseAudioTranscriptionResponseError {
    /// 响应体超过协议层正文预算。
    BodyTooLarge,
    /// 响应体不是单个合法 JSON 值。
    InvalidJson,
    /// JSON 对象出现重复键。
    DuplicateField,
    /// JSON 或业务结构超过受限预算。
    StructureLimitExceeded,
    /// 响应字段、语言或用量关系无效。
    InvalidValue,
    /// 响应包含 logprobs、时间戳或其他未建模能力。
    UnsupportedFeature,
}

impl fmt::Display for ParseAudioTranscriptionResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BodyTooLarge => "Audio 转录响应体超过大小限制",
            Self::InvalidJson => "Audio 转录响应体不是有效 JSON",
            Self::DuplicateField => "Audio 转录响应包含重复字段",
            Self::StructureLimitExceeded => "Audio 转录响应结构超过限制",
            Self::InvalidValue => "Audio 转录响应字段无效",
            Self::UnsupportedFeature => "Audio 转录响应包含当前不支持的特性",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseAudioTranscriptionResponseError {}

/// OpenAI Audio 转录响应构造错误，不保留转录正文或用量数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildAudioTranscriptionResponseError {
    /// Canonical 响应无法重新满足 OpenAI JSON 边界。
    InvalidValue,
}

impl fmt::Display for BuildAudioTranscriptionResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Audio 转录响应")
    }
}

impl Error for BuildAudioTranscriptionResponseError {}

#[cfg(test)]
mod tests;
