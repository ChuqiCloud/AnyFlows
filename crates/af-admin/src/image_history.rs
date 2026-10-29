use std::{fmt, future::Future, path::PathBuf, pin::Pin};

use af_db::{
    ImageHistoryRecord, ImageHistoryRecordParts, ImageHistoryRepository,
    ImageHistoryRepositoryError, ImageHistoryWrite,
};
use af_httpclient::{HeaderMap, Method, PooledClient};
use thiserror::Error;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use uuid::Uuid;

use crate::SessionPrincipal;

/// 每个账号最多保留的图片历史数量。
pub const MAX_IMAGE_HISTORY_PER_USER: usize = 30;
/// 每个账号的图片与缩略图总存储上限。
pub const MAX_IMAGE_HISTORY_BYTES_PER_USER: u64 = 100 * 1024 * 1024;
/// 单张历史缩略图的最大字节数。
pub const MAX_IMAGE_HISTORY_THUMBNAIL_BYTES: usize = 512 * 1024;
/// 单张历史原图的最大字节数。
pub const MAX_IMAGE_HISTORY_IMAGE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImageOutputFormat {
    Png,
    Jpeg,
    Webp,
}

impl ImageOutputFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

/// 上传到私有图片历史的已校验字段。
pub struct ImageHistoryUpload {
    model: String,
    prompt: String,
    revised_prompt: Option<String>,
    request_id: Option<String>,
    image: ImageHistoryImageSource,
    thumbnail: Option<(ImageOutputFormat, Vec<u8>)>,
}

enum ImageHistoryImageSource {
    Bytes(ImageOutputFormat, Vec<u8>),
    Url(String),
}

impl fmt::Debug for ImageHistoryImageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(format, bytes) => formatter
                .debug_tuple("Bytes")
                .field(format)
                .field(&bytes.len())
                .finish(),
            Self::Url(_) => formatter.write_str("Url(<redacted>)"),
        }
    }
}

impl ImageHistoryUpload {
    /// 校验图片元数据和本地文件预算。
    pub fn new(
        model: String,
        prompt: String,
        revised_prompt: Option<String>,
        request_id: Option<String>,
        image: Option<Vec<u8>>,
        image_url: Option<String>,
        thumbnail: Option<Vec<u8>>,
    ) -> Result<Self, ImageHistoryError> {
        validate_text(&model, 256, false)?;
        validate_prompt(&prompt, false)?;
        if let Some(revised_prompt) = &revised_prompt {
            validate_prompt(revised_prompt, true)?;
        }
        if let Some(request_id) = &request_id {
            validate_text(request_id, 128, true)?;
        }
        let image = match (image, image_url) {
            (Some(bytes), None) => {
                let (format, bytes) = validate_image(bytes)?;
                ImageHistoryImageSource::Bytes(format, bytes)
            }
            (None, Some(url)) => ImageHistoryImageSource::Url(validate_image_url(url)?),
            _ => return Err(ImageHistoryError::InvalidInput),
        };
        let thumbnail = if let Some(thumbnail) = thumbnail {
            if thumbnail.is_empty() || thumbnail.len() > MAX_IMAGE_HISTORY_THUMBNAIL_BYTES {
                return Err(ImageHistoryError::InvalidInput);
            }
            Some(validate_image(thumbnail)?)
        } else {
            None
        };
        Ok(Self {
            model,
            prompt,
            revised_prompt,
            request_id,
            image,
            thumbnail,
        })
    }
}

impl fmt::Debug for ImageHistoryUpload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryUpload")
            .field("image", &self.image)
            .field("has_thumbnail", &self.thumbnail.is_some())
            .finish_non_exhaustive()
    }
}

/// 当前用户可查看的图片历史元数据。
pub struct ImageHistoryEntry {
    id: String,
    model: String,
    prompt: String,
    revised_prompt: Option<String>,
    request_id: Option<String>,
    format: String,
    byte_size: u64,
    thumbnail_format: Option<String>,
    thumbnail_byte_size: Option<u64>,
    created_at: i64,
}

