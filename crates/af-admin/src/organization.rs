use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{OrganizationAccessRecord, OrganizationRepository, OrganizationRepositoryError};
use af_domain::{
    OrganizationId, OrganizationMembershipId, OrganizationPermission, OrganizationRole, UserId,
};
use thiserror::Error;

use crate::SessionPrincipal;

/// 已按会话用户和企业路径完成校验的企业上下文。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationContext {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    user_id: UserId,
    role: OrganizationRole,
    name: String,
    slug: String,
}

impl OrganizationContext {
    fn from_access(access: &OrganizationAccessRecord) -> Self {
        let membership = access.membership();
        Self {
            organization_id: access.organization().id(),
            membership_id: membership.id(),
            user_id: membership.user_id(),
            role: membership.role(),
            name: access.organization().name().to_owned(),
            slug: access.organization().slug().to_owned(),
        }
    }

    /// 返回已校验的企业标识。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回当前用户在企业中的成员关系标识。
    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }

    /// 返回被校验的会话用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回固定企业角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }

    /// 返回已校验的企业名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回已校验的企业短标识。
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// 判断当前企业角色是否拥有指定权限。
    #[must_use]
    pub fn allows(&self, permission: OrganizationPermission) -> bool {
        self.role.allows(permission)
    }
}

/// 企业目录中返回的最小安全摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAccessSummary {
    organization_id: OrganizationId,
    name: String,
    slug: String,
    role: OrganizationRole,
}

impl OrganizationAccessSummary {
    fn from_access(access: OrganizationAccessRecord) -> Self {
        Self {
            organization_id: access.organization().id(),
            name: access.organization().name().to_owned(),
            slug: access.organization().slug().to_owned(),
            role: access.membership().role(),
        }
    }

    /// 返回企业标识。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回企业名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回企业短标识。
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// 返回当前用户角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationAuthorizationError {
    /// 企业路径、成员关系或企业状态不满足进入条件。
    #[error("企业上下文不存在或当前用户无权访问")]
    NotFound,
    /// 当前企业角色未授予所需权限。
    #[error("当前企业角色无权执行该操作")]
    Forbidden,
    /// 企业授权查询发生内部错误。
    #[error("企业授权查询失败")]
    Internal,
}

pub type OrganizationAccessFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationContext, OrganizationAuthorizationError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<OrganizationAccessSummary>, OrganizationAuthorizationError>>
            + Send
            + 'a,
    >,
>;

/// 企业上下文授权服务；平台管理员角色不会绕过企业成员校验。
pub trait OrganizationAuthorizationService: Send + Sync {
    fn list<'a>(&'a self, principal: SessionPrincipal) -> OrganizationListFuture<'a>;
    fn context<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        permission: OrganizationPermission,
    ) -> OrganizationAccessFuture<'a>;
}

/// 使用组织仓储实现固定企业 RBAC。
pub struct DatabaseOrganizationAuthorizationService {
    repository: Arc<OrganizationRepository>,
}

impl DatabaseOrganizationAuthorizationService {
    /// 绑定启动期创建的企业仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationAuthorizationService for DatabaseOrganizationAuthorizationService {
    fn list<'a>(&'a self, principal: SessionPrincipal) -> OrganizationListFuture<'a> {
        Box::pin(async move {
            self.repository
                .list_authorized_access(principal.user_id())
                .await
                .map_err(map_repository_error)
                .map(|records| {
                    records
                        .into_iter()
                        .map(OrganizationAccessSummary::from_access)
                        .collect()
                })
        })
    }

    fn context<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        permission: OrganizationPermission,
    ) -> OrganizationAccessFuture<'a> {
        Box::pin(async move {
            let access = self
                .repository
                .authorized_access(organization_id, principal.user_id())
                .await
                .map_err(map_repository_error)?;
            let context = OrganizationContext::from_access(&access);
            if !context.allows(permission) {
                return Err(OrganizationAuthorizationError::Forbidden);
            }
            Ok(context)
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationAuthorizationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationAuthorizationService(<repository>)")
    }
}

fn map_repository_error(error: OrganizationRepositoryError) -> OrganizationAuthorizationError {
    match error {
        OrganizationRepositoryError::NotFound => OrganizationAuthorizationError::NotFound,
        OrganizationRepositoryError::Query
        | OrganizationRepositoryError::Timeout
        | OrganizationRepositoryError::Invariant
        | OrganizationRepositoryError::InvalidInput
        | OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::Conflict
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationAuthorizationError::Internal
        }
    }
}
