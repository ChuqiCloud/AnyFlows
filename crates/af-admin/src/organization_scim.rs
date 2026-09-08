use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationScimAuditPageRecord, OrganizationScimAuditQuery, OrganizationScimAuditRepository,
    OrganizationScimAuditRepositoryError, OrganizationScimAuditWriteOutcome,
    OrganizationScimGroupRepository, OrganizationScimGroupRepositoryError,
    OrganizationScimGroupSyncResult, OrganizationScimTokenRecord, OrganizationScimTokenRepository,
    OrganizationScimTokenRepositoryError, OrganizationScimUserRepository,
    OrganizationScimUserRepositoryError, OrganizationScimUserSyncResult,
};
pub use af_db::{OrganizationScimGroupSyncOutcome, OrganizationScimUserSyncOutcome};
use af_domain::{
    DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT, DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS,
    DEFAULT_TOKEN_SCIM_RATE_LIMIT, OrganizationId, OrganizationScimAuditFacts,
    OrganizationScimGroupResource, OrganizationScimRateLimitDecision,
    OrganizationScimRateLimitFuture, OrganizationScimRateLimitRule, OrganizationScimRateLimitStore,
    OrganizationScimRateLimitSubject, OrganizationScimRateLimitWindow, OrganizationScimResourceId,
    OrganizationScimResourceVersion, OrganizationScimToken, OrganizationScimUserResource,
};
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

/// SCIM 组资源同步服务向 HTTP 层暴露的稳定错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimGroupSyncError {
    #[error("SCIM 组同步令牌不可用")]
    Restricted,
    #[error("SCIM 组同步范围不足")]
    Forbidden,
    #[error("SCIM 组同步版本或绑定冲突")]
    Conflict,
    #[error("SCIM 组同步成员不可用")]
    MemberUnavailable,
    #[error("SCIM 组同步团队不可用")]
    TeamUnavailable,
    #[error("SCIM 组同步生命周期被拒绝")]
    LifecycleRejected,
    #[error("SCIM 组同步服务暂不可用")]
    Internal,
}

/// SCIM 脱敏审计写入错误；审计不可用时调用方必须失败关闭。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuditError {
    #[error("SCIM 审计服务暂不可用")]
    Internal,
}

/// 管理端读取 SCIM 脱敏审计的稳定错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuditReadError {
    #[error("SCIM 审计读取服务暂不可用")]
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

pub type OrganizationScimGroupSyncFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationScimGroupSyncResult, OrganizationScimGroupSyncError>>
            + Send
            + 'a,
    >,
>;

pub type OrganizationScimRateLimitAdmissionFuture<'a> = OrganizationScimRateLimitFuture<'a>;

pub type OrganizationScimAuditFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditError>>
            + Send
            + 'a,
    >,
>;

pub type OrganizationScimAuditListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationScimAuditPageRecord, OrganizationScimAuditReadError>>
            + Send
            + 'a,
    >,
>;

/// 以企业路径为边界校验 SCIM 令牌的服务契约。
pub trait OrganizationScimService: Send + Sync {
    fn record_audit<'a>(
        &'a self,
        facts: OrganizationScimAuditFacts,
    ) -> OrganizationScimAuditFuture<'a>;

    /// 按企业范围读取已脱敏的 SCIM 审计事实。
    fn list_audit<'a>(
        &'a self,
        organization_id: OrganizationId,
        query: OrganizationScimAuditQuery,
        retention_days: i64,
    ) -> OrganizationScimAuditListFuture<'a>;

    fn admit_rate_limit<'a>(
        &'a self,
        organization_id: OrganizationId,
        token: &'a OrganizationScimTokenRecord,
    ) -> OrganizationScimRateLimitAdmissionFuture<'a>;

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

    fn synchronize_group<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimGroupResource,
    ) -> OrganizationScimGroupSyncFuture<'a>;

    fn delete_group<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource_id: OrganizationScimResourceId,
        expected_version: OrganizationScimResourceVersion,
    ) -> OrganizationScimGroupSyncFuture<'a>;
}

/// 使用数据库令牌仓储实现受控 SCIM 认证。
pub struct DatabaseOrganizationScimService {
    repository: Arc<OrganizationScimTokenRepository>,
    user_repository: Arc<OrganizationScimUserRepository>,
    group_repository: Arc<OrganizationScimGroupRepository>,
    audit_repository: Arc<OrganizationScimAuditRepository>,
    rate_limit_store: Option<Arc<dyn OrganizationScimRateLimitStore>>,
}

impl DatabaseOrganizationScimService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationScimTokenRepository>,
        user_repository: Arc<OrganizationScimUserRepository>,
        group_repository: Arc<OrganizationScimGroupRepository>,
        audit_repository: Arc<OrganizationScimAuditRepository>,
        rate_limit_store: Option<Arc<dyn OrganizationScimRateLimitStore>>,
    ) -> Self {
        Self {
            repository,
            user_repository,
            group_repository,
            audit_repository,
            rate_limit_store,
        }
    }
}

