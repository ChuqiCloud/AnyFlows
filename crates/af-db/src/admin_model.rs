use std::{collections::BTreeSet, fmt, time::Duration};

use af_domain::{MAX_MODEL_NAME_BYTES, ModelId};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use url::Url;

use crate::{DatabasePool, entity::models};

/// 单页模型元数据查询允许返回的最大记录数。
pub const MAX_ADMIN_MODEL_PAGE_SIZE: usize = 100;
/// 模型展示名允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES: usize = 128;
/// 厂商标识允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_MODEL_PROVIDER_BYTES: usize = 64;
/// 模型描述允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_MODEL_DESCRIPTION_BYTES: usize = 4_096;
/// 模型图标 URL 允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_MODEL_ICON_URL_BYTES: usize = 2_048;
/// 单个模型允许的最大标签数。
pub const MAX_ADMIN_MODEL_TAGS: usize = 32;
/// 单个标签允许的最大 UTF-8 字节数。
pub const MAX_ADMIN_MODEL_TAG_BYTES: usize = 64;
/// 标签 JSON 编码后的最大字节数。
pub const MAX_ADMIN_MODEL_TAGS_BYTES: usize = 8 * 1_024;
/// 上下文窗口的公开上限，避免损坏记录进入前端和后续调度。
pub const MAX_ADMIN_MODEL_CONTEXT_WINDOW: i64 = 2_147_483_647;

/// 模型商品对不同访问主体的可见范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelVisibilityRecord {
    /// 游客和登录用户均可见。
    Public,
    /// 仅登录用户可见。
    Authenticated,
    /// 仅管理员管理界面可见。
    Hidden,
}

impl AdminModelVisibilityRecord {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::Public => 1,
            Self::Authenticated => 2,
            Self::Hidden => 3,
        }
    }

    fn try_from_database(value: i16) -> Result<Self, AdminModelRepositoryError> {
        match value {
            1 => Ok(Self::Public),
            2 => Ok(Self::Authenticated),
            3 => Ok(Self::Hidden),
            _ => Err(internal_invariant()),
        }
    }
}

/// 模型商品的运营生命周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelLifecycleRecord {
    /// 尚未对业务目录生效的草稿。
    Draft,
    /// 正常提供的活动模型。
    Active,
    /// 仍可识别但应引导迁移的弃用模型。
    Deprecated,
    /// 已退出业务目录的退役模型。
    Retired,
}

impl AdminModelLifecycleRecord {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::Draft => 1,
            Self::Active => 2,
            Self::Deprecated => 3,
            Self::Retired => 4,
        }
    }

    fn try_from_database(value: i16) -> Result<Self, AdminModelRepositoryError> {
        match value {
            1 => Ok(Self::Draft),
            2 => Ok(Self::Active),
            3 => Ok(Self::Deprecated),
            4 => Ok(Self::Retired),
            _ => Err(internal_invariant()),
        }
    }
}

/// 输入或输出方向上的闭合模态能力集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminModelModalitiesRecord {
    text: bool,
    image: bool,
    audio: bool,
    video: bool,
}

impl AdminModelModalitiesRecord {
    /// 从四种权威模态标记构造持久化能力集合。
    #[must_use]
    pub const fn new(text: bool, image: bool, audio: bool, video: bool) -> Self {
        Self {
            text,
            image,
            audio,
            video,
        }
    }

    #[must_use]
    pub const fn text(self) -> bool {
        self.text
    }

    #[must_use]
    pub const fn image(self) -> bool {
        self.image
    }

    #[must_use]
    pub const fn audio(self) -> bool {
        self.audio
    }

    #[must_use]
    pub const fn video(self) -> bool {
        self.video
    }

    pub(super) const fn is_empty(self) -> bool {
        !self.text && !self.image && !self.audio && !self.video
    }
}

/// 管理端可读取的已校验模型商品元数据快照。
pub struct AdminModelRecord {
    model_id: ModelId,
    model: String,
    display_name: String,
    provider: String,
    description: Option<String>,
    icon_url: Option<String>,
    tags: Vec<String>,
    context_window: Option<i64>,
    input_modalities: AdminModelModalitiesRecord,
    output_modalities: AdminModelModalitiesRecord,
    supports_reasoning: bool,
    supports_tool_calls: bool,
    visibility: AdminModelVisibilityRecord,
    lifecycle: AdminModelLifecycleRecord,
    created_at: i64,
    updated_at: i64,
}

