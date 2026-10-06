use std::{error::Error, fmt};

use bytes::Bytes;

/// OpenAI 文件转录允许的单文件最大字节数。
pub const MAX_AUDIO_FILE_BYTES: usize = 25 * 1_024 * 1_024;

/// OpenAI 文件转录支持的音频容器或编码格式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AudioFileFormat {
    /// MPEG Layer III 音频。
    Mp3,
    /// ISO Base Media 容器。
    Mp4,
    /// MPEG 音频或节目流。
    Mpeg,
    /// MPEG 音频别名。
    Mpga,
    /// MPEG-4 Audio 容器。
    M4a,
    /// RIFF Wave 音频。
    Wav,
    /// WebM 容器。
    Webm,
}

impl AudioFileFormat {
    /// 从受控文件名扩展名识别官方支持的格式。
    pub fn from_file_name(file_name: &str) -> Result<Self, AudioFileError> {
        let (_, extension) = file_name
            .rsplit_once('.')
            .ok_or(AudioFileError::UnsupportedFormat)?;
        if extension.is_empty() || !extension.is_ascii() {
            return Err(AudioFileError::UnsupportedFormat);
        }
        if extension.eq_ignore_ascii_case("mp3") {
            Ok(Self::Mp3)
        } else if extension.eq_ignore_ascii_case("mp4") {
            Ok(Self::Mp4)
        } else if extension.eq_ignore_ascii_case("mpeg") {
            Ok(Self::Mpeg)
        } else if extension.eq_ignore_ascii_case("mpga") {
            Ok(Self::Mpga)
        } else if extension.eq_ignore_ascii_case("m4a") {
            Ok(Self::M4a)
        } else if extension.eq_ignore_ascii_case("wav") {
            Ok(Self::Wav)
        } else if extension.eq_ignore_ascii_case("webm") {
            Ok(Self::Webm)
        } else {
            Err(AudioFileError::UnsupportedFormat)
        }
    }

    /// 返回向上游重建 multipart 文件名时使用的稳定扩展名。
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Mp4 => "mp4",
            Self::Mpeg => "mpeg",
            Self::Mpga => "mpga",
            Self::M4a => "m4a",
            Self::Wav => "wav",
            Self::Webm => "webm",
        }
    }

    /// 返回向上游重建 multipart 文件部件时使用的稳定 MIME。
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Mp3 | Self::Mpeg | Self::Mpga => "audio/mpeg",
            Self::Mp4 | Self::M4a => "audio/mp4",
            Self::Wav => "audio/wav",
            Self::Webm => "audio/webm",
        }
    }

    /// 校验客户端声明的 MIME 是否与文件格式兼容。
    #[must_use]
    pub fn accepts_content_type(self, content_type: Option<&str>) -> bool {
        let Some(content_type) = content_type else {
            return true;
        };
        let essence = content_type
            .split_once(';')
            .map_or(content_type, |(essence, _)| essence)
            .trim();
        if essence.eq_ignore_ascii_case("application/octet-stream") {
            return true;
        }
        match self {
            Self::Mp3 => ["audio/mpeg", "audio/mp3"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
            Self::Mp4 => ["audio/mp4", "video/mp4", "application/mp4"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
            Self::Mpeg => ["audio/mpeg", "video/mpeg"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
            Self::Mpga => essence.eq_ignore_ascii_case("audio/mpeg"),
            Self::M4a => ["audio/mp4", "audio/x-m4a"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
            Self::Wav => ["audio/wav", "audio/wave", "audio/x-wav"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
            Self::Webm => ["audio/webm", "video/webm"]
                .iter()
                .any(|value| essence.eq_ignore_ascii_case(value)),
        }
    }

    fn matches_signature(self, bytes: &[u8]) -> bool {
        match self {
            Self::Mp3 | Self::Mpga => is_mpeg_audio(bytes),
            Self::Mpeg => is_mpeg_audio(bytes) || is_mpeg_program_stream(bytes),
            Self::Mp4 | Self::M4a => bytes.len() >= 12 && &bytes[4..8] == b"ftyp",
            Self::Wav => {
                bytes.len() >= 12
                    && (&bytes[..4] == b"RIFF" || &bytes[..4] == b"RIFX" || &bytes[..4] == b"RF64")
                    && &bytes[8..12] == b"WAVE"
            }
            Self::Webm => bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]),
        }
    }
}

/// 经过格式、签名和字节预算校验的转录文件。
#[derive(Clone, Eq, PartialEq)]
pub struct AudioFile {
    format: AudioFileFormat,
    bytes: Bytes,
}

impl AudioFile {
    /// 构造不保留客户端原始文件名的受限音频文件。
    pub fn new(format: AudioFileFormat, bytes: Bytes) -> Result<Self, AudioFileError> {
        if bytes.is_empty() || bytes.len() > MAX_AUDIO_FILE_BYTES {
            return Err(AudioFileError::InvalidSize);
        }
        if !format.matches_signature(&bytes) {
            return Err(AudioFileError::SignatureMismatch);
        }
        Ok(Self { format, bytes })
    }

    /// 返回由扩展名和文件签名共同确认的格式。
    #[must_use]
    pub const fn format(&self) -> AudioFileFormat {
        self.format
    }

    /// 返回经过预算校验的文件字节。
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// 返回音频文件字节数，供后续预扣和审计边界使用。
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

impl fmt::Debug for AudioFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioFile")
            .field("format", &self.format)
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// 音频文件边界错误，不保留文件名、MIME 或文件正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioFileError {
    /// 文件扩展名不在官方支持集合中。
    UnsupportedFormat,
    /// 文件为空或超过单文件字节预算。
    InvalidSize,
    /// 文件签名与声明格式不一致。
    SignatureMismatch,
}

impl fmt::Display for AudioFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedFormat => "音频文件格式不受支持",
            Self::InvalidSize => "音频文件字节数无效",
            Self::SignatureMismatch => "音频文件签名与格式不一致",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioFileError {}

fn is_mpeg_audio(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 && bytes[1] & 0x06 != 0)
}

fn is_mpeg_program_stream(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..3] == [0x00, 0x00, 0x01] && matches!(bytes[3], 0xba | 0xb3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_formats_require_matching_signatures() {
        let cases = [
            (AudioFileFormat::Mp3, Bytes::from_static(b"ID3audio")),
            (
                AudioFileFormat::Mp4,
                Bytes::from_static(b"\x00\x00\x00\x18ftypM4A "),
            ),
            (
                AudioFileFormat::Mpeg,
                Bytes::from_static(b"\x00\x00\x01\xbastream"),
            ),
            (AudioFileFormat::Mpga, Bytes::from_static(b"\xff\xfbdata")),
            (
                AudioFileFormat::M4a,
                Bytes::from_static(b"\x00\x00\x00\x18ftypM4A "),
            ),
            (
                AudioFileFormat::Wav,
                Bytes::from_static(b"RIFF\x04\x00\x00\x00WAVEdata"),
            ),
            (
                AudioFileFormat::Webm,
                Bytes::from_static(b"\x1a\x45\xdf\xa3webm"),
            ),
        ];

        for (format, bytes) in cases {
            assert!(AudioFile::new(format, bytes).is_ok(), "format={format:?}");
        }
        assert_eq!(
            AudioFile::new(AudioFileFormat::Wav, Bytes::from_static(b"ID3audio")),
            Err(AudioFileError::SignatureMismatch)
        );
    }
}
