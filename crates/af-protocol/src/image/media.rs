use std::{error::Error, fmt};

use super::ImageOutputFormat;

/// 单张解码后图片允许的最大字节数。
pub const MAX_GENERATED_IMAGE_BYTES: usize = 50 * 1_024 * 1_024;
/// 单次响应全部解码后图片允许的最大字节数。
pub const MAX_TOTAL_GENERATED_IMAGE_BYTES: usize = 128 * 1_024 * 1_024;

/// 一张经过 Base64 解码和文件签名校验的生成图片。
#[derive(Clone, Eq, PartialEq)]
pub struct GeneratedImage {
    format: ImageOutputFormat,
    bytes: Vec<u8>,
}

impl GeneratedImage {
    /// 从解码后的图片字节构造受限结果并识别文件格式。
    pub fn new(bytes: Vec<u8>) -> Result<Self, GeneratedImageError> {
        if bytes.is_empty() || bytes.len() > MAX_GENERATED_IMAGE_BYTES {
            return Err(GeneratedImageError::InvalidSize);
        }
        let format = detect_image_format(&bytes).ok_or(GeneratedImageError::InvalidFormat)?;
        Ok(Self { format, bytes })
    }

    /// 返回由文件签名识别的输出格式。
    #[must_use]
    pub const fn format(&self) -> ImageOutputFormat {
        self.format
    }

    /// 返回经过预算校验的图片字节。
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for GeneratedImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedImage")
            .field("format", &self.format)
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// 生成图片字节校验错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedImageError {
    /// 图片为空或超过单张字节预算。
    InvalidSize,
    /// 图片文件签名不是 PNG、JPEG 或 WebP。
    InvalidFormat,
}

impl fmt::Display for GeneratedImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSize => "生成图片字节数无效",
            Self::InvalidFormat => "生成图片文件格式无效",
        };
        formatter.write_str(message)
    }
}

impl Error for GeneratedImageError {}

fn detect_image_format(bytes: &[u8]) -> Option<ImageOutputFormat> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG_SIGNATURE) {
        return Some(ImageOutputFormat::Png);
    }
    if bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8, 0xff]) && bytes.ends_with(&[0xff, 0xd9])
    {
        return Some(ImageOutputFormat::Jpeg);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(ImageOutputFormat::Webp);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_image_detects_supported_signatures() {
        assert_eq!(
            GeneratedImage::new(b"\x89PNG\r\n\x1a\n".to_vec())
                .unwrap()
                .format(),
            ImageOutputFormat::Png
        );
        assert_eq!(
            GeneratedImage::new(vec![0xff, 0xd8, 0xff, 0xff, 0xd9])
                .unwrap()
                .format(),
            ImageOutputFormat::Jpeg
        );
        assert_eq!(
            GeneratedImage::new(b"RIFF\x00\x00\x00\x00WEBP".to_vec())
                .unwrap()
                .format(),
            ImageOutputFormat::Webp
        );
        assert!(GeneratedImage::new(b"not-an-image".to_vec()).is_err());
    }
}
