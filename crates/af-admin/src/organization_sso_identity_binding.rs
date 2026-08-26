use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationSsoIdentityRecord, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use af_domain::{
    OrganizationId, OrganizationMembershipId, OrganizationSsoIdentityBindingMethod,
    OrganizationSsoIdentityStatus, OrganizationSsoProtocol,
};
use thiserror::Error;

use crate::{
    ExchangedOrganizationSsoLoginTicket, SessionPrincipal, UserProfileError, UserProfileService,
    UserSecurityStepUpCommand,
};
use crate::{
    PasskeyAuthenticationCommand, PasskeyAuthenticationError, PasskeyAuthenticationService,
};

/// 显式绑定成功后的脱敏身份投影，不返回 OIDC subject 摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationSsoExplicitIdentityBinding {
    id: i64,
    organization_id: OrganizationId,
    provider_id: i64,
    membership_id: OrganizationMembershipId,
    status: OrganizationSsoIdentityStatus,
    binding_method: OrganizationSsoIdentityBindingMethod,
    version: i64,
}

impl OrganizationSsoExplicitIdentityBinding {
    fn from_record(record: OrganizationSsoIdentityRecord) -> Self {
        Self {
            id: record.id(),
            organization_id: record.organization_id(),
            provider_id: record.provider_id(),
            membership_id: record.membership_id(),
            status: record.status(),
            binding_method: record.binding_method(),
            version: record.version(),
        }
    }

    /// 返回身份绑定的内部标识。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回身份绑定所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回已经由 ticket 固定的 Provider 内部标识。
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }

    /// 返回当前登录成员在企业内的成员关系标识。
    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }

    /// 返回身份可用状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationSsoIdentityStatus {
        self.status
    }

    /// 返回身份绑定来源；本服务仅会生成 Explicit。
    #[must_use]
    pub const fn binding_method(&self) -> OrganizationSsoIdentityBindingMethod {
        self.binding_method
    }

    /// 返回身份版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 显式身份绑定的稳定错误分类；不携带企业、票据、subject 或认证材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoIdentityBindingServiceError {
    /// ticket 协议或已验证投影不满足显式绑定前提。
    #[error("企业 SSO 显式身份绑定请求无效")]
    InvalidInput,
    /// 当前会话二次验证未通过，或企业身份绑定资格不可用。
    #[error("企业 SSO 显式身份绑定被拒绝")]
    Rejected,
    /// 同一 Provider 的 subject 或成员已经存在身份绑定。
    #[error("企业 SSO 显式身份绑定发生冲突")]
    Conflict,
    /// 仓储或依赖服务发生不可安全恢复的内部失败。
    #[error("企业 SSO 显式身份绑定内部失败")]
    Internal,
}

/// 显式身份绑定的对象安全 Future。
pub type OrganizationSsoExplicitIdentityBindingFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationSsoExplicitIdentityBinding,
                    OrganizationSsoIdentityBindingServiceError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 显式身份绑定使用的二次验证证明。
#[derive(Debug)]
pub enum OrganizationSsoIdentityBindingStepUp {
    /// 当前密码以及已启用的 TOTP 或备份码。
    PasswordAndSecondFactor(UserSecurityStepUpCommand),
    /// 已消费的一次性 Passkey 认证挑战。
    Passkey(PasskeyAuthenticationCommand),
}

/// 将已验证 SSO ticket 绑定到当前已登录成员的受控服务边界。
pub trait OrganizationSsoIdentityBindingService: Send + Sync {
    /// 完成当前成员的密码加已启用二次因子确认，并写入显式身份绑定。
    fn bind_explicit_identity<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        ticket: ExchangedOrganizationSsoLoginTicket,
        step_up: UserSecurityStepUpCommand,
    ) -> OrganizationSsoExplicitIdentityBindingFuture<'a>;

    /// 使用密码/TOTP/备份码或 Passkey 完成显式绑定前的二次验证。
    fn bind_explicit_identity_with_step_up<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        ticket: ExchangedOrganizationSsoLoginTicket,
        step_up: OrganizationSsoIdentityBindingStepUp,
    ) -> OrganizationSsoExplicitIdentityBindingFuture<'a> {
        match step_up {
            OrganizationSsoIdentityBindingStepUp::PasswordAndSecondFactor(command) => {
                self.bind_explicit_identity(principal, organization_id, ticket, command)
            }
            OrganizationSsoIdentityBindingStepUp::Passkey(_) => {
                Box::pin(async { Err(OrganizationSsoIdentityBindingServiceError::Internal) })
            }
        }
    }
}

