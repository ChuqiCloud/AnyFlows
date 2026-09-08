use std::{error::Error, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Operation};

use super::{AudioFile, AudioTranscriptionOptions};

/// 文件转录请求的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalAudioTranscriptionRequest {
    model: String,
    file: AudioFile,
    options: AudioTranscriptionOptions,
}

impl CanonicalAudioTranscriptionRequest {
    /// 构造经过模型名、文件和可选参数校验的转录请求。
    pub fn new(
        model: String,
        file: AudioFile,
        options: AudioTranscriptionOptions,
    ) -> Result<Self, CanonicalAudioTranscriptionRequestError> {
        validate_model(&model)?;
        Ok(Self {
            model,
            file,
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

    /// 返回经过格式、签名和字节预算校验的音频文件。
    #[must_use]
    pub const fn file(&self) -> &AudioFile {
        &self.file
    }

    /// 返回经过关联校验的转录可选参数。
    #[must_use]
    pub const fn options(&self) -> &AudioTranscriptionOptions {
        &self.options
    }
}

impl fmt::Debug for CanonicalAudioTranscriptionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalAudioTranscriptionRequest")
            .field("operation", &Operation::Audio)
            .field("model", &"<已脱敏>")
            .field("file", &self.file)
            .field("options", &self.options)
            .finish()
    }
}

/// Canonical 音频转录请求错误，不保留模型名或文件内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalAudioTranscriptionRequestError {
    /// 模型名为空、超长、含控制字符或带首尾空白。
    InvalidModel,
}

impl fmt::Display for CanonicalAudioTranscriptionRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("音频转录模型名无效")
    }
}

impl Error for CanonicalAudioTranscriptionRequestError {}

fn validate_model(model: &str) -> Result<(), CanonicalAudioTranscriptionRequestError> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
    {
        return Err(CanonicalAudioTranscriptionRequestError::InvalidModel);
    }
    Ok(())
}
