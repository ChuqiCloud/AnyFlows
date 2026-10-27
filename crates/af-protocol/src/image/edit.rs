use std::{error::Error, fmt};

use af_domain::Operation;

use super::{CanonicalImageGenerationRequest, ImageEditFile, ImageGenerationOptions};

/// 单次修图请求允许的图片文件数量上限，不包含可选 mask。
pub const MAX_IMAGE_EDIT_FILES: usize = 16;
/// 单次修图请求全部输入文件允许的字节上限。
pub const MAX_IMAGE_EDIT_TOTAL_BYTES: usize = 30 * 1_024 * 1_024;
/// OpenAI Images mask 的独占字节上限（官方要求小于 4 MB）。
pub const MAX_IMAGE_EDIT_MASK_BYTES: usize = 4_000_000;

/// OpenAI GPT Image 修图输入保真度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageInputFidelity {
    /// 优先速度与成本。
    Low,
    /// 优先保留输入图像细节。
    High,
}

impl ImageInputFidelity {
    /// 返回 OpenAI Images multipart 中的字段值。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }
}

/// OpenAI Images 修图请求的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalImageEditRequest {
    model: String,
    prompt: String,
    options: ImageGenerationOptions,
    input_fidelity: Option<ImageInputFidelity>,
    images: Vec<ImageEditFile>,
    mask: Option<ImageEditFile>,
    total_image_bytes: usize,
    total_image_pixels: u64,
}

impl CanonicalImageEditRequest {
    /// 构造经过模型、提示词、图像和 mask 关联校验的修图请求。
    pub fn new(
        model: String,
        prompt: String,
        options: ImageGenerationOptions,
        images: Vec<ImageEditFile>,
        mask: Option<ImageEditFile>,
    ) -> Result<Self, CanonicalImageEditRequestError> {
        let common =
            CanonicalImageGenerationRequest::new(model, prompt, options).map_err(|error| {
                match error {
                    super::CanonicalImageGenerationRequestError::InvalidModel => {
                        CanonicalImageEditRequestError::InvalidModel
                    }
                    super::CanonicalImageGenerationRequestError::InvalidPrompt => {
                        CanonicalImageEditRequestError::InvalidPrompt
                    }
                    super::CanonicalImageGenerationRequestError::InvalidParameterCombination => {
                        CanonicalImageEditRequestError::InvalidParameterCombination
                    }
                }
            })?;
        if images.is_empty() || images.len() > MAX_IMAGE_EDIT_FILES {
            return Err(CanonicalImageEditRequestError::InvalidImageCount);
        }
        if let Some(mask) = &mask
            && (mask.format() != super::ImageOutputFormat::Png
                || mask.bytes().len() >= MAX_IMAGE_EDIT_MASK_BYTES
                || mask.width() != images[0].width()
                || mask.height() != images[0].height())
        {
            return Err(CanonicalImageEditRequestError::InvalidMask);
        }
        let total_image_bytes = images
            .iter()
            .map(|image| image.bytes().len())
            .chain(mask.iter().map(|image| image.bytes().len()))
            .try_fold(0_usize, |total, bytes| total.checked_add(bytes))
            .filter(|total| *total <= MAX_IMAGE_EDIT_TOTAL_BYTES)
            .ok_or(CanonicalImageEditRequestError::ImageBudgetExceeded)?;
        let total_image_pixels = images
            .iter()
            .chain(mask.iter())
            .map(ImageEditFile::pixels)
            .sum();
        Ok(Self {
            model: common.model().to_owned(),
            prompt: common.prompt().to_owned(),
            options,
            input_fidelity: None,
            images,
            mask,
            total_image_bytes,
            total_image_pixels,
        })
    }

    /// 返回该请求固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::ImageEdit
    }

    /// 返回尚未经过渠道映射的客户端模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回经过预算校验的修图提示词。
    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// 返回提示词 UTF-8 字节数。
    #[must_use]
    pub fn prompt_bytes(&self) -> usize {
        self.prompt.len()
    }

    /// 返回已校验的图片生成参数。
    #[must_use]
    pub const fn options(&self) -> ImageGenerationOptions {
        self.options
    }

    /// 返回调用方显式指定的输入图像保真度。
    #[must_use]
    pub const fn input_fidelity(&self) -> Option<ImageInputFidelity> {
        self.input_fidelity
    }

    /// 设置可选输入图像保真度，不为特定模型补默认值。
    #[must_use]
    pub const fn with_input_fidelity(mut self, input_fidelity: Option<ImageInputFidelity>) -> Self {
        self.input_fidelity = input_fidelity;
        self
    }

    /// 返回修图输入图片。
    #[must_use]
    pub fn images(&self) -> &[ImageEditFile] {
        &self.images
    }

    /// 返回可选的 PNG 遮罩。
    #[must_use]
    pub const fn mask(&self) -> Option<&ImageEditFile> {
        self.mask.as_ref()
    }

    /// 返回所有输入文件的总字节数，包含 mask。
    #[must_use]
    pub const fn total_image_bytes(&self) -> usize {
        self.total_image_bytes
    }

    /// 返回所有图片输入的总像素数，包含可选 mask。
    #[must_use]
    pub const fn total_image_pixels(&self) -> u64 {
        self.total_image_pixels
    }

    /// 仅替换已映射到上游的模型名，保留已校验的请求数据。
    pub fn with_model(&self, model: String) -> Result<Self, CanonicalImageEditRequestError> {
        Self::new(
            model,
            self.prompt.clone(),
            self.options,
            self.images.clone(),
            self.mask.clone(),
        )
        .map(|request| request.with_input_fidelity(self.input_fidelity))
    }
}

impl fmt::Debug for CanonicalImageEditRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalImageEditRequest")
            .field("operation", &Operation::ImageEdit)
            .field("model", &"<已脱敏>")
            .field("prompt", &"<已脱敏>")
            .field("prompt_bytes", &self.prompt.len())
            .field("options", &self.options)
            .field("input_fidelity", &self.input_fidelity)
            .field("image_count", &self.images.len())
            .field("has_mask", &self.mask.is_some())
            .field("total_image_bytes", &self.total_image_bytes)
            .field("total_image_pixels", &self.total_image_pixels)
            .finish()
    }
}

/// Canonical 修图请求错误，不包含外部模型名、提示词或图片内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalImageEditRequestError {
    /// 模型名无效。
    InvalidModel,
    /// 提示词为空或超过字符、字节预算。
    InvalidPrompt,
    /// 图片数量不在允许范围内。
    InvalidImageCount,
    /// 输入图片和 mask 总字节数超过预算。
    ImageBudgetExceeded,
    /// mask 必须小于 4 MB，为 PNG，且与第一张输入图片尺寸相同。
    InvalidMask,
    /// 图片参数之间存在不兼容组合。
    InvalidParameterCombination,
}

impl fmt::Display for CanonicalImageEditRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidModel => "修图模型名无效",
            Self::InvalidPrompt => "修图提示词无效",
            Self::InvalidImageCount => "修图输入图片数量无效",
            Self::ImageBudgetExceeded => "修图输入图片超过总字节预算",
            Self::InvalidMask => "修图 mask 必须为同尺寸 PNG 图片",
            Self::InvalidParameterCombination => "修图图片参数组合无效",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalImageEditRequestError {}