/// 图片历史应用记录中可移交给接口层的命名字段。
pub struct ImageHistoryEntryParts {
    pub id: String,
    pub model: String,
    pub prompt: String,
    pub revised_prompt: Option<String>,
    pub request_id: Option<String>,
    pub format: String,
    pub byte_size: u64,
    pub thumbnail_format: Option<String>,
    pub thumbnail_byte_size: Option<u64>,
    pub created_at: i64,
}

impl ImageHistoryEntry {
    /// 消费记录并返回接口响应所需字段。
    #[must_use]
    pub fn into_parts(self) -> ImageHistoryEntryParts {
        ImageHistoryEntryParts {
            id: self.id,
            model: self.model,
            prompt: self.prompt,
            revised_prompt: self.revised_prompt,
            request_id: self.request_id,
            format: self.format,
            byte_size: self.byte_size,
            thumbnail_format: self.thumbnail_format,
            thumbnail_byte_size: self.thumbnail_byte_size,
            created_at: self.created_at,
        }
    }
}

impl fmt::Debug for ImageHistoryEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryEntry")
            .field("format", &self.format)
            .field("byte_size", &self.byte_size)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

/// 当前用户读取的图片原始字节及格式。
pub struct ImageHistoryContent {
    format: String,
    bytes: Vec<u8>,
}

impl ImageHistoryContent {
    /// 消费内容并返回图片格式与字节。
    #[must_use]
    pub fn into_parts(self) -> (String, Vec<u8>) {
        (self.format, self.bytes)
    }
}

impl fmt::Debug for ImageHistoryContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryContent")
            .field("format", &self.format)
            .field("byte_size", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

/// 私有图片历史操作的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ImageHistoryError {
    /// 图片、提示词、模型或格式不符合上传边界。
    #[error("图片历史参数无效")]
    InvalidInput,
    /// 图片历史标识未知或不属于当前用户。
    #[error("图片历史不存在")]
    NotFound,
    /// 当前账号的图片历史空间已达到上限。
    #[error("图片历史空间已达上限")]
    LimitReached,
    /// 安全图片下载或本地持久化当前不可用。
    #[error("图片历史服务暂不可用")]
    Unavailable,
    /// 数据库或本地存储发生内部故障。
    #[error("图片历史存储失败")]
    Internal,
}

pub type ImageHistoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ImageHistoryError>> + Send + 'a>>;

/// 登录用户的本地图片历史应用端口。
pub trait ImageHistoryService: Send + Sync {
    /// 保存一张归属于当前用户的图片及缩略图。
    fn save(
        &self,
        principal: SessionPrincipal,
        upload: ImageHistoryUpload,
    ) -> ImageHistoryFuture<'_, ImageHistoryEntry>;

    /// 列出当前用户最近的有界历史。
    fn list(&self, principal: SessionPrincipal) -> ImageHistoryFuture<'_, Vec<ImageHistoryEntry>>;

    /// 读取当前用户的原图或缩略图。
    fn read(
        &self,
        principal: SessionPrincipal,
        image_id: String,
        thumbnail: bool,
    ) -> ImageHistoryFuture<'_, ImageHistoryContent>;

    /// 删除当前用户的一张图片历史。
    fn delete(&self, principal: SessionPrincipal, image_id: String) -> ImageHistoryFuture<'_, ()>;

    /// 清空当前用户的图片历史。
    fn clear(&self, principal: SessionPrincipal) -> ImageHistoryFuture<'_, ()>;
}

/// 本地文件与数据库元数据组合的用户私有图片历史服务。
pub struct LocalImageHistoryService {
    repository: ImageHistoryRepository,
    directory: PathBuf,
    write_lock: Mutex<()>,
    http_client: PooledClient,
}

impl LocalImageHistoryService {
    /// 使用单独配置的图片目录和数据库元数据仓储创建服务。
    pub fn new(
        repository: ImageHistoryRepository,
        directory: PathBuf,
        http_client: PooledClient,
    ) -> Result<Self, ImageHistoryServiceConfigError> {
        if directory.as_os_str().is_empty() {
            return Err(ImageHistoryServiceConfigError::EmptyDirectory);
        }
        std::fs::create_dir_all(&directory)
            .map_err(|_| ImageHistoryServiceConfigError::StorageDirectoryUnavailable)?;
        set_private_directory_permissions_sync(&directory)
            .map_err(|_| ImageHistoryServiceConfigError::StorageDirectoryUnavailable)?;
        Ok(Self {
            repository,
            directory,
            write_lock: Mutex::new(()),
            http_client,
        })
    }

