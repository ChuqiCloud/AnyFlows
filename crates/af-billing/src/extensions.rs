use std::{future::Future, pin::Pin};

use af_db::DatabaseTimestamp;
use af_domain::{
    BillingContractPriceSnapshot, BillingReservationId, OrganizationId,
    OrganizationServiceAccountRuntimeIdentity, Protocol,
};
use thiserror::Error;

/// 解析一次请求使用的不可变合同价；具体仓储由发行版装配层提供。
pub trait ContractPriceSource: Send + Sync + 'static {
    fn resolve<'a>(
        &'a self,
        organization_id: OrganizationId,
        model: &'a str,
        protocol: Protocol,
        request_time: DatabaseTimestamp,
    ) -> ContractPriceSourceFuture<'a>;
}

pub type ContractPriceSourceFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<BillingContractPriceSnapshot>, ContractPriceSourceError>>
            + Send
            + 'a,
    >,
>;

/// 合同价解析错误，不携带仓储细节、模型名或价格。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ContractPriceSourceError {
    #[error("合同价输入无效")]
    InvalidInput,
    #[error("合同价目录不可用")]
    Unavailable,
    #[error("合同价目录操作超时")]
    Timeout,
    #[error("合同价目录状态无效")]
    Invariant,
}

/// 在公共用量事实落库后，幂等投影服务账号调用身份到企业审计存储。
pub trait ServiceAccountAuditSink: Send + Sync + 'static {
    fn record<'a>(
        &'a self,
        event_id: BillingReservationId,
        identity: OrganizationServiceAccountRuntimeIdentity,
    ) -> ServiceAccountAuditFuture<'a>;
}

pub type ServiceAccountAuditFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), ServiceAccountAuditError>> + Send + 'a>>;

/// 服务账号审计错误；分类用于保留现有队列重试语义。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ServiceAccountAuditError {
    #[error("服务账号审计存储不可用")]
    Unavailable,
    #[error("服务账号审计记录冲突")]
    Conflict,
    #[error("服务账号审计状态无效")]
    Invariant,
}
