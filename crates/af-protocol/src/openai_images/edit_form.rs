use std::{collections::BTreeMap, error::Error, fmt};

use bytes::Bytes;

use crate::{
    CanonicalImageEditRequest, ImageEditFile, ImageGenerationOptions, ImageInputFidelity,
    MAX_IMAGE_EDIT_FILE_BYTES, MAX_IMAGE_EDIT_FILES, MAX_IMAGE_EDIT_MASK_BYTES,
    MAX_IMAGE_EDIT_TOTAL_BYTES, MAX_IMAGE_PROMPT_BYTES,
};

/// 修图 multipart 表单允许的部件数量。
pub const MAX_IMAGE_EDIT_FORM_PARTS: usize = 32;
/// 修图表单全部文本值允许的最大字节数。
pub const MAX_IMAGE_EDIT_FORM_TEXT_BYTES: usize = MAX_IMAGE_PROMPT_BYTES;
/// multipart 字段名允许的最大字节数。
pub const MAX_IMAGE_EDIT_FIELD_NAME_BYTES: usize = 64;
/// multipart 文件名允许的最大字节数。
pub const MAX_IMAGE_EDIT_FILE_NAME_BYTES: usize = 255;
/// multipart MIME 允许的最大字节数。
pub const MAX_IMAGE_EDIT_CONTENT_TYPE_BYTES: usize = 128;

/// HTTP 层完成原始 multipart 解析后交给协议层的有序修图表单。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiImageEditForm {
    parts: Vec<OpenAiImageEditFormPart>,
}

impl OpenAiImageEditForm {
    /// 构造受部件数、文本总量和图片总量约束的表单。
    pub fn new(parts: Vec<OpenAiImageEditFormPart>) -> Result<Self, OpenAiImageEditFormError> {
        if parts.len() > MAX_IMAGE_EDIT_FORM_PARTS {
            return Err(OpenAiImageEditFormError::TooManyParts);
        }
        let mut text_bytes = 0_usize;
        let mut file_bytes = 0_usize;
        for part in &parts {
            match part {
                OpenAiImageEditFormPart::Text(part) => {
                    text_bytes = text_bytes
                        .checked_add(part.value.len())
                        .ok_or(OpenAiImageEditFormError::TextBudgetExceeded)?;
                    if text_bytes > MAX_IMAGE_EDIT_FORM_TEXT_BYTES {
                        return Err(OpenAiImageEditFormError::TextBudgetExceeded);
                    }
                }
                OpenAiImageEditFormPart::File(part) => {
                    if part.name == "mask" && part.bytes.len() >= MAX_IMAGE_EDIT_MASK_BYTES {
                        return Err(OpenAiImageEditFormError::InvalidFileSize);
                    }
                    file_bytes = file_bytes
                        .checked_add(part.bytes.len())
                        .ok_or(OpenAiImageEditFormError::FileBudgetExceeded)?;
                    if part.bytes.is_empty() || part.bytes.len() > MAX_IMAGE_EDIT_FILE_BYTES {
                        return Err(OpenAiImageEditFormError::InvalidFileSize);
                    }
                    if file_bytes > MAX_IMAGE_EDIT_TOTAL_BYTES {
                        return Err(OpenAiImageEditFormError::FileBudgetExceeded);
                    }
                }
            }
        }
        Ok(Self { parts })
    }

    /// 返回有序只读表单部件。
    #[must_use]
    pub fn parts(&self) -> &[OpenAiImageEditFormPart] {
        &self.parts
    }

    pub(super) fn into_parts(self) -> Vec<OpenAiImageEditFormPart> {
        self.parts
    }
}

impl fmt::Debug for OpenAiImageEditForm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text_parts = self
            .parts
            .iter()
            .filter(|part| matches!(part, OpenAiImageEditFormPart::Text(_)))
            .count();
        let file_parts = self.parts.len().saturating_sub(text_parts);
        formatter
            .debug_struct("OpenAiImageEditForm")
            .field("part_count", &self.parts.len())
            .field("text_part_count", &text_parts)
            .field("file_part_count", &file_parts)
            .finish()
    }
}

