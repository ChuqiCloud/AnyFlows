use std::{fmt, time::Duration};

use af_domain::{BillingReservationId, OrganizationId, Quota, QuotaDelta, UserId, WalletEventId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_wallet_ledger_entries, organization_wallets, organizations,
    },
};

pub const MAX_ORGANIZATION_WALLET_LEDGER_PAGE_SIZE: usize = 100;
pub const MAX_ORGANIZATION_WALLET_ADJUSTMENT_REASON_BYTES: usize = 500;

/// 企业钱包账本的闭合事件类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum OrganizationWalletLedgerEntryType {
    OpeningBalance = 1,
    AdminAdjustment = 2,
    Topup = 3,
    UsageSettlement = 4,
}

impl OrganizationWalletLedgerEntryType {
    fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::OpeningBalance),
            2 => Some(Self::AdminAdjustment),
            3 => Some(Self::Topup),
            4 => Some(Self::UsageSettlement),
            _ => None,
        }
    }
}

/// 企业账本中的一条不可变事实。
pub struct OrganizationWalletLedgerEntryRecord {
    id: i64,
    organization_id: OrganizationId,
    event_id: WalletEventId,
    actor_user_id: Option<UserId>,
    entry_type: OrganizationWalletLedgerEntryType,
    quota_delta: QuotaDelta,
    balance_before: Quota,
    balance_after: Quota,
    source: String,
    reason: Option<String>,
    created_at: i64,
}

impl OrganizationWalletLedgerEntryRecord {
    pub const fn id(&self) -> i64 {
        self.id
    }
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }
    pub const fn actor_user_id(&self) -> Option<UserId> {
        self.actor_user_id
    }
    pub const fn entry_type(&self) -> OrganizationWalletLedgerEntryType {
        self.entry_type
    }
    pub const fn quota_delta(&self) -> QuotaDelta {
        self.quota_delta
    }
    pub const fn balance_before(&self) -> Quota {
        self.balance_before
    }
    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    fn try_from_model(
        model: organization_wallet_ledger_entries::Model,
    ) -> Result<Self, OrganizationWalletError> {
        let organization_id = OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationWalletError::Invariant)?;
        let event_id = WalletEventId::from_persistence_key(&model.event_key)
            .map_err(|_| OrganizationWalletError::Invariant)?;
        let actor_user_id = model
            .actor_user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| OrganizationWalletError::Invariant)?;
        let entry_type = OrganizationWalletLedgerEntryType::from_database(model.entry_type)
            .ok_or(OrganizationWalletError::Invariant)?;
        let quota_delta =
            QuotaDelta::new(model.quota_delta).map_err(|_| OrganizationWalletError::Invariant)?;
        let balance_before =
            Quota::new(model.balance_before).map_err(|_| OrganizationWalletError::Invariant)?;
        let balance_after =
            Quota::new(model.balance_after).map_err(|_| OrganizationWalletError::Invariant)?;
        let reason = model.reason.map(|value| value.as_str().to_owned());
        if model.id <= 0
            || quota_delta.is_zero()
            || balance_before.checked_apply(quota_delta) != Ok(balance_after)
            || model.source.is_empty()
            || (entry_type == OrganizationWalletLedgerEntryType::AdminAdjustment
                && (actor_user_id.is_none() || reason.as_deref().is_none_or(str::is_empty)))
        {
            return Err(OrganizationWalletError::Invariant);
        }
        Ok(Self {
            id: model.id,
            organization_id,
            event_id,
            actor_user_id,
            entry_type,
            quota_delta,
            balance_before,
            balance_after,
            source: model.source,
            reason,
            created_at: model.created_at.unix_timestamp(),
        })
    }

    fn matches_adjustment(&self, write: &OrganizationWalletAdjustmentWrite) -> bool {
        self.organization_id == write.organization_id
            && self.entry_type == OrganizationWalletLedgerEntryType::AdminAdjustment
            && self.actor_user_id == Some(write.actor_user_id)
            && self.quota_delta == write.quota_delta
            && self.reason.as_deref() == Some(write.reason.as_str())
    }
}

