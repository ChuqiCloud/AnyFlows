use std::{collections::BTreeSet, fmt, future::Future, pin::Pin};

use af_db::{
    AdminModelCreateRecord, AdminModelDeleteOutcome, AdminModelMutationOutcome,
    AdminModelRepository, AdminModelRepositoryError, AdminModelWriteRecord,
    MAX_ADMIN_MODEL_CONTEXT_WINDOW, MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
    MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES, MAX_ADMIN_MODEL_ICON_URL_BYTES,
    MAX_ADMIN_MODEL_PROVIDER_BYTES, MAX_ADMIN_MODEL_TAG_BYTES, MAX_ADMIN_MODEL_TAGS,
    MAX_ADMIN_MODEL_TAGS_BYTES,
};
use af_domain::{MAX_MODEL_NAME_BYTES, ModelId};
use thiserror::Error;
use url::Url;

use crate::{
    AdminModel, AdminModelLifecycle, AdminModelModalities, AdminModelVisibility, SessionPrincipal,
    SessionRole,
};

/// 管理员创建模型元数据时允许写入的完整字段。
pub struct AdminModelCreateCommand {
    model: String,
    fields: AdminModelWriteFields,
}

impl AdminModelCreateCommand {
    /// 校验不可变 Canonical 标识及全部结构化元数据字段。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与模型元数据创建契约一一对应"
    )]
    pub fn new(
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
    ) -> Result<Self, AdminModelWriteError> {
        validate_text(&model, MAX_MODEL_NAME_BYTES)?;
        Ok(Self {
            model,
            fields: AdminModelWriteFields::new(
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
            )?,
        })
    }

    fn into_record(self) -> AdminModelCreateRecord {
        AdminModelCreateRecord::new(self.model, self.fields.into_record())
    }
}

impl fmt::Debug for AdminModelCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelCreateCommand(<redacted>)")
    }
}

/// 管理员完整更新模型元数据时允许覆盖的字段。
pub struct AdminModelUpdateCommand {
    fields: AdminModelWriteFields,
}

impl AdminModelUpdateCommand {
    /// 校验全部可变元数据字段；Canonical 标识不属于更新契约。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与模型元数据更新契约一一对应"
    )]
    pub fn new(
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
    ) -> Result<Self, AdminModelWriteError> {
        Ok(Self {
            fields: AdminModelWriteFields::new(
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
            )?,
        })
    }

    pub(crate) fn into_record(self) -> AdminModelWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminModelUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelUpdateCommand(<redacted>)")
    }
}

struct AdminModelWriteFields {
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
}

impl AdminModelWriteFields {
    #[allow(clippy::too_many_arguments, reason = "统一校验入口需要完整模型元数据")]
    fn new(
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
    ) -> Result<Self, AdminModelWriteError> {
        validate_text(&display_name, MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES)?;
        validate_text(&provider, MAX_ADMIN_MODEL_PROVIDER_BYTES)?;
        validate_optional_text(description.as_deref(), MAX_ADMIN_MODEL_DESCRIPTION_BYTES)?;
        validate_optional_icon_url(icon_url.as_deref())?;
        validate_tags(&tags)?;
        if context_window
            .is_some_and(|value| !(1..=MAX_ADMIN_MODEL_CONTEXT_WINDOW).contains(&value))
            || input_modalities.is_empty()
            || output_modalities.is_empty()
        {
            return Err(AdminModelWriteError::InvalidInput);
        }
        Ok(Self {
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
        })
    }

    fn into_record(self) -> AdminModelWriteRecord {
        AdminModelWriteRecord::new(
            self.display_name,
            self.provider,
            self.description,
            self.icon_url,
            self.tags,
            self.context_window,
            self.input_modalities.into_record(),
            self.output_modalities.into_record(),
            self.supports_reasoning,
            self.supports_tool_calls,
            self.visibility.into(),
            self.lifecycle.into(),
        )
    }
}

/// 管理模型元数据写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelWriteError {
    /// 请求字段违反公开边界。
    #[error("管理模型元数据写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理模型元数据写入权限不足")]
    Forbidden,
    /// Canonical 模型标识与当前有效元数据冲突。
    #[error("管理模型元数据标识冲突")]
    Conflict,
    /// 元数据不存在或已经软删除。
    #[error("管理模型元数据不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理模型元数据写入内部失败")]
    Internal,
}

pub type AdminModelCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModel, AdminModelWriteError>> + Send + 'a>>;
pub type AdminModelUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModel, AdminModelWriteError>> + Send + 'a>>;
pub type AdminModelDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminModelWriteError>> + Send + 'a>>;

/// 管理模型元数据写入端口；角色校验必须在进入仓储前完成。
pub trait AdminModelWriter: Send + Sync {
    /// 创建模型元数据并返回管理快照。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelCreateCommand,
    ) -> AdminModelCreateFuture<'a>;

    /// 完整更新可变模型元数据字段。
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        model_id: ModelId,
        command: AdminModelUpdateCommand,
    ) -> AdminModelUpdateFuture<'a>;

    /// 仅软删除模型元数据。
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        model_id: ModelId,
    ) -> AdminModelDeleteFuture<'a>;
}

