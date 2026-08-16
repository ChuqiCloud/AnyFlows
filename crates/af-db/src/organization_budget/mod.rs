use std::{fmt, time::Duration};

use af_domain::{BillingReservationId, OrganizationId, Quota, UserId};
use sea_orm::{TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

mod billing;
mod policy;
mod types;
mod window;

pub use types::{
    OrganizationBillingMutationOutcome, OrganizationBillingPrincipal,
    OrganizationBillingReservationRecord, OrganizationBillingReservationStatus,
    OrganizationBudgetAllocationRecord, OrganizationBudgetPolicyRecord,
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepositoryConfigError,
    OrganizationBudgetRepositoryError, OrganizationBudgetScope, OrganizationBudgetWindowRecord,
};

/// 企业预算策略与钱包预留的统一持久化边界。
#[derive(Clone)]
pub struct OrganizationBudgetRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    reservation_ttl: Duration,
}

impl OrganizationBudgetRepository {
    /// 构造企业预算仓储；所有写操作共用明确截止时间和预留期限。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
        reservation_ttl: Duration,
    ) -> Result<Self, OrganizationBudgetRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationBudgetRepositoryConfigError::ZeroOperationTimeout);
        }
        if reservation_ttl.is_zero() {
            return Err(OrganizationBudgetRepositoryConfigError::ZeroReservationTtl);
        }
        Ok(Self {
            pool,
            operation_timeout,
            reservation_ttl,
        })
    }

    /// 创建一个版本化预算策略，并在同一企业锁内关闭前一策略的未来生效段。
    pub async fn create_policy(
        &self,
        write: OrganizationBudgetPolicyWrite,
    ) -> Result<OrganizationBudgetPolicyRecord, OrganizationBudgetRepositoryError> {
        let operation = self
            .create_policy_inner(write)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?
    }

    /// 列出企业全部历史与当前预算策略版本。
    pub async fn list_policies(
        &self,
        organization_id: OrganizationId,
        actor_user_id: UserId,
    ) -> Result<Vec<OrganizationBudgetPolicyRecord>, OrganizationBudgetRepositoryError> {
        let operation = self
            .list_policies_inner(organization_id, actor_user_id)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Timeout)?
    }

    /// 读取当前仍有效的预算窗口快照，历史窗口不会无界返回。
    pub async fn list_windows(
        &self,
        organization_id: OrganizationId,
        actor_user_id: UserId,
    ) -> Result<Vec<OrganizationBudgetWindowRecord>, OrganizationBudgetRepositoryError> {
        let operation = self
            .list_windows_inner(organization_id, actor_user_id)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Timeout)?
    }

    /// 同时冻结企业钱包及所有适用预算窗口；相同事实可安全重放。
    pub async fn reserve(
        &self,
        id: BillingReservationId,
        principal: OrganizationBillingPrincipal,
        amount: Quota,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        let operation = self
            .reserve_inner(id, principal, amount)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?
    }

    /// 使用同一预留标识结算实际额度；补扣不足会固化待结算事实并保护钱包。
    pub async fn settle(
        &self,
        id: BillingReservationId,
        actual: Quota,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        let operation = self
            .settle_inner(id, actual)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?
    }

    /// 释放仍处于预留态的钱包与预算冻结额度；终态释放可安全重放。
    pub async fn release(
        &self,
        id: BillingReservationId,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        let operation = self
            .release_inner(id)
            .with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?
    }

    /// 读取企业预留的安全快照，供结果未知恢复使用。
    pub async fn reservation(
        &self,
        id: BillingReservationId,
    ) -> Result<OrganizationBillingReservationRecord, OrganizationBudgetRepositoryError> {
        let operation =
            billing::load(self.pool.connection(), id).with_subscriber(NoSubscriber::default());
        timeout(self.operation_timeout, operation)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Timeout)?
            .and_then(|record| record.ok_or(OrganizationBudgetRepositoryError::NotFound))
    }

    async fn create_policy_inner(
        &self,
        write: OrganizationBudgetPolicyWrite,
    ) -> Result<OrganizationBudgetPolicyRecord, OrganizationBudgetRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        match policy::create(&transaction, &write).await {
            Ok(record) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?;
                Ok(record)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn list_policies_inner(
        &self,
        organization_id: OrganizationId,
        actor_user_id: UserId,
    ) -> Result<Vec<OrganizationBudgetPolicyRecord>, OrganizationBudgetRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        let result = policy::list(&transaction, organization_id, actor_user_id).await;
        let _ = transaction.rollback().await;
        result
    }

    async fn list_windows_inner(
        &self,
        organization_id: OrganizationId,
        actor_user_id: UserId,
    ) -> Result<Vec<OrganizationBudgetWindowRecord>, OrganizationBudgetRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        let result = window::list_current(
            &transaction,
            organization_id,
            actor_user_id,
            TimeDateTimeWithTimeZone::now_utc(),
        )
        .await;
        let _ = transaction.rollback().await;
        result
    }

    async fn reserve_inner(
        &self,
        id: BillingReservationId,
        principal: OrganizationBillingPrincipal,
        amount: Quota,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        if let Some(existing) = billing::load(self.pool.connection(), id).await? {
            return existing_reserve(existing, principal, amount);
        }
        let expires_at = reservation_expiration(self.reservation_ttl)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        match billing::reserve(&transaction, id, principal, amount, expires_at).await {
            Ok(outcome) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?;
                Ok(outcome)
            }
            Err(OrganizationBudgetRepositoryError::Conflict) => {
                let _ = transaction.rollback().await;
                let existing = billing::load(self.pool.connection(), id)
                    .await?
                    .ok_or(OrganizationBudgetRepositoryError::Conflict)?;
                existing_reserve(existing, principal, amount)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn settle_inner(
        &self,
        id: BillingReservationId,
        actual: Quota,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        let observed = billing::load(self.pool.connection(), id)
            .await?
            .ok_or(OrganizationBudgetRepositoryError::NotFound)?;
        if observed.status() == OrganizationBillingReservationStatus::Settled {
            return (observed.actual_quota() == Some(actual))
                .then_some(OrganizationBillingMutationOutcome::Existing(
                    OrganizationBillingReservationStatus::Settled,
                ))
                .ok_or(OrganizationBudgetRepositoryError::Conflict);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        match billing::settle(&transaction, &observed, actual).await {
            Ok(outcome) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?;
                Ok(outcome)
            }
            Err(
                OrganizationBudgetRepositoryError::SettlementPending
                | OrganizationBudgetRepositoryError::BudgetQuotaInsufficient,
            ) => {
                let _ = transaction.rollback().await;
                self.persist_pending(&observed, actual).await?;
                Err(OrganizationBudgetRepositoryError::SettlementPending)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn persist_pending(
        &self,
        observed: &OrganizationBillingReservationRecord,
        actual: Quota,
    ) -> Result<(), OrganizationBudgetRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        match billing::mark_pending_and_protect(&transaction, observed, actual).await {
            Ok(()) => transaction
                .commit()
                .await
                .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown),
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn release_inner(
        &self,
        id: BillingReservationId,
    ) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
        let observed = billing::load(self.pool.connection(), id)
            .await?
            .ok_or(OrganizationBudgetRepositoryError::NotFound)?;
        if observed.status() == OrganizationBillingReservationStatus::Released {
            return Ok(OrganizationBillingMutationOutcome::Existing(
                OrganizationBillingReservationStatus::Released,
            ));
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        match billing::release(&transaction, &observed).await {
            Ok(outcome) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationBudgetRepositoryError::OutcomeUnknown)?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for OrganizationBudgetRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationBudgetRepository")
            .field("operation_timeout", &self.operation_timeout)
            .field("reservation_ttl", &self.reservation_ttl)
            .finish_non_exhaustive()
    }
}

fn existing_reserve(
    existing: OrganizationBillingReservationRecord,
    principal: OrganizationBillingPrincipal,
    amount: Quota,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    if existing.principal != principal || existing.reserved_quota() != amount {
        return Err(OrganizationBudgetRepositoryError::Conflict);
    }
    Ok(OrganizationBillingMutationOutcome::Existing(
        existing.status(),
    ))
}

fn reservation_expiration(
    ttl: Duration,
) -> Result<TimeDateTimeWithTimeZone, OrganizationBudgetRepositoryError> {
    let ttl_nanos = i128::try_from(ttl.as_nanos())
        .map_err(|_| OrganizationBudgetRepositoryError::InvalidInput)?;
    TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp_nanos()
        .checked_add(ttl_nanos)
        .and_then(|timestamp| TimeDateTimeWithTimeZone::from_unix_timestamp_nanos(timestamp).ok())
        .ok_or(OrganizationBudgetRepositoryError::InvalidInput)
}
