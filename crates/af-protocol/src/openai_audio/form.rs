use std::{error::Error, fmt};

use bytes::Bytes;

use crate::{MAX_AUDIO_FILE_BYTES, MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES};

/// 单次转录表单允许的最大部件数量。
pub const MAX_TRANSCRIPTION_FORM_PARTS: usize = 128;
/// 单次转录表单全部文本值允许的最大字节数。
pub const MAX_TRANSCRIPTION_FORM_TEXT_BYTES: usize = 64 * 1_024;
/// multipart 字段名允许的最大字节数。
pub const MAX_TRANSCRIPTION_FIELD_NAME_BYTES: usize = 64;
/// multipart 文件名允许的最大字节数。
pub const MAX_TRANSCRIPTION_FILE_NAME_BYTES: usize = 255;
/// multipart 文件 MIME 允许的最大字节数。
pub const MAX_TRANSCRIPTION_CONTENT_TYPE_BYTES: usize = 128;

/// HTTP 层完成原始 multipart 解析后交给协议层的有序表单。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiTranscriptionForm {
    parts: Vec<OpenAiTranscriptionFormPart>,
}

impl OpenAiTranscriptionForm {
    /// 构造受部件数、文本总量和文件总量约束的有序表单。
    pub fn new(
        parts: Vec<OpenAiTranscriptionFormPart>,
    ) -> Result<Self, OpenAiTranscriptionFormError> {
        if parts.len() > MAX_TRANSCRIPTION_FORM_PARTS {
            return Err(OpenAiTranscriptionFormError::TooManyParts);
        }
        let mut text_bytes = 0_usize;
        let mut file_bytes = 0_usize;
        for part in &parts {
            match part {
                OpenAiTranscriptionFormPart::Text(part) => {
                    text_bytes = text_bytes
                        .checked_add(part.value.len())
                        .ok_or(OpenAiTranscriptionFormError::TextBudgetExceeded)?;
                    if text_bytes > MAX_TRANSCRIPTION_FORM_TEXT_BYTES {
                        return Err(OpenAiTranscriptionFormError::TextBudgetExceeded);
                    }
                }
                OpenAiTranscriptionFormPart::File(part) => {
                    file_bytes = file_bytes
                        .checked_add(part.bytes.len())
                        .ok_or(OpenAiTranscriptionFormError::FileBudgetExceeded)?;
                    if file_bytes > MAX_AUDIO_FILE_BYTES {
                        return Err(OpenAiTranscriptionFormError::FileBudgetExceeded);
                    }
                }
            }
        }
        Ok(Self { parts })
    }

    /// 返回有序只读表单部件，供 HTTP 或适配层编码 multipart。
    #[must_use]
    pub fn parts(&self) -> &[OpenAiTranscriptionFormPart] {
        &self.parts
    }

    pub(super) fn into_parts(self) -> Vec<OpenAiTranscriptionFormPart> {
        self.parts
    }
}

impl fmt::Debug for OpenAiTranscriptionForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text_parts = self
            .parts
            .iter()
            .filter(|part| matches!(part, OpenAiTranscriptionFormPart::Text(_)))
            .count();
        let file_parts = self.parts.len().saturating_sub(text_parts);
        formatter
            .debug_struct("OpenAiTranscriptionForm")
            .field("part_count", &self.parts.len())
            .field("text_part_count", &text_parts)
            .field("file_part_count", &file_parts)
            .finish()
    }
}

/// OpenAI 转录表单中的文本或文件部件。
#[derive(Clone, Eq, PartialEq)]
pub enum OpenAiTranscriptionFormPart {
    /// 普通文本字段。
    Text(OpenAiTranscriptionTextPart),
    /// 文件字段。
    File(OpenAiTranscriptionFilePart),
}

impl OpenAiTranscriptionFormPart {
    /// 构造受控文本部件。
    pub fn text(name: String, value: String) -> Result<Self, OpenAiTranscriptionFormError> {
        validate_part_name(&name)?;
        if value.len() > MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES {
            return Err(OpenAiTranscriptionFormError::TextValueTooLarge);
        }
        Ok(Self::Text(OpenAiTranscriptionTextPart { name, value }))
    }

