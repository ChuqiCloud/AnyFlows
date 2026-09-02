use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationCreditInvoicePageRecord, OrganizationCreditInvoiceRecord,
    OrganizationCreditInvoiceRepository, OrganizationCreditInvoiceRepositoryError,
    OrganizationCreditRepaymentAllocationPageRecord, OrganizationCreditRepaymentAllocationRecord,
    OrganizationCreditRepaymentPageRecord, OrganizationCreditRepaymentRecord,
    OrganizationCreditRepaymentRepository, OrganizationCreditRepaymentRepositoryError,
    OrganizationCreditTermsPageRecord, OrganizationCreditTermsRecord,
    OrganizationCreditTermsRepository, OrganizationCreditTermsRepositoryError,
    OrganizationCreditTermsWrite,
};
use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditInvoiceId, OrganizationCreditLimit,
    OrganizationCreditRepaymentAllocationId, OrganizationCreditRepaymentId,
    OrganizationCreditTermId, OrganizationCreditTermPeriod, OrganizationCreditTermStatus,
    OrganizationPermission,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 创建企业授信策略的管理命令；时间使用 Unix 秒。
pub struct OrganizationCreditTermsCreateCommand {
    pub(crate) currency: OrganizationCreditCurrency,
    pub(crate) credit_limit: OrganizationCreditLimit,
    pub(crate) period: OrganizationCreditTermPeriod,
    pub(crate) effective_from: i64,
    pub(crate) effective_until: Option<i64>,
    pub(crate) expected_version: Option<i64>,
}

impl OrganizationCreditTermsCreateCommand {
    /// 校验时间和版本 CAS 输入。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        currency: OrganizationCreditCurrency,
        credit_limit: OrganizationCreditLimit,
        period: OrganizationCreditTermPeriod,
        effective_from: i64,
        effective_until: Option<i64>,
        expected_version: Option<i64>,
    ) -> Result<Self, OrganizationCreditTermsServiceError> {
        if effective_from < 0
            || effective_until.is_some_and(|until| until <= effective_from)
            || expected_version.is_some_and(|version| version < 0)
        {
            return Err(OrganizationCreditTermsServiceError::InvalidRequest);
        }
        Ok(Self {
            currency,
            credit_limit,
            period,
            effective_from,
            effective_until,
            expected_version,
        })
    }
}

/// 推进企业授信策略状态的管理命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationCreditTermsStatusCommand {
    pub(crate) term_id: OrganizationCreditTermId,
    pub(crate) status: OrganizationCreditTermStatus,
    pub(crate) expected_version: i64,
}

impl OrganizationCreditTermsStatusCommand {
    /// 校验策略标识和版本。
    pub fn new(
        term_id: OrganizationCreditTermId,
        status: OrganizationCreditTermStatus,
        expected_version: i64,
    ) -> Result<Self, OrganizationCreditTermsServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationCreditTermsServiceError::InvalidRequest);
        }
        Ok(Self {
            term_id,
            status,
            expected_version,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationCreditTermsServiceError {
    #[error("企业授信请求无效")]
    InvalidRequest,
    #[error("企业授信权限不足")]
    Forbidden,
    #[error("企业授信策略不存在")]
    NotFound,
    #[error("企业授信有效区间冲突")]
    IntervalConflict,
    #[error("企业授信版本冲突")]
    Conflict,
    #[error("企业授信状态转换无效")]
    InvalidTransition,
    #[error("企业授信内部错误")]
    Internal,
}

pub type OrganizationCreditTermsListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationCreditTermsRecord>,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditTermsPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditTermsPageRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditInvoicePageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditInvoicePageRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditRepaymentPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditRepaymentPageRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditRepaymentAllocationPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditRepaymentAllocationPageRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditInvoiceFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditInvoiceRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditRepaymentFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditRepaymentRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditRepaymentAllocationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationCreditRepaymentAllocationRecord,
                    OrganizationCreditTermsServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationCreditTermsFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationCreditTermsRecord, OrganizationCreditTermsServiceError>,
            > + Send
            + 'a,
    >,
>;

/// 企业授信管理服务；权限和企业边界由服务层再次校验。
pub trait OrganizationCreditTermsService: Send + Sync {
    fn list<'a>(&'a self, context: OrganizationContext) -> OrganizationCreditTermsListFuture<'a>;
    fn list_page<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditTermsPageFuture<'a>;
    fn list_invoices<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditInvoicePageFuture<'a>;
    fn get_invoice<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditInvoiceId,
    ) -> OrganizationCreditInvoiceFuture<'a>;
    fn list_repayments<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditRepaymentPageFuture<'a>;
    fn get_repayment<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditRepaymentId,
    ) -> OrganizationCreditRepaymentFuture<'a>;
    fn list_allocations<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditRepaymentAllocationPageFuture<'a>;
    fn get_allocation<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditRepaymentAllocationId,
    ) -> OrganizationCreditRepaymentAllocationFuture<'a>;
    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditTermId,
    ) -> OrganizationCreditTermsFuture<'a>;
    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCreditTermsCreateCommand,
    ) -> OrganizationCreditTermsFuture<'a>;
    fn update_status<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCreditTermsStatusCommand,
    ) -> OrganizationCreditTermsFuture<'a>;
}

