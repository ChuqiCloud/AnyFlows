use std::{error::Error, fmt};

use af_domain::Operation;

use crate::TokenCount;

use super::{
    CanonicalImageEditRequest, CanonicalImageGenerationRequest, GeneratedImage, ImageBackground,
    ImageDimensions, ImageGenerationUsage, ImageOutputFormat, ImageQuality, ImageResponseFormat,
    ImageSize, MAX_IMAGE_GENERATION_COUNT, MAX_TOTAL_GENERATED_IMAGE_BYTES,
};

/// 图片生成非流式响应的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalImageGenerationResponse {
    created: u64,
    images: Vec<GeneratedImage>,
    output_format: Option<ImageOutputFormat>,
    background: Option<ImageBackground>,
    quality: Option<ImageQuality>,
    size: Option<ImageDimensions>,
    usage: Option<ImageGenerationUsage>,
}

impl CanonicalImageGenerationResponse {
    /// 构造并校验图片集合、元数据和总内存预算。
    pub fn new(
        created: u64,
        images: Vec<GeneratedImage>,
        background: Option<ImageBackground>,
        quality: Option<ImageQuality>,
        size: Option<ImageDimensions>,
        usage: Option<ImageGenerationUsage>,
    ) -> Result<Self, CanonicalImageGenerationResponseError> {
        Self::new_with_output_format(created, images, None, background, quality, size, usage)
    }

    pub(crate) fn new_with_output_format(
        created: u64,
        images: Vec<GeneratedImage>,
        output_format: Option<ImageOutputFormat>,
        background: Option<ImageBackground>,
        quality: Option<ImageQuality>,
        size: Option<ImageDimensions>,
        usage: Option<ImageGenerationUsage>,
    ) -> Result<Self, CanonicalImageGenerationResponseError> {
        if images.is_empty() || images.len() > MAX_IMAGE_GENERATION_COUNT {
            return Err(CanonicalImageGenerationResponseError::InvalidImageCount);
        }
        if background == Some(ImageBackground::Auto) || quality == Some(ImageQuality::Auto) {
            return Err(CanonicalImageGenerationResponseError::InvalidMetadata);
        }

        let mut format = None;
        let mut total_bytes = 0_usize;
        for image in &images {
            if let Some(image_format) = image.format() {
                if format.is_some_and(|format| format != image_format) {
                    return Err(CanonicalImageGenerationResponseError::MixedImageFormats);
                }
                format = Some(image_format);
            }
            total_bytes = total_bytes
                .checked_add(image.byte_length())
                .ok_or(CanonicalImageGenerationResponseError::ImageBudgetExceeded)?;
            if total_bytes > MAX_TOTAL_GENERATED_IMAGE_BYTES {
                return Err(CanonicalImageGenerationResponseError::ImageBudgetExceeded);
            }
        }
        if output_format.is_some_and(|declared| format.is_some_and(|actual| declared != actual)) {
            return Err(CanonicalImageGenerationResponseError::InvalidMetadata);
        }

        Ok(Self {
            created,
            images,
            output_format: output_format.or(format),
            background,
            quality,
            size,
            usage,
        })
    }

    /// 返回该响应固定对应的操作类型。
    #[must_use]
    pub const fn operation(&self) -> Operation {
        Operation::Image
    }

    /// 返回上游创建时间戳。
    #[must_use]
    pub const fn created(&self) -> u64 {
        self.created
    }

    /// 返回经过解码和签名校验的图片集合。
    #[must_use]
    pub fn images(&self) -> &[GeneratedImage] {
        &self.images
    }

    /// 返回图片集合的统一文件格式。
    #[must_use]
    pub fn output_format(&self) -> Option<ImageOutputFormat> {
        self.output_format
    }

    /// 返回上游显式回显的背景策略。
    #[must_use]
    pub const fn background(&self) -> Option<ImageBackground> {
        self.background
    }

