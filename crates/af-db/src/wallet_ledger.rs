use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{Quota, QuotaDelta, QuotaError, UserId, WalletEventId};
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
    entity::{SensitiveString, WalletLedgerKey, users, wallet_ledger_entries},
};

/// 单页钱包账本查询允许返回的最大记录数。
pub const MAX_WALLET_LEDGER_PAGE_SIZE: usize = 100;
/// 管理员调账原因允许的最大 UTF-8 字节数。
pub const MAX_WALLET_ADJUSTMENT_REASON_BYTES: usize = 500;

/// 钱包账本事件的闭合业务类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum WalletLedgerEntryType {
    /// 用户创建或迁移升级时固化的初始非零余额。
    OpeningBalance = 1,
    /// 管理员通过有符号增量执行的人工调账。
    AdminAdjustment = 2,
    /// 已验证支付事件在订单事务内完成的充值到账。
    Topup = 3,
    /// 一次性兑换码在原子消费事务内完成的额度到账。
    Redemption = 4,
    /// 邀请关系触发的注册返利额度到账。
    InviteRebate = 5,
}

impl WalletLedgerEntryType {
    const fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::OpeningBalance),
            2 => Some(Self::AdminAdjustment),
            3 => Some(Self::Topup),
            4 => Some(Self::Redemption),
            5 => Some(Self::InviteRebate),
            _ => None,
        }
    }
}

/// 已持久化的一条钱包余额变更事实。
pub struct WalletLedgerEntryRecord {
    id: i64,
    event_id: WalletEventId,
    user_id: UserId,
    actor_user_id: Option<UserId>,
    entry_type: WalletLedgerEntryType,
    quota_delta: QuotaDelta,
    balance_before: Quota,
    balance_after: Quota,
    reason: Option<String>,
    created_at: i64,
}

impl WalletLedgerEntryRecord {
    /// 返回单调账本主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回调用方重试和审计关联使用的稳定事件标识。
    #[must_use]
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }

    /// 返回余额所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回发起变更的管理员；opening 基线没有管理员主体。
    #[must_use]
    pub const fn actor_user_id(&self) -> Option<UserId> {
        self.actor_user_id
    }

    /// 返回账本事件类型。
    #[must_use]
    pub const fn entry_type(&self) -> WalletLedgerEntryType {
        self.entry_type
    }

    /// 返回本次有符号额度增量。
    #[must_use]
    pub const fn quota_delta(&self) -> QuotaDelta {
        self.quota_delta
    }

    /// 返回事务内变更前余额快照。
    #[must_use]
    pub const fn balance_before(&self) -> Quota {
        self.balance_before
    }

    /// 返回事务内变更后余额快照。
    #[must_use]
    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }

    /// 返回人工调账原因；opening 基线没有原因。
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    fn try_from_model(model: wallet_ledger_entries::Model) -> Result<Self, WalletLedgerError> {
        let event_id = WalletEventId::from_persistence_key(model.event_key.as_str())
            .map_err(|_| internal(WalletLedgerError::Invariant))?;
        let user_id =
            UserId::new(model.user_id).map_err(|_| internal(WalletLedgerError::Invariant))?;
        let actor_user_id = model
            .actor_user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| internal(WalletLedgerError::Invariant))?;
        let entry_type = WalletLedgerEntryType::from_database(model.entry_type)
            .ok_or_else(|| internal(WalletLedgerError::Invariant))?;
        let quota_delta = QuotaDelta::new(model.quota_delta)
            .map_err(|_| internal(WalletLedgerError::Invariant))?;
        let balance_before =
            Quota::new(model.balance_before).map_err(|_| internal(WalletLedgerError::Invariant))?;
        let balance_after =
            Quota::new(model.balance_after).map_err(|_| internal(WalletLedgerError::Invariant))?;
        let reason = model.reason.map(|reason| reason.as_str().to_owned());
        if model.id <= 0
            || quota_delta.is_zero()
            || balance_before.checked_apply(quota_delta) != Ok(balance_after)
            || !valid_event_shape(
                event_id,
                entry_type,
                actor_user_id,
                quota_delta,
                balance_before,
                reason.as_deref(),
            )
        {
            return Err(internal(WalletLedgerError::Invariant));
        }
        Ok(Self {
            id: model.id,
            event_id,
            user_id,
            actor_user_id,
            entry_type,
            quota_delta,
            balance_before,
            balance_after,
            reason,
            created_at: model.created_at.unix_timestamp(),
        })
    }

    fn matches_adjustment(&self, write: &WalletAdjustmentWrite) -> bool {
        self.entry_type == WalletLedgerEntryType::AdminAdjustment
            && self.user_id == write.user_id
            && self.actor_user_id == Some(write.actor_user_id)
            && self.quota_delta == write.quota_delta
            && self.reason.as_deref() == Some(write.reason.as_str())
    }
}