/// OpenAI Images 修图表单中的文本或文件部件。
#[derive(Clone, Eq, PartialEq)]
pub enum OpenAiImageEditFormPart {
    /// 普通文本字段。
    Text(OpenAiImageEditTextPart),
    /// 图片文件字段。
    File(OpenAiImageEditFilePart),
}

impl OpenAiImageEditFormPart {
    /// 构造受控文本部件。
    pub fn text(name: String, value: String) -> Result<Self, OpenAiImageEditFormError> {
        validate_part_name(&name)?;
        if value.len() > MAX_IMAGE_EDIT_FORM_TEXT_BYTES {
            return Err(OpenAiImageEditFormError::TextValueTooLarge);
        }
        Ok(Self::Text(OpenAiImageEditTextPart { name, value }))
    }

    /// 构造受控图片文件部件；格式和签名由协议解析器关联校验。
    pub fn file(
        name: String,
        file_name: String,
        content_type: Option<String>,
        bytes: Bytes,
    ) -> Result<Self, OpenAiImageEditFormError> {
        validate_part_name(&name)?;
        if file_name.is_empty()
            || file_name.len() > MAX_IMAGE_EDIT_FILE_NAME_BYTES
            || file_name.trim() != file_name
            || file_name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\' | '"'))
        {
            return Err(OpenAiImageEditFormError::InvalidFileName);
        }
        if content_type.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > MAX_IMAGE_EDIT_CONTENT_TYPE_BYTES
                || value.trim() != value
                || !value.is_ascii()
                || value.chars().any(char::is_control)
        }) {
            return Err(OpenAiImageEditFormError::InvalidContentType);
        }
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_EDIT_FILE_BYTES {
            return Err(OpenAiImageEditFormError::InvalidFileSize);
        }
        Ok(Self::File(OpenAiImageEditFilePart {
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
    pub const fn as_text(&self) -> Option<&OpenAiImageEditTextPart> {
        match self {
            Self::Text(part) => Some(part),
            Self::File(_) => None,
        }
    }

    /// 返回文件部件视图。
    #[must_use]
    pub const fn as_file(&self) -> Option<&OpenAiImageEditFilePart> {
        match self {
            Self::Text(_) => None,
            Self::File(part) => Some(part),
        }
    }
}

impl fmt::Debug for OpenAiImageEditFormPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(part) => part.fmt(formatter),
            Self::File(part) => part.fmt(formatter),
        }
    }
}

/// 受控 multipart 文本字段。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiImageEditTextPart {
    name: String,
    value: String,
}

impl OpenAiImageEditTextPart {
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

impl fmt::Debug for OpenAiImageEditTextPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiImageEditTextPart")
            .field("name", &self.name)
            .field("value", &"<已脱敏>")
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

/// 受控 multipart 图片文件字段。
#[derive(Clone, Eq, PartialEq)]
pub struct OpenAiImageEditFilePart {
    name: String,
    file_name: String,
    content_type: Option<String>,
    bytes: Bytes,
}

impl OpenAiImageEditFilePart {
    /// 返回字段名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回用于格式校验的文件名；调用方不得写入日志。
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// 返回客户端声明的 MIME。
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// 返回已预算校验的原始图片字节。
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    pub(super) fn into_parts(self) -> (String, String, Option<String>, Bytes) {
        (self.name, self.file_name, self.content_type, self.bytes)
    }
}

impl fmt::Debug for OpenAiImageEditFilePart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiImageEditFilePart")
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

/// 修图 multipart 表单校验错误，不包含外部字段值或文件正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenAiImageEditFormError {
    /// 字段名无效。
    InvalidPartName,
    /// 单个文本值超过预算。
    TextValueTooLarge,
    /// 文件名无效。
    InvalidFileName,
    /// MIME 无效。
    InvalidContentType,
    /// 文件为空或超过单文件字节预算。
    InvalidFileSize,
    /// 表单部件数超过上限。
    TooManyParts,
    /// 文本字段超过总预算。
    TextBudgetExceeded,
    /// 图片文件超过总预算。
    FileBudgetExceeded,
}