    /// 返回上游显式回显的输出质量。
    #[must_use]
    pub const fn quality(&self) -> Option<ImageQuality> {
        self.quality
    }

    /// 返回上游显式回显的输出尺寸。
    #[must_use]
    pub const fn size(&self) -> Option<ImageDimensions> {
        self.size
    }

    /// 返回上游明确提供的图片 token 用量。
    #[must_use]
    pub const fn usage(&self) -> Option<ImageGenerationUsage> {
        self.usage
    }

    /// 返回全部解码后图片字节数。
    #[must_use]
    pub fn total_image_bytes(&self) -> usize {
        self.images.iter().map(GeneratedImage::byte_length).sum()
    }

    /// 验证响应图片与原始生成请求的张数、格式、参数和输入用量一致。
    pub fn validate_for_request(
        &self,
        request: &CanonicalImageGenerationRequest,
    ) -> Result<(), CanonicalImageGenerationResponseError> {
        let options = request.options();
        if self.images.len() != usize::from(options.effective_count().get()) {
            return Err(CanonicalImageGenerationResponseError::ImageCountMismatch);
        }
        if options
            .output_format()
            .is_some_and(|format| self.output_format().is_some_and(|actual| actual != format))
        {
            return Err(CanonicalImageGenerationResponseError::OutputFormatMismatch);
        }
        if !response_format_matches(options.response_format(), &self.images) {
            return Err(CanonicalImageGenerationResponseError::ResponseFormatMismatch);
        }
        if let (Some(requested), Some(actual)) = (options.background(), self.background)
            && requested != ImageBackground::Auto
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }
        if let (Some(requested), Some(actual)) = (options.quality(), self.quality)
            && requested != ImageQuality::Auto
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }
        if let (Some(ImageSize::Exact(requested)), Some(actual)) = (options.size(), self.size)
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }

        if let Some(usage) = self.usage {
            if usage.input_details().image_tokens() != TokenCount::ZERO {
                return Err(CanonicalImageGenerationResponseError::UnexpectedInputImageUsage);
            }
            let input_tokens = usize::try_from(usage.input_tokens().get())
                .map_err(|_| CanonicalImageGenerationResponseError::UsageExceedsPromptBudget)?;
            if input_tokens > request.prompt_bytes() {
                return Err(CanonicalImageGenerationResponseError::UsageExceedsPromptBudget);
            }
        }
        Ok(())
    }

    /// 验证响应图片与原始修图请求的张数、格式、参数及输入图片用量上界。
    pub fn validate_for_edit_request(
        &self,
        request: &CanonicalImageEditRequest,
    ) -> Result<(), CanonicalImageGenerationResponseError> {
        let options = request.options();
        if self.images.len() != usize::from(options.effective_count().get()) {
            return Err(CanonicalImageGenerationResponseError::ImageCountMismatch);
        }
        if options
            .output_format()
            .is_some_and(|format| self.output_format().is_some_and(|actual| actual != format))
        {
            return Err(CanonicalImageGenerationResponseError::OutputFormatMismatch);
        }
        if !response_format_matches(options.response_format(), &self.images) {
            return Err(CanonicalImageGenerationResponseError::ResponseFormatMismatch);
        }
        if let (Some(requested), Some(actual)) = (options.background(), self.background)
            && requested != ImageBackground::Auto
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }
        if let (Some(requested), Some(actual)) = (options.quality(), self.quality)
            && requested != ImageQuality::Auto
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }
        if let (Some(ImageSize::Exact(requested)), Some(actual)) = (options.size(), self.size)
            && requested != actual
        {
            return Err(CanonicalImageGenerationResponseError::MetadataMismatch);
        }

        if let Some(usage) = self.usage {
            let text_tokens = usize::try_from(usage.input_details().text_tokens().get())
                .map_err(|_| CanonicalImageGenerationResponseError::UsageExceedsPromptBudget)?;
            if text_tokens > request.prompt_bytes() {
                return Err(CanonicalImageGenerationResponseError::UsageExceedsPromptBudget);
            }
            let image_tokens = usage.input_details().image_tokens().get();
            let image_token_bound = request.total_image_pixels().div_ceil(128);
            if u64::try_from(image_tokens).unwrap_or(u64::MAX) > image_token_bound {
                return Err(CanonicalImageGenerationResponseError::UsageExceedsImageBudget);
            }
            let total_input_bound = request
                .prompt_bytes()
                .checked_add(usize::try_from(image_token_bound).unwrap_or(usize::MAX))
                .ok_or(CanonicalImageGenerationResponseError::UsageExceedsImageBudget)?;
            let input_tokens = usize::try_from(usage.input_tokens().get())
                .map_err(|_| CanonicalImageGenerationResponseError::UsageExceedsImageBudget)?;
            if input_tokens > total_input_bound {
                return Err(CanonicalImageGenerationResponseError::UsageExceedsImageBudget);
            }
        }
        Ok(())
    }
}