impl fmt::Debug for WalletLedgerEntryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletLedgerEntryRecord(<redacted>)")
    }
}

/// 一页按主键倒序排列的钱包账本结果。
pub struct WalletLedgerPageRecord {
    entries: Vec<WalletLedgerEntryRecord>,
    next_cursor: Option<i64>,
}

impl WalletLedgerPageRecord {
    /// 消费页面并返回账本记录和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<WalletLedgerEntryRecord>, Option<i64>) {
        (self.entries, self.next_cursor)
    }
}

impl fmt::Debug for WalletLedgerPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletLedgerPageRecord(<redacted>)")
    }
}

/// 当前钱包额度状态的同一行持久化快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalletBalanceRecord {
    balance: Quota,
    used_quota: Quota,
    frozen_quota: Quota,
}

impl WalletBalanceRecord {
    /// 返回当前可用余额。
    #[must_use]
    pub const fn balance(self) -> Quota {
        self.balance
    }

    /// 返回累计完成结算的消耗额度。
    #[must_use]
    pub const fn used_quota(self) -> Quota {
        self.used_quota
    }

    /// 返回在途请求已经预扣但尚未终态结算的额度。
    #[must_use]
    pub const fn frozen_quota(self) -> Quota {
        self.frozen_quota
    }

    fn try_from_user(model: users::Model) -> Result<Self, WalletLedgerError> {
        Ok(Self {
            balance: Quota::new(model.quota).map_err(|_| WalletLedgerError::Invariant)?,
            used_quota: Quota::new(model.used_quota).map_err(|_| WalletLedgerError::Invariant)?,
            frozen_quota: Quota::new(model.frozen_quota)
                .map_err(|_| WalletLedgerError::Invariant)?,
        })
    }
}

/// 用户范围钱包余额快照读取结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletBalanceLookupOutcome {
    /// 用户存在，并返回同一次行读取获得的三个额度字段。
    Found(WalletBalanceRecord),
    /// 用户不存在或已经软删除。
    NotFound,
}

/// 用户范围账本读取结果；不存在和已软删除用户统一视为未找到。
pub enum WalletLedgerListOutcome {
    /// 用户存在，并返回一页只追加账本。
    Found(WalletLedgerPageRecord),
    /// 用户不存在或已经软删除。
    NotFound,
}

/// 管理员人工调账的闭合写入事实。
pub struct WalletAdjustmentWrite {
    event_id: WalletEventId,
    user_id: UserId,
    actor_user_id: UserId,
    quota_delta: QuotaDelta,
    reason: String,
}

impl WalletAdjustmentWrite {
    /// 校验非零增量、保留事件命名空间和可审计原因。
    pub fn new(
        event_id: WalletEventId,
        user_id: UserId,
        actor_user_id: UserId,
        quota_delta: QuotaDelta,
        reason: String,
    ) -> Result<Self, WalletLedgerError> {
        if event_id.is_system_opening() || quota_delta.is_zero() || !valid_reason(&reason) {
            return Err(WalletLedgerError::InvalidInput);
        }
        Ok(Self {
            event_id,
            user_id,
            actor_user_id,
            quota_delta,
            reason,
        })
    }
}

impl fmt::Debug for WalletAdjustmentWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WalletAdjustmentWrite(<redacted>)")
    }
}

/// 幂等人工调账结果。
pub enum WalletAdjustmentOutcome {
    /// 本次调用提交了新的余额快照和账本记录。
    Applied(WalletLedgerEntryRecord),
    /// 相同事件键和相同事实已经存在，本次未重复调账。
    Existing(WalletLedgerEntryRecord),
    /// 目标用户不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for WalletAdjustmentOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => formatter.write_str("WalletAdjustmentOutcome::Applied(<redacted>)"),
            Self::Existing(_) => {
                formatter.write_str("WalletAdjustmentOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("WalletAdjustmentOutcome::NotFound"),
        }
    }
}

