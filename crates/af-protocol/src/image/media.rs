use std::{error::Error, fmt};

use bytes::Bytes;

use super::ImageOutputFormat;

/// 单张解码后图片允许的最大字节数。
pub const MAX_GENERATED_IMAGE_BYTES: usize = 50 * 1_024 * 1_024;
/// 单次响应全部解码后图片允许的最大字节数。
pub const MAX_TOTAL_GENERATED_IMAGE_BYTES: usize = 128 * 1_024 * 1_024;
/// 单个上游图片 URL 的最大长度。
pub const MAX_GENERATED_IMAGE_URL_BYTES: usize = 8_192;

/// 一张经过 Base64 解码和文件签名校验的生成图片。
#[derive(Clone, Eq, PartialEq)]
pub struct GeneratedImage {
    source: GeneratedImageSource,
}

#[derive(Clone, Eq, PartialEq)]
enum GeneratedImageSource {
    Bytes {
        format: ImageOutputFormat,
        bytes: Vec<u8>,
        revised_prompt: Option<String>,
    },
    Url {
        url: String,
        revised_prompt: Option<String>,
    },
}

impl GeneratedImage {
    /// 从解码后的图片字节构造受限结果并识别文件格式。
    pub fn new(bytes: Vec<u8>) -> Result<Self, GeneratedImageError> {
        Self::from_bytes(bytes, None)
    }

    /// 从解码后的图片字节和可选提示词改写构造受限结果。
    pub fn from_bytes(
        bytes: Vec<u8>,
        revised_prompt: Option<String>,
    ) -> Result<Self, GeneratedImageError> {
        if bytes.is_empty() || bytes.len() > MAX_GENERATED_IMAGE_BYTES {
            return Err(GeneratedImageError::InvalidSize);
        }
        if revised_prompt
            .as_ref()
            .is_some_and(|prompt| prompt.len() > crate::MAX_IMAGE_PROMPT_BYTES)
        {
            return Err(GeneratedImageError::InvalidSize);
        }
        let format = detect_image_format(&bytes).ok_or(GeneratedImageError::InvalidFormat)?;
        Ok(Self {
            source: GeneratedImageSource::Bytes {
                format,
                bytes,
                revised_prompt,
            },
        })
    }

    /// 从 OpenAI Images 的临时 URL 响应构造图片结果，不会由网关访问该 URL。
    pub fn from_url(
        value: String,
        revised_prompt: Option<String>,
    ) -> Result<Self, GeneratedImageError> {
        if value.is_empty()
            || value.len() > MAX_GENERATED_IMAGE_URL_BYTES
            || value.chars().any(char::is_control)
            || revised_prompt
                .as_ref()
                .is_some_and(|prompt| prompt.len() > crate::MAX_IMAGE_PROMPT_BYTES)
        {
            return Err(GeneratedImageError::InvalidUrl);
        }
        let parsed = url::Url::parse(&value).map_err(|_| GeneratedImageError::InvalidUrl)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(GeneratedImageError::InvalidUrl);
        }
        Ok(Self {
            source: GeneratedImageSource::Url {
                url: value,
                revised_prompt,
            },
        })
    }

    /// 返回由文件签名识别的输出格式。
    #[must_use]
    pub const fn format(&self) -> Option<ImageOutputFormat> {
        match &self.source {
            GeneratedImageSource::Bytes { format, .. } => Some(*format),
            GeneratedImageSource::Url { .. } => None,
        }
    }

    /// 返回经过预算校验的图片字节。
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.source {
            GeneratedImageSource::Bytes { bytes, .. } => Some(bytes),
            GeneratedImageSource::Url { .. } => None,
        }
    }

    /// 返回 OpenAI Images 临时图片 URL。
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        match &self.source {
            GeneratedImageSource::Bytes { .. } => None,
            GeneratedImageSource::Url { url, .. } => Some(url),
        }
    }

    /// 返回上游生成的提示词改写。
    #[must_use]
    pub fn revised_prompt(&self) -> Option<&str> {
        match &self.source {
            GeneratedImageSource::Bytes { revised_prompt, .. } => revised_prompt.as_deref(),
            GeneratedImageSource::Url { revised_prompt, .. } => revised_prompt.as_deref(),
        }
    }

    /// 返回 Base64 图片实际字节数；URL 结果不计入图片正文预算。
    #[must_use]
    pub fn byte_length(&self) -> usize {
        self.bytes().map_or(0, <[u8]>::len)
    }
}

impl fmt::Debug for GeneratedImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedImage")
            .field("format", &self.format())
            .field("is_url", &self.url().is_some())
            .field("byte_length", &self.byte_length())
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
    /// 图片 URL 不是受支持的 HTTP(S) 地址或超过容量上限。
    InvalidUrl,
}

