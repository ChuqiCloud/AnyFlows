use std::{fmt, sync::Arc, time::Duration};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use af_domain::{
    OrganizationId, Quota, QuotaDelta, QuotaError, TopupOrderId, TopupOrderStatus,
    TopupPaymentEventType, TopupRequestId, UserId, WalletEventId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend,
    EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, WalletLedgerEntryType,
    entity::{
        SensitiveString, WalletLedgerKey, topup_orders, topup_payment_events, users,
        wallet_ledger_entries,
    },
};

use super::types::{
    TopupOrderCreate, TopupOrderCreateOutcome, TopupOrderRecord, TopupOrderSubmission,
    TopupOrderSubmitOutcome, TopupPaymentEventOutcome, TopupPaymentEventRejection,
    TopupPaymentEventWrite, TopupRepositoryConfigError, TopupRepositoryError, valid_currency,
    valid_payment_method, valid_provider, valid_provider_value,
};
use super::{TopupCreditOutcome, TopupExtension};

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 原子维护充值订单、支付事件和到账钱包账本的数据库仓储。
#[derive(Clone)]
pub struct TopupRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    extension: Option<Arc<dyn TopupExtension>>,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl TopupRepository {
    /// 使用共享连接池和单次写操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, TopupRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(TopupRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            extension: None,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Register distribution-owned crediting for non-personal wallet targets.
    #[must_use]
    pub fn with_extension(mut self, extension: Arc<dyn TopupExtension>) -> Self {
        self.extension = Some(extension);
        self
    }

    /// 幂等创建本地充值订单；目标用户必须存在且未软删除。
    pub async fn create_order(
        &self,
        write: TopupOrderCreate,
    ) -> Result<TopupOrderCreateOutcome, TopupRepositoryError> {
        let operation = self
            .create_order_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(TopupRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        self.maybe_inject_unknown(&outcome, is_created_order)?;
        Ok(outcome)
    }

    /// 按稳定订单标识读取充值订单，供支付 webhook 路由先判定订单类型。
    pub async fn get_order(
        &self,
        order_id: TopupOrderId,
    ) -> Result<Option<TopupOrderRecord>, TopupRepositoryError> {
        let operation = load_order_by_key(self.pool.connection(), order_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(TopupRepositoryError::Timeout)),
        }
    }

