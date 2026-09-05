use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{
    OrganizationId, OrganizationMembershipStatus, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderRequestId, OrganizationPlanOrderStatus, OrganizationPlanPaymentEventId,
    OrganizationPlanPaymentEventType, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Condition, Expr, LockType},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_memberships, organization_plan_orders,
        organization_plan_payment_events, organization_plan_prices, organization_plans,
        organizations, users,
    },
    organization::{mutation as organization_mutation, read as organization_read},
    organization_audit::{
        OrganizationAuditOutcome, OrganizationAuditWrite, record_in_transaction as record_audit,
    },
};

use super::types::{
    OrganizationPlanOrderCreate, OrganizationPlanOrderCreateOutcome, OrganizationPlanOrderRecord,
    OrganizationPlanOrderRepositoryConfigError, OrganizationPlanOrderRepositoryError,
    OrganizationPlanOrderSubmission, OrganizationPlanOrderSubmitOutcome,
    OrganizationPlanPaymentConfirmationOutcome, OrganizationPlanPaymentEventOutcome,
    OrganizationPlanPaymentEventRecord, OrganizationPlanPaymentEventRejection,
    OrganizationPlanPaymentEventWrite,
};

/// 企业授权订单仓储；所有读取均以企业标识作为租户边界。
#[derive(Clone)]
pub struct OrganizationPlanOrderRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl OrganizationPlanOrderRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationPlanOrderRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationPlanOrderRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 在企业范围内按稳定订单标识读取订单。
    pub async fn get_order(
        &self,
        organization_id: OrganizationId,
        order_id: OrganizationPlanOrderId,
    ) -> Result<Option<OrganizationPlanOrderRecord>, OrganizationPlanOrderRepositoryError> {
        let operation = load_order_by_key(self.pool.connection(), organization_id, order_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?.map(order_record).transpose(),
            Err(_) => Err(OrganizationPlanOrderRepositoryError::Timeout),
        }
    }

    /// 在企业范围内按幂等键创建本地订单；不调用支付 Provider。
    pub async fn create_order(
        &self,
        write: OrganizationPlanOrderCreate,
    ) -> Result<OrganizationPlanOrderCreateOutcome, OrganizationPlanOrderRepositoryError> {
        let operation = self
            .create_order_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown),
        };
        #[cfg(test)]
        if matches!(&outcome, OrganizationPlanOrderCreateOutcome::Created(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown);
        }
        Ok(outcome)
    }

    /// 使用版本 CAS 把企业订单绑定到 Provider 支付订单；相同绑定可以安全重放。
    pub async fn submit_order(
        &self,
        write: OrganizationPlanOrderSubmission,
    ) -> Result<OrganizationPlanOrderSubmitOutcome, OrganizationPlanOrderRepositoryError> {
        let operation = self
            .submit_order_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown),
        };
        #[cfg(test)]
        if matches!(&outcome, OrganizationPlanOrderSubmitOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown);
        }
        Ok(outcome)
    }

    /// 追加已验签支付事件；本阶段不推进订单状态或企业授权。
    pub async fn record_payment_event(
        &self,
        write: OrganizationPlanPaymentEventWrite,
    ) -> Result<OrganizationPlanPaymentEventOutcome, OrganizationPlanOrderRepositoryError> {
        let operation = self
            .record_payment_event_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown),
        };
        #[cfg(test)]
        if matches!(&outcome, OrganizationPlanPaymentEventOutcome::Recorded(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown);
        }
        Ok(outcome)
    }

    /// 在同一事务内确认已验签事件、推进订单、替换授权并追加审计。
    pub async fn confirm_payment_event(
        &self,
        write: OrganizationPlanPaymentEventWrite,
    ) -> Result<OrganizationPlanPaymentConfirmationOutcome, OrganizationPlanOrderRepositoryError>
    {
        let operation = self
            .confirm_payment_event_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result?,
            Err(_) => return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown),
        };
        #[cfg(test)]
        if matches!(
            &outcome,
            OrganizationPlanPaymentConfirmationOutcome::Applied { .. }
        ) && self
            .outcome_unknown_after_commit
            .swap(false, Ordering::AcqRel)
        {
            return Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown);
        }
        Ok(outcome)
    }

    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    async fn create_order_inner(
        &self,
        write: &OrganizationPlanOrderCreate,
    ) -> Result<OrganizationPlanOrderCreateOutcome, OrganizationPlanOrderRepositoryError> {
        if let Some(existing) = load_order_by_request(
            self.pool.connection(),
            write.organization_id,
            write.request_id,
        )
        .await?
        {
            return classify_create(order_record(existing)?, write);
        }

        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
        let result = create_order_in_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                Ok(outcome)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                let existing = load_order_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
                classify_create(existing, write)
            }
            Err(TransactionWriteError::Repository(error)) => {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                Err(error)
            }
        }
    }

    async fn submit_order_inner(
        &self,
        write: &OrganizationPlanOrderSubmission,
    ) -> Result<OrganizationPlanOrderSubmitOutcome, OrganizationPlanOrderRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
        let Some(model) = lock_order(&transaction, write.organization_id, write.order_id).await?
        else {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            return Ok(OrganizationPlanOrderSubmitOutcome::NotFound);
        };
        let current = order_record(model.clone())?;
        if current.matches_submission(write) {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            return Ok(OrganizationPlanOrderSubmitOutcome::Existing(current));
        }
        if current.status() != af_domain::OrganizationPlanOrderStatus::Created
            || model.version != write.expected_version
            || current.provider() != write.provider
            || write.submitted_at < current.created_at()
            || current.expires_at() != Some(write.expires_at)
        {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            return Err(OrganizationPlanOrderRepositoryError::Conflict);
        }

        let next_version = model
            .version
            .checked_add(1)
            .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
        let submitted_at = to_database_time(write.submitted_at)?;
        let update = organization_plan_orders::Entity::update_many()
            .filter(organization_plan_orders::Column::Id.eq(model.id))
            .filter(
                organization_plan_orders::Column::OrganizationId.eq(write.organization_id.get()),
            )
            .filter(
                organization_plan_orders::Column::Status
                    .eq(af_domain::OrganizationPlanOrderStatus::Created.code()),
            )
            .filter(organization_plan_orders::Column::Version.eq(write.expected_version))
            .col_expr(
                organization_plan_orders::Column::ProviderOrderId,
                Expr::value(Some(SensitiveString::from(write.provider_order_id.clone()))),
            )
            .col_expr(
                organization_plan_orders::Column::PaymentMethod,
                Expr::value(Some(write.payment_method.clone())),
            )
            .col_expr(
                organization_plan_orders::Column::Status,
                Expr::value(af_domain::OrganizationPlanOrderStatus::Pending.code()),
            )
            .col_expr(
                organization_plan_orders::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_plan_orders::Column::UpdatedAt,
                Expr::value(submitted_at),
            )
            .exec(&transaction)
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
        if update.rows_affected != 1 {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            return Err(OrganizationPlanOrderRepositoryError::Conflict);
        }
        let updated = load_order_by_database_id(&transaction, model.id)
            .await?
            .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
        let updated = order_record(updated)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
        Ok(OrganizationPlanOrderSubmitOutcome::Applied(updated))
    }

    async fn record_payment_event_inner(
        &self,
        write: &OrganizationPlanPaymentEventWrite,
    ) -> Result<OrganizationPlanPaymentEventOutcome, OrganizationPlanOrderRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
        let Some(order) = lock_order(&transaction, write.organization_id, write.order_id).await?
        else {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            return Ok(OrganizationPlanPaymentEventOutcome::NotFound);
        };
        if let Some(existing) = load_event_collision(&transaction, write).await? {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
            if existing.order_id == order.id && event_matches(&existing, write) {
                return Ok(OrganizationPlanPaymentEventOutcome::Existing(event_record(
                    existing,
                    &order.order_key,
                )?));
            }
            return Err(OrganizationPlanOrderRepositoryError::Conflict);
        }

        let received_at = to_database_time(write.received_at)?;
        let inserted = organization_plan_payment_events::ActiveModel {
            event_key: Set(write.event_id.persistence_key()),
            order_id: Set(order.id),
            provider: Set(write.provider.clone()),
            provider_event_id: Set(SensitiveString::from(write.provider_event_id.clone())),
            trade_no: Set(write.trade_no.clone().map(SensitiveString::from)),
            amount_minor: Set(write.amount_minor),
            currency: Set(write.currency.clone()),
            payment_method: Set(write.payment_method.clone()),
            event_type: Set(write.event_type.code()),
            signature_key_fingerprint: Set(SensitiveString::from(hex_digest(
                write.signature_key_fingerprint,
            ))),
            payload_sha256: Set(SensitiveString::from(hex_digest(write.payload_sha256))),
            received_at: Set(received_at),
            processed_at: Set(None),
            created_at: Set(received_at),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(|error| {
            if is_event_unique_conflict(&error) {
                OrganizationPlanOrderRepositoryError::Conflict
            } else {
                OrganizationPlanOrderRepositoryError::Query
            }
        });
        let inserted = match inserted {
            Ok(model) => model,
            Err(error) => {
                let is_unique_conflict =
                    matches!(error, OrganizationPlanOrderRepositoryError::Conflict);
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                if !is_unique_conflict {
                    return Err(error);
                }
                let Some(existing) = load_event_collision(self.pool.connection(), write).await?
                else {
                    return Err(OrganizationPlanOrderRepositoryError::Invariant);
                };
                if existing.order_id == order.id && event_matches(&existing, write) {
                    return Ok(OrganizationPlanPaymentEventOutcome::Existing(event_record(
                        existing,
                        &order.order_key,
                    )?));
                }
                return Err(OrganizationPlanOrderRepositoryError::Conflict);
            }
        };
        let record = event_record(inserted, &order.order_key)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
        Ok(OrganizationPlanPaymentEventOutcome::Recorded(record))
    }

    async fn confirm_payment_event_inner(
        &self,
        write: &OrganizationPlanPaymentEventWrite,
    ) -> Result<OrganizationPlanPaymentConfirmationOutcome, OrganizationPlanOrderRepositoryError>
    {
        // 唯一键竞争只允许回滚后重新读取一次，避免把语义冲突误当成可重试错误。
        for attempt in 0..=1 {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
            let result = confirm_payment_event_in_transaction(&transaction, write).await;
            match result {
                Ok(outcome) => {
                    transaction
                        .commit()
                        .await
                        .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                    return Ok(outcome);
                }
                Err(TransactionWriteError::UniqueConflict) if attempt == 0 => {
                    transaction
                        .rollback()
                        .await
                        .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                }
                Err(TransactionWriteError::Repository(error)) => {
                    transaction
                        .rollback()
                        .await
                        .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                    return Err(error);
                }
                Err(TransactionWriteError::UniqueConflict) => {
                    transaction
                        .rollback()
                        .await
                        .map_err(|_| OrganizationPlanOrderRepositoryError::OutcomeUnknown)?;
                    return Err(OrganizationPlanOrderRepositoryError::Conflict);
                }
            }
        }
        Err(OrganizationPlanOrderRepositoryError::Invariant)
    }
}