impl fmt::Display for GeneratedImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSize => "生成图片字节数无效",
            Self::InvalidFormat => "生成图片文件格式无效",
            Self::InvalidUrl => "生成图片 URL 无效",
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

/// 单张修图输入允许的最大字节数。
pub const MAX_IMAGE_EDIT_FILE_BYTES: usize = 30 * 1_024 * 1_024;
/// 修图输入允许的最大边长。
pub const MAX_IMAGE_EDIT_EDGE: u32 = 7_680;
/// 修图输入允许的最大总像素数。
pub const MAX_IMAGE_EDIT_PIXELS: u64 = 33_177_600;

/// 一张已检查格式、尺寸和字节预算的修图输入。
#[derive(Clone, Eq, PartialEq)]
pub struct ImageEditFile {
    format: ImageOutputFormat,
    width: u32,
    height: u32,
    bytes: Bytes,
}

impl ImageEditFile {
    /// 从上传文件名、可选 MIME 和正文构造修图输入。
    pub fn from_upload(
        file_name: &str,
        content_type: Option<&str>,
        bytes: Bytes,
    ) -> Result<Self, ImageEditFileError> {
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_EDIT_FILE_BYTES {
            return Err(ImageEditFileError::InvalidSize);
        }
        let format = detect_image_format(&bytes).ok_or(ImageEditFileError::InvalidFormat)?;
        let extension = file_name
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .filter(|extension| !extension.is_empty() && extension.is_ascii());
        let declared_format = match extension {
            Some(extension) if extension.eq_ignore_ascii_case("png") => {
                Some(ImageOutputFormat::Png)
            }
            Some(extension)
                if extension.eq_ignore_ascii_case("jpg")
                    || extension.eq_ignore_ascii_case("jpeg") =>
            {
                Some(ImageOutputFormat::Jpeg)
            }
            Some(extension) if extension.eq_ignore_ascii_case("webp") => {
                Some(ImageOutputFormat::Webp)
            }
            Some(_) => return Err(ImageEditFileError::InvalidFileName),
            None if file_name.contains('.') => return Err(ImageEditFileError::InvalidFileName),
            None => None,
        };
        if declared_format.is_some_and(|declared| format != declared) {
            return Err(ImageEditFileError::FormatMismatch);
        }
        if content_type.is_some_and(|value| !accepts_image_content_type(format, value)) {
            return Err(ImageEditFileError::InvalidContentType);
        }
        let (width, height) =
            image_dimensions(format, &bytes).ok_or(ImageEditFileError::InvalidDimensions)?;
        if width == 0
            || height == 0
            || width > MAX_IMAGE_EDIT_EDGE
            || height > MAX_IMAGE_EDIT_EDGE
            || u64::from(width)
                .checked_mul(u64::from(height))
                .is_none_or(|pixels| pixels > MAX_IMAGE_EDIT_PIXELS)
        {
            return Err(ImageEditFileError::InvalidDimensions);
        }
        Ok(Self {
            format,
            width,
            height,
            bytes,
        })
    }

    /// 返回签名确认的图片格式。
    #[must_use]
    pub const fn format(&self) -> ImageOutputFormat {
        self.format
    }

    /// 返回图片宽度。
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 返回图片高度。
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// 返回图片像素数。
    #[must_use]
    pub const fn pixels(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// 返回已验证图片字节。
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }
}

impl fmt::Debug for ImageEditFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageEditFile")
            .field("format", &self.format)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// 修图上传文件校验错误，不保留文件名、MIME 或图片字节。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageEditFileError {
    /// 文件名没有受支持的扩展名。
    InvalidFileName,
    /// 文件为空或超过字节预算。
    InvalidSize,
    /// 图片签名不是 PNG、JPEG 或 WebP。
    InvalidFormat,
    /// 文件扩展名与图片签名不匹配。
    FormatMismatch,
    /// MIME 与声明的图片格式不匹配。
    InvalidContentType,
    /// 图片没有可校验的尺寸或尺寸超出预算。
    InvalidDimensions,
}

impl fmt::Display for ImageEditFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidFileName => "修图输入文件名或扩展名无效",
            Self::InvalidSize => "修图输入文件大小无效",
            Self::InvalidFormat => "修图输入图片格式无效",
            Self::FormatMismatch => "修图输入扩展名与图片签名不匹配",
            Self::InvalidContentType => "修图输入 MIME 与图片格式不匹配",
            Self::InvalidDimensions => "修图输入图片尺寸无效",
        };
        formatter.write_str(message)
    }
}

impl Error for ImageEditFileError {}

