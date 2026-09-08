use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationSsoProviderRepository, OrganizationSsoRecoverySuspend,
    OrganizationSsoRecoveryUpdate, OrganizationSsoRepositoryError,
};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationPermission, OrganizationRole,
    OrganizationSsoEnforcementPolicy,
};
use thiserror::Error;

use crate::{
    OrganizationContext, SessionPrincipal, UserProfileError, UserProfileService,
    UserSecurityStepUpCommand,
};

/// 企业 SSO 恢复操作的安全投影；不返回恢复凭据或二次因子材料。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationSsoRecovery {
    policy: OrganizationSsoEnforcementPolicy,
    recovery_ready: bool,
    version: i64,
}

impl OrganizationSsoRecovery {
    fn from_record(record: af_db::OrganizationSsoPolicyRecord) -> Self {
        Self {
            policy: record.policy(),
            recovery_ready: record.recovery_ready(),
            version: record.version(),
        }
    }

    /// 返回当前企业 SSO 强制策略。
    #[must_use]
    pub const fn policy(&self) -> OrganizationSsoEnforcementPolicy {
        self.policy
    }

    /// 返回恢复入口是否已完成受控登记。
    #[must_use]
    pub const fn recovery_ready(&self) -> bool {
        self.recovery_ready
    }

    /// 返回策略版本，用于下一次 CAS 写入。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 执行 Owner 恢复操作的命令；敏感 step-up 材料只在内存中短暂存在。
pub struct OrganizationSsoRecoveryCommand {
    step_up: UserSecurityStepUpCommand,
    expected_version: i64,
}

impl OrganizationSsoRecoveryCommand {
    /// 构造恢复操作命令并校验策略版本边界。
    pub fn new(
        step_up: UserSecurityStepUpCommand,
        expected_version: i64,
    ) -> Result<Self, OrganizationSsoRecoveryServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationSsoRecoveryServiceError::InvalidInput);
        }
        Ok(Self {
            step_up,
            expected_version,
        })
    }

    fn into_parts(self) -> (UserSecurityStepUpCommand, i64) {
        (self.step_up, self.expected_version)
    }
}

impl fmt::Debug for OrganizationSsoRecoveryCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoRecoveryCommand(<redacted>)")
    }
}

/// 企业 SSO 恢复服务的稳定错误分类。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoRecoveryServiceError {
    /// 命令或版本边界无效。
    #[error("企业 SSO 恢复请求无效")]
    InvalidInput,
    /// 当前会话不是目标企业 Owner，或缺少恢复权限。
    #[error("企业 SSO 恢复权限不足")]
    Forbidden,
    /// 企业商业授权不可用。
    #[error("企业 SSO 恢复授权不可用")]
    EntitlementRestricted,
    /// 当前密码或二次因子校验未通过。
    #[error("企业 SSO 恢复二次验证失败")]
    Rejected,
    /// 策略版本已被其他操作推进。
    #[error("企业 SSO 恢复版本冲突")]
    Conflict,
    /// 恢复事实暂时不可写入。
    #[error("企业 SSO 恢复暂不可用")]
    Unavailable,
    /// 依赖服务或持久化发生内部失败。
    #[error("企业 SSO 恢复内部失败")]
    Internal,
}

pub type OrganizationSsoRecoveryFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationSsoRecovery, OrganizationSsoRecoveryServiceError>>
            + Send
            + 'a,
    >,
>;

/// 仅允许已登录 Owner 通过当前密码和二次因子执行企业恢复操作。
pub trait OrganizationSsoRecoveryService: Send + Sync {
    fn establish<'a>(
        &'a self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> OrganizationSsoRecoveryFuture<'a>;

    /// 通过二次验证将 Required 暂停为 Optional；已暂停时幂等返回当前投影。
    fn suspend_required<'a>(
        &'a self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> OrganizationSsoRecoveryFuture<'a>;
}

/// 基于用户安全服务和企业 SSO 仓储的恢复登记实现。
#[derive(Clone)]
pub struct DatabaseOrganizationSsoRecoveryService {
    user_profile_service: Arc<dyn UserProfileService>,
    repository: OrganizationSsoProviderRepository,
}

impl DatabaseOrganizationSsoRecoveryService {
    /// 绑定用户 step-up 服务与企业 SSO 仓储。
    #[must_use]
    pub fn new(
        user_profile_service: Arc<dyn UserProfileService>,
        repository: OrganizationSsoProviderRepository,
    ) -> Self {
        Self {
            user_profile_service,
            repository,
        }
    }

