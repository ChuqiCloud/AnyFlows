use std::{fmt, future::Future, pin::Pin};

use af_db::{
    OrganizationSsoPolicyRecord, OrganizationSsoPolicyUpdate, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationPermission, OrganizationRole,
    OrganizationSsoEnforcementPolicy,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 管理端可见的企业 SSO 强制策略投影；不暴露恢复流程的内部材料。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationSsoPolicy {
    policy: OrganizationSsoEnforcementPolicy,
    recovery_ready: bool,
    version: i64,
}

impl OrganizationSsoPolicy {
    fn from_record(record: OrganizationSsoPolicyRecord) -> Self {
        Self {
            policy: record.policy(),
            recovery_ready: record.recovery_ready(),
            version: record.version(),
        }
    }

    /// 返回当前强制策略。
    #[must_use]
    pub const fn policy(&self) -> OrganizationSsoEnforcementPolicy {
        self.policy
    }

    /// 返回受控恢复流程是否已就绪，不返回恢复凭据或操作细节。
    #[must_use]
    pub const fn recovery_ready(&self) -> bool {
        self.recovery_ready
    }

    /// 返回策略的 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 企业 SSO 强制策略的 CAS 变更命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoPolicyCommand {
    policy: OrganizationSsoEnforcementPolicy,
    expected_version: i64,
}

impl OrganizationSsoPolicyCommand {
    /// 构造策略变更命令；版本必须指向当前持久化版本。
    pub fn new(
        policy: OrganizationSsoEnforcementPolicy,
        expected_version: i64,
    ) -> Result<Self, OrganizationSsoPolicyServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationSsoPolicyServiceError::InvalidInput);
        }
        Ok(Self {
            policy,
            expected_version,
        })
    }
}

/// 企业 SSO 强制策略服务的稳定错误分类。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoPolicyServiceError {
    /// 命令不满足版本等基础输入边界。
    #[error("企业 SSO 强制策略输入无效")]
    InvalidInput,
    /// 当前成员不是 Owner，或不具有恢复策略权限。
    #[error("企业 SSO 强制策略权限不足")]
    Forbidden,
    /// 当前企业没有有效的 SSO 商业授权。
    #[error("企业 SSO 商业授权不可用")]
    EntitlementRestricted,
    /// 企业范围内没有策略记录。
    #[error("企业 SSO 强制策略不存在")]
    NotFound,
    /// 策略 CAS 版本已过期。
    #[error("企业 SSO 强制策略版本冲突")]
    Conflict,
    /// Required 的 Provider、域名或恢复等服务端事实尚未满足。
    #[error("企业 SSO 强制策略暂不可启用")]
    Unavailable,
    /// 仓储出现无法安全恢复的内部错误。
    #[error("企业 SSO 强制策略服务内部失败")]
    Internal,
}

pub type OrganizationSsoPolicyFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationSsoPolicy, OrganizationSsoPolicyServiceError>>
            + Send
            + 'a,
    >,
>;

/// 企业 SSO 策略服务；仅在受控企业上下文内读写，尚不开放 HTTP 路由。
pub trait OrganizationSsoPolicyService: Send + Sync {
    /// 读取当前企业的策略投影。
    fn get(&self, context: OrganizationContext) -> OrganizationSsoPolicyFuture<'_>;

    /// 以 CAS 更新策略；Required 的依赖由仓储事务重新核验。
    fn update(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoPolicyCommand,
    ) -> OrganizationSsoPolicyFuture<'_>;
}

/// 基于企业 SSO 仓储的策略服务实现。
#[derive(Clone)]
pub struct DatabaseOrganizationSsoPolicyService {
    repository: OrganizationSsoProviderRepository,
}

impl DatabaseOrganizationSsoPolicyService {
    /// 绑定启动期创建的企业 SSO 仓储。
    #[must_use]
    pub const fn new(repository: OrganizationSsoProviderRepository) -> Self {
        Self { repository }
    }
}

impl OrganizationSsoPolicyService for DatabaseOrganizationSsoPolicyService {
    fn get(&self, context: OrganizationContext) -> OrganizationSsoPolicyFuture<'_> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .policy(context.organization_id())
                .await
                .map(OrganizationSsoPolicy::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoPolicyCommand,
    ) -> OrganizationSsoPolicyFuture<'_> {
        Box::pin(async move {
            require_recovery_manage(&context)?;
            let write = OrganizationSsoPolicyUpdate::new(command.policy, command.expected_version)
                .map_err(map_repository_error)?;
            self.repository
                .update_policy(context.organization_id(), write)
                .await
                .map(OrganizationSsoPolicy::from_record)
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationSsoPolicyService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoPolicyService(<repository>)")
    }
}

/// 读取沿用 SSO 管理面的 Owner/Admin 范围，但仍要求有效 SSO 授权。
fn require_read(context: &OrganizationContext) -> Result<(), OrganizationSsoPolicyServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::SsoRead)
    {
        return Err(OrganizationSsoPolicyServiceError::Forbidden);
    }
    require_active_sso_entitlement(context)
}

/// 策略会影响全企业登录路径，仅 Owner 能凭恢复权限提交 CAS 变更。
fn require_recovery_manage(
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoPolicyServiceError> {
    if context.role() != OrganizationRole::Owner
        || !context.allows(OrganizationPermission::SsoRecovery)
    {
        return Err(OrganizationSsoPolicyServiceError::Forbidden);
    }
    require_active_sso_entitlement(context)
}

fn require_active_sso_entitlement(
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoPolicyServiceError> {
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationSsoPolicyServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoPolicyServiceError {
    match error {
        OrganizationSsoRepositoryError::InvalidInput => {
            OrganizationSsoPolicyServiceError::InvalidInput
        }
        OrganizationSsoRepositoryError::NotFound => OrganizationSsoPolicyServiceError::NotFound,
        OrganizationSsoRepositoryError::Conflict => OrganizationSsoPolicyServiceError::Conflict,
        OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoPolicyServiceError::Unavailable
        }
        OrganizationSsoRepositoryError::AlreadyProcessed
        | OrganizationSsoRepositoryError::Expired
        | OrganizationSsoRepositoryError::InvalidTransition => {
            OrganizationSsoPolicyServiceError::Unavailable
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => OrganizationSsoPolicyServiceError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_command_rejects_invalid_cas_versions() {
        for version in [i64::MIN, -1, 0, i64::MAX] {
            assert_eq!(
                OrganizationSsoPolicyCommand::new(
                    OrganizationSsoEnforcementPolicy::Optional,
                    version,
                ),
                Err(OrganizationSsoPolicyServiceError::InvalidInput)
            );
        }
        assert!(
            OrganizationSsoPolicyCommand::new(OrganizationSsoEnforcementPolicy::Required, 1,)
                .is_ok()
        );
    }
}
