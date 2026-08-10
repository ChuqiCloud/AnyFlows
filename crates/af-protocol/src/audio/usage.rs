use std::{error::Error, fmt};

use crate::TokenCount;

/// 协议层允许记录的最大音频时长。
pub const MAX_AUDIO_DURATION_SECONDS: u64 = 24 * 60 * 60;

const NANOS_PER_SECOND: u64 = 1_000_000_000;
const MAX_AUDIO_DURATION_NANOS: u64 = MAX_AUDIO_DURATION_SECONDS * NANOS_PER_SECOND;

/// 以纳秒固定精度保存的非负音频时长。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioDuration(u64);

impl AudioDuration {
    /// 从纳秒构造不超过 24 小时的音频时长。
    pub const fn from_nanoseconds(value: u64) -> Result<Self, AudioDurationError> {
        if value > MAX_AUDIO_DURATION_NANOS {
            Err(AudioDurationError::OutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    /// 解析最多九位小数的十进制秒值。
    pub fn parse_seconds(value: &str) -> Result<Self, AudioDurationError> {
        let nanoseconds =
            parse_seconds_to_nanoseconds(value).ok_or(AudioDurationError::InvalidSyntax)?;
        Self::from_nanoseconds(nanoseconds)
    }

    /// 返回固定精度纳秒数。
    #[must_use]
    pub const fn as_nanoseconds(self) -> u64 {
        self.0
    }

    /// 向上取整到整秒，供后续按时长计费边界使用。
    #[must_use]
    pub const fn ceil_seconds(self) -> u64 {
        let seconds = self.0 / NANOS_PER_SECOND;
        if self.0.is_multiple_of(NANOS_PER_SECOND) {
            seconds
        } else {
            seconds + 1
        }
    }
}

impl fmt::Display for AudioDuration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = self.0 / NANOS_PER_SECOND;
        let fraction = self.0 % NANOS_PER_SECOND;
        if fraction == 0 {
            return write!(formatter, "{seconds}");
        }
        let mut fraction = format!("{fraction:09}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        write!(formatter, "{seconds}.{fraction}")
    }
}

/// 可选的音频输入 token 明细。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioInputTokenDetails {
    audio_tokens: Option<TokenCount>,
    text_tokens: Option<TokenCount>,
}

impl AudioInputTokenDetails {
    /// 构造至少包含一个真实明细字段的输入 token 明细。
    pub const fn new(
        audio_tokens: Option<TokenCount>,
        text_tokens: Option<TokenCount>,
    ) -> Result<Self, AudioTranscriptionUsageError> {
        if audio_tokens.is_none() && text_tokens.is_none() {
            return Err(AudioTranscriptionUsageError::EmptyInputDetails);
        }
        Ok(Self {
            audio_tokens,
            text_tokens,
        })
    }

    /// 返回上游明确提供的音频 token 数。
    #[must_use]
    pub const fn audio_tokens(self) -> Option<TokenCount> {
        self.audio_tokens
    }

    /// 返回上游明确提供的文本 token 数。
    #[must_use]
    pub const fn text_tokens(self) -> Option<TokenCount> {
        self.text_tokens
    }

    fn validate(self, input_tokens: TokenCount) -> Result<(), AudioTranscriptionUsageError> {
        if self.audio_tokens.is_some_and(|value| value > input_tokens)
            || self.text_tokens.is_some_and(|value| value > input_tokens)
        {
            return Err(AudioTranscriptionUsageError::InputDetailExceedsTotal);
        }
        if let (Some(audio), Some(text)) = (self.audio_tokens, self.text_tokens)
            && checked_add_tokens(audio, text)? != input_tokens
        {
            return Err(AudioTranscriptionUsageError::InputDetailsMismatch);
        }
        Ok(())
    }
}

/// 按 token 计费的音频转录用量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioTranscriptionTokenUsage {
    input_tokens: TokenCount,
    output_tokens: TokenCount,
    input_details: Option<AudioInputTokenDetails>,
}

