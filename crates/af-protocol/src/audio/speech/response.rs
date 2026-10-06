use std::{error::Error, fmt};

use af_domain::Operation;

use super::{AudioSpeechOutputFormat, CanonicalAudioSpeechRequest, GeneratedSpeechAudio};

/// 文本转语音完整二进制响应的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalAudioSpeechResponse {
    audio: GeneratedSpeechAudio,
}

impl CanonicalAudioSpeechResponse {
    /// 构造已经完成格式、签名和字节预算校验的 Speech 响应。
    #[must_use]
    pub const fn new(audio: GeneratedSpeechAudio) -> Self {
        Self { audio }
    }

    /// 返回该响应固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Audio
    }

    /// 返回经过校验的完整音频。
    #[must_use]
    pub const fn audio(&self) -> &GeneratedSpeechAudio {
        &self.audio
    }

    /// 返回生成音频的格式。
    #[must_use]
    pub const fn output_format(&self) -> AudioSpeechOutputFormat {
        self.audio.format()
    }

    /// 验证响应音频格式与原请求的显式或默认格式一致。
    pub fn validate_for_request(
        &self,
        request: &CanonicalAudioSpeechRequest,
    ) -> Result<(), CanonicalAudioSpeechResponseError> {
        if self.output_format() != request.options().effective_output_format() {
            return Err(CanonicalAudioSpeechResponseError::OutputFormatMismatch);
        }
        Ok(())
    }
}

impl fmt::Debug for CanonicalAudioSpeechResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalAudioSpeechResponse")
            .field("operation", &Operation::Audio)
            .field("audio", &self.audio)
            .finish()
    }
}

/// Canonical Audio Speech 响应关联错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalAudioSpeechResponseError {
    /// 响应音频格式与原请求的显式或默认格式不一致。
    OutputFormatMismatch,
}

impl fmt::Display for CanonicalAudioSpeechResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Audio Speech 响应格式与请求不一致")
    }
}

impl Error for CanonicalAudioSpeechResponseError {}
