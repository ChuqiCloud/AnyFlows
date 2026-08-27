use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationScimTokenRecord, OrganizationScimTokenRepository,
    OrganizationScimTokenRepositoryError, OrganizationScimUserRepository,
    OrganizationScimUserRepositoryError, OrganizationScimUserSyncResult,
};
use af_domain::{OrganizationId, OrganizationScimToken, OrganizationScimUserResource};
use thiserror::Error;

/// SCIM Bearer 令牌认证服务返回的边界错误，不向 HTTP 层暴露数据库细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuthenticationError {
    /// 令牌不存在、已撤销或不属于请求企业。
    #[error("企业 SCIM 令牌无效")]
    InvalidToken,
    /// 认证依赖不可用或持久化事实损坏。
    #[error("企业 SCIM 认证内部错误")]
    Internal,
}

/// SCIM 用户资源同步服务向 HTTP 层暴露的稳定错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimUserSyncError {
    #[error("SCIM 用户同步令牌不可用")]
    Restricted,
    #[error("SCIM 用户同步范围不足")]
    Forbidden,
    #[error("SCIM 用户同步版本或绑定冲突")]
    Conflict,
    #[error("SCIM 用户同步目标不可用")]
    UserUnavailable,
    #[error("SCIM 用户同步生命周期被拒绝")]
    LifecycleRejected,
    #[error("SCIM 用户同步服务暂不可用")]
    Internal,
}

pub type OrganizationScimAuthenticationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationScimTokenRecord, OrganizationScimAuthenticationError>,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationScimUserSyncFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationScimUserSyncResult, OrganizationScimUserSyncError>>
            + Send
            + 'a,
    >,
>;

/// 以企业路径为边界校验 SCIM 令牌的服务契约。
pub trait OrganizationScimService: Send + Sync {
    fn authenticate<'a>(
        &'a self,
        organization_id: OrganizationId,
        token: OrganizationScimToken,
    ) -> OrganizationScimAuthenticationFuture<'a>;

    fn synchronize_user<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimUserResource,
    ) -> OrganizationScimUserSyncFuture<'a>;
}

/// 使用数据库令牌仓储实现受控 SCIM 认证。
pub struct DatabaseOrganizationScimService {
    repository: Arc<OrganizationScimTokenRepository>,
    user_repository: Arc<OrganizationScimUserRepository>,
}

impl DatabaseOrganizationScimService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationScimTokenRepository>,
        user_repository: Arc<OrganizationScimUserRepository>,
    ) -> Self {
        Self {
            repository,
            user_repository,
        }
    }
}

impl OrganizationScimService for DatabaseOrganizationScimService {
    fn authenticate<'a>(
        &'a self,
        organization_id: OrganizationId,
        token: OrganizationScimToken,
    ) -> OrganizationScimAuthenticationFuture<'a> {
        Box::pin(async move {
            self.repository
                .authenticate_for_organization(organization_id, token)
                .await
                .map_err(map_repository_error)
        })
    }

    fn synchronize_user<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimUserResource,
    ) -> OrganizationScimUserSyncFuture<'a> {
        Box::pin(async move {
            self.user_repository
                .synchronize(token, resource)
                .await
                .map_err(map_user_repository_error)
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationScimService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationScimService(<repository>)")
    }
}

fn map_repository_error(
    error: OrganizationScimTokenRepositoryError,
) -> OrganizationScimAuthenticationError {
    match error {
        OrganizationScimTokenRepositoryError::NotFound
        | OrganizationScimTokenRepositoryError::Restricted => {
            OrganizationScimAuthenticationError::InvalidToken
        }
        OrganizationScimTokenRepositoryError::InvalidInput
        | OrganizationScimTokenRepositoryError::Conflict
        | OrganizationScimTokenRepositoryError::Query
        | OrganizationScimTokenRepositoryError::Timeout
        | OrganizationScimTokenRepositoryError::Invariant => {
            OrganizationScimAuthenticationError::Internal
        }
    }
}

fn map_user_repository_error(
    error: OrganizationScimUserRepositoryError,
) -> OrganizationScimUserSyncError {
    match error {
        OrganizationScimUserRepositoryError::Restricted => {
            OrganizationScimUserSyncError::Restricted
        }
        OrganizationScimUserRepositoryError::Forbidden => OrganizationScimUserSyncError::Forbidden,
        OrganizationScimUserRepositoryError::Conflict => OrganizationScimUserSyncError::Conflict,
        OrganizationScimUserRepositoryError::UserUnavailable => {
            OrganizationScimUserSyncError::UserUnavailable
        }
        OrganizationScimUserRepositoryError::LifecycleRejected => {
            OrganizationScimUserSyncError::LifecycleRejected
        }
        OrganizationScimUserRepositoryError::Query
        | OrganizationScimUserRepositoryError::Timeout
        | OrganizationScimUserRepositoryError::Invariant => OrganizationScimUserSyncError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_errors_never_leak_storage_details() {
        assert_eq!(
            map_repository_error(OrganizationScimTokenRepositoryError::NotFound),
            OrganizationScimAuthenticationError::InvalidToken
        );
        assert_eq!(
            map_repository_error(OrganizationScimTokenRepositoryError::Query),
            OrganizationScimAuthenticationError::Internal
        );
        assert_eq!(
            map_repository_error(OrganizationScimTokenRepositoryError::Invariant),
            OrganizationScimAuthenticationError::Internal
        );
    }
}
