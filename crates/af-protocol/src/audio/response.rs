use std::{error::Error, fmt};

use af_domain::Operation;

use super::{AudioLanguageCode, AudioTranscriptionUsage};

/// 非流式转录文本允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_TRANSCRIPT_TEXT_BYTES: usize = 8 * 1_024 * 1_024;
/// 响应允许返回的最大检测语言数量。
pub const MAX_AUDIO_DETECTED_LANGUAGES: usize = 16;

/// 文件转录非流式响应的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalAudioTranscriptionResponse {
    text: String,
    languages: Option<Vec<AudioLanguageCode>>,
    usage: Option<AudioTranscriptionUsage>,
}

impl CanonicalAudioTranscriptionResponse {
    /// 构造并校验转录文本、检测语言和可选用量。
    pub fn new(
        text: String,
        languages: Option<Vec<AudioLanguageCode>>,
        usage: Option<AudioTranscriptionUsage>,
    ) -> Result<Self, CanonicalAudioTranscriptionResponseError> {
        if text.len() > MAX_AUDIO_TRANSCRIPT_TEXT_BYTES {
            return Err(CanonicalAudioTranscriptionResponseError::TranscriptTooLarge);
        }
        if let Some(languages) = &languages {
            if languages.len() > MAX_AUDIO_DETECTED_LANGUAGES {
                return Err(CanonicalAudioTranscriptionResponseError::TooManyLanguages);
            }
            for (index, language) in languages.iter().enumerate() {
                if languages[..index].contains(language) {
                    return Err(CanonicalAudioTranscriptionResponseError::DuplicateLanguage);
                }
            }
        }
        Ok(Self {
            text,
            languages,
            usage,
        })
    }

    /// 返回该响应固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Audio
    }

    /// 返回经过字节预算校验的转录文本。
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 返回上游明确提供的检测语言集合；空数组与字段缺失保持可区分。
    #[must_use]
    pub fn languages(&self) -> Option<&[AudioLanguageCode]> {
        self.languages.as_deref()
    }

    /// 返回上游明确提供的 token 或时长用量。
    #[must_use]
    pub const fn usage(&self) -> Option<AudioTranscriptionUsage> {
        self.usage
    }
}

impl fmt::Debug for CanonicalAudioTranscriptionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalAudioTranscriptionResponse")
            .field("operation", &Operation::Audio)
            .field("text", &"<已脱敏>")
            .field("text_bytes", &self.text.len())
            .field("language_count", &self.languages.as_ref().map(Vec::len))
            .field("usage", &self.usage)
            .finish()
    }
}

/// Canonical 音频转录响应错误，不保留转录正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalAudioTranscriptionResponseError {
    /// 转录文本超过单响应字节预算。
    TranscriptTooLarge,
    /// 检测语言数量超过上限。
    TooManyLanguages,
    /// 检测语言集合包含重复项。
    DuplicateLanguage,
}

impl fmt::Display for CanonicalAudioTranscriptionResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TranscriptTooLarge => "音频转录文本超过字节预算",
            Self::TooManyLanguages => "音频检测语言数量超过限制",
            Self::DuplicateLanguage => "音频检测语言不能重复",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalAudioTranscriptionResponseError {}
