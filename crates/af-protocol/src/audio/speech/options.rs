use std::{error::Error, fmt};

/// 命名声音允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_SPEECH_VOICE_NAME_BYTES: usize = 256;
/// Custom voice ID 允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_SPEECH_VOICE_ID_BYTES: usize = 512;
/// 语音生成指令允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES: usize = 64 * 1_024;

const SPEED_SCALE: u32 = 1_000_000;
const MIN_SPEED_MILLIONTHS: u32 = 250_000;
const MAX_SPEED_MILLIONTHS: u32 = 4_000_000;

/// 经长度和控制字符校验的命名声音。
#[derive(Clone, Eq, PartialEq)]
pub struct AudioSpeechVoiceName(String);

impl AudioSpeechVoiceName {
    /// 构造保留供应商兼容名称的受限声音标识。
    pub fn new(value: String) -> Result<Self, AudioSpeechVoiceError> {
        validate_opaque_voice(&value, MAX_AUDIO_SPEECH_VOICE_NAME_BYTES)?;
        Ok(Self(value))
    }

    /// 返回经过校验的声音名称。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AudioSpeechVoiceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioSpeechVoiceName")
            .field("byte_length", &self.0.len())
            .finish()
    }
}

/// 经长度和控制字符校验的 custom voice ID。
#[derive(Clone, Eq, PartialEq)]
pub struct AudioSpeechVoiceId(String);

impl AudioSpeechVoiceId {
    /// 构造不假设供应商前缀的受限 custom voice ID。
    pub fn new(value: String) -> Result<Self, AudioSpeechVoiceError> {
        validate_opaque_voice(&value, MAX_AUDIO_SPEECH_VOICE_ID_BYTES)?;
        Ok(Self(value))
    }

    /// 返回经过校验的 custom voice ID。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AudioSpeechVoiceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioSpeechVoiceId")
            .field("byte_length", &self.0.len())
            .finish()
    }
}

/// OpenAI Audio Speech 支持的命名声音或 custom voice 引用。
#[derive(Clone, Eq, PartialEq)]
pub enum AudioSpeechVoice {
    /// 内建声音或兼容供应商提供的命名声音。
    Named(AudioSpeechVoiceName),
    /// OpenAI custom voice ID 对象。
    Custom(AudioSpeechVoiceId),
}

impl fmt::Debug for AudioSpeechVoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => formatter.debug_tuple("Named").field(name).finish(),
            Self::Custom(id) => formatter.debug_tuple("Custom").field(id).finish(),
        }
    }
}

/// Speech API 支持的音频输出格式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AudioSpeechOutputFormat {
    /// MP3 压缩音频。
    Mp3,
    /// Ogg Opus 压缩音频。
    Opus,
    /// AAC 压缩音频。
    Aac,
    /// FLAC 无损压缩音频。
    Flac,
    /// WAV 容器音频。
    Wav,
    /// 24kHz 16-bit little-endian 单声道原始 PCM 音频。
    Pcm,
}

impl AudioSpeechOutputFormat {
    /// 返回 OpenAI wire 使用的稳定格式标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Pcm => "pcm",
        }
    }

    /// 返回下游响应可使用的稳定 MIME。
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Opus => "audio/ogg",
            Self::Aac => "audio/aac",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Pcm => "audio/pcm",
        }
    }
}

impl fmt::Display for AudioSpeechOutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 以百万分之一保存的 `0.25..=4.0` 语速。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioSpeechSpeed(u32);

impl AudioSpeechSpeed {
    /// 官方默认语速 `1.0`。
    pub const DEFAULT: Self = Self(SPEED_SCALE);

    /// 从百万分之一单位构造语速。
    pub const fn from_millionths(value: u32) -> Result<Self, AudioSpeechSpeedError> {
        if value < MIN_SPEED_MILLIONTHS || value > MAX_SPEED_MILLIONTHS {
            Err(AudioSpeechSpeedError::OutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    /// 解析不超过六位小数的十进制语速。
    pub fn parse(value: &str) -> Result<Self, AudioSpeechSpeedError> {
        let scaled = parse_scaled_decimal(value, 6).ok_or(AudioSpeechSpeedError::InvalidSyntax)?;
        let scaled = u32::try_from(scaled).map_err(|_| AudioSpeechSpeedError::OutOfRange)?;
        Self::from_millionths(scaled)
    }

    /// 返回百万分之一单位的语速。
    #[must_use]
    pub const fn millionths(self) -> u32 {
        self.0
    }
}

impl fmt::Display for AudioSpeechSpeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_scaled_decimal(formatter, u64::from(self.0), 6)
    }
}

/// 首个非 SSE 切片支持的 Speech API 输出流格式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSpeechStreamFormat {
    /// 原始音频字节流；调用方仍可选择完整读取后再构造 Canonical 响应。
    Audio,
}

impl AudioSpeechStreamFormat {
    /// 返回 OpenAI wire 使用的稳定格式标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
        }
    }
}

/// OpenAI Audio Speech 的协议无关参数集合。
#[derive(Clone, Eq, PartialEq)]
pub struct AudioSpeechOptions {
    voice: AudioSpeechVoice,
    instructions: Option<String>,
    output_format: Option<AudioSpeechOutputFormat>,
    speed: Option<AudioSpeechSpeed>,
    stream_format: Option<AudioSpeechStreamFormat>,
}

