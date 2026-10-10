use std::{fmt, future::Future, pin::Pin, time::Duration};

use std::sync::Arc;

#[cfg(test)]
use std::sync::atomic::{AtomicU8, Ordering};

use af_domain::{BillingContractPriceSnapshot, BillingReservationId, GatewayPrincipal, Quota};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, QueryResult, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::SelectStatement,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

mod funding;
mod group_windows;
pub use funding::{QuotaFundingContext, QuotaFundingExtension, QuotaFundingFuture};
mod sql;
mod state;
pub(crate) mod subscription;
mod token_windows;

use state::{GroupQuotaState, ReservationState, TokenQuotaState, UserQuotaState};
pub use state::{
    QuotaMutationOutcome, QuotaRepositoryError, QuotaReservationKind, QuotaReservationStatus,
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_RESERVATION_TTL: Duration = Duration::from_secs(15 * 60);

pub type QuotaExtensionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<QuotaMutationOutcome, QuotaRepositoryError>> + Send + 'a>>;

/// Distribution-owned quota handling for organization principals and reservations.
/// Implementations must preserve the reservation's atomic and idempotent behavior.
pub trait QuotaExtension: Send + Sync {
    fn precharge<'a>(
        &'a self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
        kind: QuotaReservationKind,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> QuotaExtensionFuture<'a>;

    fn settle<'a>(
        &'a self,
        id: BillingReservationId,
        actual: Quota,
        kind: QuotaReservationKind,
    ) -> QuotaExtensionFuture<'a>;

    fn refund<'a>(
        &'a self,
        id: BillingReservationId,
        kind: QuotaReservationKind,
    ) -> QuotaExtensionFuture<'a>;
}

#[derive(Clone, Copy)]
pub(crate) enum QuotaRepositoryOperation {
    Precharge = 1,
    Settle = 2,
    Refund = 3,
}

/// 以持久化预留状态机原子维护用户钱包和令牌额度。
#[derive(Clone)]
pub struct QuotaRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    reservation_ttl: Duration,
    extension: Option<Arc<dyn QuotaExtension>>,
    funding_extension: Option<Arc<dyn QuotaFundingExtension>>,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicU8>,
}

