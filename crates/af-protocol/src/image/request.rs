use std::{error::Error, fmt};

use af_domain::{MAX_MODEL_NAME_BYTES, Operation};

use super::ImageGenerationOptions;

/// GPT Image 提示词允许的最大 Unicode 字符数。
pub const MAX_IMAGE_PROMPT_CHARS: usize = 32_000;
/// GPT Image 提示词允许的最大 UTF-8 字节数。
pub const MAX_IMAGE_PROMPT_BYTES: usize = 128 * 1_024;

/// 图片生成请求的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalImageGenerationRequest {
    model: String,
    prompt: String,
    options: ImageGenerationOptions,
}

impl CanonicalImageGenerationRequest {
    /// 构造经过模型、提示词和参数关联校验的图片生成请求。
    pub fn new(
        model: String,
        prompt: String,
        options: ImageGenerationOptions,
    ) -> Result<Self, CanonicalImageGenerationRequestError> {
        validate_model(&model).map_err(|_| CanonicalImageGenerationRequestError::InvalidModel)?;
        if prompt.trim().is_empty()
            || prompt.len() > MAX_IMAGE_PROMPT_BYTES
            || prompt.chars().count() > MAX_IMAGE_PROMPT_CHARS
        {
            return Err(CanonicalImageGenerationRequestError::InvalidPrompt);
        }
        Ok(Self {
            model,
            prompt,
            options,
        })
    }

    /// 返回该请求固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Image
    }

    /// 返回尚未经过渠道映射的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回经过预算校验的提示词。
    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// 返回已校验的图片生成参数。
    #[must_use]
    pub const fn options(&self) -> ImageGenerationOptions {
        self.options
    }

    /// 返回提示词 UTF-8 字节数，供后续生产预扣设置安全上界。
    #[must_use]
    pub fn prompt_bytes(&self) -> usize {
        self.prompt.len()
    }
}

impl fmt::Debug for CanonicalImageGenerationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalImageGenerationRequest")
            .field("operation", &Operation::Image)
            .field("model", &"<已脱敏>")
            .field("prompt", &"<已脱敏>")
            .field("prompt_bytes", &self.prompt.len())
            .field("options", &self.options)
            .finish()
    }
}

/// Canonical 图片生成请求错误，不保留模型名或提示词。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalImageGenerationRequestError {
    /// 模型名为空、超长、含控制字符或带首尾空白。
    InvalidModel,
    /// 提示词为空或超过字符、字节预算。
    InvalidPrompt,
    /// 图片参数之间存在不兼容组合。
    InvalidParameterCombination,
}

impl fmt::Display for CanonicalImageGenerationRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "图片生成模型名无效",
            Self::InvalidPrompt => "图片生成提示词无效",
            Self::InvalidParameterCombination => "图片生成参数组合无效",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalImageGenerationRequestError {}

fn validate_model(model: &str) -> Result<(), ()> {
    if model.is_empty()
        || model.len() > MAX_MODEL_NAME_BYTES
        || model.trim() != model
        || model.chars().any(char::is_control)
    {
        return Err(());
    }
    Ok(())
}