/// 基于当前会话、已验证 ticket 和企业事务的显式身份绑定实现。
#[derive(Clone)]
pub struct DatabaseOrganizationSsoIdentityBindingService {
    user_profile_service: Arc<dyn UserProfileService>,
    passkey_authentication_service: Option<Arc<dyn PasskeyAuthenticationService>>,
    repository: OrganizationSsoProviderRepository,
}

impl DatabaseOrganizationSsoIdentityBindingService {
    /// 组装显式绑定所需的用户二次验证服务和企业 SSO 仓储。
    #[must_use]
    pub fn new(
        user_profile_service: Arc<dyn UserProfileService>,
        repository: OrganizationSsoProviderRepository,
    ) -> Self {
        Self {
            user_profile_service,
            passkey_authentication_service: None,
            repository,
        }
    }

    /// 注入可选的 Passkey 认证服务；未注入时 Passkey step-up 失败关闭。
    #[must_use]
    pub fn with_passkey_authentication_service(
        mut self,
        service: Arc<dyn PasskeyAuthenticationService>,
    ) -> Self {
        self.passkey_authentication_service = Some(service);
        self
    }

    async fn bind_explicit_identity_inner(
        &self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        ticket: ExchangedOrganizationSsoLoginTicket,
        step_up: OrganizationSsoIdentityBindingStepUp,
    ) -> Result<OrganizationSsoExplicitIdentityBinding, OrganizationSsoIdentityBindingServiceError>
    {
        // SAML 尚未具备受控 subject 投影，不能通过本入口绑定身份。
        if ticket.protocol() != OrganizationSsoProtocol::Oidc {
            return Err(OrganizationSsoIdentityBindingServiceError::Rejected);
        }
        let subject_digest = ticket
            .subject_digest()
            .ok_or(OrganizationSsoIdentityBindingServiceError::InvalidInput)?
            .to_owned();
        match step_up {
            OrganizationSsoIdentityBindingStepUp::PasswordAndSecondFactor(command) => self
                .user_profile_service
                .step_up(principal, command)
                .await
                .map_err(map_user_profile_error)?,
            OrganizationSsoIdentityBindingStepUp::Passkey(command) => {
                verify_passkey_step_up(
                    self.passkey_authentication_service.as_deref(),
                    principal,
                    command,
                )
                .await?;
            }
        }
        let record = self
            .repository
            .bind_explicit_identity(
                organization_id,
                principal.user_id(),
                ticket.provider_id(),
                subject_digest,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(OrganizationSsoExplicitIdentityBinding::from_record(record))
    }
}

impl OrganizationSsoIdentityBindingService for DatabaseOrganizationSsoIdentityBindingService {
    fn bind_explicit_identity<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        ticket: ExchangedOrganizationSsoLoginTicket,
        step_up: UserSecurityStepUpCommand,
    ) -> OrganizationSsoExplicitIdentityBindingFuture<'a> {
        Box::pin(self.bind_explicit_identity_inner(
            principal,
            organization_id,
            ticket,
            OrganizationSsoIdentityBindingStepUp::PasswordAndSecondFactor(step_up),
        ))
    }

    fn bind_explicit_identity_with_step_up<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        ticket: ExchangedOrganizationSsoLoginTicket,
        step_up: OrganizationSsoIdentityBindingStepUp,
    ) -> OrganizationSsoExplicitIdentityBindingFuture<'a> {
        Box::pin(self.bind_explicit_identity_inner(principal, organization_id, ticket, step_up))
    }
}

impl fmt::Debug for DatabaseOrganizationSsoIdentityBindingService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoIdentityBindingService(<redacted>)")
    }
}

fn map_user_profile_error(error: UserProfileError) -> OrganizationSsoIdentityBindingServiceError {
    match error {
        UserProfileError::InvalidInput
        | UserProfileError::InvalidSession
        | UserProfileError::CurrentPasswordInvalid
        | UserProfileError::TwoFactorRequired
        | UserProfileError::TwoFactorInvalid => {
            OrganizationSsoIdentityBindingServiceError::Rejected
        }
        UserProfileError::TwoFactorAlreadyEnabled
        | UserProfileError::TwoFactorNotEnabled
        | UserProfileError::PasskeyRegistrationRejected
        | UserProfileError::PasskeyNotFound
        | UserProfileError::Conflict
        | UserProfileError::Internal => OrganizationSsoIdentityBindingServiceError::Internal,
    }
}