impl QuotaRepository {
    /// 使用默认五秒操作截止时间和十五分钟预留期限构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            reservation_ttl: DEFAULT_RESERVATION_TTL,
            extension: None,
            funding_extension: None,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicU8::new(0)),
        }
    }

    /// 使用显式截止时间和预留期限构造仓储；截止时间必须大于零，预留期限至少一秒。
    pub fn with_config(
        pool: DatabasePool,
        operation_timeout: Duration,
        reservation_ttl: Duration,
    ) -> Result<Self, QuotaRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(QuotaRepositoryError::InvalidConfiguration);
        }
        reservation_times(reservation_ttl)?;
        Ok(Self {
            pool,
            operation_timeout,
            reservation_ttl,
            extension: None,
            funding_extension: None,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicU8::new(0)),
        })
    }

    /// 注入发行版负责的企业额度实现；公共核心不直接访问企业钱包或预算表。
    #[must_use]
    pub fn with_extension(mut self, extension: Arc<dyn QuotaExtension>) -> Self {
        self.extension = Some(extension);
        self
    }

    /// Keep the shared reservation lifecycle and key/group controls while replacing funding.
    #[must_use]
    pub fn with_funding_extension(mut self, extension: Arc<dyn QuotaFundingExtension>) -> Self {
        self.funding_extension = Some(extension);
        self
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self, operation: QuotaRepositoryOperation) {
        self.outcome_unknown_after_commit
            .store(operation as u8, Ordering::Release);
    }

    /// 创建一次严格正数的额度预留；相同标识重放时不会再次扣减。
    pub async fn precharge(
        &self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.reserve_with_kind(id, principal, amount, QuotaReservationKind::Request, None)
            .await
    }

    /// 创建绑定企业合同价快照的请求预留；快照只在预留首次写入时采纳。
    pub async fn precharge_with_contract_price(
        &self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.reserve_with_kind(
            id,
            principal,
            amount,
            QuotaReservationKind::Request,
            contract_price,
        )
        .await
    }

    /// 为异步批量任务冻结钱包余额与有限令牌额度；不使用订阅，也不允许后续补扣。
    pub async fn reserve_batch_task(
        &self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.reserve_with_kind(id, principal, amount, QuotaReservationKind::BatchTask, None)
            .await
    }

    async fn reserve_with_kind(
        &self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
        reservation_kind: QuotaReservationKind,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        if let Some(contract_price) = contract_price {
            let Some(organization) = principal.organization_principal() else {
                return Err(QuotaRepositoryError::Invariant);
            };
            if contract_price.organization_id() != organization.organization_id() {
                return Err(QuotaRepositoryError::Invariant);
            }
        }
        if amount.is_zero() {
            return Err(QuotaRepositoryError::ZeroAmount);
        }
        let operation = self
            .precharge_inner(id, principal, amount, reservation_kind, contract_price)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => self.finish_operation(result, QuotaRepositoryOperation::Precharge),
            Err(_) => Err(record_internal_error(QuotaRepositoryError::OutcomeUnknown)),
        }
    }

    /// 按实际额度结算预留；待结算记录可使用相同实际额度安全重试。
    pub async fn settle(
        &self,
        id: BillingReservationId,
        actual: Quota,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.settle_with_kind(id, actual, QuotaReservationKind::Request)
            .await
    }

    /// 结算批量任务；实际额度不得超过创建任务时冻结的额度。
    pub async fn settle_batch_task(
        &self,
        id: BillingReservationId,
        actual: Quota,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.settle_with_kind(id, actual, QuotaReservationKind::BatchTask)
            .await
    }

    async fn settle_with_kind(
        &self,
        id: BillingReservationId,
        actual: Quota,
        reservation_kind: QuotaReservationKind,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let operation = self
            .settle_inner(id, actual, reservation_kind)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => self.finish_operation(result, QuotaRepositoryOperation::Settle),
            Err(_) => Err(record_internal_error(QuotaRepositoryError::OutcomeUnknown)),
        }
    }

    /// 全额退还仍处于预留中的记录；终态退款可安全重放。
    pub async fn refund(
        &self,
        id: BillingReservationId,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.release_with_kind(id, QuotaReservationKind::Request)
            .await
    }

    /// 释放尚未结算的批量任务冻结额度；终态释放可安全重放。
    pub async fn release_batch_task(
        &self,
        id: BillingReservationId,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        self.release_with_kind(id, QuotaReservationKind::BatchTask)
            .await
    }

    async fn release_with_kind(
        &self,
        id: BillingReservationId,
        reservation_kind: QuotaReservationKind,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let operation = self
            .refund_inner(id, reservation_kind)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => self.finish_operation(result, QuotaRepositoryOperation::Refund),
            Err(_) => Err(record_internal_error(QuotaRepositoryError::OutcomeUnknown)),
        }
    }

    fn finish_operation(
        &self,
        result: Result<QuotaMutationOutcome, QuotaRepositoryError>,
        _operation: QuotaRepositoryOperation,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let result = result.map_err(record_internal_error)?;
        #[cfg(test)]
        if result == QuotaMutationOutcome::Applied
            && self
                .outcome_unknown_after_commit
                .compare_exchange(_operation as u8, 0, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return Err(record_internal_error(QuotaRepositoryError::OutcomeUnknown));
        }
        Ok(result)
    }

    async fn precharge_inner(
        &self,
        id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
        reservation_kind: QuotaReservationKind,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        if principal.organization_principal().is_some() && self.funding_extension.is_none() {
            let Some(extension) = self.extension.as_ref() else {
                return Err(QuotaRepositoryError::ExtensionUnavailable);
            };
            return extension
                .precharge(id, principal, amount, reservation_kind, contract_price)
                .await;
        }
        let key = id.persistence_key();
        let (now, expires_at) = reservation_times(self.reservation_ttl)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        let backend = transaction.get_database_backend();

        let funding = principal
            .organization_principal()
            .map(|organization| QuotaFundingContext {
                id,
                organization_id: organization.organization_id(),
                user_id: principal.user_id(),
                token_id: principal.token_id(),
                group_id: principal.group_id(),
                reserved: amount,
                kind: reservation_kind,
                now,
                expires_at: Some(expires_at),
            });
        if let Some(context) = funding.as_ref()
            && let Err(error) = self
                .funding_extension
                .as_ref()
                .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                .lock(&transaction, context)
                .await
        {
            return rollback_with_error(transaction, error).await;
        }

        // 先取得会被修改的父行锁，避免 MySQL 外键检查先持有共享锁后再升级而死锁。
        let (group_state, _, token_state) = match lock_subject(
            &transaction,
            principal.group_id().get(),
            principal.user_id().get(),
            principal.token_id().get(),
            funding
                .as_ref()
                .map(|context| context.organization_id.get()),
        )
        .await
        {
            Ok(state) => state,
            Err(error) => return rollback_with_error(transaction, error).await,
        };

        if !token_matches_principal(&token_state, principal) {
            return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
        }
        match transaction
            .execute(backend.build(&sql::insert_reservation(
                key.clone(),
                principal,
                amount,
                reservation_kind,
                now,
                expires_at,
                contract_price,
            )))
            .await
        {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
            }
            Err(error) if is_unique_violation(&error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
                return self
                    .existing_precharge(
                        id,
                        key,
                        principal,
                        amount,
                        reservation_kind,
                        contract_price,
                    )
                    .await;
            }
            Err(_) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
            }
        }

        let parent = match load_reservation(&transaction, key.clone(), true).await {
            Ok(Some(state))
                if state.status == QuotaReservationStatus::Reserved
                    && state.matches_precharge(principal, amount, reservation_kind) =>
            {
                state
            }
            Ok(Some(_)) | Ok(None) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
            }
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        if reservation_kind.uses_group_windows()
            && let Err(error) =
                group_windows::reserve(&transaction, &key, &parent, group_state, amount, now).await
        {
            return rollback_with_error(transaction, error).await;
        }
        let uses_subscription = if funding.is_none() && reservation_kind.allows_subscription() {
            match subscription::try_reserve(&transaction, &key, &parent, amount, now).await {
                Ok(value) => value,
                Err(error) => return rollback_with_error(transaction, error).await,
            }
        } else {
            false
        };

        if let Some(context) = funding.as_ref() {
            if let Err(error) = self
                .funding_extension
                .as_ref()
                .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                .reserve(&transaction, context, principal)
                .await
            {
                return rollback_with_error(transaction, error).await;
            }
        } else if !uses_subscription {
            let user_result = transaction
                .execute(backend.build(&sql::precharge_user(
                    principal.user_id().get(),
                    amount,
                    now,
                )))
                .await;
            match user_result {
                Ok(result) if result.rows_affected() == 1 => {}
                Ok(_) => {
                    let error = classify_user_precharge(&transaction, principal, amount).await?;
                    return rollback_with_error(transaction, error).await;
                }
                Err(_) => {
                    return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
                }
            }
        }

        let token_result = transaction
            .execute(backend.build(&sql::precharge_token(principal, amount, now)))
            .await;
        match token_result {
            Ok(result) if result.rows_affected() == 1 => {}
            Ok(_) => {
                let error = classify_token_precharge(&transaction, principal, amount).await?;
                return rollback_with_error(transaction, error).await;
            }
            Err(_) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
            }
        }

        if reservation_kind.uses_token_windows()
            && let Err(error) =
                token_windows::reserve(&transaction, &key, &parent, token_state, amount, now).await
        {
            return rollback_with_error(transaction, error).await;
        }

        if !token_state.unlimited_quota {
            let snapshot = transaction
                .execute(backend.build(&sql::snapshot_token_reservation(key, amount, now)))
                .await
                .map_err(|_| QuotaRepositoryError::Query)?;
            if snapshot.rows_affected() != 1 {
                return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
            }
        }

        commit_applied(transaction).await
    }

    async fn existing_precharge(
        &self,
        id: BillingReservationId,
        key: String,
        principal: GatewayPrincipal,
        amount: Quota,
        reservation_kind: QuotaReservationKind,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let result = query_one(
            self.pool.connection(),
            sql::reservation_state(key.clone(), false),
        )
        .await?;
        let state = result
            .as_ref()
            .ok_or(QuotaRepositoryError::Invariant)
            .and_then(ReservationState::try_from_result)?;
        subscription::load_allocation(self.pool.connection(), &key, &state).await?;
        group_windows::load_allocation(self.pool.connection(), &key, &state).await?;
        token_windows::load_allocation(self.pool.connection(), &key, &state).await?;
        if state.matches_precharge(principal, amount, reservation_kind)
            && state.contract_price == contract_price
        {
            if principal.organization_principal().is_some() && self.funding_extension.is_some() {
                let transaction = self
                    .pool
                    .connection()
                    .begin()
                    .await
                    .map_err(|_| QuotaRepositoryError::Query)?;
                let context = funding::context(id, &state, TimeDateTimeWithTimeZone::now_utc())?
                    .ok_or(QuotaRepositoryError::Invariant)?;
                self.funding_extension
                    .as_ref()
                    .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                    .replay(&transaction, &context, principal)
                    .await?;
                transaction
                    .rollback()
                    .await
                    .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
            }
            Ok(QuotaMutationOutcome::Existing(state.status))
        } else {
            Err(QuotaRepositoryError::Conflict)
        }
    }

    async fn settle_inner(
        &self,
        id: BillingReservationId,
        actual: Quota,
        reservation_kind: QuotaReservationKind,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let key = id.persistence_key();
        // 主体预读不进入事务快照；SQLite 随后可先取得写锁，再读取 reservation 最新状态。
        let observed = load_reservation(self.pool.connection(), key.clone(), false)
            .await?
            .ok_or(QuotaRepositoryError::NotFound)?;
        if observed.reservation_kind != reservation_kind {
            return Err(QuotaRepositoryError::Conflict);
        }
        if !reservation_kind.allows_supplement() && actual.units() > observed.reserved_quota {
            return Err(QuotaRepositoryError::ActualExceedsReservation);
        }
        if observed.organization_id.is_some() && self.funding_extension.is_none() {
            let Some(extension) = self.extension.as_ref() else {
                return Err(QuotaRepositoryError::ExtensionUnavailable);
            };
            return extension.settle(id, actual, reservation_kind).await;
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        let backend = transaction.get_database_backend();
        let funding = funding::context(id, &observed, TimeDateTimeWithTimeZone::now_utc())?;
        if let Some(context) = funding.as_ref()
            && let Err(error) = self
                .funding_extension
                .as_ref()
                .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                .lock(&transaction, context)
                .await
        {
            return rollback_with_error(transaction, error).await;
        }

        let (group_state, user_state, token_state) = match lock_subject(
            &transaction,
            observed.group_id,
            observed.user_id,
            observed.token_id,
            observed.organization_id,
        )
        .await
        {
            Ok(state) => state,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        let mut state = match load_reservation(&transaction, key.clone(), true).await {
            Ok(Some(state)) => state,
            Ok(None) => {
                return rollback_with_error(transaction, QuotaRepositoryError::NotFound).await;
            }
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        if !state.matches_immutable_snapshot(&observed) {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        if state.reservation_kind != reservation_kind {
            return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
        }
        if !reservation_kind.allows_supplement() && actual.units() > state.reserved_quota {
            return rollback_with_error(
                transaction,
                QuotaRepositoryError::ActualExceedsReservation,
            )
            .await;
        }
        let allocation = match subscription::load_allocation(&transaction, &key, &state).await {
            Ok(allocation) => allocation,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        let group_window_allocation =
            match group_windows::load_allocation(&transaction, &key, &state).await {
                Ok(allocation) => allocation,
                Err(error) => return rollback_with_error(transaction, error).await,
            };
        let token_window_allocation =
            match token_windows::load_allocation(&transaction, &key, &state).await {
                Ok(allocation) => allocation,
                Err(error) => return rollback_with_error(transaction, error).await,
            };
        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut subscription_settlement = None;

        match state.status {
            QuotaReservationStatus::Reserved => {
                if let Some(current) = allocation.as_ref() {
                    subscription_settlement = match subscription::prepare_settlement(
                        &transaction,
                        &state,
                        current,
                        actual,
                    )
                    .await
                    {
                        Ok(settlement) => Some(settlement),
                        Err(error) => return rollback_with_error(transaction, error).await,
                    };
                }
                let transition = transaction
                    .execute(backend.build(&sql::begin_settlement(key.clone(), actual, now)))
                    .await;
                if !matches!(transition, Ok(ref result) if result.rows_affected() == 1) {
                    let error = if transition.is_err() {
                        QuotaRepositoryError::Query
                    } else {
                        QuotaRepositoryError::Invariant
                    };
                    return rollback_with_error(transaction, error).await;
                }
                if let (Some(current), Some(settlement)) =
                    (allocation.as_ref(), subscription_settlement)
                    && let Err(error) =
                        subscription::persist_settlement(&transaction, current, settlement, now)
                            .await
                {
                    return rollback_with_error(transaction, error).await;
                }
                state = match load_reservation(&transaction, key.clone(), true).await {
                    Ok(Some(state)) => state,
                    Ok(None) => {
                        return rollback_with_error(transaction, QuotaRepositoryError::Invariant)
                            .await;
                    }
                    Err(error) => return rollback_with_error(transaction, error).await,
                };
                if !state.matches_immutable_snapshot(&observed)
                    || state.status != QuotaReservationStatus::SettlementPending
                    || !state.matches_actual(actual)
                {
                    return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
                }
                if let Err(error) = subscription::load_allocation(&transaction, &key, &state).await
                {
                    return rollback_with_error(transaction, error).await;
                }
                if let Err(error) = group_windows::load_allocation(&transaction, &key, &state).await
                {
                    return rollback_with_error(transaction, error).await;
                }
                if let Err(error) = token_windows::load_allocation(&transaction, &key, &state).await
                {
                    return rollback_with_error(transaction, error).await;
                }
            }
            QuotaReservationStatus::SettlementPending if state.matches_actual(actual) => {
                if let Some(current) = allocation.as_ref() {
                    subscription_settlement = match subscription::replay_settlement(
                        &transaction,
                        &state,
                        current,
                        actual,
                    )
                    .await
                    {
                        Ok(settlement) => Some(settlement),
                        Err(error) => return rollback_with_error(transaction, error).await,
                    };
                }
            }
            QuotaReservationStatus::Settled if state.matches_actual(actual) => {
                return rollback_with_outcome(
                    transaction,
                    QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled),
                )
                .await;
            }
            QuotaReservationStatus::SettlementPending
            | QuotaReservationStatus::Settled
            | QuotaReservationStatus::Refunded => {
                return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
            }
        }

        let reserved = quota_from_db(state.reserved_quota)?;
        let token_reserved = quota_from_db(state.token_reserved_quota)?;
        let savepoint = match transaction.begin().await {
            Ok(savepoint) => savepoint,
            Err(_) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
            }
        };
        let adjustment = settle_adjustment(
            &savepoint,
            SettlementAdjustment {
                key: &key,
                state: &state,
                reserved,
                token_reserved,
                actual,
                user_state: &user_state,
                group_state,
                token_state,
                subscription_settlement,
                group_window_allocation: group_window_allocation.as_ref(),
                token_window_allocation: token_window_allocation.as_ref(),
                now,
                funding: funding.as_ref(),
                funding_extension: self.funding_extension.as_deref(),
            },
        )
        .await;

        match adjustment {
            Ok(()) => {
                if savepoint.commit().await.is_err() {
                    return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
                }
                commit_applied(transaction).await
            }
            Err(
                error @ (QuotaRepositoryError::OrganizationQuotaInsufficient
                | QuotaRepositoryError::UserQuotaInsufficient
                | QuotaRepositoryError::TokenQuotaInsufficient
                | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
                | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }),
            ) => {
                savepoint
                    .rollback()
                    .await
                    .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
                if let Some(context) = funding.as_ref()
                    && let Err(error) = self
                        .funding_extension
                        .as_ref()
                        .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                        .pending(&transaction, context, actual)
                        .await
                {
                    return rollback_with_error(transaction, error).await;
                }
                transaction
                    .commit()
                    .await
                    .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
                Err(error)
            }
            Err(error) => {
                savepoint
                    .rollback()
                    .await
                    .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
                rollback_with_error(transaction, error).await
            }
        }
    }

    async fn refund_inner(
        &self,
        id: BillingReservationId,
        reservation_kind: QuotaReservationKind,
    ) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
        let key = id.persistence_key();
        let observed = load_reservation(self.pool.connection(), key.clone(), false)
            .await?
            .ok_or(QuotaRepositoryError::NotFound)?;
        if observed.reservation_kind != reservation_kind {
            return Err(QuotaRepositoryError::Conflict);
        }
        if observed.organization_id.is_some() && self.funding_extension.is_none() {
            let Some(extension) = self.extension.as_ref() else {
                return Err(QuotaRepositoryError::ExtensionUnavailable);
            };
            return extension.refund(id, reservation_kind).await;
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        let backend = transaction.get_database_backend();
        let funding = funding::context(id, &observed, TimeDateTimeWithTimeZone::now_utc())?;
        if let Some(context) = funding.as_ref()
            && let Err(error) = self
                .funding_extension
                .as_ref()
                .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                .lock(&transaction, context)
                .await
        {
            return rollback_with_error(transaction, error).await;
        }

        if let Err(error) = lock_subject(
            &transaction,
            observed.group_id,
            observed.user_id,
            observed.token_id,
            observed.organization_id,
        )
        .await
        {
            return rollback_with_error(transaction, error).await;
        }
        let mut state = match load_reservation(&transaction, key.clone(), true).await {
            Ok(Some(state)) => state,
            Ok(None) => {
                return rollback_with_error(transaction, QuotaRepositoryError::NotFound).await;
            }
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        if !state.matches_immutable_snapshot(&observed) {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        if state.reservation_kind != reservation_kind {
            return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
        }
        let allocation = match subscription::load_allocation(&transaction, &key, &state).await {
            Ok(allocation) => allocation,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        if let Err(error) = group_windows::load_allocation(&transaction, &key, &state).await {
            return rollback_with_error(transaction, error).await;
        }
        if let Err(error) = token_windows::load_allocation(&transaction, &key, &state).await {
            return rollback_with_error(transaction, error).await;
        }
        let now = TimeDateTimeWithTimeZone::now_utc();

        match state.status {
            QuotaReservationStatus::Reserved => {
                if let Some(current) = allocation.as_ref()
                    && let Err(error) =
                        subscription::lock_for_refund(&transaction, &state, current).await
                {
                    return rollback_with_error(transaction, error).await;
                }
                let transition = transaction
                    .execute(backend.build(&sql::begin_refund(key.clone(), now)))
                    .await;
                if !matches!(transition, Ok(ref result) if result.rows_affected() == 1) {
                    let error = if transition.is_err() {
                        QuotaRepositoryError::Query
                    } else {
                        QuotaRepositoryError::Invariant
                    };
                    return rollback_with_error(transaction, error).await;
                }
                state = match load_reservation(&transaction, key, true).await {
                    Ok(Some(state)) => state,
                    Ok(None) => {
                        return rollback_with_error(transaction, QuotaRepositoryError::Invariant)
                            .await;
                    }
                    Err(error) => return rollback_with_error(transaction, error).await,
                };
                if !state.matches_immutable_snapshot(&observed)
                    || state.status != QuotaReservationStatus::Refunded
                {
                    return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
                }
            }
            QuotaReservationStatus::Refunded => {
                return rollback_with_outcome(
                    transaction,
                    QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded),
                )
                .await;
            }
            QuotaReservationStatus::SettlementPending | QuotaReservationStatus::Settled => {
                return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
            }
        }

        let reserved = quota_from_db(state.reserved_quota)?;
        let token_reserved = quota_from_db(state.token_reserved_quota)?;
        if let Some(context) = funding.as_ref() {
            if let Err(error) = self
                .funding_extension
                .as_ref()
                .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
                .refund(&transaction, context)
                .await
            {
                return rollback_with_error(transaction, error).await;
            }
        } else if allocation.is_none() {
            let user_result = transaction
                .execute(backend.build(&sql::refund_user(state.user_id, reserved, now)))
                .await;
            if !matches!(user_result, Ok(ref result) if result.rows_affected() == 1) {
                let error = if user_result.is_err() {
                    QuotaRepositoryError::Query
                } else {
                    QuotaRepositoryError::Invariant
                };
                return rollback_with_error(transaction, error).await;
            }
        }
        let token_result = transaction
            .execute(backend.build(&sql::refund_token(
                state.token_id,
                state.user_id,
                token_reserved,
                now,
            )))
            .await;
        if !matches!(token_result, Ok(ref result) if result.rows_affected() == 1) {
            let error = if token_result.is_err() {
                QuotaRepositoryError::Query
            } else {
                QuotaRepositoryError::Invariant
            };
            return rollback_with_error(transaction, error).await;
        }
        commit_applied(transaction).await
    }
}

impl fmt::Debug for QuotaRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QuotaRepository")
            .finish_non_exhaustive()
    }
}

/// 一次保存点内完成结算所需的不可变事务快照。
struct SettlementAdjustment<'a> {
    key: &'a str,
    state: &'a ReservationState,
    reserved: Quota,
    token_reserved: Quota,
    actual: Quota,
    user_state: &'a UserQuotaState,
    group_state: GroupQuotaState,
    token_state: TokenQuotaState,
    subscription_settlement: Option<subscription::SubscriptionSettlement>,
    group_window_allocation: Option<&'a group_windows::GroupWindowAllocation>,
    token_window_allocation: Option<&'a token_windows::TokenWindowAllocation>,
    now: TimeDateTimeWithTimeZone,
    funding: Option<&'a QuotaFundingContext>,
    funding_extension: Option<&'a dyn QuotaFundingExtension>,
}