/// 钱包账本仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WalletLedgerRepositoryConfigError {
    /// 零超时无法形成有效的数据库操作截止时间。
    #[error("钱包账本操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 钱包账本仓储错误；不携带主体、余额、原因或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WalletLedgerError {
    /// 事件标识、增量、原因或分页参数违反公开边界。
    #[error("钱包账本输入无效")]
    InvalidInput,
    /// 相同幂等键已经绑定不同业务事实。
    #[error("钱包账本事件冲突")]
    Conflict,
    /// 负向调账会使可用余额低于零。
    #[error("钱包余额不足")]
    InsufficientQuota,
    /// 正向调账会超过额度整数上界。
    #[error("钱包余额溢出")]
    Overflow,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("钱包账本数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须复用同一事件键查询或重试。
    #[error("钱包账本操作结果未知")]
    OutcomeUnknown,
    /// 查询超过配置的硬截止时间。
    #[error("钱包账本查询超时")]
    Timeout,
    /// 持久化余额快照或账本事件违反不变量。
    #[error("钱包账本持久化状态损坏")]
    Invariant,
}

/// 原子维护用户余额快照和追加账本的数据库仓储。
#[derive(Clone)]
pub struct WalletLedgerRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl WalletLedgerRepository {
    /// 使用共享连接池和单次读写截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, WalletLedgerRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(WalletLedgerRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 按账本 ID 从新到旧读取指定有效用户的一页事件。
    pub async fn list(
        &self,
        user_id: UserId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<WalletLedgerListOutcome, WalletLedgerError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_WALLET_LEDGER_PAGE_SIZE).contains(&limit)
        {
            return Err(WalletLedgerError::InvalidInput);
        }
        let operation = self
            .list_inner(user_id, before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(WalletLedgerError::Timeout)),
        }
    }

    /// 读取指定有效用户当前余额、累计消耗与在途冻结额度。
    pub async fn balance(
        &self,
        user_id: UserId,
    ) -> Result<WalletBalanceLookupOutcome, WalletLedgerError> {
        let operation = self
            .balance_inner(user_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(WalletLedgerError::Timeout)),
        }
    }

    /// 原子应用一次有符号管理员调账；相同事件和事实可安全重放。
    pub async fn adjust(
        &self,
        write: WalletAdjustmentWrite,
    ) -> Result<WalletAdjustmentOutcome, WalletLedgerError> {
        let operation = self
            .adjust_inner(&write)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(WalletLedgerError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, WalletAdjustmentOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(WalletLedgerError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    async fn list_inner(
        &self,
        user_id: UserId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<WalletLedgerListOutcome, WalletLedgerError> {
        let exists = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(|_| WalletLedgerError::Query)?
            .is_some();
        if !exists {
            return Ok(WalletLedgerListOutcome::NotFound);
        }

        let mut query = wallet_ledger_entries::Entity::find()
            .filter(wallet_ledger_entries::Column::UserId.eq(user_id.get()))
            .order_by_desc(wallet_ledger_entries::Column::Id)
            .limit((limit + 1) as u64);
        if let Some(before) = before {
            query = query.filter(wallet_ledger_entries::Column::Id.lt(before));
        }
        let mut models = query
            .all(self.pool.connection())
            .await
            .map_err(|_| WalletLedgerError::Query)?;
        let has_more = models.len() > limit;
        if has_more {
            models.truncate(limit);
        }
        let entries = models
            .into_iter()
            .map(WalletLedgerEntryRecord::try_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| entries.last().map(WalletLedgerEntryRecord::id))
            .flatten();
        Ok(WalletLedgerListOutcome::Found(WalletLedgerPageRecord {
            entries,
            next_cursor,
        }))
    }

    async fn balance_inner(
        &self,
        user_id: UserId,
    ) -> Result<WalletBalanceLookupOutcome, WalletLedgerError> {
        let model = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(|_| WalletLedgerError::Query)?;
        model.map_or(Ok(WalletBalanceLookupOutcome::NotFound), |model| {
            WalletBalanceRecord::try_from_user(model).map(WalletBalanceLookupOutcome::Found)
        })
    }

    async fn adjust_inner(
        &self,
        write: &WalletAdjustmentWrite,
    ) -> Result<WalletAdjustmentOutcome, WalletLedgerError> {
        if let Some(existing) = load_event(self.pool.connection(), write.event_id).await? {
            return classify_existing(existing, write);
        }

        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| WalletLedgerError::Query)?;
        let balance_before = match lock_active_user(&transaction, write.user_id).await {
            Ok(Some(balance)) => balance,
            Ok(None) => {
                rollback(transaction).await?;
                return Ok(WalletAdjustmentOutcome::NotFound);
            }
            Err(error) => {
                rollback(transaction).await?;
                return Err(error);
            }
        };

        if let Some(existing) = load_event(&transaction, write.event_id).await? {
            rollback(transaction).await?;
            return classify_existing(existing, write);
        }

        let balance_after = balance_before
            .checked_apply(write.quota_delta)
            .map_err(map_quota_error)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let updated = users::Entity::update_many()
            .filter(users::Column::Id.eq(write.user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::Quota.eq(balance_before.units()))
            .col_expr(users::Column::Quota, Expr::value(balance_after.units()))
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .await
            .map_err(|_| WalletLedgerError::Query)?;
        if updated.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(WalletLedgerError::Invariant);
        }

        let inserted = wallet_ledger_entries::ActiveModel {
            event_key: Set(WalletLedgerKey::parse(&write.event_id.persistence_key())
                .map_err(|_| WalletLedgerError::Invariant)?),
            user_id: Set(write.user_id.get()),
            actor_user_id: Set(Some(write.actor_user_id.get())),
            entry_type: Set(WalletLedgerEntryType::AdminAdjustment as i16),
            quota_delta: Set(write.quota_delta.units()),
            balance_before: Set(balance_before.units()),
            balance_after: Set(balance_after.units()),
            reason: Set(Some(SensitiveString::from(write.reason.clone()))),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await;
        let inserted = match inserted {
            Ok(inserted) => inserted,
            Err(error) => {
                let unique_conflict = is_event_key_conflict(&error);
                rollback(transaction).await?;
                if unique_conflict {
                    let existing = load_event(self.pool.connection(), write.event_id)
                        .await?
                        .ok_or(WalletLedgerError::Invariant)?;
                    return classify_existing(existing, write);
                }
                return Err(WalletLedgerError::Query);
            }
        };
        let entry = WalletLedgerEntryRecord::try_from_model(inserted)?;
        transaction
            .commit()
            .await
            .map_err(|_| WalletLedgerError::OutcomeUnknown)?;
        Ok(WalletAdjustmentOutcome::Applied(entry))
    }
}

impl fmt::Debug for WalletLedgerRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WalletLedgerRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 在用户创建事务内追加非零 opening 余额；零余额不制造无意义账本事件。
pub(crate) async fn insert_opening_entry(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    quota: Quota,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<(), sea_orm::DbErr> {
    if quota.is_zero() {
        return Ok(());
    }
    let event_key = opening_event_key(user_id);
    wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse(&event_key)
            .map_err(|_| sea_orm::DbErr::Custom("钱包 opening 事件键构造失败".to_owned()))?),
        user_id: Set(user_id.get()),
        actor_user_id: Set(None),
        entry_type: Set(WalletLedgerEntryType::OpeningBalance as i16),
        quota_delta: Set(quota.units()),
        balance_before: Set(0),
        balance_after: Set(quota.units()),
        reason: Set(None),
        created_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await?;
    Ok(())
}

fn opening_event_key(user_id: UserId) -> String {
    format!("0000000000000001{:016x}", user_id.get())
}

async fn lock_active_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<Quota>, WalletLedgerError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，恒等更新在读取余额前取得数据库写锁。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| WalletLedgerError::Query)?;
        if result.rows_affected == 0 {
            return Ok(None);
        }
        if result.rows_affected != 1 {
            return Err(WalletLedgerError::Invariant);
        }
    }
    let mut query =
        users::Entity::find_by_id(user_id.get()).filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let model = query
        .one(transaction)
        .await
        .map_err(|_| WalletLedgerError::Query)?;
    model
        .map(|model| Quota::new(model.quota).map_err(|_| WalletLedgerError::Invariant))
        .transpose()
}

