use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    MAX_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE, OrganizationCustomRoleCreate,
    OrganizationCustomRolePageRecord, OrganizationCustomRoleRecord,
    OrganizationCustomRoleRepository, OrganizationCustomRoleRepositoryError,
    OrganizationCustomRoleUpdate,
};
use af_domain::{
    OrganizationCustomRoleId, OrganizationCustomRoleKey, OrganizationEntitlementStatus,
    OrganizationPermission,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 企业自定义角色目录默认页大小。
pub const DEFAULT_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE: usize = 50;

/// 已校验的角色目录查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleListQuery {
    after: Option<OrganizationCustomRoleId>,
    limit: usize,
}

impl OrganizationCustomRoleListQuery {
    /// 校验稳定游标和页大小。
    pub fn new(
        after: Option<OrganizationCustomRoleId>,
        limit: usize,
    ) -> Result<Self, OrganizationCustomRoleServiceError> {
        if !(1..=MAX_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationCustomRoleServiceError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<OrganizationCustomRoleId> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for OrganizationCustomRoleListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ORGANIZATION_CUSTOM_ROLE_PAGE_SIZE,
        }
    }
}

/// 创建角色目录定义的完整资料。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleCreateCommand {
    name: String,
    description: Option<String>,
    permissions: Vec<OrganizationPermission>,
}

impl OrganizationCustomRoleCreateCommand {
    #[must_use]
    pub fn new(
        name: String,
        description: Option<String>,
        permissions: Vec<OrganizationPermission>,
    ) -> Self {
        Self {
            name,
            description,
            permissions,
        }
    }
}

/// 按版本 CAS 完整替换活动角色定义。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleChangeCommand {
    role_key: OrganizationCustomRoleKey,
    name: String,
    description: Option<String>,
    permissions: Vec<OrganizationPermission>,
    expected_version: i64,
}

impl OrganizationCustomRoleChangeCommand {
    pub fn new(
        role_key: OrganizationCustomRoleKey,
        name: String,
        description: Option<String>,
        permissions: Vec<OrganizationPermission>,
        expected_version: i64,
    ) -> Result<Self, OrganizationCustomRoleServiceError> {
        validate_version(expected_version)?;
        Ok(Self {
            role_key,
            name,
            description,
            permissions,
            expected_version,
        })
    }
}

/// 按版本 CAS 不可逆停用角色目录定义。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleDisableCommand {
    role_key: OrganizationCustomRoleKey,
    expected_version: i64,
}

impl OrganizationCustomRoleDisableCommand {
    pub fn new(
        role_key: OrganizationCustomRoleKey,
        expected_version: i64,
    ) -> Result<Self, OrganizationCustomRoleServiceError> {
        validate_version(expected_version)?;
        Ok(Self {
            role_key,
            expected_version,
        })
    }
}

/// 企业自定义角色稳定游标页。
#[derive(Debug, PartialEq)]
pub struct OrganizationCustomRolePage {
    records: Vec<OrganizationCustomRoleRecord>,
    next_cursor: Option<OrganizationCustomRoleId>,
}

impl OrganizationCustomRolePage {
    #[must_use]
    pub fn records(&self) -> &[OrganizationCustomRoleRecord] {
        &self.records
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationCustomRoleId> {
        self.next_cursor
    }
}

/// 企业自定义角色管理服务错误；不携带角色资料原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleServiceError {
    #[error("企业自定义角色请求无效")]
    InvalidInput,
    #[error("当前企业角色无权管理自定义角色")]
    Forbidden,
    #[error("企业自定义角色不存在")]
    NotFound,
    #[error("企业自定义角色版本冲突")]
    Conflict,
    #[error("企业自定义角色状态转换无效")]
    InvalidTransition,
    #[error("企业商业授权不允许自定义角色")]
    EntitlementRestricted,
    #[error("企业自定义角色服务内部错误")]
    Internal,
}

pub type OrganizationCustomRolePageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationCustomRolePage, OrganizationCustomRoleServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationCustomRoleFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationCustomRoleRecord, OrganizationCustomRoleServiceError>,
            > + Send
            + 'a,
    >,
>;

/// 企业自定义角色目录管理端口；成员绑定和运行时授权留在后续切片。
pub trait OrganizationCustomRoleService: Send + Sync {
    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationCustomRoleListQuery,
    ) -> OrganizationCustomRolePageFuture<'a>;

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        role_key: OrganizationCustomRoleKey,
    ) -> OrganizationCustomRoleFuture<'a>;

    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleCreateCommand,
    ) -> OrganizationCustomRoleFuture<'a>;

    fn update<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleChangeCommand,
    ) -> OrganizationCustomRoleFuture<'a>;

    fn disable<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleDisableCommand,
    ) -> OrganizationCustomRoleFuture<'a>;
}

