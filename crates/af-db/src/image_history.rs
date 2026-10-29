use std::{fmt, future::Future, time::Duration};

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{ImageHistoryKey, SensitiveString, image_history, users},
};

pub const MAX_IMAGE_HISTORY_ENTRIES_PER_PAGE: usize = 30;
pub const MAX_IMAGE_HISTORY_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_IMAGE_HISTORY_THUMBNAIL_BYTES: u64 = 512 * 1024;
const MAX_MODEL_BYTES: usize = 256;
const MAX_PROMPT_BYTES: usize = 128 * 1024;
const MAX_REQUEST_ID_BYTES: usize = 128;

type Error = ImageHistoryRepositoryError;

/// 一张图片的待持久化元数据。图像文件本身由 af-admin 写入媒体目录。
pub struct ImageHistoryWrite {
    pub id: String,
    pub model: String,
    pub prompt: String,
    pub revised_prompt: Option<String>,
    pub request_id: Option<String>,
    pub format: String,
    pub byte_size: u64,
    pub thumbnail_format: Option<String>,
    pub thumbnail_byte_size: Option<u64>,
}

impl fmt::Debug for ImageHistoryWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryWrite")
            .field("format", &self.format)
            .field("byte_size", &self.byte_size)
            .field("thumbnail_format", &self.thumbnail_format)
            .field("thumbnail_byte_size", &self.thumbnail_byte_size)
            .finish_non_exhaustive()
    }
}