impl fmt::Display for OpenAiImageEditFormError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidPartName => "修图表单字段名无效",
            Self::TextValueTooLarge => "修图表单文本字段超过限制",
            Self::InvalidFileName => "修图表单文件名无效",
            Self::InvalidContentType => "修图表单 MIME 无效",
            Self::InvalidFileSize => "修图表单图片字节数无效",
            Self::TooManyParts => "修图表单部件数量超过限制",
            Self::TextBudgetExceeded => "修图表单文本超过总预算",
            Self::FileBudgetExceeded => "修图表单图片超过总预算",
        };
        formatter.write_str(message)
    }
}

impl Error for OpenAiImageEditFormError {}

fn validate_part_name(name: &str) -> Result<(), OpenAiImageEditFormError> {
    if name.is_empty()
        || name.len() > MAX_IMAGE_EDIT_FIELD_NAME_BYTES
        || !name.is_ascii()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'[' | b']'))
    {
        return Err(OpenAiImageEditFormError::InvalidPartName);
    }
    Ok(())
}

#[derive(Default)]
struct EditFields {
    model: Option<String>,
    prompt: Option<String>,
    n: Option<String>,
    size: Option<String>,
    quality: Option<String>,
    background: Option<String>,
    moderation: Option<String>,
    output_format: Option<String>,
    output_compression: Option<String>,
    response_format: Option<String>,
    style: Option<String>,
    input_fidelity: Option<String>,
    stream: Option<String>,
    user: Option<String>,
    images: Vec<ImageEditFile>,
    mask: Option<ImageEditFile>,
    indexed_images: BTreeMap<usize, ImageEditFile>,
}

/// 解析已拆分并经过通用预算约束的 OpenAI Images multipart 修图表单。
pub fn parse_edit_request(
    form: OpenAiImageEditForm,
) -> Result<CanonicalImageEditRequest, ParseImageEditRequestError> {
    let mut fields = EditFields::default();
    for part in form.into_parts() {
        match part {
            OpenAiImageEditFormPart::Text(part) => {
                let (name, value) = part.into_name_value();
                fields.push_text(name, value)?;
            }
            OpenAiImageEditFormPart::File(part) => fields.push_file(part)?,
        }
    }
    fields.finish()
}

impl EditFields {
    fn push_text(&mut self, name: String, value: String) -> Result<(), ParseImageEditRequestError> {
        match name.as_str() {
            "model" => set_once(&mut self.model, value),
            "prompt" => set_once(&mut self.prompt, value),
            "n" => set_once(&mut self.n, value),
            "size" => set_once(&mut self.size, value),
            "quality" => set_once(&mut self.quality, value),
            "background" => set_once(&mut self.background, value),
            "moderation" => set_once(&mut self.moderation, value),
            "output_format" => set_once(&mut self.output_format, value),
            "output_compression" => set_once(&mut self.output_compression, value),
            "response_format" => set_once(&mut self.response_format, value),
            "style" => set_once(&mut self.style, value),
            "input_fidelity" => set_once(&mut self.input_fidelity, value),
            "stream" => set_once(&mut self.stream, value),
            "user" => set_once(&mut self.user, value),
            "partial_images" => Err(ParseImageEditRequestError::UnsupportedFeature),
            _ => Err(ParseImageEditRequestError::UnknownField),
        }
    }

