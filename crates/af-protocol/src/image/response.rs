use std::{error::Error, fmt};

use af_domain::Operation;

use crate::TokenCount;

use super::{
    CanonicalImageGenerationRequest, GeneratedImage, ImageBackground, ImageDimensions,
    ImageGenerationUsage, ImageOutputFormat, ImageQuality, ImageSize, MAX_IMAGE_GENERATION_COUNT,
    MAX_TOTAL_GENERATED_IMAGE_BYTES,
};

/// 图片生成非流式响应的协议无关表示。
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalImageGenerationResponse {
    created: u64,
    images: Vec<GeneratedImage>,
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
        if images.is_empty() || images.len() > MAX_IMAGE_GENERATION_COUNT {
            return Err(CanonicalImageGenerationResponseError::InvalidImageCount);
        }
        if background == Some(ImageBackground::Auto) || quality == Some(ImageQuality::Auto) {
            return Err(CanonicalImageGenerationResponseError::InvalidMetadata);
        }

        let format = images[0].format();
        let mut total_bytes = 0_usize;
        for image in &images {
            if image.format() != format {
                return Err(CanonicalImageGenerationResponseError::MixedImageFormats);
            }
            total_bytes = total_bytes
                .checked_add(image.bytes().len())
                .ok_or(CanonicalImageGenerationResponseError::ImageBudgetExceeded)?;
            if total_bytes > MAX_TOTAL_GENERATED_IMAGE_BYTES {
                return Err(CanonicalImageGenerationResponseError::ImageBudgetExceeded);
            }
        }

        Ok(Self {
            created,
            images,
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
    pub fn output_format(&self) -> ImageOutputFormat {
        self.images[0].format()
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
        self.images.iter().map(|image| image.bytes().len()).sum()
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
        if self.output_format() != options.effective_output_format() {
            return Err(CanonicalImageGenerationResponseError::OutputFormatMismatch);
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
        };
        formatter.write_str(message)
    }
}

impl Error for CanonicalImageGenerationResponseError {}