impl AudioSpeechOptions {
    /// 构造并校验声音、指令、输出格式、语速和非 SSE 流格式。
    pub fn new(
        voice: AudioSpeechVoice,
        instructions: Option<String>,
        output_format: Option<AudioSpeechOutputFormat>,
        speed: Option<AudioSpeechSpeed>,
        stream_format: Option<AudioSpeechStreamFormat>,
    ) -> Result<Self, AudioSpeechOptionsError> {
        if instructions.as_ref().is_some_and(|instructions| {
            instructions.trim().is_empty()
                || instructions.len() > MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES
        }) {
            return Err(AudioSpeechOptionsError::InvalidInstructions);
        }
        Ok(Self {
            voice,
            instructions,
            output_format,
            speed,
            stream_format,
        })
    }

    /// 返回命名声音或 custom voice 引用。
    #[must_use]
    pub const fn voice(&self) -> &AudioSpeechVoice {
        &self.voice
    }

    /// 返回可选的发声控制指令。
    #[must_use]
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// 返回客户端显式提供的音频输出格式。
    #[must_use]
    pub const fn output_format(&self) -> Option<AudioSpeechOutputFormat> {
        self.output_format
    }

    /// 返回按官方默认值归一后的音频输出格式。
    #[must_use]
    pub const fn effective_output_format(&self) -> AudioSpeechOutputFormat {
        match self.output_format {
            Some(format) => format,
            None => AudioSpeechOutputFormat::Mp3,
        }
    }

    /// 返回客户端显式提供的语速。
    #[must_use]
    pub const fn speed(&self) -> Option<AudioSpeechSpeed> {
        self.speed
    }

    /// 返回按官方默认值归一后的语速。
    #[must_use]
    pub const fn effective_speed(&self) -> AudioSpeechSpeed {
        match self.speed {
            Some(speed) => speed,
            None => AudioSpeechSpeed::DEFAULT,
        }
    }

    /// 返回客户端显式提供的原始音频流格式。
    #[must_use]
    pub const fn stream_format(&self) -> Option<AudioSpeechStreamFormat> {
        self.stream_format
    }
}

impl fmt::Debug for AudioSpeechOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioSpeechOptions")
            .field("voice", &self.voice)
            .field(
                "instruction_bytes",
                &self.instructions.as_ref().map(String::len),
            )
            .field("output_format", &self.output_format)
            .field("speed", &self.speed)
            .field("stream_format", &self.stream_format)
            .finish()
    }
}

/// Speech voice 校验错误，不保留声音名称或 ID。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSpeechVoiceError {
    /// 声音名称或 ID 为空、超长、含控制字符或带首尾空白。
    InvalidValue,
}

impl fmt::Display for AudioSpeechVoiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Audio Speech 声音标识无效")
    }
}

impl Error for AudioSpeechVoiceError {}

/// Speech 语速校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSpeechSpeedError {
    /// 语速不是受控的十进制文本。
    InvalidSyntax,
    /// 语速不在 `0.25..=4.0` 范围内。
    OutOfRange,
}

impl fmt::Display for AudioSpeechSpeedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSyntax => "Audio Speech 语速格式无效",
            Self::OutOfRange => "Audio Speech 语速超出允许范围",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioSpeechSpeedError {}

/// Speech 参数组合错误，不保留指令正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioSpeechOptionsError {
    /// 指令为空白或超过单次字节预算。
    InvalidInstructions,
}

impl fmt::Display for AudioSpeechOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Audio Speech 指令无效")
    }
}

impl Error for AudioSpeechOptionsError {}

fn validate_opaque_voice(value: &str, max_bytes: usize) -> Result<(), AudioSpeechVoiceError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(AudioSpeechVoiceError::InvalidValue);
    }
    Ok(())
}

fn parse_scaled_decimal(value: &str, fractional_digits: u32) -> Option<u64> {
    if value.is_empty() || value.trim() != value {
        return None;
    }
    let (whole, fraction) = match value.split_once('.') {
        Some((_, "")) => return None,
        Some(parts) => parts,
        None => (value, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > usize::try_from(fractional_digits).ok()?
    {
        return None;
    }
    let scale = 10_u64.checked_pow(fractional_digits)?;
    let whole = whole.parse::<u64>().ok()?;
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u64>()
            .ok()?
            .checked_mul(10_u64.checked_pow(
                fractional_digits.checked_sub(u32::try_from(fraction.len()).ok()?)?,
            )?)?
    };
    whole.checked_mul(scale)?.checked_add(fraction_value)
}

fn write_scaled_decimal(
    formatter: &mut fmt::Formatter<'_>,
    value: u64,
    fractional_digits: u32,
) -> fmt::Result {
    let scale = 10_u64.checked_pow(fractional_digits).ok_or(fmt::Error)?;
    let whole = value / scale;
    let fraction = value % scale;
    if fraction == 0 {
        return write!(formatter, "{whole}");
    }
    let width = usize::try_from(fractional_digits).map_err(|_| fmt::Error)?;
    let mut fraction = format!("{fraction:0width$}");
    while fraction.ends_with('0') {
        fraction.pop();
    }
    write!(formatter, "{whole}.{fraction}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_uses_fixed_precision_and_official_bounds() {
        assert_eq!(
            AudioSpeechSpeed::parse("0.250000").unwrap().to_string(),
            "0.25"
        );
        assert_eq!(
            AudioSpeechSpeed::parse("4").unwrap().millionths(),
            4_000_000
        );
        assert_eq!(AudioSpeechSpeed::DEFAULT.to_string(), "1");
        for value in ["", ".25", "0.249999", "4.000001", "1e0", " 1"] {
            assert!(AudioSpeechSpeed::parse(value).is_err(), "value={value}");
        }
    }
}