fn accepts_image_content_type(format: ImageOutputFormat, content_type: &str) -> bool {
    let essence = content_type
        .split_once(';')
        .map_or(content_type, |(essence, _)| essence)
        .trim();
    if essence.eq_ignore_ascii_case("application/octet-stream") {
        return true;
    }
    match format {
        ImageOutputFormat::Png => essence.eq_ignore_ascii_case("image/png"),
        ImageOutputFormat::Jpeg => ["image/jpeg", "image/jpg", "image/pjpeg"]
            .iter()
            .any(|value| essence.eq_ignore_ascii_case(value)),
        ImageOutputFormat::Webp => essence.eq_ignore_ascii_case("image/webp"),
    }
}

fn image_dimensions(format: ImageOutputFormat, bytes: &[u8]) -> Option<(u32, u32)> {
    match format {
        ImageOutputFormat::Png => png_dimensions(bytes),
        ImageOutputFormat::Jpeg => jpeg_dimensions(bytes),
        ImageOutputFormat::Webp => webp_dimensions(bytes),
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || bytes.get(12..16)? != b"IHDR" || bytes.get(8..12)? != [0, 0, 0, 13] {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return None;
    }
    let mut offset = 2;
    while offset < bytes.len().saturating_sub(2) {
        if bytes[offset] != 0xff {
            return None;
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if matches!(marker, 0xd8 | 0x01 | 0xd0..=0xd7) {
            continue;
        }
        let segment_length = usize::from(u16::from_be_bytes(
            bytes.get(offset..offset + 2)?.try_into().ok()?,
        ));
        if segment_length < 2 {
            return None;
        }
        let end = offset.checked_add(segment_length)?;
        if end > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u32::from(u16::from_be_bytes(
                bytes.get(offset + 3..offset + 5)?.try_into().ok()?,
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes.get(offset + 5..offset + 7)?.try_into().ok()?,
            ));
            return Some((width, height));
        }
        offset = end;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 25 || !bytes.starts_with(b"RIFF") || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    let chunk = bytes.get(12..16)?;
    let data = bytes.get(20..)?;
    match chunk {
        b"VP8X" if data.len() >= 10 => {
            let width = 1 + read_u24_le(data.get(4..7)?)?;
            let height = 1 + read_u24_le(data.get(7..10)?)?;
            Some((width, height))
        }
        b"VP8 " if data.len() >= 10 && data.get(3..6)? == [0x9d, 0x01, 0x2a] => {
            let width = u16::from_le_bytes(data.get(6..8)?.try_into().ok()?) & 0x3fff;
            let height = u16::from_le_bytes(data.get(8..10)?.try_into().ok()?) & 0x3fff;
            Some((u32::from(width), u32::from(height)))
        }
        b"VP8L" if data.len() >= 5 && data.first() == Some(&0x2f) => {
            let b1 = u32::from(*data.get(1)?);
            let b2 = u32::from(*data.get(2)?);
            let b3 = u32::from(*data.get(3)?);
            let b4 = u32::from(*data.get(4)?);
            let width = 1 + b1 + ((b2 & 0x3f) << 8);
            let height = 1 + ((b2 >> 6) & 0x03) + (b3 << 2) + ((b4 & 0x0f) << 10);
            Some((width, height))
        }
        _ => None,
    }
}

fn read_u24_le(bytes: &[u8]) -> Option<u32> {
    Some(
        u32::from(*bytes.first()?)
            | (u32::from(*bytes.get(1)?) << 8)
            | (u32::from(*bytes.get(2)?) << 16),
    )
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
            Some(ImageOutputFormat::Png)
        );
        assert_eq!(
            GeneratedImage::new(vec![0xff, 0xd8, 0xff, 0xff, 0xd9])
                .unwrap()
                .format(),
            Some(ImageOutputFormat::Jpeg)
        );
        assert_eq!(
            GeneratedImage::new(b"RIFF\x00\x00\x00\x00WEBP".to_vec())
                .unwrap()
                .format(),
            Some(ImageOutputFormat::Webp)
        );
        assert!(GeneratedImage::new(b"not-an-image".to_vec()).is_err());
    }

    #[test]
    fn generated_image_accepts_url_results_without_fetching_them() {
        let image = GeneratedImage::from_url(
            "https://images.example/result.png?signature=private".to_owned(),
            Some("revised prompt".to_owned()),
        )
        .unwrap();
        assert_eq!(
            image.url(),
            Some("https://images.example/result.png?signature=private")
        );
        assert_eq!(image.revised_prompt(), Some("revised prompt"));
        assert_eq!(image.format(), None);
        assert_eq!(image.byte_length(), 0);
        assert!(GeneratedImage::from_url("file:///private/image.png".to_owned(), None).is_err());
    }
}
