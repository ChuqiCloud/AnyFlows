use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationCustomRoleBindingRecord, OrganizationCustomRoleBindingRepository,
    OrganizationCustomRoleBindingRepositoryError,
};
use af_domain::{
    OrganizationCustomRoleKey, OrganizationEntitlementStatus, OrganizationMembershipId,
    OrganizationPermission, OrganizationRole,
};
use thiserror::Error;

use crate::{OrganizationContext, OrganizationCustomRoleService};

/// 成员自定义角色绑定写入命令；清除绑定时 `role_key` 为空。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleBindingCommand {
    membership_id: OrganizationMembershipId,
    role_key: Option<OrganizationCustomRoleKey>,
    expected_version: i64,
}

impl OrganizationCustomRoleBindingCommand {
    /// 校验成员版本并构造绑定或清除命令。
    pub fn new(
        membership_id: OrganizationMembershipId,
        role_key: Option<OrganizationCustomRoleKey>,
        expected_version: i64,
    ) -> Result<Self, OrganizationCustomRoleBindingServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationCustomRoleBindingServiceError::InvalidInput);
        }
        Ok(Self {
            membership_id,
            role_key,
            expected_version,
        })
    }

    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.membership_id
    }

    #[must_use]
    pub const fn role_key(self) -> Option<OrganizationCustomRoleKey> {
        self.role_key
    }

    #[must_use]
    pub const fn expected_version(self) -> i64 {
        self.expected_version
    }
}

/// 绑定写入后返回的公开安全投影，不暴露数据库内部角色 ID。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCustomRoleBindingResult {
    membership_id: OrganizationMembershipId,
    role_key: Option<OrganizationCustomRoleKey>,
    membership_version: i64,
}

impl OrganizationCustomRoleBindingResult {
    #[must_use]
    pub const fn membership_id(self) -> OrganizationMembershipId {
        self.membership_id
    }

    #[must_use]
    pub const fn role_key(self) -> Option<OrganizationCustomRoleKey> {
        self.role_key
    }

    #[must_use]
    pub const fn membership_version(self) -> i64 {
        self.membership_version
    }
}

/// 成员自定义角色绑定服务错误；内部错误不携带数据库原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCustomRoleBindingServiceError {
    #[error("成员自定义角色绑定请求无效")]
    InvalidInput,
    #[error("当前企业角色无权分配自定义角色")]
    Forbidden,
    #[error("成员或自定义角色不存在")]
    NotFound,
    #[error("成员当前状态不允许绑定自定义角色")]
    MembershipUnavailable,
    #[error("自定义角色当前不可绑定")]
    RoleUnavailable,
    #[error("成员绑定版本冲突")]
    Conflict,
    #[error("企业商业授权不允许自定义角色绑定")]
    EntitlementRestricted,
    #[error("成员自定义角色绑定服务内部错误")]
    Internal,
}

pub type OrganizationCustomRoleBindingFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCustomRoleBindingResult,
                    OrganizationCustomRoleBindingServiceError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 成员自定义角色绑定服务；权限检查和角色解析均在服务边界完成。
pub trait OrganizationCustomRoleBindingService: Send + Sync {
    fn set<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleBindingCommand,
    ) -> OrganizationCustomRoleBindingFuture<'a>;
}

pub struct DatabaseOrganizationCustomRoleBindingService {
    repository: Arc<OrganizationCustomRoleBindingRepository>,
    custom_role_service: Arc<dyn OrganizationCustomRoleService>,
}

impl DatabaseOrganizationCustomRoleBindingService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationCustomRoleBindingRepository>,
        custom_role_service: Arc<dyn OrganizationCustomRoleService>,
    ) -> Self {
        Self {
            repository,
            custom_role_service,
        }
    }
}