impl fmt::Debug for OrganizationPlanOrderRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationPlanOrderRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 在订单事务内完成企业支付确认，调用方不得在此处拆分授权写入事务。
async fn confirm_payment_event_in_transaction(
    transaction: &DatabaseTransaction,
    write: &OrganizationPlanPaymentEventWrite,
) -> Result<OrganizationPlanPaymentConfirmationOutcome, TransactionWriteError> {
    let organization =
        match organization_mutation::lock_organization(transaction, write.organization_id).await {
            Ok(organization) => organization,
            Err(crate::OrganizationRepositoryError::NotFound) => {
                return Ok(OrganizationPlanPaymentConfirmationOutcome::NotFound);
            }
            Err(error) => return Err(map_organization_error(error).into()),
        };
    if organization.status != af_domain::OrganizationStatus::Active.code()
        || organization.closed_at.is_some()
    {
        return Ok(OrganizationPlanPaymentConfirmationOutcome::NotFound);
    }
    let Some(order_model) = lock_order(transaction, write.organization_id, write.order_id).await?
    else {
        return Ok(OrganizationPlanPaymentConfirmationOutcome::NotFound);
    };
    let order = order_record(order_model.clone())?;
    if let Some(existing) = load_event_collision(transaction, write).await? {
        if existing.order_id != order_model.id || !event_matches(&existing, write) {
            return Err(OrganizationPlanOrderRepositoryError::Conflict.into());
        }
        if existing.processed_at.is_some() {
            let entitlement = if order.status == OrganizationPlanOrderStatus::Paid {
                load_entitlement_record(transaction, order.organization_id()).await?
            } else {
                None
            };
            return Ok(OrganizationPlanPaymentConfirmationOutcome::Existing { order, entitlement });
        }
        confirm_new_event(transaction, &order_model, order, existing, write).await
    } else {
        let received_at = to_database_time(write.received_at)?;
        let inserted = organization_plan_payment_events::ActiveModel {
            event_key: Set(write.event_id.persistence_key()),
            order_id: Set(order_model.id),
            provider: Set(write.provider.clone()),
            provider_event_id: Set(SensitiveString::from(write.provider_event_id.clone())),
            trade_no: Set(write.trade_no.clone().map(SensitiveString::from)),
            amount_minor: Set(write.amount_minor),
            currency: Set(write.currency.clone()),
            payment_method: Set(write.payment_method.clone()),
            event_type: Set(write.event_type.code()),
            signature_key_fingerprint: Set(SensitiveString::from(hex_digest(
                write.signature_key_fingerprint,
            ))),
            payload_sha256: Set(SensitiveString::from(hex_digest(write.payload_sha256))),
            received_at: Set(received_at),
            processed_at: Set(None),
            created_at: Set(received_at),
            ..Default::default()
        }
        .insert(transaction)
        .await
        .map_err(|error| {
            if is_event_unique_conflict(&error) {
                TransactionWriteError::UniqueConflict
            } else {
                OrganizationPlanOrderRepositoryError::Query.into()
            }
        })?;
        confirm_new_event(transaction, &order_model, order, inserted, write).await
    }
}