/// 当前用户可见的一张图片及其独立文件元数据。
#[derive(PartialEq, Eq)]
pub struct ImageHistoryRecord {
    id: String,
    owner_user_id: i64,
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

/// 图片历史记录中可移交给应用层的命名字段。
pub struct ImageHistoryRecordParts {
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

impl ImageHistoryRecord {
    /// 消费记录并返回面向调用方的字段。
    #[must_use]
    pub fn into_parts(self) -> ImageHistoryRecordParts {
        ImageHistoryRecordParts {
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

    #[must_use]
    pub const fn owner_user_id(&self) -> i64 {
        self.owner_user_id
    }
}

impl fmt::Debug for ImageHistoryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryRecord")
            .field("owner_user_id", &self.owner_user_id)
            .field("format", &self.format)
            .field("byte_size", &self.byte_size)
            .field("thumbnail_format", &self.thumbnail_format)
            .field("thumbnail_byte_size", &self.thumbnail_byte_size)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ImageHistoryRepositoryConfigError {
    #[error("图片历史仓储超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ImageHistoryRepositoryError {
    #[error("图片历史输入无效")]
    InvalidInput,
    #[error("图片历史所有者不可用")]
    OwnerUnavailable,
    #[error("图片历史标识已存在")]
    Conflict,
    #[error("图片历史数据库访问失败")]
    Query,
    #[error("图片历史数据库访问超时")]
    Timeout,
    #[error("图片历史持久化状态无效")]
    Invariant,
}

#[derive(Clone)]
pub struct ImageHistoryRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl ImageHistoryRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, ImageHistoryRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(ImageHistoryRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn create(
        &self,
        owner_user_id: UserId,
        write: ImageHistoryWrite,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<(ImageHistoryRecord, Vec<String>), ImageHistoryRepositoryError> {
        self.bounded(create(self, owner_user_id, write, max_entries, max_bytes))
            .await
    }

    /// 返回当前用户最近的 30 条记录，不载入媒体文件。
    pub async fn list(
        &self,
        owner_user_id: UserId,
    ) -> Result<Vec<ImageHistoryRecord>, ImageHistoryRepositoryError> {
        self.bounded(list(self, owner_user_id)).await
    }

    pub async fn find(
        &self,
        owner_user_id: UserId,
        id: &str,
    ) -> Result<Option<ImageHistoryRecord>, ImageHistoryRepositoryError> {
        let Ok(id) = ImageHistoryKey::parse(id) else {
            return Ok(None);
        };
        self.bounded(find(self, owner_user_id, id)).await
    }

    /// 删除当前用户拥有的指定记录；未知、格式错误或他人记录均返回 false。
    pub async fn delete(
        &self,
        owner_user_id: UserId,
        id: &str,
    ) -> Result<bool, ImageHistoryRepositoryError> {
        let Ok(id) = ImageHistoryKey::parse(id) else {
            return Ok(false);
        };
        self.bounded(delete(self, owner_user_id, id)).await
    }

    /// 原子删除当前用户的全部元数据并返回待清理的文件 ID。
    pub async fn delete_all(
        &self,
        owner_user_id: UserId,
    ) -> Result<Vec<String>, ImageHistoryRepositoryError> {
        self.bounded(delete_all(self, owner_user_id)).await
    }

    async fn bounded<T>(
        &self,
        operation: impl Future<Output = Result<T, ImageHistoryRepositoryError>>,
    ) -> Result<T, ImageHistoryRepositoryError> {
        timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| record_internal_error(Error::Timeout))?
    }
}

impl fmt::Debug for ImageHistoryRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageHistoryRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn create(
    repository: &ImageHistoryRepository,
    owner_user_id: UserId,
    write: ImageHistoryWrite,
    max_entries: usize,
    max_bytes: u64,
) -> Result<(ImageHistoryRecord, Vec<String>), Error> {
    let image_id = ImageHistoryKey::parse(&write.id).map_err(|_| Error::InvalidInput)?;
    validate_write(&write)?;
    let byte_size = i64::try_from(write.byte_size).map_err(|_| Error::InvalidInput)?;
    let thumbnail_byte_size = write
        .thumbnail_byte_size
        .map(i64::try_from)
        .transpose()
        .map_err(|_| Error::InvalidInput)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    if !lock_active_owner(&transaction, owner_user_id).await? {
        transaction
            .rollback()
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        return Err(Error::OwnerUnavailable);
    }

    if max_entries == 0 {
        transaction
            .rollback()
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        return Err(Error::InvalidInput);
    }
    let incoming_bytes = write
        .byte_size
        .checked_add(write.thumbnail_byte_size.unwrap_or_default())
        .ok_or(Error::InvalidInput)?;
    if incoming_bytes > max_bytes {
        transaction
            .rollback()
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        return Err(Error::InvalidInput);
    }

    let existing = image_history::Entity::find()
        .select_only()
        .column(image_history::Column::ImageId)
        .column(image_history::Column::ByteSize)
        .column(image_history::Column::ThumbnailByteSize)
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .order_by_asc(image_history::Column::CreatedAt)
        .order_by_asc(image_history::Column::ImageId)
        .into_tuple::<(ImageHistoryKey, i64, Option<i64>)>()
        .all(&transaction)
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    if existing.iter().any(|(id, _, _)| id.as_str() == write.id) {
        transaction
            .rollback()
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        return Err(Error::Conflict);
    }

    let mut remaining_count = existing.len();
    let mut remaining_bytes = existing
        .iter()
        .try_fold(0_u64, |total, (_, image, thumbnail)| {
            let image = u64::try_from(*image).ok()?;
            let thumbnail = thumbnail
                .map(u64::try_from)
                .transpose()
                .ok()?
                .unwrap_or_default();
            total.checked_add(image)?.checked_add(thumbnail)
        })
        .ok_or_else(internal_invariant)?;
    let mut evicted_ids = Vec::new();
    for (id, image, thumbnail) in &existing {
        if remaining_count < max_entries && remaining_bytes <= max_bytes - incoming_bytes {
            break;
        }
        let image = u64::try_from(*image).map_err(|_| internal_invariant())?;
        let thumbnail = thumbnail
            .map(u64::try_from)
            .transpose()
            .map_err(|_| internal_invariant())?
            .unwrap_or_default();
        remaining_count -= 1;
        remaining_bytes = remaining_bytes
            .checked_sub(image)
            .and_then(|total| total.checked_sub(thumbnail))
            .ok_or_else(internal_invariant)?;
        evicted_ids.push(id.as_str().to_owned());
    }
    if remaining_count >= max_entries || remaining_bytes > max_bytes - incoming_bytes {
        return Err(internal_invariant());
    }
    for evicted_id in &evicted_ids {
        let id = ImageHistoryKey::parse(evicted_id).map_err(|_| internal_invariant())?;
        image_history::Entity::delete_many()
            .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
            .filter(image_history::Column::ImageId.eq(id))
            .exec(&transaction)
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
    }

    let inserted = image_history::ActiveModel {
        image_id: Set(image_id),
        owner_user_id: Set(owner_user_id.get()),
        model: Set(SensitiveString::from(write.model)),
        prompt: Set(SensitiveString::from(write.prompt)),
        revised_prompt: Set(write.revised_prompt.map(SensitiveString::from)),
        request_id: Set(write.request_id.map(SensitiveString::from)),
        format: Set(write.format),
        byte_size: Set(byte_size),
        thumbnail_format: Set(write.thumbnail_format),
        thumbnail_byte_size: Set(thumbnail_byte_size),
        created_at: Set(now),
    }
    .insert(&transaction)
    .await
    .map_err(|_| record_internal_error(Error::Query))?;
    let record = try_from_model(inserted)?;
    transaction
        .commit()
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    Ok((record, evicted_ids))
}

async fn list(
    repository: &ImageHistoryRepository,
    owner_user_id: UserId,
) -> Result<Vec<ImageHistoryRecord>, Error> {
    let rows = image_history::Entity::find()
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .order_by_desc(image_history::Column::CreatedAt)
        .order_by_desc(image_history::Column::ImageId)
        .limit(MAX_IMAGE_HISTORY_ENTRIES_PER_PAGE as u64)
        .all(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    rows.into_iter().map(try_from_model).collect()
}

async fn find(
    repository: &ImageHistoryRepository,
    owner_user_id: UserId,
    id: ImageHistoryKey,
) -> Result<Option<ImageHistoryRecord>, Error> {
    image_history::Entity::find()
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(image_history::Column::ImageId.eq(id))
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(Error::Query))?
        .map(try_from_model)
        .transpose()
}

async fn delete(
    repository: &ImageHistoryRepository,
    owner_user_id: UserId,
    id: ImageHistoryKey,
) -> Result<bool, Error> {
    let result = image_history::Entity::delete_many()
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(image_history::Column::ImageId.eq(id))
        .exec(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    match result.rows_affected {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(record_internal_error(Error::Invariant)),
    }
}

async fn delete_all(
    repository: &ImageHistoryRepository,
    owner_user_id: UserId,
) -> Result<Vec<String>, Error> {
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    if !lock_active_owner(&transaction, owner_user_id).await? {
        transaction
            .rollback()
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        return Err(Error::OwnerUnavailable);
    }
    let ids = image_history::Entity::find()
        .select_only()
        .column(image_history::Column::ImageId)
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .into_tuple::<ImageHistoryKey>()
        .all(&transaction)
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    let deleted = image_history::Entity::delete_many()
        .filter(image_history::Column::OwnerUserId.eq(owner_user_id.get()))
        .exec(&transaction)
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    if deleted.rows_affected != ids.len() as u64 {
        return Err(record_internal_error(Error::Invariant));
    }
    transaction
        .commit()
        .await
        .map_err(|_| record_internal_error(Error::Query))?;
    Ok(ids.into_iter().map(|id| id.as_str().to_owned()).collect())
}

async fn lock_active_owner(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<bool, Error> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(owner_user_id.get()))
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| record_internal_error(Error::Query))?;
        if result.rows_affected != 1 {
            return Ok(false);
        }
    }
    let mut query = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::Id.eq(owner_user_id.get()))
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map(|row| row.is_some())
        .map_err(|_| record_internal_error(Error::Query))
}

fn try_from_model(model: image_history::Model) -> Result<ImageHistoryRecord, Error> {
    let id = model.image_id.as_str().to_owned();
    let model_name = model.model.as_str().to_owned();
    let prompt = model.prompt.as_str().to_owned();
    let revised_prompt = model.revised_prompt.map(|value| value.as_str().to_owned());
    let request_id = model.request_id.map(|value| value.as_str().to_owned());
    let byte_size = u64::try_from(model.byte_size).map_err(|_| internal_invariant())?;
    let thumbnail_byte_size = model
        .thumbnail_byte_size
        .map(u64::try_from)
        .transpose()
        .map_err(|_| internal_invariant())?;
    let created_at = model.created_at.unix_timestamp();
    validate_record_fields(
        &id,
        &model_name,
        &prompt,
        revised_prompt.as_deref(),
        request_id.as_deref(),
        &model.format,
        byte_size,
        model.thumbnail_format.as_deref(),
        thumbnail_byte_size,
        created_at,
    )
    .map_err(|_| internal_invariant())?;
    if model.owner_user_id <= 0 {
        return Err(internal_invariant());
    }
    Ok(ImageHistoryRecord {
        id,
        owner_user_id: model.owner_user_id,
        model: model_name,
        prompt,
        revised_prompt,
        request_id,
        format: model.format,
        byte_size,
        thumbnail_format: model.thumbnail_format,
        thumbnail_byte_size,
        created_at,
    })
}

fn validate_write(write: &ImageHistoryWrite) -> Result<(), Error> {
    validate_record_fields(
        &write.id,
        &write.model,
        &write.prompt,
        write.revised_prompt.as_deref(),
        write.request_id.as_deref(),
        &write.format,
        write.byte_size,
        write.thumbnail_format.as_deref(),
        write.thumbnail_byte_size,
        1,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_record_fields(
    id: &str,
    model: &str,
    prompt: &str,
    revised_prompt: Option<&str>,
    request_id: Option<&str>,
    format: &str,
    byte_size: u64,
    thumbnail_format: Option<&str>,
    thumbnail_byte_size: Option<u64>,
    created_at: i64,
) -> Result<(), Error> {
    let valid_text = |value: &str, limit: usize| {
        !value.is_empty() && value.len() <= limit && !value.contains('\0')
    };
    let valid_model = valid_text(model, MAX_MODEL_BYTES)
        && model.trim() == model
        && !model.chars().any(char::is_control);
    let valid_request_id = request_id.is_none_or(|value| {
        valid_text(value, MAX_REQUEST_ID_BYTES)
            && value.trim() == value
            && !value.chars().any(char::is_control)
    });
    if ImageHistoryKey::parse(id).is_err()
        || !valid_model
        || !valid_text(prompt, MAX_PROMPT_BYTES)
        || revised_prompt.is_some_and(|value| !valid_text(value, MAX_PROMPT_BYTES))
        || !valid_request_id
        || !valid_image_format(format)
        || !(1..=MAX_IMAGE_HISTORY_IMAGE_BYTES).contains(&byte_size)
        || created_at <= 0
    {
        return Err(Error::InvalidInput);
    }
    match (thumbnail_format, thumbnail_byte_size) {
        (None, None) => Ok(()),
        (Some(format), Some(size))
            if valid_image_format(format)
                && (1..=MAX_IMAGE_HISTORY_THUMBNAIL_BYTES).contains(&size) =>
        {
            Ok(())
        }
        _ => Err(Error::InvalidInput),
    }
}

fn valid_image_format(format: &str) -> bool {
    matches!(format, "png" | "jpeg" | "webp")
}

fn internal_invariant() -> Error {
    record_internal_error(Error::Invariant)
}

fn record_internal_error(error: Error) -> Error {
    match error {
        Error::InvalidInput | Error::OwnerUnavailable | Error::Conflict => error,
        Error::Query => {
            tracing::error!(target: "af_db::image_history", error_kind = "image_history_query", "图片历史仓储发生内部错误");
            error
        }
        Error::Timeout => {
            tracing::error!(target: "af_db::image_history", error_kind = "image_history_timeout", "图片历史仓储发生内部错误");
            error
        }
        Error::Invariant => {
            tracing::error!(target: "af_db::image_history", error_kind = "image_history_invariant", "图片历史仓储发生内部错误");
            error
        }
    }
}
