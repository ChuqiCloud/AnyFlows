use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationBillingReservationRecord, OrganizationBudgetPolicyRecord,
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepository, OrganizationBudgetRepositoryError,
    OrganizationBudgetScope, OrganizationBudgetWindowRecord,
};
use af_domain::{
    BillingReservationId, OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationPermission,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 创建预算策略时使用的管理端命令；生效时间使用 Unix 秒，避免 HTTP 层泄漏 ORM 时间类型。
pub struct OrganizationBudgetPolicyCreateCommand {
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
}

impl OrganizationBudgetPolicyCreateCommand {
    pub fn new(
        scope: OrganizationBudgetScope,
        period: OrganizationBudgetPeriod,
        limit: OrganizationBudgetLimit,
        effective_from: i64,
    ) -> Result<Self, OrganizationBudgetServiceError> {
        if effective_from < 0 {
            return Err(OrganizationBudgetServiceError::InvalidRequest);
        }
        Ok(Self {
            scope,
            period,
            limit,
            effective_from,
        })
    }
}

pub type OrganizationBudgetPoliciesFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationBudgetPolicyRecord>,
                    OrganizationBudgetServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationBudgetWindowsFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationBudgetWindowRecord>,
                    OrganizationBudgetServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationBudgetReservationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationBillingReservationRecord,
                    OrganizationBudgetServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationBudgetPolicyFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationBudgetPolicyRecord, OrganizationBudgetServiceError>>
            + Send
            + 'a,
    >,
>;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationBudgetServiceError {
    #[error("企业预算请求无效")]
    InvalidRequest,
    #[error("企业预算权限不足")]
    Forbidden,
    #[error("企业预算资源不存在")]
    NotFound,
    #[error("企业预算版本冲突")]
    Conflict,
    #[error("企业预算写入结果未知")]
    OutcomeUnknown,
    #[error("企业预算内部错误")]
    Internal,
}

pub trait OrganizationBudgetService: Send + Sync {
    fn list_policies<'a>(
        &'a self,
        context: OrganizationContext,
    ) -> OrganizationBudgetPoliciesFuture<'a>;

    fn create_policy<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationBudgetPolicyCreateCommand,
    ) -> OrganizationBudgetPolicyFuture<'a>;

    fn list_windows<'a>(
        &'a self,
        context: OrganizationContext,
    ) -> OrganizationBudgetWindowsFuture<'a>;

    fn reservation<'a>(
        &'a self,
        context: OrganizationContext,
        id: BillingReservationId,
    ) -> OrganizationBudgetReservationFuture<'a>;
}

pub struct DatabaseOrganizationBudgetService {
    repository: Arc<OrganizationBudgetRepository>,
}

impl DatabaseOrganizationBudgetService {
    pub fn new(repository: Arc<OrganizationBudgetRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationBudgetService for DatabaseOrganizationBudgetService {
    fn list_policies<'a>(
        &'a self,
        context: OrganizationContext,
    ) -> OrganizationBudgetPoliciesFuture<'a> {
        Box::pin(async move {
            require_permission(&context)?;
            self.repository
                .list_policies(context.organization_id(), context.user_id())
                .await
                .map_err(map_repository_error)
        })
    }

    fn create_policy<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationBudgetPolicyCreateCommand,
    ) -> OrganizationBudgetPolicyFuture<'a> {
        Box::pin(async move {
            require_permission(&context)?;
            let effective_from =
                crate::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?;
            let write = OrganizationBudgetPolicyWrite::new(
                context.organization_id(),
                command.scope,
                command.period,
                command.limit,
                effective_from,
                context.user_id(),
            );
            self.repository
                .create_policy(write)
                .await
                .map_err(map_repository_error)
        })
    }

    fn list_windows<'a>(
        &'a self,
        context: OrganizationContext,
    ) -> OrganizationBudgetWindowsFuture<'a> {
        Box::pin(async move {
            require_permission(&context)?;
            self.repository
                .list_windows(context.organization_id(), context.user_id())
                .await
                .map_err(map_repository_error)
        })
    }

    fn reservation<'a>(
        &'a self,
        context: OrganizationContext,
        id: BillingReservationId,
    ) -> OrganizationBudgetReservationFuture<'a> {
        Box::pin(async move {
            require_permission(&context)?;
            let reservation = self
                .repository
                .reservation(id)
                .await
                .map_err(map_repository_error)?;
            // 预留键是全局幂等键，不能因为调用者有预算权限就跨企业读取。
            if reservation.principal().organization_id() != context.organization_id() {
                return Err(OrganizationBudgetServiceError::NotFound);
            }
            Ok(reservation)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationBudgetService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationBudgetService(<repository>)")
    }
}

fn require_permission(context: &OrganizationContext) -> Result<(), OrganizationBudgetServiceError> {
    context
        .allows(OrganizationPermission::BudgetsManage)
        .then_some(())
        .ok_or(OrganizationBudgetServiceError::Forbidden)
}

fn map_repository_error(
    error: OrganizationBudgetRepositoryError,
) -> OrganizationBudgetServiceError {
    match error {
        OrganizationBudgetRepositoryError::InvalidInput => {
            OrganizationBudgetServiceError::InvalidRequest
        }
        OrganizationBudgetRepositoryError::NotFound => OrganizationBudgetServiceError::NotFound,
        OrganizationBudgetRepositoryError::PermissionDenied => {
            OrganizationBudgetServiceError::Forbidden
        }
        OrganizationBudgetRepositoryError::Conflict => OrganizationBudgetServiceError::Conflict,
        OrganizationBudgetRepositoryError::OutcomeUnknown => {
            OrganizationBudgetServiceError::OutcomeUnknown
        }
        OrganizationBudgetRepositoryError::WalletQuotaInsufficient
        | OrganizationBudgetRepositoryError::BudgetQuotaInsufficient
        | OrganizationBudgetRepositoryError::BillingProtected
        | OrganizationBudgetRepositoryError::SettlementPending
        | OrganizationBudgetRepositoryError::Timeout
        | OrganizationBudgetRepositoryError::Query
        | OrganizationBudgetRepositoryError::Invariant => OrganizationBudgetServiceError::Internal,
    }
}