    /// 使用预期版本把 Created 订单绑定为 Pending；相同绑定可以安全重放。
    pub async fn submit_order(
        &self,
        write: TopupOrderSubmission,
    ) -> Result<TopupOrderSubmitOutcome, TopupRepositoryError> {
        let operation = self
            .submit_order_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(TopupRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        self.maybe_inject_unknown(&outcome, is_applied_submission)?;
        Ok(outcome)
    }

    /// 审计并处理一个已经由 Provider 适配器完成验签的支付事件。
    pub async fn accept_verified_event(
        &self,
        write: TopupPaymentEventWrite,
    ) -> Result<TopupPaymentEventOutcome, TopupRepositoryError> {
        let operation = self
            .accept_verified_event_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(TopupRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        self.maybe_inject_unknown(&outcome, is_applied_event)?;
        Ok(outcome)
    }

    /// 仅供回归测试模拟事务已提交但调用方没有收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn maybe_inject_unknown<T>(
        &self,
        outcome: &T,
        committed: fn(&T) -> bool,
    ) -> Result<(), TopupRepositoryError> {
        if committed(outcome)
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(TopupRepositoryError::OutcomeUnknown));
        }
        Ok(())
    }

    async fn create_order_inner(
        &self,
        write: &TopupOrderCreate,
    ) -> Result<TopupOrderCreateOutcome, TopupRepositoryError> {
        if write.organization_id.is_some() && self.extension.is_none() {
            return Err(internal(TopupRepositoryError::UnsupportedTarget));
        }
        if let Some(existing) = load_create_collision(self.pool.connection(), write).await? {
            return classify_create_collision(existing, write);
        }

        let transaction = begin(&self.pool).await?;
        let result =
            create_order_in_transaction(&transaction, write, self.extension.as_deref()).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                let existing = load_create_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(TopupRepositoryError::Invariant)?;
                classify_create_collision(existing, write)
            }
            Err(TransactionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }

    async fn submit_order_inner(
        &self,
        write: &TopupOrderSubmission,
    ) -> Result<TopupOrderSubmitOutcome, TopupRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let result = submit_order_in_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(error) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }

    async fn accept_verified_event_inner(
        &self,
        write: &TopupPaymentEventWrite,
    ) -> Result<TopupPaymentEventOutcome, TopupRepositoryError> {
        let Some(preloaded) = load_order_by_key(self.pool.connection(), write.order_id).await?
        else {
            return Ok(TopupPaymentEventOutcome::NotFound);
        };
        if let Some(existing) = load_event_collision(self.pool.connection(), write).await? {
            return classify_event_collision(existing, &preloaded, write);
        }

        let transaction = begin(&self.pool).await?;
        let result = accept_event_in_transaction(
            &transaction,
            preloaded.user_id,
            write,
            self.extension.as_deref(),
        )
        .await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                let order = load_order_by_key(self.pool.connection(), write.order_id)
                    .await?
                    .ok_or(TopupRepositoryError::Invariant)?;
                let existing = load_event_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(TopupRepositoryError::Invariant)?;
                classify_event_collision(existing, &order, write)
            }
            Err(TransactionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for TopupRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TopupRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

enum TransactionWriteError {
    UniqueConflict,
    Repository(TopupRepositoryError),
}

impl From<TopupRepositoryError> for TransactionWriteError {
    fn from(error: TopupRepositoryError) -> Self {
        Self::Repository(error)
    }
}

async fn create_order_in_transaction(
    transaction: &DatabaseTransaction,
    write: &TopupOrderCreate,
    extension: Option<&dyn TopupExtension>,
) -> Result<TopupOrderCreateOutcome, TransactionWriteError> {
    if lock_user(transaction, write.user_id, true).await?.is_none() {
        return Ok(TopupOrderCreateOutcome::NotFound);
    }
    if let Some(organization_id) = write.organization_id {
        let extension = extension.ok_or(TopupRepositoryError::UnsupportedTarget)?;
        if !extension
            .validate_target(transaction, organization_id)
            .await?
        {
            return Ok(TopupOrderCreateOutcome::NotFound);
        }
    }
    if let Some(existing) = load_create_collision(transaction, write).await? {
        return classify_create_collision(existing, write).map_err(Into::into);
    }

    let created_at = to_database_time(write.created_at)?;
    let inserted = topup_orders::ActiveModel {
        order_key: Set(write.order_id.persistence_key()),
        user_id: Set(write.user_id.get()),
        organization_id: Set(write.organization_id.map(OrganizationId::get)),
        provider: Set(write.provider.clone()),
        payment_method: Set(Some(write.payment_method.clone())),
        provider_order_id: Set(None),
        trade_no: Set(None),
        status: Set(TopupOrderStatus::Created.code()),
        amount_minor: Set(write.amount_minor),
        currency: Set(write.currency.clone()),
        quota_amount: Set(write.quota_amount.units()),
        idempotency_key: Set(write.request_id.persistence_key()),
        version: Set(1),
        expires_at: Set(None),
        paid_at: Set(None),
        closed_at: Set(None),
        created_at: Set(created_at),
        updated_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    match inserted {
        Ok(model) => Ok(TopupOrderCreateOutcome::Created(order_record(model)?)),
        Err(error) if is_order_unique_conflict(&error) => {
            Err(TransactionWriteError::UniqueConflict)
        }
        Err(_) => Err(TopupRepositoryError::Query.into()),
    }
}

async fn submit_order_in_transaction(
    transaction: &DatabaseTransaction,
    write: &TopupOrderSubmission,
) -> Result<TopupOrderSubmitOutcome, TopupRepositoryError> {
    let Some(model) = lock_order(transaction, write.order_id).await? else {
        return Ok(TopupOrderSubmitOutcome::NotFound);
    };
    let current = order_record(model.clone())?;
    if current.matches_submission(write) {
        return Ok(TopupOrderSubmitOutcome::Existing(current));
    }
    if current.status != TopupOrderStatus::Created
        || model.version != write.expected_version
        || write.submitted_at < current.created_at
    {
        return Err(TopupRepositoryError::Conflict);
    }

    let next_version = model
        .version
        .checked_add(1)
        .ok_or(TopupRepositoryError::Invariant)?;
    let submitted_at = to_database_time(write.submitted_at)?;
    let expires_at = to_database_time(write.expires_at)?;
    let update = topup_orders::Entity::update_many()
        .filter(topup_orders::Column::Id.eq(model.id))
        .filter(topup_orders::Column::Status.eq(TopupOrderStatus::Created.code()))
        .filter(topup_orders::Column::Version.eq(write.expected_version))
        .col_expr(
            topup_orders::Column::ProviderOrderId,
            Expr::value(Some(SensitiveString::from(write.provider_order_id.clone()))),
        )
        .col_expr(
            topup_orders::Column::Status,
            Expr::value(TopupOrderStatus::Pending.code()),
        )
        .col_expr(topup_orders::Column::ExpiresAt, Expr::value(expires_at))
        .col_expr(topup_orders::Column::Version, Expr::value(next_version))
        .col_expr(topup_orders::Column::UpdatedAt, Expr::value(submitted_at))
        .exec(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(TopupRepositoryError::Conflict);
    }
    let updated = load_order_by_database_id(transaction, model.id)
        .await?
        .ok_or(TopupRepositoryError::Invariant)?;
    Ok(TopupOrderSubmitOutcome::Applied(updated))
}

async fn accept_event_in_transaction(
    transaction: &DatabaseTransaction,
    preloaded_user_id: UserId,
    write: &TopupPaymentEventWrite,
    extension: Option<&dyn TopupExtension>,
) -> Result<TopupPaymentEventOutcome, TransactionWriteError> {
    let user = lock_user(transaction, preloaded_user_id, false)
        .await?
        .ok_or(TopupRepositoryError::Invariant)?;
    let Some(order_model) = lock_order(transaction, write.order_id).await? else {
        return Ok(TopupPaymentEventOutcome::NotFound);
    };
    let order = order_record(order_model.clone())?;
    if order.user_id != preloaded_user_id {
        return Err(TopupRepositoryError::Invariant.into());
    }
    if let Some(existing) = load_event_collision(transaction, write).await? {
        return classify_event_collision(existing, &order, write).map_err(Into::into);
    }

    let received_at = to_database_time(write.received_at)?;
    let inserted = topup_payment_events::ActiveModel {
        event_key: Set(write.event_id.persistence_key()),
        order_id: Set(order.database_id),
        provider: Set(write.provider.clone()),
        provider_event_id: Set(SensitiveString::from(write.provider_event_id.clone())),
        trade_no: Set(write.trade_no.clone().map(SensitiveString::from)),
        amount_minor: Set(Some(write.amount_minor)),
        currency: Set(Some(write.currency.clone())),
        payment_method: Set(Some(write.payment_method.clone())),
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
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) if is_event_unique_conflict(&error) => {
            return Err(TransactionWriteError::UniqueConflict);
        }
        Err(_) => return Err(TopupRepositoryError::Query.into()),
    };

    if write.provider != order.provider {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::ProviderMismatch,
        ));
    }
    if write.amount_minor as u64 != order.amount_minor {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::AmountMismatch,
        ));
    }
    if write.currency != order.currency {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::CurrencyMismatch,
        ));
    }
    if order.payment_method.as_deref() != Some(write.payment_method.as_str()) {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::PaymentMethodMismatch,
        ));
    }
    if write.received_at < order.created_at {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::TimingConflict,
        ));
    }
    if order
        .trade_no
        .as_deref()
        .zip(write.trade_no.as_deref())
        .is_some_and(|(existing, incoming)| existing != incoming)
    {
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::TradeNumberConflict,
        ));
    }

    if !order.status.is_open() {
        if terminal_event_matches(&order, write) {
            mark_event_processed_at(transaction, inserted.id, processing_time(received_at)).await?;
            return Ok(TopupPaymentEventOutcome::Acknowledged(order));
        }
        return Ok(unprocessed(
            order,
            TopupPaymentEventRejection::TerminalConflict,
        ));
    }

    let processed_at = processing_time(received_at);
    match write.event_type {
        TopupPaymentEventType::Succeeded => {
            let delta = QuotaDelta::new(order.quota_amount.units())
                .map_err(|_| TopupRepositoryError::Invariant)?;
            if let Some(organization_id) = order.organization_id {
                let extension = extension.ok_or(TopupRepositoryError::UnsupportedTarget)?;
                let outcome = extension
                    .credit(
                        transaction,
                        organization_id,
                        WalletEventId::new(write.event_id.bytes())
                            .map_err(|_| TopupRepositoryError::Invariant)?,
                        order.user_id,
                        order.quota_amount,
                        received_at,
                    )
                    .await?;
                match outcome {
                    TopupCreditOutcome::Overflow => {
                        return Ok(unprocessed(
                            order,
                            TopupPaymentEventRejection::CreditOverflow,
                        ));
                    }
                    TopupCreditOutcome::Credited => {}
                }
            } else {
                let balance_before =
                    Quota::new(user.quota).map_err(|_| TopupRepositoryError::Invariant)?;
                let balance_after = match balance_before.checked_apply(delta) {
                    Ok(balance) => balance,
                    Err(QuotaError::Overflow) => {
                        return Ok(unprocessed(
                            order,
                            TopupPaymentEventRejection::CreditOverflow,
                        ));
                    }
                    Err(_) => return Err(TopupRepositoryError::Invariant.into()),
                };
                credit_wallet(
                    transaction,
                    &user,
                    write,
                    balance_before,
                    balance_after,
                    received_at,
                    processed_at,
                )
                .await?;
            }
        }
        TopupPaymentEventType::Failed | TopupPaymentEventType::Expired => {}
    }

    transition_order(transaction, &order_model, write, received_at, processed_at).await?;
    mark_event_processed_at(transaction, inserted.id, processed_at).await?;
    let updated = load_order_by_database_id(transaction, order.database_id)
        .await?
        .ok_or(TopupRepositoryError::Invariant)?;
    Ok(TopupPaymentEventOutcome::Applied(updated))
}

