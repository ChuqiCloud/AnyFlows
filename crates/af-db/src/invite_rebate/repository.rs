use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{Quota, QuotaDelta, UserId, WalletEventId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, WalletLedgerEntryType,
    entity::{WalletLedgerKey, invite_rebate_events, users, wallet_ledger_entries},
};

use super::{
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateRecord, InviteRebateRejection,
    InviteRebateRepositoryConfigError, InviteRebateRepositoryError,
};

/// 原子维护邀请返利审计、邀请人钱包和返利累计字段的仓储。
#[derive(Clone)]
pub struct InviteRebateRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl InviteRebateRepository {
    /// 使用共享连接池和单次读写截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, InviteRebateRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(InviteRebateRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 按事件键和被邀请用户幂等发放注册邀请返利。
    pub async fn grant(
        &self,
        grant: &InviteRebateGrant,
    ) -> Result<InviteRebateGrantOutcome, InviteRebateRepositoryError> {
        let operation = self
            .grant_inner(grant)
            .with_subscriber(NoSubscriber::default());
        let outcome = match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal)?,
            Err(_) => return Err(internal(InviteRebateRepositoryError::OutcomeUnknown)),
        };
        #[cfg(test)]
        if matches!(&outcome, InviteRebateGrantOutcome::Applied(_))
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(internal(InviteRebateRepositoryError::OutcomeUnknown));
        }
        Ok(outcome)
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    async fn grant_inner(
        &self,
        grant: &InviteRebateGrant,
    ) -> Result<InviteRebateGrantOutcome, InviteRebateRepositoryError> {
        if let Some(model) = load_event_by_key(self.pool.connection(), grant.event_id).await? {
            return classify_existing_event(self.pool.connection(), model, grant).await;
        }
        if let Some(model) =
            load_event_by_invitee(self.pool.connection(), grant.invitee_user_id).await?
        {
            validate_existing_event(self.pool.connection(), model).await?;
            return Ok(InviteRebateGrantOutcome::Rejected(
                InviteRebateRejection::AlreadyCredited,
            ));
        }

        let transaction = begin(&self.pool).await?;
        match grant_in_transaction(&transaction, grant).await {
            Ok(InviteRebateGrantOutcome::Applied(record)) => {
                commit(transaction).await?;
                Ok(InviteRebateGrantOutcome::Applied(record))
            }
            Ok(outcome) => {
                rollback(transaction).await?;
                Ok(outcome)
            }
            Err(InviteRebateRepositoryError::Conflict) => {
                rollback(transaction).await?;
                recover_after_collision(self.pool.connection(), grant).await
            }
            Err(error) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }
}