/// 使用数据库仓储实现管理员模型元数据写入。
pub struct DatabaseAdminModelWriter {
    repository: AdminModelRepository,
}

impl DatabaseAdminModelWriter {
    /// 绑定已经配置查询和写入截止时间的模型元数据仓储。
    #[must_use]
    pub const fn new(repository: AdminModelRepository) -> Self {
        Self { repository }
    }
}

impl AdminModelWriter for DatabaseAdminModelWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelCreateCommand,
    ) -> AdminModelCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .create(command.into_record())
                .await
                .map(AdminModel::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        model_id: ModelId,
        command: AdminModelUpdateCommand,
    ) -> AdminModelUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .update(model_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                AdminModelMutationOutcome::Mutated(record) => Ok(AdminModel::from_record(record)),
                AdminModelMutationOutcome::NotFound => Err(AdminModelWriteError::NotFound),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        model_id: ModelId,
    ) -> AdminModelDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete(model_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminModelDeleteOutcome::Deleted => Ok(()),
                AdminModelDeleteOutcome::NotFound => Err(AdminModelWriteError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminModelWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminModelWriter(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminModelWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminModelWriteError::Forbidden)
    }
}

fn map_repository_error(error: AdminModelRepositoryError) -> AdminModelWriteError {
    match error {
        AdminModelRepositoryError::Conflict => AdminModelWriteError::Conflict,
        AdminModelRepositoryError::Query
        | AdminModelRepositoryError::Timeout
        | AdminModelRepositoryError::Invariant => AdminModelWriteError::Internal,
    }
}

pub(crate) fn validate_text(value: &str, maximum_bytes: usize) -> Result<(), AdminModelWriteError> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(AdminModelWriteError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn validate_optional_text(
    value: Option<&str>,
    maximum_bytes: usize,
) -> Result<(), AdminModelWriteError> {
    value.map_or(Ok(()), |value| validate_text(value, maximum_bytes))
}

pub(crate) fn validate_optional_icon_url(value: Option<&str>) -> Result<(), AdminModelWriteError> {
    let Some(value) = value else {
        return Ok(());
    };
    validate_text(value, MAX_ADMIN_MODEL_ICON_URL_BYTES)?;
    if value.starts_with('/') && !value.starts_with("//") {
        return if value.contains('?') || value.contains('#') {
            Err(AdminModelWriteError::InvalidInput)
        } else {
            Ok(())
        };
    }
    let url = Url::parse(value).map_err(|_| AdminModelWriteError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AdminModelWriteError::InvalidInput);
    }
    Ok(())
}

pub(crate) fn validate_tags(tags: &[String]) -> Result<(), AdminModelWriteError> {
    if tags.len() > MAX_ADMIN_MODEL_TAGS
        || tags
            .iter()
            .any(|tag| validate_text(tag, MAX_ADMIN_MODEL_TAG_BYTES).is_err())
        || tags.iter().collect::<BTreeSet<_>>().len() != tags.len()
        || !serde_json::to_vec(tags)
            .is_ok_and(|encoded| encoded.len() <= MAX_ADMIN_MODEL_TAGS_BYTES)
    {
        return Err(AdminModelWriteError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use af_domain::UserId;

    use super::*;

    fn valid_create() -> Result<AdminModelCreateCommand, AdminModelWriteError> {
        AdminModelCreateCommand::new(
            "gpt-5.5".to_owned(),
            "GPT-5.5".to_owned(),
            "openai".to_owned(),
            Some("通用推理模型".to_owned()),
            Some("/assets/models/openai.svg".to_owned()),
            vec!["推理".to_owned(), "工具".to_owned()],
            Some(128_000),
            AdminModelModalities::new(true, true, false, false),
            AdminModelModalities::new(true, false, false, false),
            true,
            true,
            AdminModelVisibility::Public,
            AdminModelLifecycle::Active,
        )
    }

    #[test]
    fn commands_validate_closed_metadata_boundaries() {
        let command = valid_create().unwrap();
        assert_eq!(
            format!("{command:?}"),
            "AdminModelCreateCommand(<redacted>)"
        );
        assert!(matches!(
            AdminModelCreateCommand::new(
                " bad ".to_owned(),
                "Bad".to_owned(),
                "provider".to_owned(),
                None,
                None,
                Vec::new(),
                None,
                AdminModelModalities::new(true, false, false, false),
                AdminModelModalities::new(true, false, false, false),
                false,
                false,
                AdminModelVisibility::Hidden,
                AdminModelLifecycle::Draft,
            ),
            Err(AdminModelWriteError::InvalidInput)
        ));
        assert!(matches!(
            AdminModelUpdateCommand::new(
                "Bad".to_owned(),
                "provider".to_owned(),
                None,
                None,
                Vec::new(),
                Some(0),
                AdminModelModalities::new(false, false, false, false),
                AdminModelModalities::new(true, false, false, false),
                false,
                false,
                AdminModelVisibility::Hidden,
                AdminModelLifecycle::Draft,
            ),
            Err(AdminModelWriteError::InvalidInput)
        ));
    }

    #[test]
    fn normal_user_is_rejected_before_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminModelWriteError::Forbidden)
        );
    }
}