impl fmt::Debug for CanonicalImageGenerationResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalImageGenerationResponse")
            .field("operation", &Operation::Image)
            .field("created", &self.created)
            .field("image_count", &self.images.len())
            .field("output_format", &self.output_format())
            .field("total_image_bytes", &self.total_image_bytes())
            .field("background", &self.background)
            .field("quality", &self.quality)
            .field("size", &self.size)
            .field("usage", &self.usage)
            .finish()
    }
}

fn response_format_matches(
    requested: Option<ImageResponseFormat>,
    images: &[GeneratedImage],
) -> bool {
    match requested {
        None => true,
        Some(ImageResponseFormat::Url) => images.iter().all(|image| image.url().is_some()),
        Some(ImageResponseFormat::B64Json) => images.iter().all(|image| image.bytes().is_some()),
    }
}

/// Canonical 图片生成响应错误，不保留图片正文或提示词。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalImageGenerationResponseError {
    /// 图片集合为空或超过单次张数上限。
    InvalidImageCount,
    /// 响应图片使用了多种文件格式。
    MixedImageFormats,
    /// 图片总字节数超过单次响应预算。
    ImageBudgetExceeded,
    /// 响应背景、质量或尺寸元数据无效。
    InvalidMetadata,
    /// 响应图片张数与原请求不一致。
    ImageCountMismatch,
    /// 响应图片格式与原请求不一致。
    OutputFormatMismatch,
    /// 响应元数据与原请求的显式参数不一致。
    MetadataMismatch,
    /// 纯文本生成请求出现了输入图片 token。
    UnexpectedInputImageUsage,
    /// 上游输入 token 超过提示词 UTF-8 字节安全上界。
    UsageExceedsPromptBudget,
    /// 上游输入图片 token 超过图片像素安全上界。
    UsageExceedsImageBudget,
    /// 上游返回的图片载体与请求的 response_format 不一致。
    ResponseFormatMismatch,
}

impl fmt::Display for CanonicalImageGenerationResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidImageCount => "图片生成响应张数无效",
            Self::MixedImageFormats => "图片生成响应包含混合文件格式",
            Self::ImageBudgetExceeded => "图片生成响应超过字节预算",
            Self::InvalidMetadata => "图片生成响应元数据无效",
            Self::ImageCountMismatch => "图片生成响应张数与请求不一致",
            Self::OutputFormatMismatch => "图片生成响应格式与请求不一致",
            Self::MetadataMismatch => "图片生成响应元数据与请求不一致",
            Self::UnexpectedInputImageUsage => "图片生成响应用量包含非预期输入图片 token",
            Self::UsageExceedsPromptBudget => "图片生成响应用量超过提示词预算",
            Self::UsageExceedsImageBudget => "图片编辑响应用量超过输入图片预算",
            Self::ResponseFormatMismatch => "图片响应载体与请求格式不一致",
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalImageGenerationResponseError {}
