use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminModelLifecycleRecord, AdminModelLookupOutcome, AdminModelModalitiesRecord,
    AdminModelRecord, AdminModelRepository, AdminModelRepositoryError, AdminModelVisibilityRecord,
    MAX_ADMIN_MODEL_PAGE_SIZE,
};
use af_domain::ModelId;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理模型元数据列表默认页大小。
pub const DEFAULT_ADMIN_MODEL_PAGE_SIZE: usize = 50;

/// 已校验的管理模型元数据列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminModelListQuery {
    after: Option<ModelId>,
    limit: usize,
}

impl AdminModelListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<ModelId>, limit: usize) -> Result<Self, AdminModelReadError> {
        if !(1..=MAX_ADMIN_MODEL_PAGE_SIZE).contains(&limit) {
            return Err(AdminModelReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<ModelId> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminModelListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_MODEL_PAGE_SIZE,
        }
    }
}

/// 模型商品对不同访问主体的可见范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelVisibility {
    /// 游客和登录用户均可见。
    Public,
    /// 仅登录用户可见。
    Authenticated,
    /// 仅管理员管理界面可见。
    Hidden,
}

impl From<AdminModelVisibilityRecord> for AdminModelVisibility {
    fn from(value: AdminModelVisibilityRecord) -> Self {
        match value {
            AdminModelVisibilityRecord::Public => Self::Public,
            AdminModelVisibilityRecord::Authenticated => Self::Authenticated,
            AdminModelVisibilityRecord::Hidden => Self::Hidden,
        }
    }
}

impl From<AdminModelVisibility> for AdminModelVisibilityRecord {
    fn from(value: AdminModelVisibility) -> Self {
        match value {
            AdminModelVisibility::Public => Self::Public,
            AdminModelVisibility::Authenticated => Self::Authenticated,
            AdminModelVisibility::Hidden => Self::Hidden,
        }
    }
}

/// 模型商品的运营生命周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelLifecycle {
    /// 尚未对业务目录生效的草稿。
    Draft,
    /// 正常提供的活动模型。
    Active,
    /// 仍可识别但应引导迁移的弃用模型。
    Deprecated,
    /// 已退出业务目录的退役模型。
    Retired,
}

impl From<AdminModelLifecycleRecord> for AdminModelLifecycle {
    fn from(value: AdminModelLifecycleRecord) -> Self {
        match value {
            AdminModelLifecycleRecord::Draft => Self::Draft,
            AdminModelLifecycleRecord::Active => Self::Active,
            AdminModelLifecycleRecord::Deprecated => Self::Deprecated,
            AdminModelLifecycleRecord::Retired => Self::Retired,
        }
    }
}

impl From<AdminModelLifecycle> for AdminModelLifecycleRecord {
    fn from(value: AdminModelLifecycle) -> Self {
        match value {
            AdminModelLifecycle::Draft => Self::Draft,
            AdminModelLifecycle::Active => Self::Active,
            AdminModelLifecycle::Deprecated => Self::Deprecated,
            AdminModelLifecycle::Retired => Self::Retired,
        }
    }
}

/// 输入或输出方向上的闭合模态能力集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminModelModalities {
    text: bool,
    image: bool,
    audio: bool,
    video: bool,
}

impl AdminModelModalities {
    /// 从结构化开关构造模态集合；是否为空由写入命令统一校验。
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

    pub(super) const fn into_record(self) -> AdminModelModalitiesRecord {
        AdminModelModalitiesRecord::new(self.text, self.image, self.audio, self.video)
    }

    fn from_record(value: AdminModelModalitiesRecord) -> Self {
        Self::new(value.text(), value.image(), value.audio(), value.video())
    }
}

/// 管理 API 可读取的模型商品元数据快照。
pub struct AdminModel {
    model_id: ModelId,
    model: String,
    display_name: String,
    provider: String,
    description: Option<String>,
    icon_url: Option<String>,
    tags: Vec<String>,
    context_window: Option<i64>,
    input_modalities: AdminModelModalities,
    output_modalities: AdminModelModalities,
    supports_reasoning: bool,
    supports_tool_calls: bool,
    visibility: AdminModelVisibility,
    lifecycle: AdminModelLifecycle,
    created_at: i64,
    updated_at: i64,
}