    async fn resolve_image(
        &self,
        source: ImageHistoryImageSource,
    ) -> Result<(ImageOutputFormat, Vec<u8>), ImageHistoryError> {
        match source {
            ImageHistoryImageSource::Bytes(format, bytes) => Ok((format, bytes)),
            ImageHistoryImageSource::Url(url) => {
                let mut target = url;
                for redirect_count in 0..=3 {
                    let response = self
                        .http_client
                        .execute(Method::GET, &target, HeaderMap::new(), None)
                        .await
                        .map_err(|_| ImageHistoryError::Unavailable)?;
                    if response.status().is_redirection() {
                        if redirect_count == 3 {
                            return Err(ImageHistoryError::InvalidInput);
                        }
                        let location = response
                            .headers()
                            .get("location")
                            .and_then(|value| value.to_str().ok())
                            .ok_or(ImageHistoryError::InvalidInput)?;
                        let current = url::Url::parse(&target)
                            .map_err(|_| ImageHistoryError::InvalidInput)?;
                        let next = current
                            .join(location)
                            .map_err(|_| ImageHistoryError::InvalidInput)?;
                        if current.scheme() == "https" && next.scheme() != "https" {
                            return Err(ImageHistoryError::InvalidInput);
                        }
                        target = validate_image_url(next.to_string())?;
                        continue;
                    }
                    if !response.status().is_success()
                        || response
                            .content_length()
                            .is_some_and(|length| length > MAX_IMAGE_HISTORY_IMAGE_BYTES as u64)
                    {
                        return Err(ImageHistoryError::InvalidInput);
                    }
                    let mut stream = response.into_bytes_stream();
                    let mut bytes = Vec::new();
                    while let Some(chunk) = stream.next_chunk().await {
                        let chunk = chunk.map_err(|_| ImageHistoryError::Unavailable)?;
                        if bytes.len().saturating_add(chunk.len()) > MAX_IMAGE_HISTORY_IMAGE_BYTES {
                            return Err(ImageHistoryError::InvalidInput);
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    return validate_image(bytes);
                }
                Err(ImageHistoryError::InvalidInput)
            }
        }
    }

    fn image_path(&self, owner_id: i64, image_id: &str, format: &str, thumbnail: bool) -> PathBuf {
        self.directory
            .join("users")
            .join(owner_id.to_string())
            .join(if thumbnail {
                format!("{image_id}.thumb.{format}")
            } else {
                format!("{image_id}.{format}")
            })
    }

    async fn remove_files(&self, owner_id: i64, image_id: &str) {
        for format in ["png", "jpeg", "webp"] {
            for thumbnail in [false, true] {
                let _ =
                    tokio::fs::remove_file(self.image_path(owner_id, image_id, format, thumbnail))
                        .await;
            }
        }
    }

    async fn write_file(
        &self,
        path: &std::path::Path,
        bytes: &[u8],
    ) -> Result<(), ImageHistoryError> {
        let parent = path.parent().ok_or(ImageHistoryError::Internal)?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| ImageHistoryError::Internal)?;
        set_private_directory_permissions(parent).await?;
        if let Some(user_root) = parent.parent() {
            set_private_directory_permissions(user_root).await?;
        }
        let temporary = path.with_extension("pending");
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await
            .map_err(|_| ImageHistoryError::Internal)?;
        if file.write_all(bytes).await.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(ImageHistoryError::Internal);
        }
        drop(file);
        if tokio::fs::rename(&temporary, path).await.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(ImageHistoryError::Internal);
        }
        if let Err(error) = set_private_file_permissions(path).await {
            let _ = tokio::fs::remove_file(path).await;
            return Err(error);
        }
        Ok(())
    }

    async fn read_content(
        &self,
        owner_id: af_domain::UserId,
        image_id: &str,
        thumbnail: bool,
    ) -> Result<ImageHistoryContent, ImageHistoryError> {
        if !valid_image_id(image_id) {
            return Err(ImageHistoryError::NotFound);
        }
        let record = self
            .repository
            .find(owner_id, image_id)
            .await
            .map_err(map_repository_error)?
            .ok_or(ImageHistoryError::NotFound)?;
        let ImageHistoryRecordParts {
            id,
            format,
            byte_size,
            thumbnail_format,
            thumbnail_byte_size: thumbnail_size,
            ..
        } = record.into_parts();
        let (format, expected_size, use_thumbnail) = if thumbnail && thumbnail_size.is_some() {
            (
                thumbnail_format.unwrap_or(format),
                thumbnail_size.unwrap_or(byte_size),
                true,
            )
        } else {
            (format, byte_size, false)
        };
        let bytes = tokio::fs::read(self.image_path(owner_id.get(), &id, &format, use_thumbnail))
            .await
            .map_err(|_| ImageHistoryError::NotFound)?;
        if bytes.len() as u64 != expected_size {
            return Err(ImageHistoryError::Internal);
        }
        Ok(ImageHistoryContent { format, bytes })
    }
}

