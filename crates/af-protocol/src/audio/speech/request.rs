use std::{error::Error, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Operation};

use super::AudioSpeechOptions;

/// Speech API 单次输入允许的最大 Unicode 字符数。
pub const MAX_AUDIO_SPEECH_INPUT_CHARS: usize = 4_096;
/// Speech API 单次输入允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_SPEECH_INPUT_BYTES: usize = MAX_AUDIO_SPEECH_INPUT_CHARS * 4;

/// 文本转语音请求的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalAudioSpeechRequest {
    model: String,
    input: String,
    options: AudioSpeechOptions,
}

impl CanonicalAudioSpeechRequest {
    /// 构造经过模型名、输入文本和 Speech 参数校验的请求。
    pub fn new(
        model: String,
        input: String,
        options: AudioSpeechOptions,
    ) -> Result<Self, CanonicalAudioSpeechRequestError> {
        validate_model(&model)?;
        if input.trim().is_empty()
            || input.len() > MAX_AUDIO_SPEECH_INPUT_BYTES
            || input.chars().count() > MAX_AUDIO_SPEECH_INPUT_CHARS
        {
            return Err(CanonicalAudioSpeechRequestError::InvalidInput);
        }
        Ok(Self {
            model,
            input,
            options,
        })
    }

    /// 返回该请求固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Audio
    }

    /// 返回尚未经过渠道映射的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回经过字符数和字节预算校验的待合成文本。
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// 返回经过关联校验的 Speech 参数。
    #[must_use]
    pub const fn options(&self) -> &AudioSpeechOptions {
        &self.options
    }

    /// 返回输入文本 UTF-8 字节数，供后续计费预扣设置安全上界。
    #[must_use]
    pub fn input_bytes(&self) -> usize {
        self.input.len()
    }
}

impl fmt::Debug for CanonicalAudioSpeechRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalAudioSpeechRequest")
            .field("operation", &Operation::Audio)
            .field("model", &"<已脱敏>")
            .field("input", &"<已脱敏>")
            .field("input_bytes", &self.input.len())
            .field("options", &self.options)
            .finish()
    }
}

/// Canonical Audio Speech 请求错误，不保留模型名或输入正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalAudioSpeechRequestError {
    /// 模型名为空、超长、含控制字符或带首尾空白。
    InvalidModel,
    /// 输入为空白或超过官方字符/字节预算。
    InvalidInput,
}

impl fmt::Display for CanonicalAudioSpeechRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "Audio Speech 模型名无效",
            Self::InvalidInput => "Audio Speech 输入文本无效",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalAudioSpeechRequestError {}

fn validate_model(model: &str) -> Result<(), CanonicalAudioSpeechRequestError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
    {
        return Err(CanonicalAudioSpeechRequestError::InvalidModel);
    }
    Ok(())
}