    /// 构造受控文件部件；具体扩展名、MIME 和签名关联由协议解析器继续校验。
    pub fn file(
        name: String,
        file_name: String,
        content_type: Option<String>,
        bytes: Bytes,
    ) -> Result<Self, OpenAiTranscriptionFormError> {
        validate_part_name(&name)?;
        if file_name.is_empty()
            || file_name.len() > MAX_TRANSCRIPTION_FILE_NAME_BYTES
            || file_name.trim() != file_name
            || file_name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\' | '"'))
        {
            return Err(OpenAiTranscriptionFormError::InvalidFileName);
        }
        if content_type.as_ref().is_some_and(|content_type| {
            content_type.is_empty()
                || content_type.len() > MAX_TRANSCRIPTION_CONTENT_TYPE_BYTES
                || content_type.trim() != content_type
                || !content_type.is_ascii()
                || content_type.chars().any(char::is_control)
        }) {
            return Err(OpenAiTranscriptionFormError::InvalidContentType);
        }
        if bytes.is_empty() || bytes.len() > MAX_AUDIO_FILE_BYTES {
            return Err(OpenAiTranscriptionFormError::InvalidFileSize);
        }
        Ok(Self::File(OpenAiTranscriptionFilePart {
            name,
            file_name,
            content_type,
            bytes,
        }))
    }

    /// 返回 multipart 字段名。
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Text(part) => part.name(),
            Self::File(part) => part.name(),
        }
    }

    /// 返回文本部件视图。
    #[must_use]
    pub const fn as_text(&self) -> Option<&OpenAiTranscriptionTextPart> {
        match self {
            Self::Text(part) => Some(part),
            Self::File(_) => None,
        }
    }

    /// 返回文件部件视图。
    #[must_use]
    pub const fn as_file(&self) -> Option<&OpenAiTranscriptionFilePart> {
        match self {
            Self::Text(_) => None,
            Self::File(part) => Some(part),
        }
    }
}

impl fmt::Debug for OpenAiTranscriptionFormPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(part) => part.fmt(formatter),
            Self::File(part) => part.fmt(formatter),
        }
    }
}

/// 受控 multipart 文本部件。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiTranscriptionTextPart {
    name: String,
    value: String,
}

impl OpenAiTranscriptionTextPart {
    /// 返回字段名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回字段值；调用方不得写入日志。
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    pub(super) fn into_name_value(self) -> (String, String) {
        (self.name, self.value)
    }
}

impl fmt::Debug for OpenAiTranscriptionTextPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiTranscriptionTextPart")
            .field("name", &self.name)
            .field("value", &"<已脱敏>")
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

/// 受控 multipart 文件部件。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiTranscriptionFilePart {
    name: String,
    file_name: String,
    content_type: Option<String>,
    bytes: Bytes,
}

impl OpenAiTranscriptionFilePart {
    /// 返回字段名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回经过元数据预算校验的文件名；调用方不得写入日志。
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// 返回客户端声明的可选 MIME。
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// 返回经过单文件预算校验的字节。
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    pub(super) fn into_metadata_and_bytes(self) -> (String, Option<String>, Bytes) {
        (self.file_name, self.content_type, self.bytes)
    }
}

impl fmt::Debug for OpenAiTranscriptionFilePart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiTranscriptionFilePart")
            .field("name", &self.name)
            .field("file_name", &"<已脱敏>")
            .field(
                "content_type",
                &self.content_type.as_ref().map(|_| "<已脱敏>"),
            )
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// 转录表单构造错误，不保留字段值、文件名或文件正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiTranscriptionFormError {
    /// 字段名为空、超长或含 multipart 边界外字符。
    InvalidPartName,
    /// 单个文本值超过部件预算。
    TextValueTooLarge,
    /// 文件名为空、超长或包含路径/控制字符。
    InvalidFileName,
    /// 文件 MIME 为空、超长或包含非法字符。
    InvalidContentType,
    /// 文件为空或超过单文件预算。
    InvalidFileSize,
    /// 表单部件数量超过上限。
    TooManyParts,
    /// 全部文本值超过表单预算。
    TextBudgetExceeded,
    /// 全部文件字节超过单请求预算。
    FileBudgetExceeded,
}

impl fmt::Display for OpenAiTranscriptionFormError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidPartName => "音频转录表单字段名无效",
            Self::TextValueTooLarge => "音频转录表单文本字段超过限制",
            Self::InvalidFileName => "音频转录表单文件名无效",
            Self::InvalidContentType => "音频转录表单文件 MIME 无效",
            Self::InvalidFileSize => "音频转录表单文件字节数无效",
            Self::TooManyParts => "音频转录表单部件数量超过限制",
            Self::TextBudgetExceeded => "音频转录表单文本超过总预算",
            Self::FileBudgetExceeded => "音频转录表单文件超过总预算",
        };
        formatter.write_str(message)
    }
}

impl Error for OpenAiTranscriptionFormError {}

fn validate_part_name(name: &str) -> Result<(), OpenAiTranscriptionFormError> {
    if name.is_empty()
        || name.len() > MAX_TRANSCRIPTION_FIELD_NAME_BYTES
        || !name.is_ascii()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'[' | b']'))
    {
        return Err(OpenAiTranscriptionFormError::InvalidPartName);
    }
    Ok(())
}