/// 在调用方已经开启的事务内完成邀请返利，不自行提交或回滚。
pub(crate) async fn grant_in_transaction(
    transaction: &DatabaseTransaction,
    grant: &InviteRebateGrant,
) -> Result<InviteRebateGrantOutcome, InviteRebateRepositoryError> {
    let Some(invitee) = lock_user(transaction, grant.invitee_user_id).await? else {
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::InviteeNotFound,
        ));
    };
    let Some(inviter_id) = invitee.inviter_id.and_then(|id| UserId::new(id).ok()) else {
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::NoInviter,
        ));
    };
    let Some(inviter) = lock_user(transaction, inviter_id).await? else {
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::InviterNotFound,
        ));
    };
    if inviter.id == invitee.id {
        return Err(InviteRebateRepositoryError::Invariant);
    }

    if let Some(model) = load_event_by_key(transaction, grant.event_id).await? {
        return classify_existing_event(transaction, model, grant).await;
    }
    if let Some(model) = load_event_by_invitee(transaction, grant.invitee_user_id).await? {
        validate_existing_event(transaction, model).await?;
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::AlreadyCredited,
        ));
    }

    let delta = QuotaDelta::new(grant.quota_amount.units())
        .map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let balance_before =
        Quota::new(inviter.quota).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let balance_after = match balance_before.checked_apply(delta) {
        Ok(value) => value,
        Err(_) => {
            return Ok(InviteRebateGrantOutcome::Rejected(
                InviteRebateRejection::CreditOverflow,
            ));
        }
    };
    if inviter.aff_quota < 0 || inviter.aff_history_quota < 0 {
        return Err(InviteRebateRepositoryError::Invariant);
    }
    let Some(aff_quota) = inviter.aff_quota.checked_add(grant.quota_amount.units()) else {
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::CreditOverflow,
        ));
    };
    let Some(aff_history_quota) = inviter
        .aff_history_quota
        .checked_add(grant.quota_amount.units())
    else {
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::CreditOverflow,
        ));
    };
    let credited_at = to_database_time(grant.credited_at)?;
    let updated_at = inviter
        .updated_at
        .max(TimeDateTimeWithTimeZone::now_utc())
        .max(credited_at);

    let update = users::Entity::update_many()
        .filter(users::Column::Id.eq(inviter.id))
        .filter(users::Column::DeletedAt.is_null())
        .col_expr(users::Column::Quota, Expr::value(balance_after.units()))
        .col_expr(users::Column::AffQuota, Expr::value(aff_quota))
        .col_expr(
            users::Column::AffHistoryQuota,
            Expr::value(aff_history_quota),
        )
        .col_expr(users::Column::UpdatedAt, Expr::value(updated_at))
        .exec(transaction)
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(InviteRebateRepositoryError::Conflict);
    }

    let wallet = wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse(&grant.event_id.persistence_key())
            .map_err(|_| InviteRebateRepositoryError::Invariant)?),
        user_id: Set(inviter.id),
        actor_user_id: Set(None),
        entry_type: Set(WalletLedgerEntryType::InviteRebate as i16),
        quota_delta: Set(grant.quota_amount.units()),
        balance_before: Set(balance_before.units()),
        balance_after: Set(balance_after.units()),
        reason: Set(None),
        created_at: Set(credited_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|error| {
        if is_unique_conflict(&error) {
            InviteRebateRepositoryError::Conflict
        } else {
            InviteRebateRepositoryError::Query
        }
    })?;

    let event = invite_rebate_events::ActiveModel {
        event_key: Set(WalletLedgerKey::parse(&grant.event_id.persistence_key())
            .map_err(|_| InviteRebateRepositoryError::Invariant)?),
        inviter_user_id: Set(inviter.id),
        invitee_user_id: Set(invitee.id),
        quota_amount: Set(grant.quota_amount.units()),
        balance_after: Set(balance_after.units()),
        wallet_ledger_entry_id: Set(wallet.id),
        credited_at: Set(credited_at),
        created_at: Set(credited_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|error| {
        if is_unique_conflict(&error) {
            InviteRebateRepositoryError::Conflict
        } else {
            InviteRebateRepositoryError::Query
        }
    })?;

    Ok(InviteRebateGrantOutcome::Applied(event_record(event)?))
}

async fn recover_after_collision<C>(
    connection: &C,
    grant: &InviteRebateGrant,
) -> Result<InviteRebateGrantOutcome, InviteRebateRepositoryError>
where
    C: ConnectionTrait,
{
    if let Some(model) = load_event_by_key(connection, grant.event_id).await? {
        return classify_existing_event(connection, model, grant).await;
    }
    if let Some(model) = load_event_by_invitee(connection, grant.invitee_user_id).await? {
        validate_existing_event(connection, model).await?;
        return Ok(InviteRebateGrantOutcome::Rejected(
            InviteRebateRejection::AlreadyCredited,
        ));
    }
    Err(InviteRebateRepositoryError::Conflict)
}

async fn classify_existing_event<C>(
    connection: &C,
    model: invite_rebate_events::Model,
    grant: &InviteRebateGrant,
) -> Result<InviteRebateGrantOutcome, InviteRebateRepositoryError>
where
    C: ConnectionTrait,
{
    let record = validate_existing_event(connection, model).await?;
    if record.event_id() == grant.event_id
        && record.invitee_user_id() == grant.invitee_user_id
        && record.quota_amount() == grant.quota_amount
        && record.credited_at() == grant.credited_at
    {
        Ok(InviteRebateGrantOutcome::Existing(record))
    } else {
        Err(InviteRebateRepositoryError::Conflict)
    }
}