async fn load_event<C>(
    connection: &C,
    event_id: WalletEventId,
) -> Result<Option<WalletLedgerEntryRecord>, WalletLedgerError>
where
    C: sea_orm::ConnectionTrait,
{
    let key = WalletLedgerKey::parse(&event_id.persistence_key())
        .map_err(|_| WalletLedgerError::Invariant)?;
    wallet_ledger_entries::Entity::find()
        .filter(wallet_ledger_entries::Column::EventKey.eq(key))
        .one(connection)
        .await
        .map_err(|_| WalletLedgerError::Query)?
        .map(WalletLedgerEntryRecord::try_from_model)
        .transpose()
}

fn classify_existing(
    existing: WalletLedgerEntryRecord,
    write: &WalletAdjustmentWrite,
) -> Result<WalletAdjustmentOutcome, WalletLedgerError> {
    if existing.matches_adjustment(write) {
        Ok(WalletAdjustmentOutcome::Existing(existing))
    } else {
        Err(WalletLedgerError::Conflict)
    }
}

fn valid_event_shape(
    event_id: WalletEventId,
    entry_type: WalletLedgerEntryType,
    actor_user_id: Option<UserId>,
    quota_delta: QuotaDelta,
    balance_before: Quota,
    reason: Option<&str>,
) -> bool {
    match entry_type {
        WalletLedgerEntryType::OpeningBalance => {
            event_id.is_system_opening()
                && actor_user_id.is_none()
                && quota_delta.is_positive()
                && balance_before.is_zero()
                && reason.is_none()
        }
        WalletLedgerEntryType::AdminAdjustment => {
            !event_id.is_system_opening()
                && actor_user_id.is_some()
                && reason.is_some_and(valid_reason)
        }
        WalletLedgerEntryType::Topup
        | WalletLedgerEntryType::Redemption
        | WalletLedgerEntryType::InviteRebate => {
            !event_id.is_system_opening()
                && actor_user_id.is_none()
                && quota_delta.is_positive()
                && reason.is_none()
        }
    }
}