async fn verify_passkey_step_up(
    service: Option<&dyn PasskeyAuthenticationService>,
    principal: SessionPrincipal,
    command: PasskeyAuthenticationCommand,
) -> Result<(), OrganizationSsoIdentityBindingServiceError> {
    let service = service.ok_or(OrganizationSsoIdentityBindingServiceError::Internal)?;
    let authenticated_user = service
        .finish(command)
        .await
        .map_err(map_passkey_authentication_error)?;
    if authenticated_user != principal.user_id() {
        return Err(OrganizationSsoIdentityBindingServiceError::Rejected);
    }
    Ok(())
}

fn map_passkey_authentication_error(
    error: PasskeyAuthenticationError,
) -> OrganizationSsoIdentityBindingServiceError {
    match error {
        PasskeyAuthenticationError::InvalidInput
        | PasskeyAuthenticationError::LoginDisabled
        | PasskeyAuthenticationError::Rejected => {
            OrganizationSsoIdentityBindingServiceError::Rejected
        }
        PasskeyAuthenticationError::Internal => {
            OrganizationSsoIdentityBindingServiceError::Internal
        }
    }
}

fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoIdentityBindingServiceError {
    match error {
        OrganizationSsoRepositoryError::Conflict => {
            OrganizationSsoIdentityBindingServiceError::Conflict
        }
        OrganizationSsoRepositoryError::InvalidInput
        | OrganizationSsoRepositoryError::NotFound
        | OrganizationSsoRepositoryError::AlreadyProcessed
        | OrganizationSsoRepositoryError::Expired
        | OrganizationSsoRepositoryError::InvalidTransition
        | OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoIdentityBindingServiceError::Rejected
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => {
            OrganizationSsoIdentityBindingServiceError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{TrustedClientIp, UserId};
    use serde_json::json;

    use super::{
        OrganizationSsoIdentityBindingServiceError, PasskeyAuthenticationCommand,
        PasskeyAuthenticationError, PasskeyAuthenticationFuture,
        PasskeyAuthenticationOptionsFuture, PasskeyAuthenticationService, SessionPrincipal,
        verify_passkey_step_up,
    };
    use crate::SessionRole;

    #[derive(Clone, Copy)]
    struct MockPasskeyService {
        result: Result<UserId, PasskeyAuthenticationError>,
    }

    impl PasskeyAuthenticationService for MockPasskeyService {
        fn start<'a>(
            &'a self,
            _client_ip: TrustedClientIp,
            _username: &'a str,
        ) -> PasskeyAuthenticationOptionsFuture<'a> {
            Box::pin(async { Err(PasskeyAuthenticationError::Internal) })
        }

        fn finish<'a>(
            &'a self,
            _command: PasskeyAuthenticationCommand,
        ) -> PasskeyAuthenticationFuture<'a> {
            let result = self.result;
            Box::pin(async move { result })
        }
    }

    #[tokio::test]
    async fn passkey_step_up_requires_the_current_session_user() {
        let principal = SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User);
        let command = PasskeyAuthenticationCommand::new(json!({})).unwrap();
        let service = MockPasskeyService {
            result: Ok(UserId::new(7).unwrap()),
        };
        assert_eq!(
            verify_passkey_step_up(Some(&service), principal, command).await,
            Ok(())
        );

        let other = MockPasskeyService {
            result: Ok(UserId::new(8).unwrap()),
        };
        let command = PasskeyAuthenticationCommand::new(json!({})).unwrap();
        assert_eq!(
            verify_passkey_step_up(Some(&other), principal, command).await,
            Err(OrganizationSsoIdentityBindingServiceError::Rejected)
        );
    }

    #[tokio::test]
    async fn passkey_step_up_fails_closed_without_service_or_on_authentication_failure() {
        let principal = SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User);
        let command = PasskeyAuthenticationCommand::new(json!({})).unwrap();
        assert_eq!(
            verify_passkey_step_up(None, principal, command).await,
            Err(OrganizationSsoIdentityBindingServiceError::Internal)
        );

        let service = MockPasskeyService {
            result: Err(PasskeyAuthenticationError::Rejected),
        };
        let command = PasskeyAuthenticationCommand::new(json!({})).unwrap();
        assert_eq!(
            verify_passkey_step_up(Some(&service), principal, command).await,
            Err(OrganizationSsoIdentityBindingServiceError::Rejected)
        );
    }
}