async fn confirm_new_event(
    transaction: &DatabaseTransaction,
    order_model: &organization_plan_orders::Model,
    order: OrganizationPlanOrderRecord,
    event_model: organization_plan_payment_events::Model,
    write: &OrganizationPlanPaymentEventWrite,
) -> Result<OrganizationPlanPaymentConfirmationOutcome, TransactionWriteError> {
    let rejection = validate_confirmation_event(&order, write);
    if let Some(reason) = rejection {
        return Ok(
            OrganizationPlanPaymentConfirmationOutcome::RecordedUnprocessed { order, reason },
        );
    }

    if !order.status().is_open() {
        if order.status() == write.event_type.target_status()
            && (order.status() != OrganizationPlanOrderStatus::Paid
                || order.trade_no() == write.trade_no.as_deref())
        {
            mark_payment_event_processed(transaction, event_model.id, write.received_at).await?;
            let entitlement = if order.status() == OrganizationPlanOrderStatus::Paid {
                load_entitlement_record(transaction, order.organization_id()).await?
            } else {
                None
            };
            return Ok(OrganizationPlanPaymentConfirmationOutcome::Acknowledged {
                order,
                entitlement,
            });
        }
        return Ok(
            OrganizationPlanPaymentConfirmationOutcome::RecordedUnprocessed {
                order,
                reason: OrganizationPlanPaymentEventRejection::TerminalConflict,
            },
        );
    }

    let processed_at = write.received_at;
    let entitlement = if write.event_type == OrganizationPlanPaymentEventType::Succeeded {
        let Some(plan) = lock_plan(transaction, order.plan_id()).await? else {
            return Ok(
                OrganizationPlanPaymentConfirmationOutcome::RecordedUnprocessed {
                    order,
                    reason: OrganizationPlanPaymentEventRejection::PlanSnapshotConflict,
                },
            );
        };
        if plan.id != order_model.plan_id
            || plan.version != i64::try_from(order.plan_version()).unwrap_or_default()
        {
            return Ok(
                OrganizationPlanPaymentConfirmationOutcome::RecordedUnprocessed {
                    order,
                    reason: OrganizationPlanPaymentEventRejection::PlanSnapshotConflict,
                },
            );
        }
        let current = organization_mutation::lock_entitlement(transaction, order.organization_id())
            .await
            .map_err(map_organization_error)?;
        let current_version = u64::try_from(current.version)
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
        let valid_from = i64::try_from(processed_at)
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
        let duration = plan
            .validity_days
            .checked_mul(86_400)
            .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
        let valid_until = valid_from
            .checked_add(duration)
            .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
        let snapshot = af_domain::OrganizationEntitlementSnapshot::new(
            af_domain::OrganizationEntitlementCapacity::new(
                plan.seat_limit,
                plan.team_limit,
                plan.organization_key_limit,
            )
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?,
            plan.team_management_enabled,
            plan.sso_enabled,
            plan.custom_roles_enabled,
            plan.audit_retention_days,
            valid_from,
            valid_until,
            Some(valid_until),
        )
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
        let entitlement_write = crate::OrganizationEntitlementWrite::new(
            snapshot,
            "organization_plan_order".to_owned(),
            Some(order.order_id().persistence_key()),
            Some(current_version),
        )
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
        Some(
            organization_mutation::replace_entitlement_in_transaction(
                transaction,
                order.organization_id(),
                entitlement_write,
            )
            .await
            .map_err(map_organization_error)?,
        )
    } else {
        None
    };

    transition_payment_order(transaction, order_model, write, processed_at).await?;
    mark_payment_event_processed(transaction, event_model.id, processed_at).await?;
    append_payment_audit(transaction, &order, write).await?;
    let updated = load_order_by_database_id(transaction, order_model.id)
        .await?
        .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
    let updated = order_record(updated)?;
    Ok(OrganizationPlanPaymentConfirmationOutcome::Applied {
        order: updated,
        entitlement,
    })
}