    async fn establish_inner(
        &self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> Result<OrganizationSsoRecovery, OrganizationSsoRecoveryServiceError> {
        require_owner(&principal, &context)?;
        require_active_entitlement(&context)?;
        let (step_up, expected_version) = command.into_parts();
        self.user_profile_service
            .step_up(principal, step_up)
            .await
            .map_err(map_user_profile_error)?;
        let write =
            OrganizationSsoRecoveryUpdate::new(expected_version).map_err(map_repository_error)?;
        self.repository
            .mark_recovery_ready(context.organization_id(), write)
            .await
            .map(OrganizationSsoRecovery::from_record)
            .map_err(map_repository_error)
    }

    async fn suspend_required_inner(
        &self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> Result<OrganizationSsoRecovery, OrganizationSsoRecoveryServiceError> {
        require_owner(&principal, &context)?;
        require_active_sso_entitlement(&context)?;
        let (step_up, expected_version) = command.into_parts();
        self.user_profile_service
            .step_up(principal, step_up)
            .await
            .map_err(map_user_profile_error)?;
        let write =
            OrganizationSsoRecoverySuspend::new(expected_version).map_err(map_repository_error)?;
        self.repository
            .suspend_required(context.organization_id(), write)
            .await
            .map(OrganizationSsoRecovery::from_record)
            .map_err(map_repository_error)
    }
}

impl OrganizationSsoRecoveryService for DatabaseOrganizationSsoRecoveryService {
    fn establish<'a>(
        &'a self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> OrganizationSsoRecoveryFuture<'a> {
        Box::pin(self.establish_inner(principal, context, command))
    }

    fn suspend_required<'a>(
        &'a self,
        principal: SessionPrincipal,
        context: OrganizationContext,
        command: OrganizationSsoRecoveryCommand,
    ) -> OrganizationSsoRecoveryFuture<'a> {
        Box::pin(self.suspend_required_inner(principal, context, command))
    }
}

impl fmt::Debug for DatabaseOrganizationSsoRecoveryService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoRecoveryService(<redacted>)")
    }
}

fn require_owner(
    principal: &SessionPrincipal,
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoRecoveryServiceError> {
    if principal.user_id() != context.user_id()
        || context.role() != OrganizationRole::Owner
        || !context.allows(OrganizationPermission::SsoRecovery)
    {
        return Err(OrganizationSsoRecoveryServiceError::Forbidden);
    }
    Ok(())
}

fn require_active_entitlement(
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoRecoveryServiceError> {
    if context.entitlement().status() != OrganizationEntitlementStatus::Active {
        return Err(OrganizationSsoRecoveryServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn require_active_sso_entitlement(
    context: &OrganizationContext,
) -> Result<(), OrganizationSsoRecoveryServiceError> {
    require_active_entitlement(context)?;
    if !context.entitlement().sso_enabled() {
        return Err(OrganizationSsoRecoveryServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn map_user_profile_error(error: UserProfileError) -> OrganizationSsoRecoveryServiceError {
    match error {
        UserProfileError::InvalidInput
        | UserProfileError::InvalidSession
        | UserProfileError::CurrentPasswordInvalid
        | UserProfileError::TwoFactorRequired
        | UserProfileError::TwoFactorInvalid => OrganizationSsoRecoveryServiceError::Rejected,
        UserProfileError::TwoFactorAlreadyEnabled
        | UserProfileError::TwoFactorNotEnabled
        | UserProfileError::PasskeyRegistrationRejected
        | UserProfileError::PasskeyNotFound
        | UserProfileError::Conflict
        | UserProfileError::Internal => OrganizationSsoRecoveryServiceError::Internal,
    }
}

fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoRecoveryServiceError {
    match error {
        OrganizationSsoRepositoryError::InvalidInput => {
            OrganizationSsoRecoveryServiceError::InvalidInput
        }
        OrganizationSsoRepositoryError::Conflict => OrganizationSsoRecoveryServiceError::Conflict,
        OrganizationSsoRepositoryError::NotFound => {
            OrganizationSsoRecoveryServiceError::Unavailable
        }
        OrganizationSsoRepositoryError::AlreadyProcessed
        | OrganizationSsoRepositoryError::Expired
        | OrganizationSsoRepositoryError::InvalidTransition
        | OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoRecoveryServiceError::Unavailable
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => {
            OrganizationSsoRecoveryServiceError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_rejects_invalid_versions_without_exposing_step_up() {
        for version in [i64::MIN, -1, 0, i64::MAX] {
            let step_up =
                UserSecurityStepUpCommand::new("password".to_owned(), Some("123456".to_owned()))
                    .expect("测试 step-up 输入有效");
            let command = OrganizationSsoRecoveryCommand::new(step_up, version);
            assert!(matches!(
                command,
                Err(OrganizationSsoRecoveryServiceError::InvalidInput)
            ));
        }

        let step_up =
            UserSecurityStepUpCommand::new("password".to_owned(), Some("123456".to_owned()))
                .expect("测试 step-up 输入有效");
        let command = OrganizationSsoRecoveryCommand::new(step_up, 1).expect("有效版本应构造成功");
        assert!(!format!("{command:?}").contains("password"));
        assert!(!format!("{command:?}").contains("123456"));
    }

    #[test]
    fn step_up_failures_are_closed() {
        assert_eq!(
            map_user_profile_error(UserProfileError::CurrentPasswordInvalid),
            OrganizationSsoRecoveryServiceError::Rejected
        );
        assert_eq!(
            map_user_profile_error(UserProfileError::Internal),
            OrganizationSsoRecoveryServiceError::Internal
        );
    }
}