fn valid_reason(reason: &str) -> bool {
    !reason.is_empty()
        && reason.trim() == reason
        && reason.len() <= MAX_WALLET_ADJUSTMENT_REASON_BYTES
        && !reason.chars().any(char::is_control)
}

fn map_quota_error(error: QuotaError) -> WalletLedgerError {
    match error {
        QuotaError::Underflow => WalletLedgerError::InsufficientQuota,
        QuotaError::Overflow => WalletLedgerError::Overflow,
        QuotaError::Negative | QuotaError::InvalidDelta => WalletLedgerError::Invariant,
    }
}

fn is_event_key_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_wallet_ledger_event_key")
        || rendered.contains("wallet_ledger_entries.event_key")
        || rendered.contains("Duplicate entry")
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), WalletLedgerError> {
    transaction
        .rollback()
        .await
        .map_err(|_| WalletLedgerError::OutcomeUnknown)
}

/// 仅记录闭合内部分类，避免事件、主体、余额和原因进入日志。
fn internal(error: WalletLedgerError) -> WalletLedgerError {
    let error_kind = match error {
        WalletLedgerError::InvalidInput
        | WalletLedgerError::Conflict
        | WalletLedgerError::InsufficientQuota
        | WalletLedgerError::Overflow => return error,
        WalletLedgerError::Query => "wallet_ledger_query",
        WalletLedgerError::OutcomeUnknown => "wallet_ledger_outcome_unknown",
        WalletLedgerError::Timeout => "wallet_ledger_timeout",
        WalletLedgerError::Invariant => "wallet_ledger_invariant",
    };
    tracing::error!(
        target: "af_db::wallet_ledger",
        error_kind,
        "钱包账本仓储发生内部错误"
    );
    error
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use sea_orm::{ActiveModelTrait, EntityTrait};

    use super::*;
    use crate::{
        AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, MigrationOptions,
        entity::groups,
    };

    #[tokio::test]
    async fn opening_adjustment_pagination_and_idempotency_share_one_balance_history()
    -> Result<(), Box<dyn Error>> {
        let fixture = fixture(100).await?;
        let write = fixture.adjustment(0x81, 25, "补充活动额度")?;
        let applied = fixture.repository.adjust(write).await?;
        assert!(matches!(applied, WalletAdjustmentOutcome::Applied(_)));

        let replay = fixture
            .repository
            .adjust(fixture.adjustment(0x81, 25, "补充活动额度")?)
            .await?;
        assert!(matches!(replay, WalletAdjustmentOutcome::Existing(_)));
        assert_eq!(fixture.balance().await?, 125);

        let WalletBalanceLookupOutcome::Found(balance) =
            fixture.repository.balance(fixture.target_user_id).await?
        else {
            panic!("有效用户必须返回钱包余额快照")
        };
        assert_eq!(balance.balance().units(), 125);
        assert_eq!(balance.used_quota().units(), 0);
        assert_eq!(balance.frozen_quota().units(), 0);

        let WalletLedgerListOutcome::Found(page) = fixture
            .repository
            .list(fixture.target_user_id, None, 1)
            .await?
        else {
            panic!("有效用户必须返回账本页面")
        };
        let (entries, next) = page.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].quota_delta().units(), 25);
        let next = next.expect("opening 基线应产生下一页");

        let WalletLedgerListOutcome::Found(page) = fixture
            .repository
            .list(fixture.target_user_id, Some(next), 10)
            .await?
        else {
            panic!("有效用户必须返回账本页面")
        };
        let (entries, next) = page.into_parts();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].entry_type(),
            WalletLedgerEntryType::OpeningBalance
        );
        assert!(next.is_none());

        let conflict = fixture
            .repository
            .adjust(fixture.adjustment(0x81, 26, "补充活动额度")?)
            .await
            .unwrap_err();
        assert_eq!(conflict, WalletLedgerError::Conflict);
        fixture.pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn adjustment_rejects_underflow_overflow_and_unknown_outcome_replays()
    -> Result<(), Box<dyn Error>> {
        let small = fixture(10).await?;
        assert_eq!(
            small
                .repository
                .adjust(small.adjustment(0x82, -11, "人工扣减")?)
                .await
                .unwrap_err(),
            WalletLedgerError::InsufficientQuota
        );

        small.repository.inject_outcome_unknown_after_commit();
        assert_eq!(
            small
                .repository
                .adjust(small.adjustment(0x83, 5, "人工补充")?)
                .await
                .unwrap_err(),
            WalletLedgerError::OutcomeUnknown
        );
        assert!(matches!(
            small
                .repository
                .adjust(small.adjustment(0x83, 5, "人工补充")?)
                .await?,
            WalletAdjustmentOutcome::Existing(_)
        ));
        assert_eq!(small.balance().await?, 15);
        small.pool.close().await?;

        let maximum = fixture(i64::MAX).await?;
        assert_eq!(
            maximum
                .repository
                .adjust(maximum.adjustment(0x84, 1, "边界补充")?)
                .await
                .unwrap_err(),
            WalletLedgerError::Overflow
        );
        maximum.pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_negative_adjustments_cannot_lose_updates() -> Result<(), Box<dyn Error>> {
        let fixture = fixture(100).await?;
        let first_repository = fixture.repository.clone();
        let second_repository = fixture.repository.clone();
        let first = fixture.adjustment(0x85, -80, "并发扣减一")?;
        let second = fixture.adjustment(0x86, -80, "并发扣减二")?;
        let (first, second) = tokio::join!(
            first_repository.adjust(first),
            second_repository.adjust(second)
        );
        let outcomes = [first, second];
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Ok(WalletAdjustmentOutcome::Applied(_))))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(WalletLedgerError::InsufficientQuota)))
                .count(),
            1
        );
        assert_eq!(fixture.balance().await?, 20);
        fixture.pool.close().await?;
        Ok(())
    }

    struct Fixture {
        pool: DatabasePool,
        repository: WalletLedgerRepository,
        actor_user_id: UserId,
        target_user_id: UserId,
    }

    impl Fixture {
        fn adjustment(
            &self,
            marker: u8,
            delta: i64,
            reason: &str,
        ) -> Result<WalletAdjustmentWrite, WalletLedgerError> {
            WalletAdjustmentWrite::new(
                WalletEventId::new([marker; 16]).expect("测试事件标识必须非零"),
                self.target_user_id,
                self.actor_user_id,
                QuotaDelta::new(delta).expect("测试增量必须可表示"),
                reason.to_owned(),
            )
        }

        async fn balance(&self) -> Result<i64, sea_orm::DbErr> {
            Ok(users::Entity::find_by_id(self.target_user_id.get())
                .one(self.pool.connection())
                .await?
                .expect("测试用户必须存在")
                .quota)
        }
    }

    async fn fixture(initial_quota: i64) -> Result<Fixture, Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = groups::ActiveModel {
            name: Set("wallet-test".to_owned()),
            display_name: Set("钱包测试".to_owned()),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
        let actor = users
            .create(AdminUserCreateRecord::new(
                "wallet-admin".to_owned(),
                None,
                None,
                1,
                1,
                af_domain::GroupId::new(group.id)?,
                0,
                None,
                None,
            ))
            .await?;
        let target = users
            .create(AdminUserCreateRecord::new(
                "wallet-user".to_owned(),
                None,
                None,
                0,
                1,
                af_domain::GroupId::new(group.id)?,
                initial_quota,
                None,
                None,
            ))
            .await?;
        Ok(Fixture {
            repository: WalletLedgerRepository::new(pool.clone(), Duration::from_secs(5))?,
            pool,
            actor_user_id: actor.user_id(),
            target_user_id: target.user_id(),
        })
    }
}