impl ImageHistoryService for LocalImageHistoryService {
    fn save(
        &self,
        principal: SessionPrincipal,
        upload: ImageHistoryUpload,
    ) -> ImageHistoryFuture<'_, ImageHistoryEntry> {
        Box::pin(async move {
            let owner_id = principal.user_id();
            let (image_format, image_bytes) = self.resolve_image(upload.image).await?;
            let thumbnail_size = upload
                .thumbnail
                .as_ref()
                .map(|(_, bytes)| bytes.len() as u64);
            let _guard = self.write_lock.lock().await;

            let id = Uuid::new_v4().simple().to_string();
            let image_path = self.image_path(owner_id.get(), &id, image_format.as_str(), false);
            self.write_file(&image_path, &image_bytes).await?;
            if let Some((thumbnail_format, thumbnail)) = &upload.thumbnail
                && let Err(error) = self
                    .write_file(
                        &self.image_path(owner_id.get(), &id, thumbnail_format.as_str(), true),
                        thumbnail,
                    )
                    .await
            {
                self.remove_files(owner_id.get(), &id).await;
                return Err(error);
            }
            let write = ImageHistoryWrite {
                id: id.clone(),
                model: upload.model,
                prompt: upload.prompt,
                revised_prompt: upload.revised_prompt,
                request_id: upload.request_id,
                format: image_format.as_str().to_owned(),
                byte_size: image_bytes.len() as u64,
                thumbnail_format: upload
                    .thumbnail
                    .as_ref()
                    .map(|(format, _)| format.as_str().to_owned()),
                thumbnail_byte_size: thumbnail_size,
            };
            let (record, evicted_ids) = match self
                .repository
                .create(
                    owner_id,
                    write,
                    MAX_IMAGE_HISTORY_PER_USER,
                    MAX_IMAGE_HISTORY_BYTES_PER_USER,
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    self.remove_files(owner_id.get(), &id).await;
                    return Err(map_repository_error(error));
                }
            };
            for evicted_id in evicted_ids {
                self.remove_files(owner_id.get(), &evicted_id).await;
            }
            Ok(entry_from_record(record))
        })
    }

    fn list(&self, principal: SessionPrincipal) -> ImageHistoryFuture<'_, Vec<ImageHistoryEntry>> {
        Box::pin(async move {
            self.repository
                .list(principal.user_id())
                .await
                .map_err(map_repository_error)
                .map(|records| records.into_iter().map(entry_from_record).collect())
        })
    }

    fn read(
        &self,
        principal: SessionPrincipal,
        image_id: String,
        thumbnail: bool,
    ) -> ImageHistoryFuture<'_, ImageHistoryContent> {
        Box::pin(async move {
            self.read_content(principal.user_id(), &image_id, thumbnail)
                .await
        })
    }

    fn delete(&self, principal: SessionPrincipal, image_id: String) -> ImageHistoryFuture<'_, ()> {
        Box::pin(async move {
            let _guard = self.write_lock.lock().await;
            if !valid_image_id(&image_id) {
                return Err(ImageHistoryError::NotFound);
            }
            let owner_id = principal.user_id();
            if !self
                .repository
                .delete(owner_id, &image_id)
                .await
                .map_err(map_repository_error)?
            {
                return Err(ImageHistoryError::NotFound);
            }
            self.remove_files(owner_id.get(), &image_id).await;
            Ok(())
        })
    }

    fn clear(&self, principal: SessionPrincipal) -> ImageHistoryFuture<'_, ()> {
        Box::pin(async move {
            let _guard = self.write_lock.lock().await;
            let owner_id = principal.user_id();
            let ids = self
                .repository
                .delete_all(owner_id)
                .await
                .map_err(map_repository_error)?;
            for id in ids {
                self.remove_files(owner_id.get(), &id).await;
            }
            Ok(())
        })
    }
}

