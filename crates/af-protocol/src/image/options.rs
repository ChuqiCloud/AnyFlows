use std::{error::Error, fmt};

/// 单次请求允许生成的最大图片张数。
pub const MAX_IMAGE_GENERATION_COUNT: usize = 10;
/// 自定义分辨率允许的最小总像素数。
pub const MIN_IMAGE_PIXELS: u32 = 655_360;
/// 自定义分辨率允许的最大总像素数。
pub const MAX_IMAGE_PIXELS: u32 = 8_294_400;
/// 自定义分辨率允许的最大单边像素数。
pub const MAX_IMAGE_EDGE: u16 = 3_840;

/// 经校验的单次图片生成张数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageCount(u8);

impl ImageCount {
    /// 从外部整数构造 `1..=10` 的图片张数。
    pub fn new(value: u32) -> Result<Self, ImageCountError> {
        let value = usize::try_from(value).map_err(|_| ImageCountError::OutOfRange)?;
        if !(1..=MAX_IMAGE_GENERATION_COUNT).contains(&value) {
            return Err(ImageCountError::OutOfRange);
        }
        Ok(Self(
            u8::try_from(value).map_err(|_| ImageCountError::OutOfRange)?,
        ))
    }

    /// 返回已校验的图片张数。
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// 图片生成张数校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageCountError {
    /// 张数不在协议允许范围内。
    OutOfRange,
}

impl fmt::Display for ImageCountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("图片生成张数超出允许范围")
    }
}

impl Error for ImageCountError {}

/// 经校验的 JPEG/WebP 压缩率。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageCompression(u8);

impl ImageCompression {
    /// 从外部整数构造 `0..=100` 的压缩率。
    pub fn new(value: u32) -> Result<Self, ImageCompressionError> {
        if value > 100 {
            return Err(ImageCompressionError::OutOfRange);
        }
        Ok(Self(
            u8::try_from(value).map_err(|_| ImageCompressionError::OutOfRange)?,
        ))
    }

    /// 返回已校验的压缩率。
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// 图片压缩率校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageCompressionError {
    /// 压缩率不在协议允许范围内。
    OutOfRange,
}

impl fmt::Display for ImageCompressionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("图片压缩率超出允许范围")
    }
}

impl Error for ImageCompressionError {}

/// GPT Image 输出背景策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageBackground {
    /// 由模型自动选择。
    Auto,
    /// 使用不透明背景。
    Opaque,
    /// 使用透明背景。
    Transparent,
}

impl ImageBackground {
    /// 返回 OpenAI wire 使用的稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Opaque => "opaque",
            Self::Transparent => "transparent",
        }
    }
}

/// GPT Image 内容审核强度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageModeration {
    /// 使用标准审核策略。
    Auto,
    /// 使用较低限制的审核策略。
    Low,
}

impl ImageModeration {
    /// 返回 OpenAI wire 使用的稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
        }
    }
}

/// GPT Image 输出质量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageQuality {
    /// 由模型自动选择。
    Auto,
    /// 低质量快速生成。
    Low,
    /// 中等质量。
    Medium,
    /// 高质量。
    High,
}

impl ImageQuality {
    /// 返回 OpenAI wire 使用的稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// GPT Image 输出文件格式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageOutputFormat {
    /// PNG 文件。
    Png,
    /// JPEG 文件。
    Jpeg,
    /// WebP 文件。
    Webp,
}

impl ImageOutputFormat {
    /// 返回 OpenAI wire 使用的稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

/// 经校验的图片像素尺寸。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageDimensions {
    width: u16,
    height: u16,
}

impl ImageDimensions {
    /// 构造满足 GPT Image 通用边界的宽高。
    pub fn new(width: u32, height: u32) -> Result<Self, ImageDimensionsError> {
        let width = u16::try_from(width).map_err(|_| ImageDimensionsError::OutOfRange)?;
        let height = u16::try_from(height).map_err(|_| ImageDimensionsError::OutOfRange)?;
        if width == 0
            || height == 0
            || width > MAX_IMAGE_EDGE
            || height > MAX_IMAGE_EDGE
            || width % 16 != 0
            || height % 16 != 0
        {
            return Err(ImageDimensionsError::OutOfRange);
        }

        let width_u32 = u32::from(width);
        let height_u32 = u32::from(height);
        let pixels = width_u32
            .checked_mul(height_u32)
            .ok_or(ImageDimensionsError::OutOfRange)?;
        if !(MIN_IMAGE_PIXELS..=MAX_IMAGE_PIXELS).contains(&pixels) {
            return Err(ImageDimensionsError::OutOfRange);
        }

        let shorter = width_u32.min(height_u32);
        let longer = width_u32.max(height_u32);
        if longer
            > shorter
                .checked_mul(3)
                .ok_or(ImageDimensionsError::OutOfRange)?
        {
            return Err(ImageDimensionsError::OutOfRange);
        }

        Ok(Self { width, height })
    }