impl fmt::Debug for OrganizationWalletLedgerEntryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationWalletLedgerEntryRecord(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationWalletBalanceRecord {
    organization_id: OrganizationId,
    quota: Quota,
    used_quota: Quota,
    frozen_quota: Quota,
    version: i64,
}

impl OrganizationWalletBalanceRecord {
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }
    pub const fn quota(self) -> Quota {
        self.quota
    }
    pub const fn used_quota(self) -> Quota {
        self.used_quota
    }
    pub const fn frozen_quota(self) -> Quota {
        self.frozen_quota
    }
    pub const fn version(self) -> i64 {
        self.version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationWalletBalanceLookupOutcome {
    Found(OrganizationWalletBalanceRecord),
    NotFound,
}

pub struct OrganizationWalletLedgerPageRecord {
    entries: Vec<OrganizationWalletLedgerEntryRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationWalletLedgerPageRecord {
    pub fn into_parts(self) -> (Vec<OrganizationWalletLedgerEntryRecord>, Option<i64>) {
        (self.entries, self.next_cursor)
    }
}

pub enum OrganizationWalletLedgerListOutcome {
    Found(OrganizationWalletLedgerPageRecord),
    NotFound,
}

/// 管理员受审计调账命令；负数调账同样必须经过可用余额校验。
pub struct OrganizationWalletAdjustmentWrite {
    event_id: WalletEventId,
    organization_id: OrganizationId,
    actor_user_id: UserId,
    quota_delta: QuotaDelta,
    reason: String,
}

impl OrganizationWalletAdjustmentWrite {
    pub fn new(
        event_id: WalletEventId,
        organization_id: OrganizationId,
        actor_user_id: UserId,
        quota_delta: QuotaDelta,
        reason: String,
    ) -> Result<Self, OrganizationWalletError> {
        if event_id.is_system_opening() || quota_delta.is_zero() || !valid_reason(&reason) {
            return Err(OrganizationWalletError::InvalidInput);
        }
        Ok(Self {
            event_id,
            organization_id,
            actor_user_id,
            quota_delta,
            reason,
        })
    }
}

pub enum OrganizationWalletAdjustmentOutcome {
    Applied(OrganizationWalletLedgerEntryRecord),
    Existing(OrganizationWalletLedgerEntryRecord),
    NotFound,
}

pub enum OrganizationWalletCreditOutcome {
    Applied,
    Existing,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationWalletRepositoryConfigError {
    #[error("企业钱包操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationWalletError {
    #[error("企业钱包输入无效")]
    InvalidInput,
    #[error("企业钱包账本事件冲突")]
    Conflict,
    #[error("企业钱包余额不足")]
    InsufficientQuota,
    #[error("企业钱包余额溢出")]
    Overflow,
    #[error("企业钱包数据库操作失败")]
    Query,
    #[error("企业钱包操作结果未知")]
    OutcomeUnknown,
    #[error("企业钱包查询超时")]
    Timeout,
    #[error("企业钱包持久化状态损坏")]
    Invariant,
}

#[derive(Clone)]
pub struct OrganizationWalletRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationWalletRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationWalletRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationWalletRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn balance(
        &self,
        organization_id: OrganizationId,
    ) -> Result<OrganizationWalletBalanceLookupOutcome, OrganizationWalletError> {
        let operation = self
            .balance_inner(organization_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(OrganizationWalletError::Timeout),
        }
    }

    pub async fn list(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationWalletLedgerListOutcome, OrganizationWalletError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_WALLET_LEDGER_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationWalletError::InvalidInput);
        }
        let operation = self
            .list_inner(organization_id, before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(OrganizationWalletError::Timeout),
        }
    }

    pub async fn adjust(
        &self,
        write: OrganizationWalletAdjustmentWrite,
    ) -> Result<OrganizationWalletAdjustmentOutcome, OrganizationWalletError> {
        let operation = self
            .adjust_inner(&write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(OrganizationWalletError::OutcomeUnknown),
        }
    }

    async fn balance_inner(
        &self,
        organization_id: OrganizationId,
    ) -> Result<OrganizationWalletBalanceLookupOutcome, OrganizationWalletError> {
        let model = organization_wallets::Entity::find()
            .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationWalletError::Query)?;
        model.map_or(
            Ok(OrganizationWalletBalanceLookupOutcome::NotFound),
            |model| balance_record(model).map(OrganizationWalletBalanceLookupOutcome::Found),
        )
    }

    async fn list_inner(
        &self,
        organization_id: OrganizationId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<OrganizationWalletLedgerListOutcome, OrganizationWalletError> {
        if !organization_exists(self.pool.connection(), organization_id).await? {
            return Ok(OrganizationWalletLedgerListOutcome::NotFound);
        }
        let mut query = organization_wallet_ledger_entries::Entity::find()
            .filter(
                organization_wallet_ledger_entries::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .order_by_desc(organization_wallet_ledger_entries::Column::Id)
            .limit((limit + 1) as u64);
        if let Some(before) = before {
            query = query.filter(organization_wallet_ledger_entries::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(|_| OrganizationWalletError::Query)?;
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let entries = models
            .into_iter()
            .map(entry_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| entries.last().map(OrganizationWalletLedgerEntryRecord::id))
            .flatten();
        Ok(OrganizationWalletLedgerListOutcome::Found(
            OrganizationWalletLedgerPageRecord {
                entries,
                next_cursor,
            },
        ))
    }

    async fn adjust_inner(
        &self,
        write: &OrganizationWalletAdjustmentWrite,
    ) -> Result<OrganizationWalletAdjustmentOutcome, OrganizationWalletError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationWalletError::Query)?;
        let result = apply_adjustment(&transaction, write).await;
        match result {
            Ok(outcome) => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationWalletError::OutcomeUnknown)?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for OrganizationWalletRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationWalletRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

pub(crate) async fn credit_topup_in_transaction(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    event_id: WalletEventId,
    actor_user_id: UserId,
    quota_amount: Quota,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<OrganizationWalletCreditOutcome, OrganizationWalletError> {
    let wallet = lock_wallet(transaction, organization_id)
        .await?
        .ok_or(OrganizationWalletError::Invariant)?;
    let event_key = event_id.persistence_key();
    if let Some(existing) = load_event(transaction, organization_id, &event_key).await? {
        if existing.entry_type == OrganizationWalletLedgerEntryType::Topup
            && existing.actor_user_id == Some(actor_user_id)
            && existing.quota_delta
                == QuotaDelta::new(quota_amount.units())
                    .map_err(|_| OrganizationWalletError::Invariant)?
            && existing.source == "topup"
        {
            return Ok(OrganizationWalletCreditOutcome::Existing);
        }
        return Err(OrganizationWalletError::Conflict);
    }
    let delta =
        QuotaDelta::new(quota_amount.units()).map_err(|_| OrganizationWalletError::Invariant)?;
    let before = Quota::new(wallet.quota).map_err(|_| OrganizationWalletError::Invariant)?;
    let after = before.checked_apply(delta).map_err(|error| match error {
        af_domain::QuotaError::Overflow => OrganizationWalletError::Overflow,
        _ => OrganizationWalletError::Invariant,
    })?;
    update_wallet_quota(
        transaction,
        wallet.id,
        wallet.version,
        before,
        after,
        created_at,
    )
    .await?;
    insert_entry(
        transaction,
        LedgerEntryInput {
            organization_id,
            event_id,
            actor_user_id: Some(actor_user_id),
            entry_type: OrganizationWalletLedgerEntryType::Topup,
            quota_delta: delta,
            balance_before: before,
            balance_after: after,
            source: "topup",
            reason: None,
            created_at,
        },
    )
    .await?;
    Ok(OrganizationWalletCreditOutcome::Applied)
}

/// 在企业结算事务内追加一条不可变消费账本；零用量不制造空事件。
pub(crate) async fn append_usage_settlement_in_transaction(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    reservation_id: BillingReservationId,
    actual_quota: Quota,
    balance_before: Quota,
    balance_after: Quota,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationWalletError> {
    if actual_quota.is_zero() {
        return Ok(());
    }
    let event_id = WalletEventId::new(reservation_id.bytes())
        .map_err(|_| OrganizationWalletError::Invariant)
        .inspect_err(|_| {
            #[cfg(test)]
            eprintln!("[DEBUG-organization-budget-settle] ledger invariant=event_id");
        })?;
    let quota_delta = actual_quota
        .units()
        .checked_neg()
        .and_then(|value| QuotaDelta::new(value).ok())
        .ok_or(OrganizationWalletError::Invariant)
        .inspect_err(|_| {
            #[cfg(test)]
            eprintln!("[DEBUG-organization-budget-settle] ledger invariant=quota_delta");
        })?;
    if balance_before.checked_apply(quota_delta) != Ok(balance_after) {
        #[cfg(test)]
        eprintln!(
            "[DEBUG-organization-budget-settle] ledger invariant=balance before={} after={} actual={}",
            balance_before.units(),
            balance_after.units(),
            actual_quota.units()
        );
        return Err(OrganizationWalletError::Invariant);
    }
    if let Some(existing) =
        load_event(transaction, organization_id, &event_id.persistence_key()).await?
    {
        if existing.entry_type == OrganizationWalletLedgerEntryType::UsageSettlement
            && existing.actor_user_id.is_none()
            && existing.quota_delta == quota_delta
            && existing.balance_before == balance_before
            && existing.balance_after == balance_after
            && existing.source == "usage_settlement"
        {
            return Ok(());
        }
        return Err(OrganizationWalletError::Conflict);
    }
    insert_entry(
        transaction,
        LedgerEntryInput {
            organization_id,
            event_id,
            actor_user_id: None,
            entry_type: OrganizationWalletLedgerEntryType::UsageSettlement,
            quota_delta,
            balance_before,
            balance_after,
            source: "usage_settlement",
            reason: None,
            created_at,
        },
    )
    .await
    .inspect_err(|_error| {
        #[cfg(test)]
        eprintln!("[DEBUG-organization-budget-settle] ledger insert error={_error:?}");
    })?;
    Ok(())
}

async fn apply_adjustment(
    transaction: &DatabaseTransaction,
    write: &OrganizationWalletAdjustmentWrite,
) -> Result<OrganizationWalletAdjustmentOutcome, OrganizationWalletError> {
    if let Some(existing) = load_event(
        transaction,
        write.organization_id,
        &write.event_id.persistence_key(),
    )
    .await?
    {
        return if existing.matches_adjustment(write) {
            Ok(OrganizationWalletAdjustmentOutcome::Existing(existing))
        } else {
            Err(OrganizationWalletError::Conflict)
        };
    }
    let Some(wallet) = lock_wallet(transaction, write.organization_id).await? else {
        return Ok(OrganizationWalletAdjustmentOutcome::NotFound);
    };
    let before = Quota::new(wallet.quota).map_err(|_| OrganizationWalletError::Invariant)?;
    let after = before
        .checked_apply(write.quota_delta)
        .map_err(|error| match error {
            af_domain::QuotaError::Underflow => OrganizationWalletError::InsufficientQuota,
            af_domain::QuotaError::Overflow => OrganizationWalletError::Overflow,
            _ => OrganizationWalletError::Invariant,
        })?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    update_wallet_quota(transaction, wallet.id, wallet.version, before, after, now).await?;
    let entry = insert_entry(
        transaction,
        LedgerEntryInput {
            organization_id: write.organization_id,
            event_id: write.event_id,
            actor_user_id: Some(write.actor_user_id),
            entry_type: OrganizationWalletLedgerEntryType::AdminAdjustment,
            quota_delta: write.quota_delta,
            balance_before: before,
            balance_after: after,
            source: "admin_adjustment",
            reason: Some(write.reason.clone()),
            created_at: now,
        },
    )
    .await?;
    Ok(OrganizationWalletAdjustmentOutcome::Applied(entry))
}

async fn organization_exists<C: ConnectionTrait>(
    connection: &C,
    organization_id: OrganizationId,
) -> Result<bool, OrganizationWalletError> {
    organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(connection)
        .await
        .map(|value| value.is_some())
        .map_err(|_| OrganizationWalletError::Query)
}

async fn lock_wallet(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<Option<organization_wallets::Model>, OrganizationWalletError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_wallets::Entity::update_many()
            .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
            .col_expr(
                organization_wallets::Column::Quota,
                Expr::col(organization_wallets::Column::Quota).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationWalletError::Query)?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
    }
    let mut query = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationWalletError::Query)
}

async fn update_wallet_quota(
    transaction: &DatabaseTransaction,
    id: i64,
    version: i64,
    before: Quota,
    after: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationWalletError> {
    let next_version = version
        .checked_add(1)
        .ok_or(OrganizationWalletError::Overflow)?;
    let updated = organization_wallets::Entity::update_many()
        .filter(organization_wallets::Column::Id.eq(id))
        .filter(organization_wallets::Column::Version.eq(version))
        .filter(organization_wallets::Column::Quota.eq(before.units()))
        .col_expr(
            organization_wallets::Column::Quota,
            Expr::value(after.units()),
        )
        .col_expr(
            organization_wallets::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(organization_wallets::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map_err(|_| OrganizationWalletError::Query)?;
    if updated.rows_affected != 1 {
        return Err(OrganizationWalletError::Conflict);
    }
    Ok(())
}

struct LedgerEntryInput<'a> {
    organization_id: OrganizationId,
    event_id: WalletEventId,
    actor_user_id: Option<UserId>,
    entry_type: OrganizationWalletLedgerEntryType,
    quota_delta: QuotaDelta,
    balance_before: Quota,
    balance_after: Quota,
    source: &'a str,
    reason: Option<String>,
    created_at: TimeDateTimeWithTimeZone,
}

async fn insert_entry(
    transaction: &DatabaseTransaction,
    input: LedgerEntryInput<'_>,
) -> Result<OrganizationWalletLedgerEntryRecord, OrganizationWalletError> {
    let model = organization_wallet_ledger_entries::ActiveModel {
        organization_id: Set(input.organization_id.get()),
        event_key: Set(input.event_id.persistence_key()),
        actor_user_id: Set(input.actor_user_id.map(UserId::get)),
        entry_type: Set(input.entry_type as i16),
        quota_delta: Set(input.quota_delta.units()),
        balance_before: Set(input.balance_before.units()),
        balance_after: Set(input.balance_after.units()),
        source: Set(input.source.to_owned()),
        reason: Set(input.reason.map(SensitiveString::from)),
        created_at: Set(input.created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationWalletError::Query)?;
    entry_record(model)
}

async fn load_event<C: ConnectionTrait>(
    connection: &C,
    organization_id: OrganizationId,
    event_key: &str,
) -> Result<Option<OrganizationWalletLedgerEntryRecord>, OrganizationWalletError> {
    organization_wallet_ledger_entries::Entity::find()
        .filter(
            organization_wallet_ledger_entries::Column::OrganizationId.eq(organization_id.get()),
        )
        .filter(organization_wallet_ledger_entries::Column::EventKey.eq(event_key))
        .one(connection)
        .await
        .map_err(|_| OrganizationWalletError::Query)?
        .map(entry_record)
        .transpose()
}

fn entry_record(
    model: organization_wallet_ledger_entries::Model,
) -> Result<OrganizationWalletLedgerEntryRecord, OrganizationWalletError> {
    OrganizationWalletLedgerEntryRecord::try_from_model(model)
}

fn balance_record(
    model: organization_wallets::Model,
) -> Result<OrganizationWalletBalanceRecord, OrganizationWalletError> {
    if model.id <= 0
        || model.version <= 0
        || model.quota < 0
        || model.used_quota < 0
        || model.frozen_quota < 0
    {
        return Err(OrganizationWalletError::Invariant);
    }
    Ok(OrganizationWalletBalanceRecord {
        organization_id: OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationWalletError::Invariant)?,
        quota: Quota::new(model.quota).map_err(|_| OrganizationWalletError::Invariant)?,
        used_quota: Quota::new(model.used_quota).map_err(|_| OrganizationWalletError::Invariant)?,
        frozen_quota: Quota::new(model.frozen_quota)
            .map_err(|_| OrganizationWalletError::Invariant)?,
        version: model.version,
    })
}

fn valid_reason(reason: &str) -> bool {
    !reason.is_empty()
        && reason.len() <= MAX_ORGANIZATION_WALLET_ADJUSTMENT_REASON_BYTES
        && !reason.chars().any(char::is_control)
}
