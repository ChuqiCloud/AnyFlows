use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_domain::OrganizationId;
use thiserror::Error;

use crate::{
    IssuedSession, OrganizationSsoLoginTransactionService,
    OrganizationSsoLoginTransactionServiceError, SessionAuthenticationError, SessionAuthenticator,
};

/// 企业 SSO ticket 交换并签发站点会话的异步结果。
pub type OrganizationSsoSessionFuture<'a> = Pin<
    Box<dyn Future<Output = Result<IssuedSession, OrganizationSsoSessionServiceError>> + Send + 'a>,
>;

/// 企业 SSO 会话交换的稳定错误分类，不携带 ticket、subject 或 Provider 原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoSessionServiceError {
    /// 请求缺少必要的企业或 ticket 材料。
    #[error("企业 SSO 会话交换请求无效")]
    InvalidInput,
    /// ticket、身份、成员或当前 SSO 条件不满足登录资格。
    #[error("企业 SSO 会话交换被拒绝")]
    Rejected,
    /// 依赖尚未配置，或当前状态暂时不可用。
    #[error("企业 SSO 会话交换暂不可用")]
    Unavailable,
    /// 内部依赖失败，不能安全判断会话是否签发。
    #[error("企业 SSO 会话交换内部失败")]
    Internal,
}

/// 消费已验证的一次性 ticket，并为其解析出的活动企业成员签发站点会话。
pub trait OrganizationSsoSessionService: Send + Sync {
    /// ticket 只允许在同源 POST 边界消费一次；调用方不得记录明文。
    fn exchange_ticket<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoSessionFuture<'a>;
}

/// 组合 SSO 登录事务、身份事实和现有会话签发器的生产实现。
pub struct DatabaseOrganizationSsoSessionService {
    login_service: Arc<dyn OrganizationSsoLoginTransactionService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
}

impl DatabaseOrganizationSsoSessionService {
    /// 绑定已完成回调验证的 ticket 服务和统一站点会话签发器。
    #[must_use]
    pub fn new(
        login_service: Arc<dyn OrganizationSsoLoginTransactionService>,
        session_authenticator: Arc<dyn SessionAuthenticator>,
    ) -> Self {
        Self {
            login_service,
            session_authenticator,
        }
    }

    async fn exchange_ticket_inner(
        &self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> Result<IssuedSession, OrganizationSsoSessionServiceError> {
        if ticket.is_empty() {
            return Err(OrganizationSsoSessionServiceError::InvalidInput);
        }
        let membership = self
            .login_service
            .exchange_ticket_and_resolve_identity(organization_id, ticket)
            .await
            .map_err(map_login_error)?;
        self.session_authenticator
            .issue_for_user(membership.user_id())
            .await
            .map_err(map_session_error)
    }
}

impl OrganizationSsoSessionService for DatabaseOrganizationSsoSessionService {
    fn exchange_ticket<'a>(
        &'a self,
        organization_id: OrganizationId,
        ticket: String,
    ) -> OrganizationSsoSessionFuture<'a> {
        Box::pin(self.exchange_ticket_inner(organization_id, ticket))
    }
}

impl fmt::Debug for DatabaseOrganizationSsoSessionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoSessionService(<redacted>)")
    }
}

fn map_login_error(
    error: OrganizationSsoLoginTransactionServiceError,
) -> OrganizationSsoSessionServiceError {
    match error {
        OrganizationSsoLoginTransactionServiceError::InvalidInput => {
            OrganizationSsoSessionServiceError::InvalidInput
        }
        OrganizationSsoLoginTransactionServiceError::Rejected
        | OrganizationSsoLoginTransactionServiceError::Expired
        | OrganizationSsoLoginTransactionServiceError::AlreadyProcessed
        | OrganizationSsoLoginTransactionServiceError::InvalidTransition => {
            OrganizationSsoSessionServiceError::Rejected
        }
        OrganizationSsoLoginTransactionServiceError::Unavailable => {
            OrganizationSsoSessionServiceError::Unavailable
        }
        OrganizationSsoLoginTransactionServiceError::Internal => {
            OrganizationSsoSessionServiceError::Internal
        }
    }
}

fn map_session_error(error: SessionAuthenticationError) -> OrganizationSsoSessionServiceError {
    match error {
        SessionAuthenticationError::InvalidCredentials
        | SessionAuthenticationError::TwoFactorRequired
        | SessionAuthenticationError::TwoFactorInvalid
        | SessionAuthenticationError::InvalidSession => {
            OrganizationSsoSessionServiceError::Rejected
        }
        SessionAuthenticationError::Internal => OrganizationSsoSessionServiceError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_errors_are_closed_without_sensitive_context() {
        for error in [
            OrganizationSsoLoginTransactionServiceError::Rejected,
            OrganizationSsoLoginTransactionServiceError::Expired,
            OrganizationSsoLoginTransactionServiceError::AlreadyProcessed,
            OrganizationSsoLoginTransactionServiceError::InvalidTransition,
        ] {
            assert_eq!(
                map_login_error(error),
                OrganizationSsoSessionServiceError::Rejected
            );
            assert!(!format!("{error:?}").contains("ticket"));
        }
    }

    #[test]
    fn session_errors_never_become_success() {
        assert_eq!(
            map_session_error(SessionAuthenticationError::InvalidSession),
            OrganizationSsoSessionServiceError::Rejected
        );
        assert_eq!(
            map_session_error(SessionAuthenticationError::Internal),
            OrganizationSsoSessionServiceError::Internal
        );
    }
}