fn validate_confirmation_event(
    order: &OrganizationPlanOrderRecord,
    write: &OrganizationPlanPaymentEventWrite,
) -> Option<OrganizationPlanPaymentEventRejection> {
    if write.provider != order.provider() {
        return Some(OrganizationPlanPaymentEventRejection::ProviderMismatch);
    }
    if (write.event_type == OrganizationPlanPaymentEventType::Succeeded
        && write.amount_minor != Some(order.amount_minor()))
        || write
            .amount_minor
            .is_some_and(|amount| amount != order.amount_minor())
    {
        return Some(OrganizationPlanPaymentEventRejection::AmountMismatch);
    }
    if (write.event_type == OrganizationPlanPaymentEventType::Succeeded
        && write.currency.as_deref() != Some(order.currency()))
        || write
            .currency
            .as_deref()
            .is_some_and(|currency| currency != order.currency())
    {
        return Some(OrganizationPlanPaymentEventRejection::CurrencyMismatch);
    }
    if order.payment_method().is_none() {
        return Some(OrganizationPlanPaymentEventRejection::SubmissionRequired);
    }
    if write
        .payment_method
        .as_deref()
        .is_some_and(|method| order.payment_method() != Some(method))
        || (write.event_type == OrganizationPlanPaymentEventType::Succeeded
            && write.payment_method.is_none())
    {
        return Some(OrganizationPlanPaymentEventRejection::PaymentMethodMismatch);
    }
    if write.received_at < order.created_at() {
        return Some(OrganizationPlanPaymentEventRejection::TimingConflict);
    }
    if order
        .trade_no()
        .zip(write.trade_no.as_deref())
        .is_some_and(|(expected, actual)| expected != actual)
    {
        return Some(OrganizationPlanPaymentEventRejection::TradeNumberConflict);
    }
    None
}