impl AdminModelRecord {
    #[must_use]
    pub const fn model_id(&self) -> ModelId {
        self.model_id
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub fn icon_url(&self) -> Option<&str> {
        self.icon_url.as_deref()
    }

    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    #[must_use]
    pub const fn context_window(&self) -> Option<i64> {
        self.context_window
    }

    #[must_use]
    pub const fn input_modalities(&self) -> AdminModelModalitiesRecord {
        self.input_modalities
    }

    #[must_use]
    pub const fn output_modalities(&self) -> AdminModelModalitiesRecord {
        self.output_modalities
    }

    #[must_use]
    pub const fn supports_reasoning(&self) -> bool {
        self.supports_reasoning
    }

    #[must_use]
    pub const fn supports_tool_calls(&self) -> bool {
        self.supports_tool_calls
    }

    #[must_use]
    pub const fn visibility(&self) -> AdminModelVisibilityRecord {
        self.visibility
    }

    #[must_use]
    pub const fn lifecycle(&self) -> AdminModelLifecycleRecord {
        self.lifecycle
    }

    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    pub(super) fn try_from_model(model: models::Model) -> Result<Self, AdminModelRepositoryError> {
        let model_id = ModelId::new(model.id).map_err(|_| internal_invariant())?;
        let tags =
            serde_json::from_value::<Vec<String>>(model.tags).map_err(|_| internal_invariant())?;
        let input_modalities = AdminModelModalitiesRecord::new(
            model.supports_text_input,
            model.supports_image_input,
            model.supports_audio_input,
            model.supports_video_input,
        );
        let output_modalities = AdminModelModalitiesRecord::new(
            model.supports_text_output,
            model.supports_image_output,
            model.supports_audio_output,
            model.supports_video_output,
        );
        let visibility = AdminModelVisibilityRecord::try_from_database(model.visibility)?;
        let lifecycle = AdminModelLifecycleRecord::try_from_database(model.lifecycle)?;
        let created_at = model.created_at.unix_timestamp();
        let updated_at = model.updated_at.unix_timestamp();
        if !valid_canonical_model(&model.model)
            || !valid_text(&model.display_name, MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES)
            || !valid_text(&model.provider, MAX_ADMIN_MODEL_PROVIDER_BYTES)
            || !valid_optional_text(
                model.description.as_deref(),
                MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
            )
            || !valid_optional_icon_url(model.icon_url.as_deref())
            || !valid_tags(&tags)
            || !valid_context_window(model.context_window)
            || input_modalities.is_empty()
            || output_modalities.is_empty()
            || updated_at < created_at
        {
            return Err(internal_invariant());
        }
        Ok(Self {
            model_id,
            model: model.model,
            display_name: model.display_name,
            provider: model.provider,
            description: model.description,
            icon_url: model.icon_url,
            tags,
            context_window: model.context_window,
            input_modalities,
            output_modalities,
            supports_reasoning: model.supports_reasoning,
            supports_tool_calls: model.supports_tool_calls,
            visibility,
            lifecycle,
            created_at,
            updated_at,
        })
    }
}

impl fmt::Debug for AdminModelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelRecord(<redacted>)")
    }
}

/// 一页有界模型商品元数据结果。
pub struct AdminModelPageRecord {
    models: Vec<AdminModelRecord>,
    next_cursor: Option<ModelId>,
}

impl AdminModelPageRecord {
    /// 消费页面并返回模型记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminModelRecord>, Option<ModelId>) {
        (self.models, self.next_cursor)
    }
}

impl fmt::Debug for AdminModelPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPageRecord(<redacted>)")
    }
}

/// 模型商品元数据详情查询结果。
pub enum AdminModelLookupOutcome {
    /// 找到当前未软删除元数据。
    Found(AdminModelRecord),
    /// 元数据不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminModelLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("AdminModelLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("AdminModelLookupOutcome::NotFound"),
        }
    }
}

/// 模型元数据仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("模型元数据查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 模型元数据仓储内部错误；不携带模型字段或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelRepositoryError {
    /// Canonical 模型标识与当前有效元数据冲突。
    #[error("模型元数据标识冲突")]
    Conflict,
    /// 获取连接或执行查询失败。
    #[error("模型元数据数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("模型元数据数据库查询超时")]
    Timeout,
    /// 查询输入或持久化结果违反不变量。
    #[error("模型元数据持久化状态损坏")]
    Invariant,
}