/// 使用角色仓储实现企业范围、固定角色和商业授权边界。
pub struct DatabaseOrganizationCustomRoleService {
    repository: Arc<OrganizationCustomRoleRepository>,
}

impl DatabaseOrganizationCustomRoleService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationCustomRoleRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationCustomRoleService for DatabaseOrganizationCustomRoleService {
    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationCustomRoleListQuery,
    ) -> OrganizationCustomRolePageFuture<'a> {
        Box::pin(async move {
            require_read_access(&context)?;
            let page = self
                .repository
                .list_roles(context.organization_id(), query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page_parts(page);
            Ok(OrganizationCustomRolePage {
                records,
                next_cursor,
            })
        })
    }

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        role_key: OrganizationCustomRoleKey,
    ) -> OrganizationCustomRoleFuture<'a> {
        Box::pin(async move {
            require_read_access(&context)?;
            self.repository
                .role(context.organization_id(), role_key)
                .await
                .map_err(map_repository_error)
        })
    }

    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleCreateCommand,
    ) -> OrganizationCustomRoleFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            self.repository
                .create_role(
                    context.organization_id(),
                    OrganizationCustomRoleCreate::new(
                        command.name,
                        command.description,
                        command.permissions,
                        context.user_id(),
                    ),
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn update<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleChangeCommand,
    ) -> OrganizationCustomRoleFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            self.repository
                .update_role(
                    context.organization_id(),
                    command.role_key,
                    OrganizationCustomRoleUpdate::new(
                        command.name,
                        command.description,
                        command.permissions,
                        command.expected_version,
                    ),
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn disable<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleDisableCommand,
    ) -> OrganizationCustomRoleFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            self.repository
                .disable_role(
                    context.organization_id(),
                    command.role_key,
                    command.expected_version,
                )
                .await
                .map_err(map_repository_error)
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationCustomRoleService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationCustomRoleService(<repository>)")
    }
}

fn require_read_access(
    context: &OrganizationContext,
) -> Result<(), OrganizationCustomRoleServiceError> {
    if !matches!(
        context.role(),
        af_domain::OrganizationRole::Owner | af_domain::OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::RolesRead)
    {
        return Err(OrganizationCustomRoleServiceError::Forbidden);
    }
    if context.entitlement().status() != OrganizationEntitlementStatus::Active {
        return Err(OrganizationCustomRoleServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn require_manage_access(
    context: &OrganizationContext,
) -> Result<(), OrganizationCustomRoleServiceError> {
    if !matches!(
        context.role(),
        af_domain::OrganizationRole::Owner | af_domain::OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::RolesManage)
    {
        return Err(OrganizationCustomRoleServiceError::Forbidden);
    }
    if context.entitlement().status() != OrganizationEntitlementStatus::Active {
        return Err(OrganizationCustomRoleServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn validate_version(version: i64) -> Result<(), OrganizationCustomRoleServiceError> {
    if version <= 0 {
        return Err(OrganizationCustomRoleServiceError::InvalidInput);
    }
    Ok(())
}

fn page_parts(
    page: OrganizationCustomRolePageRecord,
) -> (
    Vec<OrganizationCustomRoleRecord>,
    Option<OrganizationCustomRoleId>,
) {
    let next_cursor = page.next_cursor();
    let records = page.items().to_vec();
    (records, next_cursor)
}

fn map_repository_error(
    error: OrganizationCustomRoleRepositoryError,
) -> OrganizationCustomRoleServiceError {
    match error {
        OrganizationCustomRoleRepositoryError::InvalidInput => {
            OrganizationCustomRoleServiceError::InvalidInput
        }
        OrganizationCustomRoleRepositoryError::NotFound => {
            OrganizationCustomRoleServiceError::NotFound
        }
        OrganizationCustomRoleRepositoryError::CreatorUnavailable => {
            OrganizationCustomRoleServiceError::Forbidden
        }
        OrganizationCustomRoleRepositoryError::Conflict => {
            OrganizationCustomRoleServiceError::Conflict
        }
        OrganizationCustomRoleRepositoryError::InvalidTransition => {
            OrganizationCustomRoleServiceError::InvalidTransition
        }
        OrganizationCustomRoleRepositoryError::EntropyUnavailable
        | OrganizationCustomRoleRepositoryError::Query
        | OrganizationCustomRoleRepositoryError::Timeout
        | OrganizationCustomRoleRepositoryError::Invariant => {
            OrganizationCustomRoleServiceError::Internal
        }
    }
}