async fn transition_payment_order(
    transaction: &DatabaseTransaction,
    order: &organization_plan_orders::Model,
    write: &OrganizationPlanPaymentEventWrite,
    processed_at: u64,
) -> Result<(), OrganizationPlanOrderRepositoryError> {
    let target = write.event_type.target_status();
    let next_version = order
        .version
        .checked_add(1)
        .ok_or(OrganizationPlanOrderRepositoryError::Invariant)?;
    let timestamp = to_database_time(processed_at)?;
    let update = organization_plan_orders::Entity::update_many()
        .filter(organization_plan_orders::Column::Id.eq(order.id))
        .filter(organization_plan_orders::Column::Version.eq(order.version))
        .filter(organization_plan_orders::Column::Status.is_in([
            OrganizationPlanOrderStatus::Created.code(),
            OrganizationPlanOrderStatus::Pending.code(),
        ]))
        .col_expr(organization_plan_orders::Column::Status, Expr::value(target.code()))
        // 失败/过期事件可能不携带可选字段；不能覆盖提交阶段已固化的绑定事实。
        .col_expr(
            organization_plan_orders::Column::TradeNo,
            write.trade_no
                .clone()
                .map(|trade_no| Expr::value(Some(SensitiveString::from(trade_no))))
                .unwrap_or_else(|| Expr::col(organization_plan_orders::Column::TradeNo).into()),
        )
        .col_expr(
            organization_plan_orders::Column::PaymentMethod,
            write
                .payment_method
                .clone()
                .map(|payment_method| Expr::value(Some(payment_method)))
                .unwrap_or_else(|| Expr::col(organization_plan_orders::Column::PaymentMethod).into()),
        )
        .col_expr(organization_plan_orders::Column::Version, Expr::value(next_version))
        .col_expr(
            organization_plan_orders::Column::PaidAt,
            Expr::value((target == OrganizationPlanOrderStatus::Paid).then_some(timestamp)),
        )
        .col_expr(
            organization_plan_orders::Column::ClosedAt,
            Expr::value((target != OrganizationPlanOrderStatus::Paid).then_some(timestamp)),
        )
        .col_expr(organization_plan_orders::Column::UpdatedAt, Expr::value(timestamp))
        .exec(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(OrganizationPlanOrderRepositoryError::Conflict);
    }
    Ok(())
}