async fn settle_adjustment(
    savepoint: &DatabaseTransaction,
    adjustment: SettlementAdjustment<'_>,
) -> Result<(), QuotaRepositoryError> {
    let SettlementAdjustment {
        key,
        state,
        reserved,
        token_reserved,
        actual,
        user_state,
        group_state,
        token_state,
        subscription_settlement,
        group_window_allocation,
        token_window_allocation,
        now,
        funding,
        funding_extension,
    } = adjustment;
    let backend = savepoint.get_database_backend();
    group_windows::apply_settlement(
        savepoint,
        state,
        group_window_allocation,
        group_state,
        actual,
        now,
    )
    .await?;
    if let Some(context) = funding {
        funding_extension
            .ok_or(QuotaRepositoryError::ExtensionUnavailable)?
            .settle(savepoint, context, actual)
            .await?;
    } else if let Some(settlement) = subscription_settlement {
        subscription::apply_settlement(savepoint, state, user_state, actual, settlement, now)
            .await?;
    } else {
        let user_result = savepoint
            .execute(backend.build(&sql::settle_user(state.user_id, reserved, actual, now)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if user_result.rows_affected() != 1 {
            return Err(classify_user_settlement(savepoint, state, reserved, actual).await?);
        }
    }

    let token_result = savepoint
        .execute(backend.build(&sql::settle_token(
            state.token_id,
            state.user_id,
            token_reserved,
            reserved,
            actual,
            now,
        )))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if token_result.rows_affected() != 1 {
        return Err(
            classify_token_settlement(savepoint, state, token_reserved, reserved, actual).await?,
        );
    }

    token_windows::apply_settlement(
        savepoint,
        state,
        token_window_allocation,
        token_state,
        actual,
        now,
    )
    .await?;

    let finalized = savepoint
        .execute(backend.build(&sql::finalize_settlement(key.to_owned(), actual, now)))
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    if finalized.rows_affected() != 1 {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

async fn classify_user_precharge(
    transaction: &DatabaseTransaction,
    principal: GatewayPrincipal,
    amount: Quota,
) -> Result<QuotaRepositoryError, QuotaRepositoryError> {
    let state = load_user(transaction, principal.user_id().get(), true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)?;
    if state.quota < amount.units() {
        Ok(QuotaRepositoryError::UserQuotaInsufficient)
    } else {
        Ok(QuotaRepositoryError::Invariant)
    }
}

async fn classify_token_precharge(
    transaction: &DatabaseTransaction,
    principal: GatewayPrincipal,
    amount: Quota,
) -> Result<QuotaRepositoryError, QuotaRepositoryError> {
    let state = load_token(transaction, principal.token_id().get(), true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)?;
    if state.user_id != principal.user_id().get() {
        return Ok(QuotaRepositoryError::Invariant);
    }
    if !state.unlimited_quota && state.remain_quota < amount.units() {
        Ok(QuotaRepositoryError::TokenQuotaInsufficient)
    } else {
        Ok(QuotaRepositoryError::Invariant)
    }
}

async fn classify_user_settlement(
    transaction: &DatabaseTransaction,
    state: &ReservationState,
    reserved: Quota,
    actual: Quota,
) -> Result<QuotaRepositoryError, QuotaRepositoryError> {
    let user = load_user(transaction, state.user_id, true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)?;
    let extra = actual.units().checked_sub(reserved.units()).unwrap_or(0);
    if user.frozen_quota < reserved.units()
        || user.used_quota > i64::MAX - actual.units()
        || user.request_count == i64::MAX
    {
        return Ok(QuotaRepositoryError::Invariant);
    }
    if extra > 0 && user.quota < extra {
        Ok(QuotaRepositoryError::UserQuotaInsufficient)
    } else {
        Ok(QuotaRepositoryError::Invariant)
    }
}

async fn classify_token_settlement(
    transaction: &DatabaseTransaction,
    state: &ReservationState,
    token_reserved: Quota,
    reserved: Quota,
    actual: Quota,
) -> Result<QuotaRepositoryError, QuotaRepositoryError> {
    let token = load_token(transaction, state.token_id, true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)?;
    if token.user_id != state.user_id || token.used_quota > i64::MAX - actual.units() {
        return Ok(QuotaRepositoryError::Invariant);
    }
    let extra = actual.units().checked_sub(reserved.units()).unwrap_or(0);
    if !token_reserved.is_zero() && extra > 0 && token.remain_quota < extra {
        Ok(QuotaRepositoryError::TokenQuotaInsufficient)
    } else {
        Ok(QuotaRepositoryError::Invariant)
    }
}

/// 按分组、用户、令牌的固定顺序取得父行写锁，并核对令牌归属。
async fn lock_subject(
    transaction: &DatabaseTransaction,
    group_id: i64,
    user_id: i64,
    token_id: i64,
    organization_id: Option<i64>,
) -> Result<(GroupQuotaState, UserQuotaState, TokenQuotaState), QuotaRepositoryError> {
    let group = lock_group(transaction, group_id).await?;
    let user = lock_user(transaction, user_id).await?;
    let token = lock_token(transaction, token_id).await?;
    if token.user_id != user_id
        || (organization_id.is_none()
            && (token.organization_id.is_some()
                || token.organization_membership_id.is_some()
                || token.organization_team_id.is_some()))
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok((group, user, token))
}

/// 核对令牌持久化主体与鉴权时固化主体完全一致，禁止个人和企业路径互相回退。
fn token_matches_principal(token: &TokenQuotaState, principal: GatewayPrincipal) -> bool {
    if token.user_id != principal.user_id().get() {
        return false;
    }
    match principal.organization_principal() {
        Some(organization) => {
            token.organization_id == Some(organization.organization_id().get())
                && token.organization_membership_id == Some(organization.membership_id().get())
                && token.organization_team_id == organization.team_id().map(|team| team.get())
        }
        None => {
            token.organization_id.is_none()
                && token.organization_membership_id.is_none()
                && token.organization_team_id.is_none()
        }
    }
}

async fn lock_group(
    transaction: &DatabaseTransaction,
    group_id: i64,
) -> Result<GroupQuotaState, QuotaRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = transaction
            .execute(DbBackend::Sqlite.build(&sql::sqlite_lock_group(group_id)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if result.rows_affected() != 1 {
            return Err(QuotaRepositoryError::Invariant);
        }
    }
    query_one(transaction, sql::group_state(group_id, true))
        .await?
        .as_ref()
        .ok_or(QuotaRepositoryError::Invariant)
        .and_then(GroupQuotaState::try_from_result)
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: i64,
) -> Result<UserQuotaState, QuotaRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = transaction
            .execute(DbBackend::Sqlite.build(&sql::sqlite_lock_user(user_id)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if result.rows_affected() != 1 {
            return Err(QuotaRepositoryError::Invariant);
        }
    }
    load_user(transaction, user_id, true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)
}

async fn lock_token(
    transaction: &DatabaseTransaction,
    token_id: i64,
) -> Result<TokenQuotaState, QuotaRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = transaction
            .execute(DbBackend::Sqlite.build(&sql::sqlite_lock_token(token_id)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if result.rows_affected() != 1 {
            return Err(QuotaRepositoryError::Invariant);
        }
    }
    load_token(transaction, token_id, true)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)
}

async fn load_reservation<C>(
    connection: &C,
    key: String,
    lock: bool,
) -> Result<Option<ReservationState>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    query_one(connection, sql::reservation_state(key, lock))
        .await?
        .as_ref()
        .map(ReservationState::try_from_result)
        .transpose()
}

async fn load_user<C>(
    connection: &C,
    user_id: i64,
    lock: bool,
) -> Result<Option<UserQuotaState>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    query_one(connection, sql::user_state(user_id, lock))
        .await?
        .as_ref()
        .map(UserQuotaState::try_from_result)
        .transpose()
}

async fn load_token<C>(
    connection: &C,
    token_id: i64,
    lock: bool,
) -> Result<Option<TokenQuotaState>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    query_one(connection, sql::token_state(token_id, lock))
        .await?
        .as_ref()
        .map(TokenQuotaState::try_from_result)
        .transpose()
}