    /// 解析 `WIDTHxHEIGHT` 形式的官方尺寸字符串。
    pub fn parse(value: &str) -> Result<Self, ImageDimensionsError> {
        let (width, height) = value
            .split_once('x')
            .ok_or(ImageDimensionsError::InvalidSyntax)?;
        if width.is_empty() || height.is_empty() || height.contains('x') {
            return Err(ImageDimensionsError::InvalidSyntax);
        }
        let width = width
            .parse::<u32>()
            .map_err(|_| ImageDimensionsError::InvalidSyntax)?;
        let height = height
            .parse::<u32>()
            .map_err(|_| ImageDimensionsError::InvalidSyntax)?;
        Self::new(width, height)
    }

    /// 返回宽度像素数。
    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    /// 返回高度像素数。
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }
}

impl fmt::Display for ImageDimensions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x{}", self.width, self.height)
    }
}

/// 图片尺寸校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageDimensionsError {
    /// 尺寸字符串不是 `WIDTHxHEIGHT`。
    InvalidSyntax,
    /// 宽高、比例或总像素不在允许范围内。
    OutOfRange,
}

impl fmt::Display for ImageDimensionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSyntax => "图片尺寸格式无效",
            Self::OutOfRange => "图片尺寸超出允许范围",
        };
        formatter.write_str(message)
    }
}

impl Error for ImageDimensionsError {}

/// GPT Image 请求尺寸。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageSize {
    /// 由模型自动选择。
    Auto,
    /// 调用方指定的精确尺寸。
    Exact(ImageDimensions),
}

impl ImageSize {
    /// 解析 `auto` 或 `WIDTHxHEIGHT`。
    pub fn parse(value: &str) -> Result<Self, ImageDimensionsError> {
        if value == "auto" {
            Ok(Self::Auto)
        } else {
            ImageDimensions::parse(value).map(Self::Exact)
        }
    }
}

impl fmt::Display for ImageSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => formatter.write_str("auto"),
            Self::Exact(dimensions) => dimensions.fmt(formatter),
        }
    }
}

/// 图片生成的可选参数集合，保留字段是否由客户端显式提供。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageGenerationOptions {
    count: Option<ImageCount>,
    size: Option<ImageSize>,
    quality: Option<ImageQuality>,
    background: Option<ImageBackground>,
    moderation: Option<ImageModeration>,
    output_format: Option<ImageOutputFormat>,
    compression: Option<ImageCompression>,
}

impl ImageGenerationOptions {
    /// 构造并校验图片参数之间的关联关系。
    pub fn new(
        count: Option<ImageCount>,
        size: Option<ImageSize>,
        quality: Option<ImageQuality>,
        background: Option<ImageBackground>,
        moderation: Option<ImageModeration>,
        output_format: Option<ImageOutputFormat>,
        compression: Option<ImageCompression>,
    ) -> Result<Self, super::CanonicalImageGenerationRequestError> {
        let effective_format = output_format.unwrap_or(ImageOutputFormat::Png);
        if compression.is_some()
            && !matches!(
                effective_format,
                ImageOutputFormat::Jpeg | ImageOutputFormat::Webp
            )
        {
            return Err(super::CanonicalImageGenerationRequestError::InvalidParameterCombination);
        }
        if background == Some(ImageBackground::Transparent)
            && effective_format == ImageOutputFormat::Jpeg
        {
            return Err(super::CanonicalImageGenerationRequestError::InvalidParameterCombination);
        }
        Ok(Self {
            count,
            size,
            quality,
            background,
            moderation,
            output_format,
            compression,
        })
    }

    /// 返回客户端显式提供的图片张数。
    #[must_use]
    pub const fn count(self) -> Option<ImageCount> {
        self.count
    }

    /// 返回按官方默认值归一后的图片张数。
    #[must_use]
    pub const fn effective_count(self) -> ImageCount {
        match self.count {
            Some(count) => count,
            None => ImageCount(1),
        }
    }

    /// 返回客户端显式提供的尺寸。
    #[must_use]
    pub const fn size(self) -> Option<ImageSize> {
        self.size
    }

    /// 返回客户端显式提供的质量。
    #[must_use]
    pub const fn quality(self) -> Option<ImageQuality> {
        self.quality
    }

    /// 返回客户端显式提供的背景策略。
    #[must_use]
    pub const fn background(self) -> Option<ImageBackground> {
        self.background
    }

    /// 返回客户端显式提供的审核强度。
    #[must_use]
    pub const fn moderation(self) -> Option<ImageModeration> {
        self.moderation
    }

    /// 返回客户端显式提供的输出格式。
    #[must_use]
    pub const fn output_format(self) -> Option<ImageOutputFormat> {
        self.output_format
    }

    /// 返回按官方默认值归一后的输出格式。
    #[must_use]
    pub const fn effective_output_format(self) -> ImageOutputFormat {
        match self.output_format {
            Some(format) => format,
            None => ImageOutputFormat::Png,
        }
    }

    /// 返回客户端显式提供的输出压缩率。
    #[must_use]
    pub const fn compression(self) -> Option<ImageCompression> {
        self.compression
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_enforce_official_bounds() {
        assert_eq!(
            ImageDimensions::new(1024, 1024).unwrap().to_string(),
            "1024x1024"
        );
        assert!(ImageDimensions::new(1024, 1536).is_ok());
        assert!(ImageDimensions::new(1536, 1024).is_ok());
        assert!(ImageDimensions::new(1024, 1000).is_err());
        assert!(ImageDimensions::new(3840, 1024).is_err());
        assert!(ImageDimensions::new(512, 512).is_err());
    }
}