    fn push_file(
        &mut self,
        part: OpenAiImageEditFilePart,
    ) -> Result<(), ParseImageEditRequestError> {
        let (name, file_name, content_type, bytes) = part.into_parts();
        if let Some(index) = image_array_index(&name)? {
            if !self.images.is_empty() || self.indexed_images.len() >= MAX_IMAGE_EDIT_FILES {
                return Err(ParseImageEditRequestError::InvalidValue);
            }
            let image = parse_image_file(file_name, content_type, bytes)?;
            if self.indexed_images.insert(index, image).is_some() {
                return Err(ParseImageEditRequestError::DuplicateField);
            }
            return Ok(());
        }
        match name.as_str() {
            "image" | "image[]" => self.push_image(file_name, content_type, bytes),
            "mask" => {
                if self.mask.is_some() {
                    return Err(ParseImageEditRequestError::DuplicateField);
                }
                let mask = ImageEditFile::from_upload(&file_name, content_type.as_deref(), bytes)
                    .map_err(|_| ParseImageEditRequestError::InvalidValue)?;
                if mask.format() != crate::ImageOutputFormat::Png {
                    return Err(ParseImageEditRequestError::InvalidValue);
                }
                self.mask = Some(mask);
                Ok(())
            }
            _ => Err(ParseImageEditRequestError::UnknownField),
        }
    }

    fn push_image(
        &mut self,
        file_name: String,
        content_type: Option<String>,
        bytes: Bytes,
    ) -> Result<(), ParseImageEditRequestError> {
        if self.images.len() >= MAX_IMAGE_EDIT_FILES {
            return Err(ParseImageEditRequestError::ImageCountExceeded);
        }
        if !self.indexed_images.is_empty() {
            return Err(ParseImageEditRequestError::InvalidValue);
        }
        self.images
            .push(parse_image_file(file_name, content_type, bytes)?);
        Ok(())
    }

    fn finish(mut self) -> Result<CanonicalImageEditRequest, ParseImageEditRequestError> {
        if !self.indexed_images.is_empty() {
            self.images = std::mem::take(&mut self.indexed_images)
                .into_values()
                .collect();
        }
        let _ignored_user = self.user;
        let model = self.model.ok_or(ParseImageEditRequestError::MissingField)?;
        let prompt = self
            .prompt
            .ok_or(ParseImageEditRequestError::MissingField)?;
        let count = self.n.map(|value| parse_image_count(&value)).transpose()?;
        let size = self
            .size
            .map(|value| crate::ImageSize::parse(&value))
            .transpose()
            .map_err(|_| ParseImageEditRequestError::InvalidValue)?;
        let quality = self
            .quality
            .map(|value| parse_quality(&value))
            .transpose()?
            .flatten();
        let background = self
            .background
            .map(|value| parse_background(&value))
            .transpose()?
            .flatten();
        let moderation = self
            .moderation
            .map(|value| parse_moderation(&value))
            .transpose()?
            .flatten();
        let output_format = self
            .output_format
            .map(|value| parse_output_format(&value))
            .transpose()?
            .flatten();
        let compression = self
            .output_compression
            .map(|value| parse_compression(&value))
            .transpose()?;
        let response_format = self
            .response_format
            .map(|value| match value.as_str() {
                "url" => Ok(crate::ImageResponseFormat::Url),
                "b64_json" => Ok(crate::ImageResponseFormat::B64Json),
                _ => Err(ParseImageEditRequestError::InvalidValue),
            })
            .transpose()?;
        let style = self
            .style
            .map(|value| match value.as_str() {
                "vivid" => Ok(crate::ImageStyle::Vivid),
                "natural" => Ok(crate::ImageStyle::Natural),
                _ => Err(ParseImageEditRequestError::InvalidValue),
            })
            .transpose()?;
        let input_fidelity = self
            .input_fidelity
            .map(|value| match value.as_str() {
                "low" => Ok(ImageInputFidelity::Low),
                "high" => Ok(ImageInputFidelity::High),
                _ => Err(ParseImageEditRequestError::InvalidValue),
            })
            .transpose()?;
        match self.stream.as_deref() {
            None | Some("false") => {}
            Some("true") => return Err(ParseImageEditRequestError::UnsupportedFeature),
            Some(_) => return Err(ParseImageEditRequestError::InvalidValue),
        }
        let options = ImageGenerationOptions::new(
            count,
            size,
            quality,
            background,
            moderation,
            output_format,
            compression,
        )
        .map_err(|_| ParseImageEditRequestError::InvalidValue)?
        .with_response_format(response_format)
        .with_style(style);
        CanonicalImageEditRequest::new(model, prompt, options, self.images, self.mask)
            .map(|request| request.with_input_fidelity(input_fidelity))
            .map_err(|_| ParseImageEditRequestError::InvalidValue)
    }
}

