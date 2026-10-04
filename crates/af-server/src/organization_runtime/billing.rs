use std::sync::Arc;

use af_billing::{
    ContractPriceSource, ContractPriceSourceError, ContractPriceSourceFuture,
    ServiceAccountAuditError, ServiceAccountAuditFuture, ServiceAccountAuditSink,
};
use af_db::{
    OrganizationContractPriceRepository, OrganizationContractPriceRepositoryError,
    OrganizationServiceAccountAuditRepository, OrganizationServiceAccountAuditRepositoryError,
    OrganizationServiceAccountAuditWrite,
};
use af_domain::{
    BillingReservationId, OrganizationId, OrganizationServiceAccountRuntimeIdentity, Protocol,
};

/// 将企业合同价仓储接入公共计费快照接口。
#[derive(Clone)]
pub(super) struct DatabaseContractPriceSource {
    repository: Arc<OrganizationContractPriceRepository>,
}

impl DatabaseContractPriceSource {
    /// 使用已经完成配置校验的企业合同价仓储创建适配器。
    #[must_use]
    pub(super) fn new(repository: Arc<OrganizationContractPriceRepository>) -> Self {
        Self { repository }
    }
}

impl ContractPriceSource for DatabaseContractPriceSource {
    fn resolve<'a>(
        &'a self,
        organization_id: OrganizationId,
        model: &'a str,
        protocol: Protocol,
        request_time: af_db::DatabaseTimestamp,
    ) -> ContractPriceSourceFuture<'a> {
        Box::pin(async move {
            self.repository
                .resolve(organization_id, model, protocol, request_time)
                .await
                .map_err(map_contract_price_error)
        })
    }
}

/// 将企业服务账号审计仓储接入公共用量记录接口。
#[derive(Clone)]
pub(super) struct DatabaseServiceAccountAuditSink {
    repository: OrganizationServiceAccountAuditRepository,
}

impl DatabaseServiceAccountAuditSink {
    /// 使用已经完成数据库迁移的企业审计仓储创建适配器。
    #[must_use]
    pub(super) const fn new(repository: OrganizationServiceAccountAuditRepository) -> Self {
        Self { repository }
    }
}

impl ServiceAccountAuditSink for DatabaseServiceAccountAuditSink {
    fn record<'a>(
        &'a self,
        event_id: BillingReservationId,
        identity: OrganizationServiceAccountRuntimeIdentity,
    ) -> ServiceAccountAuditFuture<'a> {
        Box::pin(async move {
            let write = OrganizationServiceAccountAuditWrite::new(event_id, identity);
            self.repository
                .record(&write)
                .await
                .map(|_| ())
                .map_err(map_service_account_audit_error)
        })
    }
}

fn map_contract_price_error(
    error: OrganizationContractPriceRepositoryError,
) -> ContractPriceSourceError {
    match error {
        OrganizationContractPriceRepositoryError::InvalidInput => {
            ContractPriceSourceError::InvalidInput
        }
        OrganizationContractPriceRepositoryError::Timeout => ContractPriceSourceError::Timeout,
        OrganizationContractPriceRepositoryError::Query
        | OrganizationContractPriceRepositoryError::OrganizationUnavailable
        | OrganizationContractPriceRepositoryError::CreatorUnavailable
        | OrganizationContractPriceRepositoryError::NotFound => {
            ContractPriceSourceError::Unavailable
        }
        OrganizationContractPriceRepositoryError::IntervalConflict
        | OrganizationContractPriceRepositoryError::Conflict
        | OrganizationContractPriceRepositoryError::InvalidTransition
        | OrganizationContractPriceRepositoryError::Invariant => {
            ContractPriceSourceError::Invariant
        }
    }
}

fn map_service_account_audit_error(
    error: OrganizationServiceAccountAuditRepositoryError,
) -> ServiceAccountAuditError {
    match error {
        OrganizationServiceAccountAuditRepositoryError::Conflict => {
            ServiceAccountAuditError::Conflict
        }
        OrganizationServiceAccountAuditRepositoryError::InvalidInput
        | OrganizationServiceAccountAuditRepositoryError::Invariant => {
            ServiceAccountAuditError::Invariant
        }
        OrganizationServiceAccountAuditRepositoryError::UsageNotFound
        | OrganizationServiceAccountAuditRepositoryError::AccountNotFound
        | OrganizationServiceAccountAuditRepositoryError::CredentialNotFound
        | OrganizationServiceAccountAuditRepositoryError::Query
        | OrganizationServiceAccountAuditRepositoryError::OutcomeUnknown
        | OrganizationServiceAccountAuditRepositoryError::Timeout => {
            ServiceAccountAuditError::Unavailable
        }
    }
}