impl AdminModel {
    /// 组合已完成持久化校验的模型字段，供仓储适配器和测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        model_id: ModelId,
        model: String,
        display_name: String,
        provider: String,
        description: Option<String>,
        icon_url: Option<String>,
        tags: Vec<String>,
        context_window: Option<i64>,
        input_modalities: AdminModelModalities,
        output_modalities: AdminModelModalities,
        supports_reasoning: bool,
        supports_tool_calls: bool,
        visibility: AdminModelVisibility,
        lifecycle: AdminModelLifecycle,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            model_id,
            model,
            display_name,
            provider,
            description,
            icon_url,
            tags,
            context_window,
            input_modalities,
            output_modalities,
            supports_reasoning,
            supports_tool_calls,
            visibility,
            lifecycle,
            created_at,
            updated_at,
        }
    }

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
    pub const fn input_modalities(&self) -> AdminModelModalities {
        self.input_modalities
    }

    #[must_use]
    pub const fn output_modalities(&self) -> AdminModelModalities {
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
    pub const fn visibility(&self) -> AdminModelVisibility {
        self.visibility
    }

    #[must_use]
    pub const fn lifecycle(&self) -> AdminModelLifecycle {
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

    pub(super) fn from_record(record: AdminModelRecord) -> Self {
        Self::from_parts(
            record.model_id(),
            record.model().to_owned(),
            record.display_name().to_owned(),
            record.provider().to_owned(),
            record.description().map(str::to_owned),
            record.icon_url().map(str::to_owned),
            record.tags().to_vec(),
            record.context_window(),
            AdminModelModalities::from_record(record.input_modalities()),
            AdminModelModalities::from_record(record.output_modalities()),
            record.supports_reasoning(),
            record.supports_tool_calls(),
            record.visibility().into(),
            record.lifecycle().into(),
            record.created_at(),
            record.updated_at(),
        )
    }
}

impl fmt::Debug for AdminModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModel(<redacted>)")
    }
}

/// 一页管理模型商品元数据响应。
pub struct AdminModelPage {
    models: Vec<AdminModel>,
    next_cursor: Option<ModelId>,
}

impl AdminModelPage {
    /// 组合模型列表和可选下一游标。
    #[must_use]
    pub fn from_parts(models: Vec<AdminModel>, next_cursor: Option<ModelId>) -> Self {
        Self {
            models,
            next_cursor,
        }
    }

    #[must_use]
    pub fn models(&self) -> &[AdminModel] {
        &self.models
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<ModelId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminModelPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelPage(<redacted>)")
    }
}

/// 管理模型元数据读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelReadError {
    /// 游标或页大小不满足公开边界。
    #[error("管理模型元数据分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理模型元数据读取权限不足")]
    Forbidden,
    /// 元数据不存在或已经软删除。
    #[error("管理模型元数据不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理模型元数据读取内部失败")]
    Internal,
}

pub type AdminModelListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModelPage, AdminModelReadError>> + Send + 'a>>;
pub type AdminModelGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModel, AdminModelReadError>> + Send + 'a>>;

/// 管理模型元数据只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminModelReader: Send + Sync {
    /// 读取一页模型元数据。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminModelListQuery,
    ) -> AdminModelListFuture<'a>;

    /// 按模型元数据 ID 读取详情。
    fn get<'a>(&'a self, principal: SessionPrincipal, model_id: ModelId)
    -> AdminModelGetFuture<'a>;
}

/// 使用数据库仓储实现管理员模型元数据读取。
pub struct DatabaseAdminModelReader {
    repository: AdminModelRepository,
}

impl DatabaseAdminModelReader {
    /// 绑定已配置查询截止时间的模型元数据仓储。
    #[must_use]
    pub const fn new(repository: AdminModelRepository) -> Self {
        Self { repository }
    }
}

impl AdminModelReader for DatabaseAdminModelReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminModelListQuery,
    ) -> AdminModelListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            Ok(AdminModelPage::from_parts(
                records.into_iter().map(AdminModel::from_record).collect(),
                next_cursor,
            ))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        model_id: ModelId,
    ) -> AdminModelGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(model_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminModelLookupOutcome::Found(record) => Ok(AdminModel::from_record(record)),
                AdminModelLookupOutcome::NotFound => Err(AdminModelReadError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminModelReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminModelReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminModelReadError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminModelReadError::Forbidden)
    }
}

fn map_repository_error(_error: AdminModelRepositoryError) -> AdminModelReadError {
    AdminModelReadError::Internal
}

#[cfg(test)]
mod tests {
    use af_domain::UserId;

    use super::*;

    #[test]
    fn pagination_and_role_boundaries_are_closed() {
        assert_eq!(AdminModelListQuery::default().limit(), 50);
        assert_eq!(
            AdminModelListQuery::new(None, 0),
            Err(AdminModelReadError::InvalidPagination)
        );
        assert_eq!(
            AdminModelListQuery::new(None, MAX_ADMIN_MODEL_PAGE_SIZE + 1),
            Err(AdminModelReadError::InvalidPagination)
        );
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminModelReadError::Forbidden)
        );
    }
}
