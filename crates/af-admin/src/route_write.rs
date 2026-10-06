use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminRouteChannelWriteRecord, AdminRouteDeleteOutcome, AdminRouteMutationOutcome,
    AdminRouteRepository, AdminRouteRepositoryError, AdminRouteWriteRecord,
    MAX_ADMIN_ROUTE_CHANNELS, MAX_ADMIN_ROUTE_NAME_BYTES, validate_smart_route_model_mapping,
};
use af_domain::{
    ChannelId, CredentialId, RouteId, RouteMode, RouteStrategy, validate_route_model_pattern,
};
use thiserror::Error;

use crate::{AdminRoute, SessionPrincipal, SessionRole};

/// 管理路由候选的完整写入字段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRouteChannelCommand {
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: i32,
    enabled: bool,
}

impl AdminRouteChannelCommand {
    /// 校验候选 ID、优先级与权重。
    pub fn new(
        channel_id: ChannelId,
        credential_id: CredentialId,
        priority: i32,
        weight: i32,
        enabled: bool,
    ) -> Result<Self, AdminRouteWriteError> {
        if priority < 0 || weight < 0 {
            return Err(AdminRouteWriteError::InvalidInput);
        }
        Ok(Self {
            channel_id,
            credential_id,
            priority,
            weight,
            enabled,
        })
    }

    fn into_record(self) -> AdminRouteChannelWriteRecord {
        AdminRouteChannelWriteRecord::new(
            self.channel_id,
            self.credential_id,
            self.priority,
            self.weight,
            self.enabled,
        )
    }
}

/// 管理端创建路由命令。
pub struct AdminRouteCreateCommand {
    fields: AdminRouteWriteFields,
}

impl AdminRouteCreateCommand {
    /// 校验路由名称、模型匹配、映射对象和候选集合。
    pub fn new(
        name: String,
        model_pattern: String,
        mode: RouteMode,
        strategy: RouteStrategy,
        model_mapping: serde_json::Value,
        enabled: bool,
        channels: Vec<AdminRouteChannelCommand>,
    ) -> Result<Self, AdminRouteWriteError> {
        Ok(Self {
            fields: AdminRouteWriteFields::new(
                name,
                model_pattern,
                mode,
                strategy,
                model_mapping,
                enabled,
                channels,
            )?,
        })
    }

    fn into_record(self) -> AdminRouteWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminRouteCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteCreateCommand(<redacted>)")
    }
}

/// 管理端完整更新路由命令。
pub struct AdminRouteUpdateCommand {
    fields: AdminRouteWriteFields,
}

impl AdminRouteUpdateCommand {
    /// 校验完整更新字段，避免隐式保留或重置候选。
    pub fn new(
        name: String,
        model_pattern: String,
        mode: RouteMode,
        strategy: RouteStrategy,
        model_mapping: serde_json::Value,
        enabled: bool,
        channels: Vec<AdminRouteChannelCommand>,
    ) -> Result<Self, AdminRouteWriteError> {
        Ok(Self {
            fields: AdminRouteWriteFields::new(
                name,
                model_pattern,
                mode,
                strategy,
                model_mapping,
                enabled,
                channels,
            )?,
        })
    }

    fn into_record(self) -> AdminRouteWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminRouteUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteUpdateCommand(<redacted>)")
    }
}

struct AdminRouteWriteFields {
    name: String,
    model_pattern: String,
    mode: RouteMode,
    strategy: RouteStrategy,
    model_mapping: serde_json::Value,
    enabled: bool,
    channels: Vec<AdminRouteChannelCommand>,
}

impl AdminRouteWriteFields {
    fn new(
        name: String,
        model_pattern: String,
        mode: RouteMode,
        strategy: RouteStrategy,
        model_mapping: serde_json::Value,
        enabled: bool,
        channels: Vec<AdminRouteChannelCommand>,
    ) -> Result<Self, AdminRouteWriteError> {
        if name.is_empty()
            || name.len() > MAX_ADMIN_ROUTE_NAME_BYTES
            || name.chars().any(char::is_control)
            || validate_route_model_pattern(&model_pattern).is_err()
            || model_pattern.trim() != model_pattern
            || (mode == RouteMode::ExplicitGroup && model_pattern.starts_with("re:"))
            || !validate_smart_route_model_mapping(&model_mapping)
            || channels.len() > MAX_ADMIN_ROUTE_CHANNELS
        {
            return Err(AdminRouteWriteError::InvalidInput);
        }
        let mut seen = std::collections::HashSet::with_capacity(channels.len());
        if channels
            .iter()
            .any(|channel| !seen.insert((channel.channel_id.get(), channel.credential_id.get())))
        {
            return Err(AdminRouteWriteError::InvalidInput);
        }
        Ok(Self {
            name,
            model_pattern,
            mode,
            strategy,
            model_mapping,
            enabled,
            channels,
        })
    }