async fn credit_wallet(
    transaction: &DatabaseTransaction,
    user: &users::Model,
    write: &TopupPaymentEventWrite,
    balance_before: Quota,
    balance_after: Quota,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), TopupRepositoryError> {
    let user_update = users::Entity::update_many()
        .filter(users::Column::Id.eq(user.id))
        .filter(users::Column::Quota.eq(balance_before.units()))
        .col_expr(users::Column::Quota, Expr::value(balance_after.units()))
        .col_expr(users::Column::UpdatedAt, Expr::value(updated_at))
        .exec(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if user_update.rows_affected != 1 {
        return Err(TopupRepositoryError::Invariant);
    }

    wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse(&write.event_id.persistence_key())
            .map_err(|_| TopupRepositoryError::Invariant)?),
        user_id: Set(user.id),
        actor_user_id: Set(None),
        entry_type: Set(WalletLedgerEntryType::Topup as i16),
        quota_delta: Set(balance_after.units() - balance_before.units()),
        balance_before: Set(balance_before.units()),
        balance_after: Set(balance_after.units()),
        reason: Set(None),
        created_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| TopupRepositoryError::Query)?;
    Ok(())
}

async fn transition_order(
    transaction: &DatabaseTransaction,
    order: &topup_orders::Model,
    write: &TopupPaymentEventWrite,
    received_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
) -> Result<(), TopupRepositoryError> {
    let next_version = order
        .version
        .checked_add(1)
        .ok_or(TopupRepositoryError::Invariant)?;
    let target = write.event_type.target_status();
    let paid_at = (target == TopupOrderStatus::Paid).then_some(received_at);
    let closed_at = (target != TopupOrderStatus::Paid).then_some(received_at);
    let update = topup_orders::Entity::update_many()
        .filter(topup_orders::Column::Id.eq(order.id))
        .filter(topup_orders::Column::Version.eq(order.version))
        .filter(topup_orders::Column::Status.is_in([
            TopupOrderStatus::Created.code(),
            TopupOrderStatus::Pending.code(),
        ]))
        .col_expr(topup_orders::Column::Status, Expr::value(target.code()))
        .col_expr(
            topup_orders::Column::TradeNo,
            Expr::value(write.trade_no.clone().map(SensitiveString::from)),
        )
        .col_expr(topup_orders::Column::Version, Expr::value(next_version))
        .col_expr(topup_orders::Column::PaidAt, Expr::value(paid_at))
        .col_expr(topup_orders::Column::ClosedAt, Expr::value(closed_at))
        .col_expr(topup_orders::Column::UpdatedAt, Expr::value(updated_at))
        .exec(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(TopupRepositoryError::Conflict);
    }
    Ok(())
}

