use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationContractPriceRecord, OrganizationContractPriceRepository,
    OrganizationContractPriceRepositoryError, OrganizationContractPriceWrite,
};
use af_domain::{OrganizationContractPriceId, OrganizationPermission, Protocol};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::OrganizationContext;

/// 创建企业合同价版本的管理命令；时间使用 Unix 秒。
pub struct OrganizationContractPriceCreateCommand {
    pub(crate) model: String,
    pub(crate) protocol: Protocol,
    pub(crate) prices: [Decimal; 5],
    pub(crate) effective_from: i64,
    pub(crate) effective_until: Option<i64>,
    pub(crate) expected_version: Option<i64>,
}

impl OrganizationContractPriceCreateCommand {
    /// 构造合同价创建命令并校验时间与 CAS 输入。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: String,
        protocol: Protocol,
        prices: [Decimal; 5],
        effective_from: i64,
        effective_until: Option<i64>,
        expected_version: Option<i64>,
    ) -> Result<Self, OrganizationContractPriceServiceError> {
        if effective_from < 0
            || effective_until.is_some_and(|until| until <= effective_from)
            || expected_version.is_some_and(|version| version < 0)
        {
            return Err(OrganizationContractPriceServiceError::InvalidRequest);
        }
        Ok(Self {
            model,
            protocol,
            prices,
            effective_from,
            effective_until,
            expected_version,
        })
    }
}

/// 关闭合同价区间的管理命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationContractPriceCloseCommand {
    pub(crate) price_id: OrganizationContractPriceId,
    pub(crate) effective_until: i64,
    pub(crate) expected_version: i64,
}

impl OrganizationContractPriceCloseCommand {
    /// 构造关闭命令并校验版本和时间戳。
    pub fn new(
        price_id: OrganizationContractPriceId,
        effective_until: i64,
        expected_version: i64,
    ) -> Result<Self, OrganizationContractPriceServiceError> {
        if effective_until < 0 || expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationContractPriceServiceError::InvalidRequest);
        }
        Ok(Self {
            price_id,
            effective_until,
            expected_version,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationContractPriceServiceError {
    #[error("企业合同价请求无效")]
    InvalidRequest,
    #[error("企业合同价权限不足")]
    Forbidden,
    #[error("企业合同价不存在")]
    NotFound,
    #[error("企业合同价有效区间冲突")]
    IntervalConflict,
    #[error("企业合同价版本冲突")]
    Conflict,
    #[error("企业合同价状态转换无效")]
    InvalidTransition,
    #[error("企业合同价内部错误")]
    Internal,
}

pub type OrganizationContractPriceListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationContractPriceRecord>,
                    OrganizationContractPriceServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationContractPriceFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationContractPriceRecord,
                    OrganizationContractPriceServiceError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 企业合同价管理端口；权限和企业上下文由服务层再次校验。
pub trait OrganizationContractPriceService: Send + Sync {
    fn list<'a>(&'a self, context: OrganizationContext) -> OrganizationContractPriceListFuture<'a>;
    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationContractPriceId,
    ) -> OrganizationContractPriceFuture<'a>;
    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationContractPriceCreateCommand,
    ) -> OrganizationContractPriceFuture<'a>;
    fn close<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationContractPriceCloseCommand,
    ) -> OrganizationContractPriceFuture<'a>;
}

/// 使用数据库仓储实现企业合同价管理。
pub struct DatabaseOrganizationContractPriceService {
    repository: Arc<OrganizationContractPriceRepository>,
}

impl DatabaseOrganizationContractPriceService {
    /// 绑定启动期创建的合同价仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationContractPriceRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationContractPriceService for DatabaseOrganizationContractPriceService {
    fn list<'a>(&'a self, context: OrganizationContext) -> OrganizationContractPriceListFuture<'a> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .list(context.organization_id())
                .await
                .map_err(map_error)
        })
    }

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        id: OrganizationContractPriceId,
    ) -> OrganizationContractPriceFuture<'a> {
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
        command: OrganizationContractPriceCreateCommand,
    ) -> OrganizationContractPriceFuture<'a> {
        Box::pin(async move {
            require_manage(&context)?;
            let effective_from =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationContractPriceServiceError::InvalidRequest)?;
            let effective_until = command
                .effective_until
                .map(af_db::DatabaseTimestamp::from_unix_timestamp)
                .transpose()
                .map_err(|_| OrganizationContractPriceServiceError::InvalidRequest)?;
            let write = OrganizationContractPriceWrite::new(
                context.organization_id(),
                command.model,
                command.protocol,
                command.prices,
                effective_from,
                effective_until,
                command.expected_version,
                context.user_id(),
            );
            self.repository.create(write).await.map_err(map_error)
        })
    }

    fn close<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationContractPriceCloseCommand,
    ) -> OrganizationContractPriceFuture<'a> {
        Box::pin(async move {
            require_manage(&context)?;
            let effective_until =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_until)
                    .map_err(|_| OrganizationContractPriceServiceError::InvalidRequest)?;
            self.repository
                .close(
                    context.organization_id(),
                    command.price_id,
                    effective_until,
                    command.expected_version,
                )
                .await
                .map_err(map_error)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationContractPriceService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationContractPriceService(<repository>)")
    }
}

fn require_read(
    context: &OrganizationContext,
) -> Result<(), OrganizationContractPriceServiceError> {
    context
        .allows(OrganizationPermission::ContractPricesRead)
        .then_some(())
        .ok_or(OrganizationContractPriceServiceError::Forbidden)
}

fn require_manage(
    context: &OrganizationContext,
) -> Result<(), OrganizationContractPriceServiceError> {
    context
        .allows(OrganizationPermission::ContractPricesManage)
        .then_some(())
        .ok_or(OrganizationContractPriceServiceError::Forbidden)
}

fn map_error(
    error: OrganizationContractPriceRepositoryError,
) -> OrganizationContractPriceServiceError {
    match error {
        OrganizationContractPriceRepositoryError::InvalidInput => {
            OrganizationContractPriceServiceError::InvalidRequest
        }
        OrganizationContractPriceRepositoryError::NotFound
        | OrganizationContractPriceRepositoryError::OrganizationUnavailable => {
            OrganizationContractPriceServiceError::NotFound
        }
        OrganizationContractPriceRepositoryError::CreatorUnavailable => {
            OrganizationContractPriceServiceError::Forbidden
        }
        OrganizationContractPriceRepositoryError::IntervalConflict => {
            OrganizationContractPriceServiceError::IntervalConflict
        }
        OrganizationContractPriceRepositoryError::Conflict => {
            OrganizationContractPriceServiceError::Conflict
        }
        OrganizationContractPriceRepositoryError::InvalidTransition => {
            OrganizationContractPriceServiceError::InvalidTransition
        }
        OrganizationContractPriceRepositoryError::Query
        | OrganizationContractPriceRepositoryError::Timeout
        | OrganizationContractPriceRepositoryError::Invariant => {
            OrganizationContractPriceServiceError::Internal
        }
    }
}