impl OrganizationCustomRoleBindingService for DatabaseOrganizationCustomRoleBindingService {
    fn set<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCustomRoleBindingCommand,
    ) -> OrganizationCustomRoleBindingFuture<'a> {
        Box::pin(async move {
            require_binding_access(&context)?;
            let custom_role_id = if let Some(role_key) = command.role_key() {
                self.custom_role_service
                    .get(context.clone(), role_key)
                    .await
                    .map_err(map_role_error)?
                    .id()
            } else {
                None
            };
            let record = self
                .repository
                .set_binding(
                    context.organization_id(),
                    command.membership_id(),
                    custom_role_id,
                    command.expected_version(),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(OrganizationCustomRoleBindingResult::from_parts(
                record,
                command.role_key(),
            ))
        })
    }
}

impl OrganizationCustomRoleBindingResult {
    fn from_parts(
        record: OrganizationCustomRoleBindingRecord,
        role_key: Option<OrganizationCustomRoleKey>,
    ) -> Self {
        Self {
            membership_id: record.membership_id(),
            role_key,
            membership_version: record.membership_version(),
        }
    }
}

impl std::fmt::Debug for DatabaseOrganizationCustomRoleBindingService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationCustomRoleBindingService(<repository>)")
    }
}

fn require_binding_access(
    context: &OrganizationContext,
) -> Result<(), OrganizationCustomRoleBindingServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::RolesAssign)
    {
        return Err(OrganizationCustomRoleBindingServiceError::Forbidden);
    }
    let entitlement = context.entitlement();
    if entitlement.status() != OrganizationEntitlementStatus::Active
        || !entitlement.custom_roles_enabled()
    {
        return Err(OrganizationCustomRoleBindingServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn map_role_error(
    error: crate::OrganizationCustomRoleServiceError,
) -> OrganizationCustomRoleBindingServiceError {
    match error {
        crate::OrganizationCustomRoleServiceError::InvalidInput => {
            OrganizationCustomRoleBindingServiceError::InvalidInput
        }
        crate::OrganizationCustomRoleServiceError::Forbidden => {
            OrganizationCustomRoleBindingServiceError::Forbidden
        }
        crate::OrganizationCustomRoleServiceError::NotFound => {
            OrganizationCustomRoleBindingServiceError::RoleUnavailable
        }
        crate::OrganizationCustomRoleServiceError::Conflict
        | crate::OrganizationCustomRoleServiceError::InvalidTransition => {
            OrganizationCustomRoleBindingServiceError::RoleUnavailable
        }
        crate::OrganizationCustomRoleServiceError::EntitlementRestricted => {
            OrganizationCustomRoleBindingServiceError::EntitlementRestricted
        }
        crate::OrganizationCustomRoleServiceError::Internal => {
            OrganizationCustomRoleBindingServiceError::Internal
        }
    }
}

fn map_repository_error(
    error: OrganizationCustomRoleBindingRepositoryError,
) -> OrganizationCustomRoleBindingServiceError {
    match error {
        OrganizationCustomRoleBindingRepositoryError::InvalidInput => {
            OrganizationCustomRoleBindingServiceError::InvalidInput
        }
        OrganizationCustomRoleBindingRepositoryError::NotFound => {
            OrganizationCustomRoleBindingServiceError::NotFound
        }
        OrganizationCustomRoleBindingRepositoryError::MembershipUnavailable => {
            OrganizationCustomRoleBindingServiceError::MembershipUnavailable
        }
        OrganizationCustomRoleBindingRepositoryError::RoleUnavailable => {
            OrganizationCustomRoleBindingServiceError::RoleUnavailable
        }
        OrganizationCustomRoleBindingRepositoryError::Conflict => {
            OrganizationCustomRoleBindingServiceError::Conflict
        }
        OrganizationCustomRoleBindingRepositoryError::Query
        | OrganizationCustomRoleBindingRepositoryError::Timeout
        | OrganizationCustomRoleBindingRepositoryError::Invariant => {
            OrganizationCustomRoleBindingServiceError::Internal
        }
    }
}