fn parse_image_file(
    file_name: String,
    content_type: Option<String>,
    bytes: Bytes,
) -> Result<ImageEditFile, ParseImageEditRequestError> {
    ImageEditFile::from_upload(&file_name, content_type.as_deref(), bytes)
        .map_err(|_| ParseImageEditRequestError::InvalidValue)
}

/// 读取 SDK 常用的 `image[n]` 字段序号，其他形式仍作为未知字段拒绝。
fn image_array_index(name: &str) -> Result<Option<usize>, ParseImageEditRequestError> {
    if matches!(name, "image" | "image[]") {
        return Ok(None);
    }
    let Some(index) = name
        .strip_prefix("image[")
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Ok(None);
    };
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseImageEditRequestError::UnknownField);
    }
    index
        .parse()
        .map(Some)
        .map_err(|_| ParseImageEditRequestError::InvalidValue)
}

/// 构造经重新校验、可直接作为 HTTP 请求体发送的 multipart 修图表单。
pub fn build_edit_multipart(
    request: &CanonicalImageEditRequest,
    boundary: &str,
) -> Result<Bytes, BuildImageEditRequestError> {
    validate_boundary(boundary)?;
    let mut parts = vec![
        text_part("model", request.model())?,
        text_part("prompt", request.prompt())?,
    ];
    let options = request.options();
    if let Some(count) = options.count() {
        parts.push(text_part("n", &count.get().to_string())?);
    }
    if let Some(size) = options.size() {
        parts.push(text_part("size", &size.to_string())?);
    }
    if let Some(quality) = options.quality() {
        parts.push(text_part("quality", quality.as_str())?);
    }
    if let Some(background) = options.background() {
        parts.push(text_part("background", background.as_str())?);
    }
    if let Some(moderation) = options.moderation() {
        parts.push(text_part("moderation", moderation.as_str())?);
    }
    if let Some(format) = options.output_format() {
        parts.push(text_part("output_format", format.as_str())?);
    }
    if let Some(compression) = options.compression() {
        parts.push(text_part(
            "output_compression",
            &compression.get().to_string(),
        )?);
    }
    if let Some(response_format) = options.response_format() {
        parts.push(text_part("response_format", response_format.as_str())?);
    }
    if let Some(style) = options.style() {
        parts.push(text_part("style", style.as_str())?);
    }
    if let Some(input_fidelity) = request.input_fidelity() {
        parts.push(text_part("input_fidelity", input_fidelity.as_str())?);
    }
    let image_field_name = if request.images().len() > 1 {
        "image[]"
    } else {
        "image"
    };
    for (index, image) in request.images().iter().enumerate() {
        let file_name = format!("image-{index}.{}", file_extension(image.format()));
        parts.push(
            OpenAiImageEditFormPart::file(
                image_field_name.to_owned(),
                file_name,
                Some(content_type(image.format()).to_owned()),
                image.bytes().clone(),
            )
            .map_err(|_| BuildImageEditRequestError::InvalidValue)?,
        );
    }
    if let Some(mask) = request.mask() {
        parts.push(
            OpenAiImageEditFormPart::file(
                "mask".to_owned(),
                "mask.png".to_owned(),
                Some("image/png".to_owned()),
                mask.bytes().clone(),
            )
            .map_err(|_| BuildImageEditRequestError::InvalidValue)?,
        );
    }
    for part in &parts {
        let bytes = match part {
            OpenAiImageEditFormPart::Text(part) => part.value.as_bytes(),
            OpenAiImageEditFormPart::File(part) => part.bytes.as_ref(),
        };
        if contains_boundary(bytes, boundary) {
            return Err(BuildImageEditRequestError::BoundaryCollision);
        }
    }
    let form =
        OpenAiImageEditForm::new(parts).map_err(|_| BuildImageEditRequestError::InvalidValue)?;
    let rebuilt =
        parse_edit_request(form.clone()).map_err(|_| BuildImageEditRequestError::InvalidValue)?;
    if rebuilt != *request {
        return Err(BuildImageEditRequestError::InvalidValue);
    }
    encode_multipart(&form, boundary)
}

