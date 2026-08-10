use std::{error::Error, fmt};

/// 单次请求允许的语言提示数量。
pub const MAX_AUDIO_LANGUAGE_HINTS: usize = 16;
/// 单次请求允许的关键词数量。
pub const MAX_AUDIO_KEYWORDS: usize = 64;
/// 单个关键词允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_KEYWORD_BYTES: usize = 256;
/// 全部关键词允许的最大 UTF-8 字节数。
pub const MAX_TOTAL_AUDIO_KEYWORD_BYTES: usize = 8 * 1_024;
/// 转录提示词允许的最大 UTF-8 字节数。
pub const MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES: usize = 32 * 1_024;

const TEMPERATURE_SCALE: u32 = 1_000_000;

/// 经语法校验的音频语言代码。
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioLanguageCode(String);

impl AudioLanguageCode {
    /// 构造小写 ISO 639 或区域语言代码。
    pub fn new(value: String) -> Result<Self, AudioLanguageCodeError> {
        if value.trim() != value || !is_language_code(&value) {
            return Err(AudioLanguageCodeError::InvalidSyntax);
        }
        Ok(Self(value))
    }

    /// 返回稳定的语言代码文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 单语言或候选语言集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioLanguageHints(AudioLanguageHintsKind);

#[derive(Clone, Debug, Eq, PartialEq)]
enum AudioLanguageHintsKind {
    Single(AudioLanguageCode),
    Multiple(Vec<AudioLanguageCode>),
}

impl AudioLanguageHints {
    /// 构造一个明确的输入语言提示。
    #[must_use]
    pub const fn single(value: AudioLanguageCode) -> Self {
        Self(AudioLanguageHintsKind::Single(value))
    }

    /// 构造非空、无重复且受限的候选语言集合。
    pub fn multiple(values: Vec<AudioLanguageCode>) -> Result<Self, AudioLanguageHintsError> {
        if values.is_empty() || values.len() > MAX_AUDIO_LANGUAGE_HINTS {
            return Err(AudioLanguageHintsError::InvalidCount);
        }
        for (index, value) in values.iter().enumerate() {
            if values[..index].contains(value) {
                return Err(AudioLanguageHintsError::DuplicateLanguage);
            }
        }
        Ok(Self(AudioLanguageHintsKind::Multiple(values)))
    }

    /// 返回语言提示数量。
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.0 {
            AudioLanguageHintsKind::Single(_) => 1,
            AudioLanguageHintsKind::Multiple(values) => values.len(),
        }
    }

    /// 返回语言提示集合是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 返回明确的单语言提示。
    #[must_use]
    pub const fn as_single(&self) -> Option<&AudioLanguageCode> {
        match &self.0 {
            AudioLanguageHintsKind::Single(value) => Some(value),
            AudioLanguageHintsKind::Multiple(_) => None,
        }
    }

    /// 返回候选语言集合。
    #[must_use]
    pub fn as_multiple(&self) -> Option<&[AudioLanguageCode]> {
        match &self.0 {
            AudioLanguageHintsKind::Single(_) => None,
            AudioLanguageHintsKind::Multiple(values) => Some(values),
        }
    }
}

/// 经换行和长度校验的转录关键词。
#[derive(Clone, Eq, PartialEq)]
pub struct TranscriptionKeyword(String);

impl TranscriptionKeyword {
    /// 构造不会破坏 multipart 字段边界的关键词。
    pub fn new(value: String) -> Result<Self, TranscriptionKeywordError> {
        if value.is_empty()
            || value.len() > MAX_AUDIO_KEYWORD_BYTES
            || value.trim() != value
            || value
                .chars()
                .any(|character| character.is_control() || matches!(character, '<' | '>'))
        {
            return Err(TranscriptionKeywordError::InvalidValue);
        }
        Ok(Self(value))
    }

    /// 返回经过校验的关键词文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TranscriptionKeyword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TranscriptionKeyword")
            .field("byte_length", &self.0.len())
            .finish()
    }
}

/// 以百万分之一保存的 `0..=1` 转录温度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptionTemperature(u32);

impl TranscriptionTemperature {
    /// 从百万分之一单位构造温度。
    pub const fn from_millionths(value: u32) -> Result<Self, TranscriptionTemperatureError> {
        if value > TEMPERATURE_SCALE {
            Err(TranscriptionTemperatureError::OutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    /// 解析不超过六位小数的十进制温度。
    pub fn parse(value: &str) -> Result<Self, TranscriptionTemperatureError> {
        let scaled =
            parse_unit_decimal(value, 6).ok_or(TranscriptionTemperatureError::InvalidSyntax)?;
        let scaled =
            u32::try_from(scaled).map_err(|_| TranscriptionTemperatureError::OutOfRange)?;
        Self::from_millionths(scaled)
    }

    /// 返回百万分之一单位的温度值。
    #[must_use]
    pub const fn millionths(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TranscriptionTemperature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_scaled_decimal(formatter, u64::from(self.0), 6)
    }
}

/// 文件转录的协议无关可选参数。
#[derive(Clone, Eq, PartialEq)]
pub struct AudioTranscriptionOptions {
    prompt: Option<String>,
    language_hints: Option<AudioLanguageHints>,
    keywords: Vec<TranscriptionKeyword>,
    temperature: Option<TranscriptionTemperature>,
}

impl fmt::Debug for AudioTranscriptionOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioTranscriptionOptions")
            .field("prompt_bytes", &self.prompt.as_ref().map(String::len))
            .field(
                "language_hint_count",
                &self.language_hints.as_ref().map(AudioLanguageHints::len),
            )
            .field("keyword_count", &self.keywords.len())
            .field("temperature", &self.temperature)
            .finish()
    }
}

impl AudioTranscriptionOptions {
    /// 构造并校验提示词、语言提示、关键词和温度。
    pub fn new(
        prompt: Option<String>,
        language_hints: Option<AudioLanguageHints>,
        keywords: Vec<TranscriptionKeyword>,
        temperature: Option<TranscriptionTemperature>,
    ) -> Result<Self, AudioTranscriptionOptionsError> {
        if prompt.as_ref().is_some_and(|prompt| {
            prompt.trim().is_empty() || prompt.len() > MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES
        }) {
            return Err(AudioTranscriptionOptionsError::InvalidPrompt);
        }
        if keywords.len() > MAX_AUDIO_KEYWORDS {
            return Err(AudioTranscriptionOptionsError::TooManyKeywords);
        }
        let mut total_bytes = 0_usize;
        for (index, keyword) in keywords.iter().enumerate() {
            if keywords[..index].contains(keyword) {
                return Err(AudioTranscriptionOptionsError::DuplicateKeyword);
            }
            total_bytes = total_bytes
                .checked_add(keyword.as_str().len())
                .ok_or(AudioTranscriptionOptionsError::KeywordBudgetExceeded)?;
            if total_bytes > MAX_TOTAL_AUDIO_KEYWORD_BYTES {
                return Err(AudioTranscriptionOptionsError::KeywordBudgetExceeded);
            }
        }
        Ok(Self {
            prompt,
            language_hints,
            keywords,
            temperature,
        })
    }