impl AudioTranscriptionTokenUsage {
    /// 构造并校验输入、输出、可选明细和总量溢出边界。
    pub fn new(
        input_tokens: TokenCount,
        output_tokens: TokenCount,
        input_details: Option<AudioInputTokenDetails>,
    ) -> Result<Self, AudioTranscriptionUsageError> {
        if let Some(details) = input_details {
            details.validate(input_tokens)?;
        }
        checked_add_tokens(input_tokens, output_tokens)?;
        Ok(Self {
            input_tokens,
            output_tokens,
            input_details,
        })
    }

    /// 返回输入 token 总量。
    #[must_use]
    pub const fn input_tokens(self) -> TokenCount {
        self.input_tokens
    }

    /// 返回输出 token 总量。
    #[must_use]
    pub const fn output_tokens(self) -> TokenCount {
        self.output_tokens
    }

    /// 返回上游明确提供的输入 token 明细。
    #[must_use]
    pub const fn input_details(self) -> Option<AudioInputTokenDetails> {
        self.input_details
    }

    /// checked 计算输入与输出 token 总量。
    pub fn checked_total_tokens(self) -> Result<TokenCount, AudioTranscriptionUsageError> {
        checked_add_tokens(self.input_tokens, self.output_tokens)
    }
}

/// 音频转录的 token 或时长联合用量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTranscriptionUsage {
    /// 按输入与输出 token 计费。
    Tokens(AudioTranscriptionTokenUsage),
    /// 按输入音频时长计费。
    Duration(AudioDuration),
}

/// 固定精度音频时长错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioDurationError {
    /// 秒值不是最多九位小数的非负十进制文本。
    InvalidSyntax,
    /// 时长超过协议允许的 24 小时上限。
    OutOfRange,
}

impl fmt::Display for AudioDurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSyntax => "音频时长格式无效",
            Self::OutOfRange => "音频时长超出允许范围",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioDurationError {}

/// 音频转录 token 用量错误，不保留外部数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTranscriptionUsageError {
    /// 输入明细对象没有提供任何真实字段。
    EmptyInputDetails,
    /// 某个输入明细超过输入总量。
    InputDetailExceedsTotal,
    /// 完整音频与文本明细之和不等于输入总量。
    InputDetailsMismatch,
    /// token 汇总超过 `i64` 上界。
    Overflow,
}

impl fmt::Display for AudioTranscriptionUsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyInputDetails => "音频输入 token 明细不能为空",
            Self::InputDetailExceedsTotal => "音频输入 token 明细不能超过输入总量",
            Self::InputDetailsMismatch => "音频输入 token 明细与总量不一致",
            Self::Overflow => "音频转录 token 汇总溢出",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioTranscriptionUsageError {}

fn checked_add_tokens(
    left: TokenCount,
    right: TokenCount,
) -> Result<TokenCount, AudioTranscriptionUsageError> {
    let value = left
        .get()
        .checked_add(right.get())
        .ok_or(AudioTranscriptionUsageError::Overflow)?;
    TokenCount::new(value).map_err(|_| AudioTranscriptionUsageError::Overflow)
}

fn parse_seconds_to_nanoseconds(value: &str) -> Option<u64> {
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
        || fraction.len() > 9
    {
        return None;
    }
    let seconds = whole.parse::<u64>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()?.checked_mul(
            10_u64.checked_pow(9_u32.checked_sub(u32::try_from(fraction.len()).ok()?)?)?,
        )?
    };
    seconds.checked_mul(NANOS_PER_SECOND)?.checked_add(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_preserves_nanosecond_precision_and_checked_ceil() {
        let duration = AudioDuration::parse_seconds("8.470000267").unwrap();
        assert_eq!(duration.to_string(), "8.470000267");
        assert_eq!(duration.ceil_seconds(), 9);
        assert_eq!(
            AudioDuration::parse_seconds("86400")
                .unwrap()
                .ceil_seconds(),
            86_400
        );
        for value in ["", "-1", ".5", "1.", "1e2", "86400.000000001"] {
            assert!(AudioDuration::parse_seconds(value).is_err());
        }
    }
}
