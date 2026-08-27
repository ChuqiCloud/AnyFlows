use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationScimTokenRecord, OrganizationScimTokenRepository,
    OrganizationScimTokenRepositoryError,
};
use af_domain::{OrganizationId, OrganizationScimToken};
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

pub type OrganizationScimAuthenticationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationScimTokenRecord, OrganizationScimAuthenticationError>,
            > + Send
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
}

/// 使用数据库令牌仓储实现受控 SCIM 认证。
pub struct DatabaseOrganizationScimService {
    repository: Arc<OrganizationScimTokenRepository>,
}

impl DatabaseOrganizationScimService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationScimTokenRepository>) -> Self {
        Self { repository }
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