async fn mark_payment_event_processed(
    transaction: &DatabaseTransaction,
    event_id: i64,
    processed_at: u64,
) -> Result<(), OrganizationPlanOrderRepositoryError> {
    let processed_at = to_database_time(processed_at)?;
    let update = organization_plan_payment_events::Entity::update_many()
        .filter(organization_plan_payment_events::Column::Id.eq(event_id))
        .filter(organization_plan_payment_events::Column::ProcessedAt.is_null())
        .col_expr(
            organization_plan_payment_events::Column::ProcessedAt,
            Expr::value(processed_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(OrganizationPlanOrderRepositoryError::Invariant);
    }
    Ok(())
}

async fn load_entitlement_record(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<Option<crate::OrganizationEntitlementRecord>, OrganizationPlanOrderRepositoryError> {
    let model = organization_mutation::lock_entitlement(transaction, organization_id)
        .await
        .map_err(map_organization_error)?;
    organization_read::entitlement_record(model)
        .map(Some)
        .map_err(map_organization_error)
}

async fn append_payment_audit(
    transaction: &DatabaseTransaction,
    order: &OrganizationPlanOrderRecord,
    write: &OrganizationPlanPaymentEventWrite,
) -> Result<(), OrganizationPlanOrderRepositoryError> {
    let audit_write = OrganizationAuditWrite::new(
        order.organization_id(),
        order.payer_user_id(),
        None,
        "organization.plan_orders.confirm".to_owned(),
        "organization.plan_order.payment_confirmed".to_owned(),
        "organization_plan_order".to_owned(),
        order.order_id().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some("payment_webhook".to_owned()),
        format!(
            "organization-plan-payment:{}",
            write.event_id.persistence_key()
        ),
        write.payload_sha256,
        Some(
            serde_json::json!({
                "event_type": write.event_type.code(),
                "provider": write.provider.clone(),
                "amount_minor": order.amount_minor(),
                "currency": order.currency(),
            })
            .to_string(),
        ),
    )
    .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    record_audit(transaction, &audit_write)
        .await
        .map(|_| ())
        .map_err(|error| match error {
            crate::OrganizationAuditRepositoryError::Conflict => {
                OrganizationPlanOrderRepositoryError::Conflict
            }
            crate::OrganizationAuditRepositoryError::Invariant => {
                OrganizationPlanOrderRepositoryError::Invariant
            }
            crate::OrganizationAuditRepositoryError::Query
            | crate::OrganizationAuditRepositoryError::Timeout => {
                OrganizationPlanOrderRepositoryError::Query
            }
        })
}

fn map_organization_error(
    error: crate::OrganizationRepositoryError,
) -> OrganizationPlanOrderRepositoryError {
    match error {
        crate::OrganizationRepositoryError::InvalidInput => {
            OrganizationPlanOrderRepositoryError::InvalidInput
        }
        crate::OrganizationRepositoryError::Conflict => {
            OrganizationPlanOrderRepositoryError::Conflict
        }
        crate::OrganizationRepositoryError::Query => OrganizationPlanOrderRepositoryError::Query,
        crate::OrganizationRepositoryError::Timeout => {
            OrganizationPlanOrderRepositoryError::Timeout
        }
        crate::OrganizationRepositoryError::Invariant
        | crate::OrganizationRepositoryError::NotFound
        | crate::OrganizationRepositoryError::UserUnavailable
        | crate::OrganizationRepositoryError::InvalidTransition
        | crate::OrganizationRepositoryError::LastOwner
        | crate::OrganizationRepositoryError::TeamHasActiveMembers
        | crate::OrganizationRepositoryError::EntitlementRestricted
        | crate::OrganizationRepositoryError::SeatLimitReached
        | crate::OrganizationRepositoryError::InvitationExpired
        | crate::OrganizationRepositoryError::InvitationNotPending
        | crate::OrganizationRepositoryError::InvitationEmailMismatch
        | crate::OrganizationRepositoryError::InvitationBatchConflict
        | crate::OrganizationRepositoryError::TeamLimitReached
        | crate::OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationPlanOrderRepositoryError::Invariant
        }
    }
}

enum TransactionWriteError {
    UniqueConflict,
    Repository(OrganizationPlanOrderRepositoryError),
}

impl From<OrganizationPlanOrderRepositoryError> for TransactionWriteError {
    fn from(error: OrganizationPlanOrderRepositoryError) -> Self {
        Self::Repository(error)
    }
}

async fn create_order_in_transaction(
    transaction: &DatabaseTransaction,
    write: &OrganizationPlanOrderCreate,
) -> Result<OrganizationPlanOrderCreateOutcome, TransactionWriteError> {
    let organization = organizations::Entity::find_by_id(write.organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
    if organization.is_none() {
        return Ok(OrganizationPlanOrderCreateOutcome::NotFound);
    }
    if users::Entity::find_by_id(write.payer_user_id.get())
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?
        .is_none()
        || organization_memberships::Entity::find()
            .filter(
                organization_memberships::Column::OrganizationId.eq(write.organization_id.get()),
            )
            .filter(organization_memberships::Column::UserId.eq(write.payer_user_id.get()))
            .filter(
                organization_memberships::Column::Status
                    .eq(OrganizationMembershipStatus::Active.code()),
            )
            .one(transaction)
            .await
            .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?
            .is_none()
    {
        return Ok(OrganizationPlanOrderCreateOutcome::NotFound);
    }
    let Some(plan) = lock_plan(transaction, write.plan_id).await? else {
        return Ok(OrganizationPlanOrderCreateOutcome::NotFound);
    };
    if plan.status != 1 || plan.version != i64::try_from(write.plan_version).unwrap_or_default() {
        return Ok(OrganizationPlanOrderCreateOutcome::NotFound);
    }
    let Some(price) = organization_plan_prices::Entity::find()
        .filter(organization_plan_prices::Column::PlanId.eq(plan.id))
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?
    else {
        return Err(OrganizationPlanOrderRepositoryError::Invariant.into());
    };
    if price.provider != write.provider
        || price.currency != write.currency
        || price.amount_minor != write.amount_minor
    {
        return Err(OrganizationPlanOrderRepositoryError::Conflict.into());
    }
    if let Some(existing) =
        load_order_by_request(transaction, write.organization_id, write.request_id).await?
    {
        return classify_create(order_record(existing)?, write).map_err(Into::into);
    }
    let created_at = to_database_time(write.created_at)?;
    let expires_at = write.expires_at.map(to_database_time).transpose()?;
    let inserted = organization_plan_orders::ActiveModel {
        order_key: Set(write.order_id.persistence_key()),
        organization_id: Set(write.organization_id.get()),
        payer_user_id: Set(write.payer_user_id.get()),
        plan_id: Set(plan.id),
        plan_key: Set(write.plan_id.persistence_key()),
        plan_version: Set(write.plan_version as i64),
        provider: Set(write.provider.clone()),
        currency: Set(write.currency.clone()),
        amount_minor: Set(write.amount_minor),
        status: Set(OrganizationPlanOrderStatus::Created.code()),
        idempotency_key: Set(write.request_id.persistence_key()),
        version: Set(1),
        provider_order_id: Set(None),
        trade_no: Set(None),
        payment_method: Set(None),
        expires_at: Set(expires_at),
        paid_at: Set(None),
        closed_at: Set(None),
        created_at: Set(created_at),
        updated_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    match inserted {
        Ok(model) => Ok(OrganizationPlanOrderCreateOutcome::Created(order_record(
            model,
        )?)),
        Err(error) if is_order_unique_conflict(&error) => {
            Err(TransactionWriteError::UniqueConflict)
        }
        Err(_) => Err(OrganizationPlanOrderRepositoryError::Query.into()),
    }
}

async fn load_order_by_key<C: ConnectionTrait>(
    connection: &C,
    organization_id: OrganizationId,
    order_id: OrganizationPlanOrderId,
) -> Result<Option<organization_plan_orders::Model>, OrganizationPlanOrderRepositoryError> {
    organization_plan_orders::Entity::find()
        .filter(organization_plan_orders::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_plan_orders::Column::OrderKey.eq(order_id.persistence_key()))
        .one(connection)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)
}

async fn load_order_by_database_id<C: ConnectionTrait>(
    connection: &C,
    database_id: i64,
) -> Result<Option<organization_plan_orders::Model>, OrganizationPlanOrderRepositoryError> {
    organization_plan_orders::Entity::find_by_id(database_id)
        .one(connection)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)
}

async fn load_order_by_request<C: ConnectionTrait>(
    connection: &C,
    organization_id: OrganizationId,
    request_id: OrganizationPlanOrderRequestId,
) -> Result<Option<organization_plan_orders::Model>, OrganizationPlanOrderRepositoryError> {
    organization_plan_orders::Entity::find()
        .filter(organization_plan_orders::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_plan_orders::Column::IdempotencyKey.eq(request_id.persistence_key()))
        .one(connection)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)
}