async fn query_one<C>(
    connection: &C,
    statement: SelectStatement,
) -> Result<Option<QueryResult>, QuotaRepositoryError>
where
    C: ConnectionTrait,
{
    connection
        .query_one(connection.get_database_backend().build(&statement))
        .await
        .map_err(|_| QuotaRepositoryError::Query)
}

async fn commit_applied(
    transaction: DatabaseTransaction,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
    Ok(QuotaMutationOutcome::Applied)
}

async fn rollback_with_error<T>(
    transaction: DatabaseTransaction,
    error: QuotaRepositoryError,
) -> Result<T, QuotaRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
    Err(error)
}

async fn rollback_with_outcome(
    transaction: DatabaseTransaction,
    outcome: QuotaMutationOutcome,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    Ok(outcome)
}

fn reservation_times(
    reservation_ttl: Duration,
) -> Result<(TimeDateTimeWithTimeZone, TimeDateTimeWithTimeZone), QuotaRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let ttl_nanoseconds = i128::try_from(reservation_ttl.as_nanos())
        .map_err(|_| QuotaRepositoryError::InvalidConfiguration)?;
    if reservation_ttl.as_secs() == 0 {
        return Err(QuotaRepositoryError::InvalidConfiguration);
    }
    let expires_at = now
        .unix_timestamp_nanos()
        .checked_add(ttl_nanoseconds)
        .and_then(|timestamp| TimeDateTimeWithTimeZone::from_unix_timestamp_nanos(timestamp).ok())
        .ok_or(QuotaRepositoryError::InvalidConfiguration)?;
    Ok((now, expires_at))
}