/// 使用数据库仓储实现企业授信管理。
pub struct DatabaseOrganizationCreditTermsService {
    repository: Arc<OrganizationCreditTermsRepository>,
    invoice_repository: Option<Arc<OrganizationCreditInvoiceRepository>>,
    repayment_repository: Option<Arc<OrganizationCreditRepaymentRepository>>,
}

impl DatabaseOrganizationCreditTermsService {
    /// 绑定启动期创建的企业授信仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationCreditTermsRepository>) -> Self {
        Self {
            repository,
            invoice_repository: None,
            repayment_repository: None,
        }
    }

    /// 绑定账单、还款和核销只读仓储。
    #[must_use]
    pub fn with_ledger_repositories(
        mut self,
        invoice_repository: Arc<OrganizationCreditInvoiceRepository>,
        repayment_repository: Arc<OrganizationCreditRepaymentRepository>,
    ) -> Self {
        self.invoice_repository = Some(invoice_repository);
        self.repayment_repository = Some(repayment_repository);
        self
    }
}

impl OrganizationCreditTermsService for DatabaseOrganizationCreditTermsService {
    fn list<'a>(&'a self, context: OrganizationContext) -> OrganizationCreditTermsListFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .list(context.organization_id())
                .await
                .map_err(map_error)
        })
    }

    fn list_page<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditTermsPageFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .list_page(context.organization_id(), before, limit)
                .await
                .map_err(map_error)
        })
    }

    fn list_invoices<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditInvoicePageFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.invoice_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .list(context.organization_id(), before, limit)
                .await
                .map_err(map_invoice_error)
        })
    }

    fn get_invoice<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditInvoiceId,
    ) -> OrganizationCreditInvoiceFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.invoice_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .get(context.organization_id(), id)
                .await
                .map_err(map_invoice_error)
        })
    }

    fn list_repayments<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditRepaymentPageFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repayment_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .list(context.organization_id(), before, limit)
                .await
                .map_err(map_repayment_error)
        })
    }

    fn get_repayment<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditRepaymentId,
    ) -> OrganizationCreditRepaymentFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repayment_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .get(context.organization_id(), id)
                .await
                .map_err(map_repayment_error)
        })
    }

    fn list_allocations<'a>(
        &'a self,
        context: OrganizationContext,
        before: Option<i64>,
        limit: usize,
    ) -> OrganizationCreditRepaymentAllocationPageFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repayment_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .list_allocations(context.organization_id(), before, limit)
                .await
                .map_err(map_repayment_error)
        })
    }

    fn get_allocation<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditRepaymentAllocationId,
    ) -> OrganizationCreditRepaymentAllocationFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repayment_repository
                .as_ref()
                .ok_or(OrganizationCreditTermsServiceError::Internal)?
                .get_allocation(context.organization_id(), id)
                .await
                .map_err(map_repayment_error)
        })
    }

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationCreditTermId,
    ) -> OrganizationCreditTermsFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .get(context.organization_id(), id)
                .await
                .map_err(map_error)
        })
    }

    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCreditTermsCreateCommand,
    ) -> OrganizationCreditTermsFuture<'a> {
        Box::pin(async move {
            require_manage(&context)?;
            let effective_from =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationCreditTermsServiceError::InvalidRequest)?;
            let effective_until = command
                .effective_until
                .map(af_db::DatabaseTimestamp::from_unix_timestamp)
                .transpose()
                .map_err(|_| OrganizationCreditTermsServiceError::InvalidRequest)?;
            let write = OrganizationCreditTermsWrite::new(
                context.organization_id(),
                command.currency,
                command.credit_limit,
                command.period,
                effective_from,
                effective_until,
                command.expected_version,
                context.user_id(),
            );
            self.repository.create(write).await.map_err(map_error)
        })
    }

    fn update_status<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationCreditTermsStatusCommand,
    ) -> OrganizationCreditTermsFuture<'a> {
        Box::pin(async move {
            require_manage(&context)?;
            self.repository
                .update_status(
                    context.organization_id(),
                    command.term_id,
                    command.status,
                    command.expected_version,
                )
                .await
                .map_err(map_error)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationCreditTermsService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationCreditTermsService(<repository>)")
    }
}