impl OrganizationScimService for DatabaseOrganizationScimService {
    fn record_audit<'a>(
        &'a self,
        facts: OrganizationScimAuditFacts,
    ) -> OrganizationScimAuditFuture<'a> {
        Box::pin(async move {
            self.audit_repository
                .record(&facts)
                .await
                .map_err(map_audit_repository_error)
        })
    }

    fn list_audit<'a>(
        &'a self,
        organization_id: OrganizationId,
        query: OrganizationScimAuditQuery,
        retention_days: i64,
    ) -> OrganizationScimAuditListFuture<'a> {
        Box::pin(async move {
            if !(1..=3650).contains(&retention_days) {
                return Err(OrganizationScimAuditReadError::Internal);
            }
            let now: i64 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| OrganizationScimAuditReadError::Internal)?
                .as_secs()
                .try_into()
                .map_err(|_| OrganizationScimAuditReadError::Internal)?;
            let cutoff = now
                .checked_sub(
                    retention_days
                        .checked_mul(24 * 60 * 60)
                        .ok_or(OrganizationScimAuditReadError::Internal)?,
                )
                .ok_or(OrganizationScimAuditReadError::Internal)?;
            self.audit_repository
                .prune_before(organization_id, cutoff)
                .await
                .map_err(|_| OrganizationScimAuditReadError::Internal)?;
            self.audit_repository
                .list(organization_id, &query)
                .await
                .map_err(|_| OrganizationScimAuditReadError::Internal)
        })
    }

    fn admit_rate_limit<'a>(
        &'a self,
        organization_id: OrganizationId,
        token: &'a OrganizationScimTokenRecord,
    ) -> OrganizationScimRateLimitAdmissionFuture<'a> {
        Box::pin(async move {
            let Some(store) = self.rate_limit_store.as_ref() else {
                return OrganizationScimRateLimitDecision::Rejected(
                    af_domain::OrganizationScimRateLimitRejection::StorageUnavailable,
                );
            };
            let Ok(window) = OrganizationScimRateLimitWindow::new(
                DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS,
            ) else {
                return OrganizationScimRateLimitDecision::Rejected(
                    af_domain::OrganizationScimRateLimitRejection::InvalidFact,
                );
            };
            let Some(organization_limit) =
                std::num::NonZeroU32::new(DEFAULT_ORGANIZATION_SCIM_RATE_LIMIT)
            else {
                return OrganizationScimRateLimitDecision::Rejected(
                    af_domain::OrganizationScimRateLimitRejection::InvalidFact,
                );
            };
            let Some(token_limit) = std::num::NonZeroU32::new(DEFAULT_TOKEN_SCIM_RATE_LIMIT) else {
                return OrganizationScimRateLimitDecision::Rejected(
                    af_domain::OrganizationScimRateLimitRejection::InvalidFact,
                );
            };
            let rules = [
                OrganizationScimRateLimitRule::new(
                    OrganizationScimRateLimitSubject::Organization(organization_id),
                    organization_limit,
                    window,
                ),
                OrganizationScimRateLimitRule::new(
                    OrganizationScimRateLimitSubject::Token {
                        organization_id,
                        token_key: token.token_key().clone(),
                    },
                    token_limit,
                    window,
                ),
            ];
            store.admit(&rules).await
        })
    }

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

    fn synchronize_group<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimGroupResource,
    ) -> OrganizationScimGroupSyncFuture<'a> {
        Box::pin(async move {
            self.group_repository
                .synchronize(token, resource)
                .await
                .map_err(map_group_repository_error)
        })
    }

    fn delete_group<'a>(
        &'a self,
        token: OrganizationScimTokenRecord,
        resource_id: OrganizationScimResourceId,
        expected_version: OrganizationScimResourceVersion,
    ) -> OrganizationScimGroupSyncFuture<'a> {
        Box::pin(async move {
            self.group_repository
                .delete(token, resource_id, expected_version)
                .await
                .map_err(map_group_repository_error)
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

fn map_audit_repository_error(
    _error: OrganizationScimAuditRepositoryError,
) -> OrganizationScimAuditError {
    OrganizationScimAuditError::Internal
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

fn map_group_repository_error(
    error: OrganizationScimGroupRepositoryError,
) -> OrganizationScimGroupSyncError {
    match error {
        OrganizationScimGroupRepositoryError::Restricted => {
            OrganizationScimGroupSyncError::Restricted
        }
        OrganizationScimGroupRepositoryError::Forbidden => {
            OrganizationScimGroupSyncError::Forbidden
        }
        OrganizationScimGroupRepositoryError::Conflict => OrganizationScimGroupSyncError::Conflict,
        OrganizationScimGroupRepositoryError::MemberUnavailable => {
            OrganizationScimGroupSyncError::MemberUnavailable
        }
        OrganizationScimGroupRepositoryError::TeamUnavailable => {
            OrganizationScimGroupSyncError::TeamUnavailable
        }
        OrganizationScimGroupRepositoryError::LifecycleRejected => {
            OrganizationScimGroupSyncError::LifecycleRejected
        }
        OrganizationScimGroupRepositoryError::Query
        | OrganizationScimGroupRepositoryError::Timeout
        | OrganizationScimGroupRepositoryError::Invariant => {
            OrganizationScimGroupSyncError::Internal
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