/// 图片历史服务构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ImageHistoryServiceConfigError {
    /// 图片目录不能为空。
    #[error("图片历史目录不能为空")]
    EmptyDirectory,
    /// 图片目录无法创建或访问。
    #[error("图片历史目录不可用")]
    StorageDirectoryUnavailable,
}

fn entry_from_record(record: ImageHistoryRecord) -> ImageHistoryEntry {
    let ImageHistoryRecordParts {
        id,
        model,
        prompt,
        revised_prompt,
        request_id,
        format,
        byte_size,
        thumbnail_format,
        thumbnail_byte_size,
        created_at,
    } = record.into_parts();
    ImageHistoryEntry {
        id,
        model,
        prompt,
        revised_prompt,
        request_id,
        format,
        byte_size,
        thumbnail_format,
        thumbnail_byte_size,
        created_at,
    }
}

fn validate_text(
    value: &str,
    max_bytes: usize,
    empty_allowed: bool,
) -> Result<(), ImageHistoryError> {
    if (!empty_allowed && value.is_empty())
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ImageHistoryError::InvalidInput);
    }
    Ok(())
}

fn validate_prompt(value: &str, empty_allowed: bool) -> Result<(), ImageHistoryError> {
    if (!empty_allowed && value.is_empty())
        || value.len() > 64 * 1024
        || value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(ImageHistoryError::InvalidInput);
    }
    Ok(())
}

fn validate_image(bytes: Vec<u8>) -> Result<(ImageOutputFormat, Vec<u8>), ImageHistoryError> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_HISTORY_IMAGE_BYTES {
        return Err(ImageHistoryError::InvalidInput);
    }
    let format = detect_image_format(&bytes).ok_or(ImageHistoryError::InvalidInput)?;
    Ok((format, bytes))
}

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

fn validate_image_url(value: String) -> Result<String, ImageHistoryError> {
    if value.is_empty() || value.len() > 8 * 1024 || value.chars().any(char::is_control) {
        return Err(ImageHistoryError::InvalidInput);
    }
    let url = url::Url::parse(&value).map_err(|_| ImageHistoryError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ImageHistoryError::InvalidInput);
    }
    Ok(url.to_string())
}

fn valid_image_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && value.bytes().any(|byte| byte != b'0')
}

fn map_repository_error(error: ImageHistoryRepositoryError) -> ImageHistoryError {
    match error {
        ImageHistoryRepositoryError::InvalidInput => ImageHistoryError::InvalidInput,
        ImageHistoryRepositoryError::OwnerUnavailable | ImageHistoryRepositoryError::Conflict => {
            ImageHistoryError::Internal
        }
        _ => ImageHistoryError::Internal,
    }
}

#[cfg(unix)]
fn set_private_directory_permissions_sync(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory_permissions_sync(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn set_private_directory_permissions(
    path: &std::path::Path,
) -> Result<(), ImageHistoryError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .await
        .map_err(|_| ImageHistoryError::Internal)
}

#[cfg(not(unix))]
async fn set_private_directory_permissions(
    _path: &std::path::Path,
) -> Result<(), ImageHistoryError> {
    Ok(())
}

#[cfg(unix)]
async fn set_private_file_permissions(path: &std::path::Path) -> Result<(), ImageHistoryError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|_| ImageHistoryError::Internal)
}

#[cfg(not(unix))]
async fn set_private_file_permissions(_path: &std::path::Path) -> Result<(), ImageHistoryError> {
    Ok(())
}