fn encode_multipart(
    form: &OpenAiImageEditForm,
    boundary: &str,
) -> Result<Bytes, BuildImageEditRequestError> {
    let mut body = Vec::new();
    for part in form.parts() {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
        body.extend_from_slice(part.name().as_bytes());
        body.extend_from_slice(b"\"");
        match part {
            OpenAiImageEditFormPart::Text(text) => {
                body.extend_from_slice(b"\r\n\r\n");
                body.extend_from_slice(text.value.as_bytes());
            }
            OpenAiImageEditFormPart::File(file) => {
                body.extend_from_slice(b"; filename=\"");
                body.extend_from_slice(file.file_name.as_bytes());
                body.extend_from_slice(b"\"\r\nContent-Type: ");
                body.extend_from_slice(
                    file.content_type
                        .as_deref()
                        .ok_or(BuildImageEditRequestError::InvalidValue)?
                        .as_bytes(),
                );
                body.extend_from_slice(b"\r\n\r\n");
                body.extend_from_slice(file.bytes.as_ref());
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    Ok(Bytes::from(body))
}

fn text_part(
    name: &str,
    value: &str,
) -> Result<OpenAiImageEditFormPart, BuildImageEditRequestError> {
    OpenAiImageEditFormPart::text(name.to_owned(), value.to_owned())
        .map_err(|_| BuildImageEditRequestError::InvalidValue)
}

fn validate_boundary(boundary: &str) -> Result<(), BuildImageEditRequestError> {
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary.is_ascii()
        || boundary.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'\''
                        | b'('
                        | b')'
                        | b'+'
                        | b'_'
                        | b','
                        | b'-'
                        | b'.'
                        | b'/'
                        | b':'
                        | b'='
                        | b'?'
                ))
        })
    {
        return Err(BuildImageEditRequestError::InvalidBoundary);
    }
    Ok(())
}

fn contains_boundary(bytes: &[u8], boundary: &str) -> bool {
    bytes
        .windows(boundary.len())
        .any(|window| window == boundary.as_bytes())
}

fn file_extension(format: crate::ImageOutputFormat) -> &'static str {
    match format {
        crate::ImageOutputFormat::Png => "png",
        crate::ImageOutputFormat::Jpeg => "jpg",
        crate::ImageOutputFormat::Webp => "webp",
    }
}

fn content_type(format: crate::ImageOutputFormat) -> &'static str {
    match format {
        crate::ImageOutputFormat::Png => "image/png",
        crate::ImageOutputFormat::Jpeg => "image/jpeg",
        crate::ImageOutputFormat::Webp => "image/webp",
    }
}

fn set_once(slot: &mut Option<String>, value: String) -> Result<(), ParseImageEditRequestError> {
    if slot.replace(value).is_some() {
        Err(ParseImageEditRequestError::DuplicateField)
    } else {
        Ok(())
    }
}