async fn mark_event_processed_at(
    transaction: &DatabaseTransaction,
    event_id: i64,
    processed_at: TimeDateTimeWithTimeZone,
) -> Result<(), TopupRepositoryError> {
    let update = topup_payment_events::Entity::update_many()
        .filter(topup_payment_events::Column::Id.eq(event_id))
        .filter(topup_payment_events::Column::ProcessedAt.is_null())
        .col_expr(
            topup_payment_events::Column::ProcessedAt,
            Expr::value(processed_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(TopupRepositoryError::Invariant);
    }
    Ok(())
}

fn processing_time(received_at: TimeDateTimeWithTimeZone) -> TimeDateTimeWithTimeZone {
    let now = TimeDateTimeWithTimeZone::now_utc();
    // 跨主机时钟偏差或可重复测试时间都不能让处理时间早于受信接收时间。
    if now < received_at { received_at } else { now }
}

fn unprocessed(
    order: TopupOrderRecord,
    reason: TopupPaymentEventRejection,
) -> TopupPaymentEventOutcome {
    TopupPaymentEventOutcome::RecordedUnprocessed { order, reason }
}

fn terminal_event_matches(order: &TopupOrderRecord, write: &TopupPaymentEventWrite) -> bool {
    order.status == write.event_type.target_status()
        && (order.status != TopupOrderStatus::Paid
            || order.trade_no.as_deref() == write.trade_no.as_deref())
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    require_active: bool,
) -> Result<Option<users::Model>, TopupRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，用恒等更新在读取前取得写锁。
        let mut update = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into());
        if require_active {
            update = update.filter(users::Column::DeletedAt.is_null());
        }
        let result = update
            .exec(transaction)
            .await
            .map_err(|_| TopupRepositoryError::Query)?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        if result.rows_affected != 1 {
            return Err(TopupRepositoryError::Invariant);
        }
    }
    let mut query = users::Entity::find_by_id(user_id.get());
    if require_active {
        query = query.filter(users::Column::DeletedAt.is_null());
    }
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)
}