    /// 返回可选的转录上下文提示词。
    #[must_use]
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    /// 返回单语言或候选语言提示。
    #[must_use]
    pub const fn language_hints(&self) -> Option<&AudioLanguageHints> {
        self.language_hints.as_ref()
    }

    /// 返回按调用方顺序保存的关键词。
    #[must_use]
    pub fn keywords(&self) -> &[TranscriptionKeyword] {
        &self.keywords
    }

    /// 返回调用方显式提供的温度。
    #[must_use]
    pub const fn temperature(&self) -> Option<TranscriptionTemperature> {
        self.temperature
    }
}

/// 语言代码校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLanguageCodeError {
    /// 语言代码不是受控的小写 ISO/区域格式。
    InvalidSyntax,
}

impl fmt::Display for AudioLanguageCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("音频语言代码格式无效")
    }
}

impl Error for AudioLanguageCodeError {}

/// 多语言提示集合错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLanguageHintsError {
    /// 候选语言数量为空或超过上限。
    InvalidCount,
    /// 候选语言集合包含重复项。
    DuplicateLanguage,
}

impl fmt::Display for AudioLanguageHintsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidCount => "音频候选语言数量无效",
            Self::DuplicateLanguage => "音频候选语言不能重复",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioLanguageHintsError {}

/// 转录关键词校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptionKeywordError {
    /// 关键词为空、超长或包含禁用字符。
    InvalidValue,
}

impl fmt::Display for TranscriptionKeywordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("音频转录关键词无效")
    }
}

impl Error for TranscriptionKeywordError {}

/// 转录温度校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptionTemperatureError {
    /// 温度不是受控的十进制文本。
    InvalidSyntax,
    /// 温度不在 `0..=1` 范围内。
    OutOfRange,
}

impl fmt::Display for TranscriptionTemperatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSyntax => "音频转录温度格式无效",
            Self::OutOfRange => "音频转录温度超出允许范围",
        };
        formatter.write_str(message)
    }
}

impl Error for TranscriptionTemperatureError {}

/// 转录可选参数组合错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioTranscriptionOptionsError {
    /// 提示词为空或超过字节预算。
    InvalidPrompt,
    /// 关键词数量超过上限。
    TooManyKeywords,
    /// 关键词集合包含重复项。
    DuplicateKeyword,
    /// 全部关键词超过单次字节预算。
    KeywordBudgetExceeded,
}

impl fmt::Display for AudioTranscriptionOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidPrompt => "音频转录提示词无效",
            Self::TooManyKeywords => "音频转录关键词数量超过限制",
            Self::DuplicateKeyword => "音频转录关键词不能重复",
            Self::KeywordBudgetExceeded => "音频转录关键词超过字节预算",
        };
        formatter.write_str(message)
    }
}

impl Error for AudioTranscriptionOptionsError {}

fn is_language_code(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let mut segments = value.split('-');
    let Some(primary) = segments.next() else {
        return false;
    };
    if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return false;
    }
    let Some(region) = segments.next() else {
        return true;
    };
    (2..=3).contains(&region.len())
        && region.bytes().all(|byte| byte.is_ascii_lowercase())
        && segments.next().is_none()
}

fn parse_unit_decimal(value: &str, fractional_digits: u32) -> Option<u64> {
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
    fn language_codes_and_temperatures_are_strict() {
        for value in ["en", "eng", "zh-cn", "yue"] {
            assert!(AudioLanguageCode::new(value.to_owned()).is_ok());
        }
        for value in ["EN", "en-US", "english", "zh-cn-extra", " en"] {
            assert!(AudioLanguageCode::new(value.to_owned()).is_err());
        }

        assert_eq!(
            TranscriptionTemperature::parse("0.125000")
                .unwrap()
                .to_string(),
            "0.125"
        );
        assert_eq!(
            TranscriptionTemperature::parse("1").unwrap().millionths(),
            TEMPERATURE_SCALE
        );
        for value in ["", ".5", "1.000001", "-0.1", "1e-1", " 0.5"] {
            assert!(TranscriptionTemperature::parse(value).is_err());
        }
    }
}