fn require_read(context: &OrganizationContext) -> Result<(), OrganizationCreditTermsServiceError> {
    context
        .allows(OrganizationPermission::CreditRead)
        .then_some(())
        .ok_or(OrganizationCreditTermsServiceError::Forbidden)
}

fn require_manage(
    context: &OrganizationContext,
) -> Result<(), OrganizationCreditTermsServiceError> {
    context
        .allows(OrganizationPermission::CreditManage)
        .then_some(())
        .ok_or(OrganizationCreditTermsServiceError::Forbidden)
}

fn map_error(error: OrganizationCreditTermsRepositoryError) -> OrganizationCreditTermsServiceError {
    match error {
        OrganizationCreditTermsRepositoryError::InvalidInput => {
            OrganizationCreditTermsServiceError::InvalidRequest
        }
        OrganizationCreditTermsRepositoryError::NotFound
        | OrganizationCreditTermsRepositoryError::OrganizationUnavailable
        | OrganizationCreditTermsRepositoryError::CreatorUnavailable => {
            OrganizationCreditTermsServiceError::NotFound
        }
        OrganizationCreditTermsRepositoryError::IntervalConflict => {
            OrganizationCreditTermsServiceError::IntervalConflict
        }
        OrganizationCreditTermsRepositoryError::Conflict => {
            OrganizationCreditTermsServiceError::Conflict
        }
        OrganizationCreditTermsRepositoryError::InvalidTransition => {
            OrganizationCreditTermsServiceError::InvalidTransition
        }
        OrganizationCreditTermsRepositoryError::Query
        | OrganizationCreditTermsRepositoryError::Timeout
        | OrganizationCreditTermsRepositoryError::Invariant => {
            OrganizationCreditTermsServiceError::Internal
        }
    }
}

fn map_invoice_error(
    error: OrganizationCreditInvoiceRepositoryError,
) -> OrganizationCreditTermsServiceError {
    match error {
        OrganizationCreditInvoiceRepositoryError::InvalidInput => {
            OrganizationCreditTermsServiceError::InvalidRequest
        }
        OrganizationCreditInvoiceRepositoryError::NotFound
        | OrganizationCreditInvoiceRepositoryError::OrganizationUnavailable => {
            OrganizationCreditTermsServiceError::NotFound
        }
        OrganizationCreditInvoiceRepositoryError::Query
        | OrganizationCreditInvoiceRepositoryError::Timeout
        | OrganizationCreditInvoiceRepositoryError::Invariant => {
            OrganizationCreditTermsServiceError::Internal
        }
    }
}

fn map_repayment_error(
    error: OrganizationCreditRepaymentRepositoryError,
) -> OrganizationCreditTermsServiceError {
    match error {
        OrganizationCreditRepaymentRepositoryError::InvalidInput => {
            OrganizationCreditTermsServiceError::InvalidRequest
        }
        OrganizationCreditRepaymentRepositoryError::NotFound => {
            OrganizationCreditTermsServiceError::NotFound
        }
        OrganizationCreditRepaymentRepositoryError::Query
        | OrganizationCreditRepaymentRepositoryError::Timeout
        | OrganizationCreditRepaymentRepositoryError::Invariant => {
            OrganizationCreditTermsServiceError::Internal
        }
        OrganizationCreditRepaymentRepositoryError::Conflict
        | OrganizationCreditRepaymentRepositoryError::CurrencyMismatch
        | OrganizationCreditRepaymentRepositoryError::InvalidInvoiceState
        | OrganizationCreditRepaymentRepositoryError::RepaymentOverallocated
        | OrganizationCreditRepaymentRepositoryError::InvoiceOverallocated => {
            OrganizationCreditTermsServiceError::Internal
        }
    }
}