async fn lock_order(
    transaction: &DatabaseTransaction,
    order_id: TopupOrderId,
) -> Result<Option<topup_orders::Model>, TopupRepositoryError> {
    let key = order_id.persistence_key();
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // 与用户锁顺序一致，先取得数据库写锁再读取订单状态。
        let result = topup_orders::Entity::update_many()
            .filter(topup_orders::Column::OrderKey.eq(key.clone()))
            .col_expr(
                topup_orders::Column::Version,
                Expr::col(topup_orders::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| TopupRepositoryError::Query)?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        if result.rows_affected != 1 {
            return Err(TopupRepositoryError::Invariant);
        }
    }
    let mut query = topup_orders::Entity::find().filter(topup_orders::Column::OrderKey.eq(key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| TopupRepositoryError::Query)
}

async fn load_order_by_key<C>(
    connection: &C,
    order_id: TopupOrderId,
) -> Result<Option<TopupOrderRecord>, TopupRepositoryError>
where
    C: ConnectionTrait,
{
    topup_orders::Entity::find()
        .filter(topup_orders::Column::OrderKey.eq(order_id.persistence_key()))
        .one(connection)
        .await
        .map_err(|_| TopupRepositoryError::Query)?
        .map(order_record)
        .transpose()
}

async fn load_order_by_database_id<C>(
    connection: &C,
    id: i64,
) -> Result<Option<TopupOrderRecord>, TopupRepositoryError>
where
    C: ConnectionTrait,
{
    topup_orders::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(|_| TopupRepositoryError::Query)?
        .map(order_record)
        .transpose()
}

async fn load_create_collision<C>(
    connection: &C,
    write: &TopupOrderCreate,
) -> Result<Option<TopupOrderRecord>, TopupRepositoryError>
where
    C: ConnectionTrait,
{
    let models = topup_orders::Entity::find()
        .filter(
            Condition::any()
                .add(topup_orders::Column::OrderKey.eq(write.order_id.persistence_key()))
                .add(topup_orders::Column::IdempotencyKey.eq(write.request_id.persistence_key())),
        )
        .all(connection)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if models.len() > 1 {
        return Err(TopupRepositoryError::Conflict);
    }
    models.into_iter().next().map(order_record).transpose()
}

fn classify_create_collision(
    existing: TopupOrderRecord,
    write: &TopupOrderCreate,
) -> Result<TopupOrderCreateOutcome, TopupRepositoryError> {
    if existing.matches_create(write) {
        Ok(TopupOrderCreateOutcome::Existing(existing))
    } else {
        Err(TopupRepositoryError::Conflict)
    }
}

struct StoredPaymentEvent {
    order_database_id: i64,
    provider: String,
    provider_event_id: String,
    trade_no: Option<String>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    payment_method: Option<String>,
    event_type: TopupPaymentEventType,
    signature_key_fingerprint: String,
    payload_sha256: String,
}

impl StoredPaymentEvent {
    fn try_from_model(model: topup_payment_events::Model) -> Result<Self, TopupRepositoryError> {
        if model.id <= 0 || model.order_id <= 0 {
            return Err(TopupRepositoryError::Invariant);
        }
        Ok(Self {
            order_database_id: model.order_id,
            provider: model.provider,
            provider_event_id: model.provider_event_id.as_str().to_owned(),
            trade_no: model.trade_no.map(|value| value.as_str().to_owned()),
            amount_minor: model.amount_minor,
            currency: model.currency,
            payment_method: model.payment_method,
            event_type: TopupPaymentEventType::try_from(model.event_type)
                .map_err(|_| TopupRepositoryError::Invariant)?,
            signature_key_fingerprint: model.signature_key_fingerprint.as_str().to_owned(),
            payload_sha256: model.payload_sha256.as_str().to_owned(),
        })
    }

    fn matches(&self, order: &TopupOrderRecord, write: &TopupPaymentEventWrite) -> bool {
        self.order_database_id == order.database_id
            && self.provider == write.provider
            && self.provider_event_id == write.provider_event_id
            && self.trade_no == write.trade_no
            && self.amount_minor == Some(write.amount_minor)
            && self.currency.as_deref() == Some(write.currency.as_str())
            && self.payment_method.as_deref() == Some(write.payment_method.as_str())
            && self.event_type == write.event_type
            && self.signature_key_fingerprint == hex_digest(write.signature_key_fingerprint)
            && self.payload_sha256 == hex_digest(write.payload_sha256)
    }
}

async fn load_event_collision<C>(
    connection: &C,
    write: &TopupPaymentEventWrite,
) -> Result<Option<StoredPaymentEvent>, TopupRepositoryError>
where
    C: ConnectionTrait,
{
    let models = topup_payment_events::Entity::find()
        .filter(
            Condition::any()
                .add(topup_payment_events::Column::EventKey.eq(write.event_id.persistence_key()))
                .add(
                    Condition::all()
                        .add(topup_payment_events::Column::Provider.eq(write.provider.clone()))
                        .add(
                            topup_payment_events::Column::ProviderEventId
                                .eq(SensitiveString::from(write.provider_event_id.clone())),
                        ),
                ),
        )
        .all(connection)
        .await
        .map_err(|_| TopupRepositoryError::Query)?;
    if models.len() > 1 {
        return Err(TopupRepositoryError::Conflict);
    }
    models
        .into_iter()
        .next()
        .map(StoredPaymentEvent::try_from_model)
        .transpose()
}

fn classify_event_collision(
    existing: StoredPaymentEvent,
    order: &TopupOrderRecord,
    write: &TopupPaymentEventWrite,
) -> Result<TopupPaymentEventOutcome, TopupRepositoryError> {
    if existing.matches(order, write) {
        Ok(TopupPaymentEventOutcome::Existing(load_order_snapshot(
            order,
        )))
    } else {
        Err(TopupRepositoryError::Conflict)
    }
}

fn load_order_snapshot(order: &TopupOrderRecord) -> TopupOrderRecord {
    TopupOrderRecord {
        database_id: order.database_id,
        order_id: order.order_id,
        request_id: order.request_id,
        user_id: order.user_id,
        organization_id: order.organization_id,
        provider: order.provider.clone(),
        payment_method: order.payment_method.clone(),
        provider_order_id: order.provider_order_id.clone(),
        trade_no: order.trade_no.clone(),
        status: order.status,
        amount_minor: order.amount_minor,
        currency: order.currency.clone(),
        quota_amount: order.quota_amount,
        version: order.version,
        expires_at: order.expires_at,
        paid_at: order.paid_at,
        closed_at: order.closed_at,
        created_at: order.created_at,
        updated_at: order.updated_at,
    }
}

fn order_record(model: topup_orders::Model) -> Result<TopupOrderRecord, TopupRepositoryError> {
    let order_id = TopupOrderId::from_persistence_key(&model.order_key)
        .map_err(|_| TopupRepositoryError::Invariant)?;
    let request_id = TopupRequestId::from_persistence_key(&model.idempotency_key)
        .map_err(|_| TopupRepositoryError::Invariant)?;
    let user_id = UserId::new(model.user_id).map_err(|_| TopupRepositoryError::Invariant)?;
    let organization_id = model
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| TopupRepositoryError::Invariant)?;
    let status =
        TopupOrderStatus::try_from(model.status).map_err(|_| TopupRepositoryError::Invariant)?;
    let amount_minor =
        u64::try_from(model.amount_minor).map_err(|_| TopupRepositoryError::Invariant)?;
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| TopupRepositoryError::Invariant)?;
    let version = u64::try_from(model.version).map_err(|_| TopupRepositoryError::Invariant)?;
    let provider_order_id = model
        .provider_order_id
        .map(|value| value.as_str().to_owned());
    let trade_no = model.trade_no.map(|value| value.as_str().to_owned());
    let expires_at = optional_unix_seconds(model.expires_at)?;
    let paid_at = optional_unix_seconds(model.paid_at)?;
    let closed_at = optional_unix_seconds(model.closed_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let valid_state = match status {
        TopupOrderStatus::Created => trade_no.is_none() && paid_at.is_none() && closed_at.is_none(),
        TopupOrderStatus::Pending => {
            provider_order_id.is_some()
                && trade_no.is_none()
                && expires_at.is_some()
                && paid_at.is_none()
                && closed_at.is_none()
        }
        TopupOrderStatus::Paid => trade_no.is_some() && paid_at.is_some() && closed_at.is_none(),
        TopupOrderStatus::Failed | TopupOrderStatus::Canceled | TopupOrderStatus::Expired => {
            paid_at.is_none() && closed_at.is_some()
        }
    };
    if model.id <= 0
        || amount_minor == 0
        || quota_amount.is_zero()
        || version == 0
        || !valid_provider(&model.provider)
        || model
            .payment_method
            .as_deref()
            .is_some_and(|value| !valid_payment_method(value))
        || (model.payment_method.is_none() && model.version > 1)
        || !valid_currency(&model.currency)
        || provider_order_id.as_deref().is_some_and(|value| {
            !valid_provider_value(value, super::types::MAX_PROVIDER_ORDER_ID_BYTES)
        })
        || trade_no.as_deref().is_some_and(|value| {
            !valid_provider_value(value, super::types::MAX_PROVIDER_TRADE_NO_BYTES)
        })
        || expires_at.is_some_and(|value| value <= created_at)
        || updated_at < created_at
        || !valid_state
    {
        return Err(TopupRepositoryError::Invariant);
    }
    Ok(TopupOrderRecord {
        database_id: model.id,
        order_id,
        request_id,
        user_id,
        organization_id,
        provider: model.provider,
        payment_method: model.payment_method,
        provider_order_id,
        trade_no,
        status,
        amount_minor,
        currency: model.currency,
        quota_amount,
        version,
        expires_at,
        paid_at,
        closed_at,
        created_at,
        updated_at,
    })
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, TopupRepositoryError> {
    let value = i64::try_from(value).map_err(|_| TopupRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| TopupRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, TopupRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| TopupRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, TopupRepositoryError> {
    value.map(unix_seconds).transpose()
}

fn hex_digest(value: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in value {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn is_order_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_topup_orders_order_key")
        || rendered.contains("uq_topup_orders_idempotency_key")
        || rendered.contains("topup_orders.order_key")
        || rendered.contains("topup_orders.idempotency_key")
        || rendered.contains("Duplicate entry")
}

fn is_event_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_topup_payment_events_event_key")
        || rendered.contains("uq_topup_payment_events_provider_event")
        || rendered.contains("topup_payment_events.event_key")
        || rendered.contains("Duplicate entry")
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, TopupRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| TopupRepositoryError::Query)
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), TopupRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| TopupRepositoryError::OutcomeUnknown)
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), TopupRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| TopupRepositoryError::OutcomeUnknown)
}

#[cfg(test)]
fn is_created_order(outcome: &TopupOrderCreateOutcome) -> bool {
    matches!(outcome, TopupOrderCreateOutcome::Created(_))
}

#[cfg(test)]
fn is_applied_submission(outcome: &TopupOrderSubmitOutcome) -> bool {
    matches!(outcome, TopupOrderSubmitOutcome::Applied(_))
}

#[cfg(test)]
fn is_applied_event(outcome: &TopupPaymentEventOutcome) -> bool {
    matches!(outcome, TopupPaymentEventOutcome::Applied(_))
}

/// 仅记录闭合内部分类，避免订单、Provider、流水号和摘要进入日志。
fn internal(error: TopupRepositoryError) -> TopupRepositoryError {
    let error_kind = match error {
        TopupRepositoryError::Conflict | TopupRepositoryError::UnsupportedTarget => return error,
        TopupRepositoryError::Query => "topup_query",
        TopupRepositoryError::OutcomeUnknown => "topup_outcome_unknown",
        TopupRepositoryError::Timeout => "topup_timeout",
        TopupRepositoryError::Invariant => "topup_invariant",
    };
    tracing::error!(
        target: "af_db::topup",
        error_kind,
        "充值仓储发生内部错误"
    );
    error
}
