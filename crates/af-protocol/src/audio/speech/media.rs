use std::{error::Error, fmt};

use bytes::Bytes;

use super::AudioSpeechOutputFormat;

/// 单次完整 Speech 响应允许的最大音频字节数。
pub const MAX_GENERATED_SPEECH_BYTES: usize = 128 * 1_024 * 1_024;

/// 一段经过格式、签名和字节预算校验的生成语音。
#[derive(Clone, Eq, PartialEq)]
pub struct GeneratedSpeechAudio {
    format: AudioSpeechOutputFormat,
    bytes: Bytes,
}

impl GeneratedSpeechAudio {
    /// 从完整响应字节构造受限音频，并校验请求格式对应的文件签名。
    pub fn new(
        format: AudioSpeechOutputFormat,
        bytes: Bytes,
    ) -> Result<Self, GeneratedSpeechAudioError> {
        validate_size(bytes.len())?;
        if format == AudioSpeechOutputFormat::Pcm {
            if !bytes.len().is_multiple_of(2) {
                return Err(GeneratedSpeechAudioError::InvalidPcmFrame);
            }
        } else if !matches_signature(format, &bytes) {
            return Err(GeneratedSpeechAudioError::SignatureMismatch);
        }
        Ok(Self { format, bytes })
    }

    /// 返回由请求和文件签名共同确认的音频格式。
    #[must_use]
    pub const fn format(&self) -> AudioSpeechOutputFormat {
        self.format
    }

    /// 返回经过预算与签名校验的完整音频字节。
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// 返回完整音频字节数。
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

impl fmt::Debug for GeneratedSpeechAudio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedSpeechAudio")
            .field("format", &self.format)
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// 生成语音字节校验错误，不保留音频正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedSpeechAudioError {
    /// 音频为空或超过单次响应字节预算。
    InvalidSize,
    /// 文件签名与请求的输出格式不一致。
    SignatureMismatch,
    /// 24kHz 16-bit little-endian PCM 字节没有对齐到完整采样帧。
    InvalidPcmFrame,
}

impl fmt::Display for GeneratedSpeechAudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSize => "生成语音字节数无效",
            Self::SignatureMismatch => "生成语音文件签名与格式不一致",
            Self::InvalidPcmFrame => "生成语音 PCM 采样帧无效",
        };
        formatter.write_str(message)
    }
}

impl Error for GeneratedSpeechAudioError {}

fn validate_size(length: usize) -> Result<(), GeneratedSpeechAudioError> {
    if length == 0 || length > MAX_GENERATED_SPEECH_BYTES {
        Err(GeneratedSpeechAudioError::InvalidSize)
    } else {
        Ok(())
    }
}

fn matches_signature(format: AudioSpeechOutputFormat, bytes: &[u8]) -> bool {
    match format {
        AudioSpeechOutputFormat::Mp3 => is_mpeg_audio(bytes),
        AudioSpeechOutputFormat::Opus => {
            bytes.starts_with(b"OggS")
                && bytes[..bytes.len().min(512)]
                    .windows(b"OpusHead".len())
                    .any(|window| window == b"OpusHead")
        }
        AudioSpeechOutputFormat::Aac => {
            bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0
        }
        AudioSpeechOutputFormat::Flac => bytes.starts_with(b"fLaC"),
        AudioSpeechOutputFormat::Wav => {
            bytes.len() >= 12
                && (&bytes[..4] == b"RIFF" || &bytes[..4] == b"RIFX" || &bytes[..4] == b"RF64")
                && &bytes[8..12] == b"WAVE"
        }
        AudioSpeechOutputFormat::Pcm => true,
    }
}

fn is_mpeg_audio(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 && bytes[1] & 0x06 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_budget_rejects_empty_and_oversized_lengths_without_allocating() {
        assert_eq!(
            validate_size(0),
            Err(GeneratedSpeechAudioError::InvalidSize)
        );
        assert_eq!(
            validate_size(MAX_GENERATED_SPEECH_BYTES + 1),
            Err(GeneratedSpeechAudioError::InvalidSize)
        );
        assert!(validate_size(MAX_GENERATED_SPEECH_BYTES).is_ok());
    }
}