    fn into_record(self) -> AdminRouteWriteRecord {
        AdminRouteWriteRecord::new(
            self.name,
            self.model_pattern,
            self.mode,
            self.strategy,
            self.model_mapping,
            self.enabled,
            self.channels
                .into_iter()
                .map(AdminRouteChannelCommand::into_record)
                .collect(),
        )
    }
}

/// 管理路由写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminRouteWriteError {
    /// 请求字段或候选集合违反边界。
    #[error("管理路由写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理路由写入权限不足")]
    Forbidden,
    /// 路由名称冲突。
    #[error("管理路由名称冲突")]
    Conflict,
    /// 路由不存在或已删除。
    #[error("管理路由不存在")]
    NotFound,
    /// 渠道或凭据引用无效。
    #[error("管理路由候选引用无效")]
    InvalidReference,
    /// 数据库失败或提交状态未知。
    #[error("管理路由写入内部失败")]
    Internal,
}

pub type AdminRouteCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRoute, AdminRouteWriteError>> + Send + 'a>>;
pub type AdminRouteUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRoute, AdminRouteWriteError>> + Send + 'a>>;
pub type AdminRouteDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminRouteWriteError>> + Send + 'a>>;

/// 管理路由写入应用端口。
pub trait AdminRouteWriter: Send + Sync {
    /// 创建路由及其候选。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminRouteCreateCommand,
    ) -> AdminRouteCreateFuture<'a>;
    /// 完整更新路由及其候选。
    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        route_id: RouteId,
        command: AdminRouteUpdateCommand,
    ) -> AdminRouteUpdateFuture<'a>;
    /// 软删除路由。
    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        route_id: RouteId,
    ) -> AdminRouteDeleteFuture<'a>;
}

/// 数据库路由写入应用端口实现。
pub struct DatabaseAdminRouteWriter {
    repository: AdminRouteRepository,
}

impl DatabaseAdminRouteWriter {
    /// 绑定路由仓储。
    #[must_use]
    pub const fn new(repository: AdminRouteRepository) -> Self {
        Self { repository }
    }
}

impl AdminRouteWriter for DatabaseAdminRouteWriter {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminRouteCreateCommand,
    ) -> AdminRouteCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .create(command.into_record())
                .await
                .map_err(map_error)
                .map(crate::route_read::AdminRoute::from_record)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        route_id: RouteId,
        command: AdminRouteUpdateCommand,
    ) -> AdminRouteUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .update(route_id, command.into_record())
                .await
                .map_err(map_error)?
            {
                AdminRouteMutationOutcome::Mutated(record) => {
                    Ok(crate::route_read::AdminRoute::from_record(record))
                }
                AdminRouteMutationOutcome::NotFound => Err(AdminRouteWriteError::NotFound),
            }
        })
    }

    fn delete<'a>(
        &'a self,
        principal: SessionPrincipal,
        route_id: RouteId,
    ) -> AdminRouteDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self.repository.delete(route_id).await.map_err(map_error)? {
                AdminRouteDeleteOutcome::Deleted => Ok(()),
                AdminRouteDeleteOutcome::NotFound => Err(AdminRouteWriteError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminRouteWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminRouteWriter(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminRouteWriteError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AdminRouteWriteError::Forbidden)
}

fn map_error(error: AdminRouteRepositoryError) -> AdminRouteWriteError {
    match error {
        AdminRouteRepositoryError::Conflict => AdminRouteWriteError::Conflict,
        AdminRouteRepositoryError::InvalidReference => AdminRouteWriteError::InvalidReference,
        AdminRouteRepositoryError::Invariant => AdminRouteWriteError::InvalidInput,
        AdminRouteRepositoryError::Query | AdminRouteRepositoryError::Timeout => {
            AdminRouteWriteError::Internal
        }
    }
}