fn quota_from_db(units: i64) -> Result<Quota, QuotaRepositoryError> {
    Quota::new(units).map_err(|_| QuotaRepositoryError::Invariant)
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// 只记录闭合内部分类，避免主体、额度与底层诊断进入日志。
fn record_internal_error(error: QuotaRepositoryError) -> QuotaRepositoryError {
    let error_kind = match error {
        QuotaRepositoryError::InvalidConfiguration => "quota_configuration",
        QuotaRepositoryError::Query => "quota_query",
        QuotaRepositoryError::OutcomeUnknown => "quota_outcome_unknown",
        QuotaRepositoryError::Invariant => "quota_invariant",
        QuotaRepositoryError::ExtensionUnavailable => "quota_extension_unavailable",
        QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::NotFound
        | QuotaRepositoryError::Conflict
        | QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::ActualExceedsReservation => return error,
    };
    tracing::error!(
        target: "af_db::quota",
        error_kind,
        "额度仓储发生内部错误"
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsecond_reservation_ttl_is_rejected() {
        assert_eq!(
            reservation_times(Duration::from_millis(999)).unwrap_err(),
            QuotaRepositoryError::InvalidConfiguration
        );
    }

    #[test]
    fn reservation_ttl_preserves_exact_duration() {
        let (created_at, expires_at) = reservation_times(Duration::from_secs(1)).unwrap();

        assert_eq!(
            expires_at.unix_timestamp_nanos() - created_at.unix_timestamp_nanos(),
            1_000_000_000
        );
    }
}