fn parse_u32(value: &str) -> Result<u32, ()> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn parse_image_count(value: &str) -> Result<crate::ImageCount, ParseImageEditRequestError> {
    let value = parse_u32(value).map_err(|_| ParseImageEditRequestError::InvalidValue)?;
    crate::ImageCount::new(value).map_err(|_| ParseImageEditRequestError::InvalidValue)
}

fn parse_compression(value: &str) -> Result<crate::ImageCompression, ParseImageEditRequestError> {
    let value = parse_u32(value).map_err(|_| ParseImageEditRequestError::InvalidValue)?;
    crate::ImageCompression::new(value).map_err(|_| ParseImageEditRequestError::InvalidValue)
}

fn parse_quality(value: &str) -> Result<Option<crate::ImageQuality>, ParseImageEditRequestError> {
    match value {
        "auto" => Ok(Some(crate::ImageQuality::Auto)),
        "low" => Ok(Some(crate::ImageQuality::Low)),
        "medium" => Ok(Some(crate::ImageQuality::Medium)),
        "high" => Ok(Some(crate::ImageQuality::High)),
        "standard" => Ok(Some(crate::ImageQuality::Standard)),
        "hd" => Ok(Some(crate::ImageQuality::Hd)),
        _ => Err(ParseImageEditRequestError::InvalidValue),
    }
}

fn parse_background(
    value: &str,
) -> Result<Option<crate::ImageBackground>, ParseImageEditRequestError> {
    match value {
        "auto" => Ok(Some(crate::ImageBackground::Auto)),
        "opaque" => Ok(Some(crate::ImageBackground::Opaque)),
        "transparent" => Ok(Some(crate::ImageBackground::Transparent)),
        _ => Err(ParseImageEditRequestError::InvalidValue),
    }
}

fn parse_moderation(
    value: &str,
) -> Result<Option<crate::ImageModeration>, ParseImageEditRequestError> {
    match value {
        "auto" => Ok(Some(crate::ImageModeration::Auto)),
        "low" => Ok(Some(crate::ImageModeration::Low)),
        _ => Err(ParseImageEditRequestError::InvalidValue),
    }
}

fn parse_output_format(
    value: &str,
) -> Result<Option<crate::ImageOutputFormat>, ParseImageEditRequestError> {
    match value {
        "png" => Ok(Some(crate::ImageOutputFormat::Png)),
        "jpeg" => Ok(Some(crate::ImageOutputFormat::Jpeg)),
        "webp" => Ok(Some(crate::ImageOutputFormat::Webp)),
        _ => Err(ParseImageEditRequestError::InvalidValue),
    }
}

/// OpenAI Images multipart 修图请求解析错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseImageEditRequestError {
    /// model 或 prompt 缺失。
    MissingField,
    /// 单值字段重复或 mask 重复。
    DuplicateField,
    /// 存在未知表单字段。
    UnknownField,
    /// 字段类型、取值、文件签名或尺寸关联无效。
    InvalidValue,
    /// 请求包含当前不支持的能力。
    UnsupportedFeature,
    /// 图片数量超过限制。
    ImageCountExceeded,
}

impl fmt::Display for ParseImageEditRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingField => "修图请求缺少必需字段",
            Self::DuplicateField => "修图请求包含重复字段",
            Self::UnknownField => "修图请求包含未知字段",
            Self::InvalidValue => "修图请求字段或图片无效",
            Self::UnsupportedFeature => "修图请求包含当前不支持的特性",
            Self::ImageCountExceeded => "修图输入图片数量超过限制",
        };
        formatter.write_str(message)
    }
}

impl Error for ParseImageEditRequestError {}

/// OpenAI Images multipart 修图请求构造错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildImageEditRequestError {
    /// 边界格式无效。
    InvalidBoundary,
    /// 边界与文本或文件正文冲突。
    BoundaryCollision,
    /// Canonical 请求无法重新满足 multipart 约束。
    InvalidValue,
}

impl fmt::Display for BuildImageEditRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("无法构造有效的 Images 修图请求")
    }
}

impl Error for BuildImageEditRequestError {}