async fn validate_existing_event<C>(
    connection: &C,
    model: invite_rebate_events::Model,
) -> Result<InviteRebateRecord, InviteRebateRepositoryError>
where
    C: ConnectionTrait,
{
    let wallet_entry_id = model.wallet_ledger_entry_id;
    let credited_at = model.credited_at;
    let record = event_record(model)?;
    let wallet = wallet_ledger_entries::Entity::find_by_id(wallet_entry_id)
        .one(connection)
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)?
        .ok_or(InviteRebateRepositoryError::Invariant)?;
    let event_key = WalletLedgerKey::parse(&record.event_id().persistence_key())
        .map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let balance_before =
        Quota::new(wallet.balance_before).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let balance_after =
        Quota::new(wallet.balance_after).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let delta =
        QuotaDelta::new(wallet.quota_delta).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    if wallet.event_key != event_key
        || wallet.user_id != record.inviter_user_id().get()
        || wallet.actor_user_id.is_some()
        || wallet.entry_type != WalletLedgerEntryType::InviteRebate as i16
        || wallet.quota_delta != record.quota_amount().units()
        || wallet.reason.is_some()
        || wallet.created_at != credited_at
        || balance_before.checked_apply(delta) != Ok(balance_after)
        || balance_after != record.balance_after()
    {
        return Err(InviteRebateRepositoryError::Invariant);
    }
    Ok(record)
}

async fn load_event_by_key<C>(
    connection: &C,
    event_id: WalletEventId,
) -> Result<Option<invite_rebate_events::Model>, InviteRebateRepositoryError>
where
    C: ConnectionTrait,
{
    invite_rebate_events::Entity::find()
        .filter(
            invite_rebate_events::Column::EventKey
                .eq(WalletLedgerKey::parse(&event_id.persistence_key())
                    .map_err(|_| InviteRebateRepositoryError::Invariant)?),
        )
        .one(connection)
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)
}

async fn load_event_by_invitee<C>(
    connection: &C,
    invitee_user_id: UserId,
) -> Result<Option<invite_rebate_events::Model>, InviteRebateRepositoryError>
where
    C: ConnectionTrait,
{
    invite_rebate_events::Entity::find()
        .filter(invite_rebate_events::Column::InviteeUserId.eq(invitee_user_id.get()))
        .one(connection)
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, InviteRebateRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，恒等更新用于取得同一事务内的写锁。
        let update = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| InviteRebateRepositoryError::Query)?;
        if update.rows_affected == 0 {
            return Ok(None);
        }
    }
    let mut query =
        users::Entity::find_by_id(user_id.get()).filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)
}

fn event_record(
    model: invite_rebate_events::Model,
) -> Result<InviteRebateRecord, InviteRebateRepositoryError> {
    let event_id = WalletEventId::from_persistence_key(model.event_key.as_str())
        .map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let inviter =
        UserId::new(model.inviter_user_id).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let invitee =
        UserId::new(model.invitee_user_id).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let quota =
        Quota::new(model.quota_amount).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let balance_after =
        Quota::new(model.balance_after).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    let credited_at = unix_seconds(model.credited_at)?;
    if model.id <= 0
        || model.wallet_ledger_entry_id <= 0
        || event_id.is_system_opening()
        || inviter == invitee
        || quota.is_zero()
        || balance_after < quota
        || model.created_at < model.credited_at
    {
        return Err(InviteRebateRepositoryError::Invariant);
    }
    Ok(InviteRebateRecord::new(
        event_id,
        inviter,
        invitee,
        quota,
        balance_after,
        credited_at,
    ))
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, InviteRebateRepositoryError> {
    let value = i64::try_from(value).map_err(|_| InviteRebateRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| InviteRebateRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, InviteRebateRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| InviteRebateRepositoryError::Invariant)
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_invite_rebate_events_event_key")
        || rendered.contains("uq_invite_rebate_events_invitee")
        || rendered.contains("uq_wallet_ledger_event_key")
        || rendered.contains("invite_rebate_events.event_key")
        || rendered.contains("invite_rebate_events.invitee_user_id")
        || rendered.contains("wallet_ledger_entries.event_key")
        || rendered.contains("Duplicate entry")
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, InviteRebateRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| InviteRebateRepositoryError::Query)
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), InviteRebateRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| InviteRebateRepositoryError::OutcomeUnknown)
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), InviteRebateRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| InviteRebateRepositoryError::OutcomeUnknown)
}

/// 仅记录闭合内部分类，避免事件、主体和余额进入日志。
fn internal(error: InviteRebateRepositoryError) -> InviteRebateRepositoryError {
    let error_kind = match error {
        InviteRebateRepositoryError::Conflict => return error,
        InviteRebateRepositoryError::Query => "invite_rebate_query",
        InviteRebateRepositoryError::OutcomeUnknown => "invite_rebate_outcome_unknown",
        InviteRebateRepositoryError::Invariant => "invite_rebate_invariant",
    };
    tracing::error!(
        target: "af_db::invite_rebate",
        error_kind,
        "邀请返利仓储发生内部错误"
    );
    error
}

impl fmt::Debug for InviteRebateRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InviteRebateRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}