/// 管理端模型元数据列表、详情与写入共用仓储。
#[derive(Clone)]
pub struct AdminModelRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl AdminModelRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminModelRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminModelRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按单调 ID 游标读取一页未软删除模型元数据。
    pub async fn list(
        &self,
        after: Option<ModelId>,
        limit: usize,
    ) -> Result<AdminModelPageRecord, AdminModelRepositoryError> {
        if !(1..=MAX_ADMIN_MODEL_PAGE_SIZE).contains(&limit) {
            return Err(internal_invariant());
        }
        let operation = async {
            let mut query = models::Entity::find()
                .filter(models::Column::DeletedAt.is_null())
                .order_by_asc(models::Column::Id)
                .limit((limit + 1) as u64);
            if let Some(after) = after {
                query = query.filter(models::Column::Id.gt(after.get()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| AdminModelRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let mut rows = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error)?,
            Err(_) => return Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        };
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let models = rows
            .into_iter()
            .map(AdminModelRecord::try_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| models.last().map(AdminModelRecord::model_id))
            .flatten();
        Ok(AdminModelPageRecord {
            models,
            next_cursor,
        })
    }

    /// 按稳定 ID 查询当前未软删除模型元数据。
    pub async fn get(
        &self,
        model_id: ModelId,
    ) -> Result<AdminModelLookupOutcome, AdminModelRepositoryError> {
        let operation = models::Entity::find()
            .filter(models::Column::Id.eq(model_id.get()))
            .filter(models::Column::DeletedAt.is_null())
            .limit(2)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default());
        let mut rows = match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(rows)) => rows,
            Ok(Err(_)) => return Err(record_internal_error(AdminModelRepositoryError::Query)),
            Err(_) => return Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        };
        match rows.len() {
            0 => Ok(AdminModelLookupOutcome::NotFound),
            1 => Ok(AdminModelLookupOutcome::Found(
                AdminModelRecord::try_from_model(rows.pop().ok_or_else(internal_invariant)?)?,
            )),
            _ => Err(internal_invariant()),
        }
    }
}

impl fmt::Debug for AdminModelRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

pub(super) fn valid_canonical_model(value: &str) -> bool {
    valid_text(value, MAX_MODEL_NAME_BYTES)
}

pub(super) fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, maximum_bytes))
}

pub(super) fn valid_optional_icon_url(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    if !valid_text(value, MAX_ADMIN_MODEL_ICON_URL_BYTES) {
        return false;
    }
    if value.starts_with('/') && !value.starts_with("//") {
        return !value.contains('?') && !value.contains('#');
    }
    Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    })
}

pub(super) fn valid_tags(tags: &[String]) -> bool {
    if tags.len() > MAX_ADMIN_MODEL_TAGS
        || tags
            .iter()
            .any(|tag| !valid_text(tag, MAX_ADMIN_MODEL_TAG_BYTES))
    {
        return false;
    }
    let unique = tags.iter().collect::<BTreeSet<_>>().len() == tags.len();
    unique
        && serde_json::to_vec(tags).is_ok_and(|encoded| encoded.len() <= MAX_ADMIN_MODEL_TAGS_BYTES)
}

pub(super) const fn valid_context_window(value: Option<i64>) -> bool {
    match value {
        None => true,
        Some(value) => value > 0 && value <= MAX_ADMIN_MODEL_CONTEXT_WINDOW,
    }
}

fn internal_invariant() -> AdminModelRepositoryError {
    record_internal_error(AdminModelRepositoryError::Invariant)
}

pub(super) fn record_internal_error(error: AdminModelRepositoryError) -> AdminModelRepositoryError {
    let error_kind = match error {
        AdminModelRepositoryError::Conflict => "admin_model_conflict",
        AdminModelRepositoryError::Query => "admin_model_query",
        AdminModelRepositoryError::Timeout => "admin_model_timeout",
        AdminModelRepositoryError::Invariant => "admin_model_invariant",
    };
    tracing::error!(
        target: "af_db::admin_model",
        error_kind,
        "模型元数据仓储发生内部错误"
    );
    error
}