async fn load_order_collision<C: ConnectionTrait>(
    connection: &C,
    write: &OrganizationPlanOrderCreate,
) -> Result<Option<OrganizationPlanOrderRecord>, OrganizationPlanOrderRepositoryError> {
    let by_request =
        load_order_by_request(connection, write.organization_id, write.request_id).await?;
    let by_key = load_order_by_key(connection, write.organization_id, write.order_id).await?;
    match (by_request, by_key) {
        (Some(request), Some(key)) if request.id != key.id => {
            Err(OrganizationPlanOrderRepositoryError::Conflict)
        }
        (Some(model), _) | (_, Some(model)) => order_record(model).map(Some),
        (None, None) => Ok(None),
    }
}

async fn lock_order(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    order_id: OrganizationPlanOrderId,
) -> Result<Option<organization_plan_orders::Model>, OrganizationPlanOrderRepositoryError> {
    let mut query = organization_plan_orders::Entity::find()
        .filter(organization_plan_orders::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_plan_orders::Column::OrderKey.eq(order_id.persistence_key()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)
}

async fn lock_plan(
    transaction: &DatabaseTransaction,
    plan_id: OrganizationPlanId,
) -> Result<Option<organization_plans::Model>, OrganizationPlanOrderRepositoryError> {
    let mut query = organization_plans::Entity::find()
        .filter(organization_plans::Column::PlanKey.eq(plan_id.persistence_key()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)
}

fn classify_create(
    model: OrganizationPlanOrderRecord,
    write: &OrganizationPlanOrderCreate,
) -> Result<OrganizationPlanOrderCreateOutcome, OrganizationPlanOrderRepositoryError> {
    if model.matches_create(write) {
        Ok(OrganizationPlanOrderCreateOutcome::Existing(model))
    } else {
        Err(OrganizationPlanOrderRepositoryError::Conflict)
    }
}

fn order_record(
    model: organization_plan_orders::Model,
) -> Result<OrganizationPlanOrderRecord, OrganizationPlanOrderRepositoryError> {
    let order_id = OrganizationPlanOrderId::from_persistence_key(&model.order_key)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let request_id = OrganizationPlanOrderRequestId::from_persistence_key(&model.idempotency_key)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let payer_user_id = UserId::new(model.payer_user_id)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let plan_id = OrganizationPlanId::from_persistence_key(&model.plan_key)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let status = OrganizationPlanOrderStatus::try_from(model.status)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let plan_version = u64::try_from(model.plan_version)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let version = u64::try_from(model.version)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let expires_at = optional_unix_seconds(model.expires_at)?;
    let paid_at = optional_unix_seconds(model.paid_at)?;
    let closed_at = optional_unix_seconds(model.closed_at)?;
    if model.id <= 0
        || plan_version == 0
        || version == 0
        || !valid_token(&model.provider, 32)
        || !valid_currency(&model.currency)
        || model.amount_minor <= 0
        || (status.is_open() && (paid_at.is_some() || closed_at.is_some()))
        || (status == OrganizationPlanOrderStatus::Paid
            && (paid_at.is_none() || closed_at.is_some()))
        || (matches!(
            status,
            OrganizationPlanOrderStatus::Failed
                | OrganizationPlanOrderStatus::Canceled
                | OrganizationPlanOrderStatus::Expired
        ) && (paid_at.is_some() || closed_at.is_none()))
        || expires_at.is_some_and(|value| value <= created_at)
        || updated_at < created_at
    {
        return Err(OrganizationPlanOrderRepositoryError::Invariant);
    }
    Ok(OrganizationPlanOrderRecord {
        database_id: model.id,
        order_id,
        request_id,
        organization_id,
        payer_user_id,
        plan_id,
        plan_version,
        provider: model.provider,
        currency: model.currency,
        amount_minor: model.amount_minor,
        status,
        version,
        provider_order_id: model
            .provider_order_id
            .map(|value| value.as_str().to_owned()),
        trade_no: model.trade_no.map(|value| value.as_str().to_owned()),
        payment_method: model.payment_method,
        expires_at,
        paid_at,
        closed_at,
        created_at,
        updated_at,
    })
}

async fn load_event_collision<C: ConnectionTrait>(
    connection: &C,
    write: &OrganizationPlanPaymentEventWrite,
) -> Result<Option<organization_plan_payment_events::Model>, OrganizationPlanOrderRepositoryError> {
    let rows = organization_plan_payment_events::Entity::find()
        .filter(
            Condition::any()
                .add(
                    organization_plan_payment_events::Column::EventKey
                        .eq(write.event_id.persistence_key()),
                )
                .add(
                    Condition::all()
                        .add(
                            organization_plan_payment_events::Column::Provider
                                .eq(write.provider.clone()),
                        )
                        .add(
                            organization_plan_payment_events::Column::ProviderEventId
                                .eq(SensitiveString::from(write.provider_event_id.clone())),
                        ),
                ),
        )
        .all(connection)
        .await
        .map_err(|_| OrganizationPlanOrderRepositoryError::Query)?;
    if rows.len() > 1 {
        return Err(OrganizationPlanOrderRepositoryError::Conflict);
    }
    Ok(rows.into_iter().next())
}

fn event_matches(
    model: &organization_plan_payment_events::Model,
    write: &OrganizationPlanPaymentEventWrite,
) -> bool {
    model.event_key == write.event_id.persistence_key()
        && model.provider == write.provider
        && model.provider_event_id.as_str() == write.provider_event_id
        && model.trade_no.as_ref().map(SensitiveString::as_str) == write.trade_no.as_deref()
        && model.amount_minor == write.amount_minor
        && model.currency.as_deref() == write.currency.as_deref()
        && model.payment_method.as_deref() == write.payment_method.as_deref()
        && model.event_type == write.event_type.code()
        && model.signature_key_fingerprint.as_str() == hex_digest(write.signature_key_fingerprint)
        && model.payload_sha256.as_str() == hex_digest(write.payload_sha256)
}

fn event_record(
    model: organization_plan_payment_events::Model,
    order_key: &str,
) -> Result<OrganizationPlanPaymentEventRecord, OrganizationPlanOrderRepositoryError> {
    Ok(OrganizationPlanPaymentEventRecord {
        database_id: model.id,
        event_id: OrganizationPlanPaymentEventId::from_persistence_key(&model.event_key)
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?,
        order_id: OrganizationPlanOrderId::from_persistence_key(order_key)
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?,
        provider: model.provider,
        provider_event_id: model.provider_event_id.as_str().to_owned(),
        trade_no: model.trade_no.map(|value| value.as_str().to_owned()),
        amount_minor: model.amount_minor,
        currency: model.currency,
        payment_method: model.payment_method,
        event_type: OrganizationPlanPaymentEventType::try_from(model.event_type)
            .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?,
        received_at: unix_seconds(model.received_at)?,
        processed_at: optional_unix_seconds(model.processed_at)?,
    })
}

fn valid_token(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, OrganizationPlanOrderRepositoryError> {
    let value =
        i64::try_from(value).map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)
}

fn unix_seconds(
    value: TimeDateTimeWithTimeZone,
) -> Result<u64, OrganizationPlanOrderRepositoryError> {
    u64::try_from(value.unix_timestamp())
        .map_err(|_| OrganizationPlanOrderRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, OrganizationPlanOrderRepositoryError> {
    value.map(unix_seconds).transpose()
}

fn hex_digest(value: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn is_order_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_organization_plan_orders_order_key")
        || rendered.contains("uq_organization_plan_orders_org_idempotency")
        || rendered.contains("organization_plan_orders.order_key")
        || rendered.contains("organization_plan_orders.organization_id")
        || rendered.contains("Duplicate entry")
}

fn is_event_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_organization_plan_payment_events_event_key")
        || rendered.contains("uq_organization_plan_payment_events_provider_event")
        || rendered.contains("organization_plan_payment_events.event_key")
        || rendered.contains("organization_plan_payment_events.provider")
        || rendered.contains("Duplicate entry")
}
