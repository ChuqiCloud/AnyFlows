use af_domain::{
    BillingReservationId, OrganizationBudgetPolicyId, OrganizationBudgetWindowId, OrganizationId,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationTeamId, Quota, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};

use crate::entity::{
    BillingReservationKey, billing_organization_reservations, organization_memberships,
    organization_wallets, organizations,
};
use crate::organization_wallet;

use super::{
    policy,
    types::{
        OrganizationBillingMutationOutcome, OrganizationBillingPrincipal,
        OrganizationBillingReservationRecord, OrganizationBillingReservationStatus,
        OrganizationBudgetAllocationRecord, OrganizationBudgetRepositoryError,
        OrganizationBudgetScope,
    },
    window,
};

pub(super) async fn reserve(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
    principal: OrganizationBillingPrincipal,
    amount: Quota,
    expires_at: TimeDateTimeWithTimeZone,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    if amount.is_zero() {
        return Err(OrganizationBudgetRepositoryError::InvalidInput);
    }
    let now = TimeDateTimeWithTimeZone::now_utc();
    if expires_at <= now {
        return Err(OrganizationBudgetRepositoryError::InvalidInput);
    }
    let wallet = lock_wallet(transaction, principal.organization_id()).await?;
    validate_principal(transaction, principal).await?;
    if wallet.billing_protected {
        return Err(OrganizationBudgetRepositoryError::BillingProtected);
    }
    if wallet.quota < amount.units() {
        return Err(OrganizationBudgetRepositoryError::WalletQuotaInsufficient);
    }

    let organization = reserve_scope(
        transaction,
        principal.organization_id(),
        OrganizationBudgetScope::Organization,
        amount,
        now,
    )
    .await?;
    let team = match principal.team_id() {
        Some(team_id) => {
            reserve_scope(
                transaction,
                principal.organization_id(),
                OrganizationBudgetScope::Team(team_id),
                amount,
                now,
            )
            .await?
        }
        None => None,
    };
    let member = reserve_scope(
        transaction,
        principal.organization_id(),
        OrganizationBudgetScope::Member(principal.membership_id()),
        amount,
        now,
    )
    .await?;

    update_wallet_reserve(transaction, &wallet, amount, now).await?;
    let key = BillingReservationKey::parse(&id.persistence_key())
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    billing_organization_reservations::ActiveModel {
        idempotency_key: Set(key),
        organization_id: Set(principal.organization_id().get()),
        membership_id: Set(principal.membership_id().get()),
        team_id: Set(principal.team_id().map(OrganizationTeamId::get)),
        user_id: Set(principal.user_id().get()),
        status: Set(OrganizationBillingReservationStatus::Reserved.code()),
        reserved_quota: Set(amount.units()),
        actual_quota: Set(None),
        organization_policy_id: Set(organization.map(|value| value.policy_id.get())),
        organization_window_id: Set(organization.map(|value| value.window_id.get())),
        organization_policy_version: Set(organization.map(|value| value.policy_version)),
        organization_window_version: Set(organization.map(|value| value.window_version)),
        organization_reserved_quota: Set(organization.map(|value| value.reserved_quota.units())),
        team_policy_id: Set(team.map(|value| value.policy_id.get())),
        team_window_id: Set(team.map(|value| value.window_id.get())),
        team_policy_version: Set(team.map(|value| value.policy_version)),
        team_window_version: Set(team.map(|value| value.window_version)),
        team_reserved_quota: Set(team.map(|value| value.reserved_quota.units())),
        member_policy_id: Set(member.map(|value| value.policy_id.get())),
        member_window_id: Set(member.map(|value| value.window_id.get())),
        member_policy_version: Set(member.map(|value| value.policy_version)),
        member_window_version: Set(member.map(|value| value.window_version)),
        member_reserved_quota: Set(member.map(|value| value.reserved_quota.units())),
        expires_at: Set(expires_at),
        finalized_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(transaction)
    .await
    .map_err(|error| {
        if error
            .sql_err()
            .is_some_and(|error| matches!(error, sea_orm::SqlErr::UniqueConstraintViolation(_)))
        {
            OrganizationBudgetRepositoryError::Conflict
        } else {
            OrganizationBudgetRepositoryError::Query
        }
    })?;
    Ok(OrganizationBillingMutationOutcome::Applied(
        OrganizationBillingReservationStatus::Reserved,
    ))
}

pub(super) async fn settle(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
    actual: Quota,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id())
        .await
        .map_err(|error| settle_diagnostic("lock_wallet", error))?;
    let supplemental = actual
        .units()
        .checked_sub(observed.reserved_quota.units())
        .unwrap_or_default();
    if supplemental > wallet.quota {
        return Err(OrganizationBudgetRepositoryError::SettlementPending);
    }

    if let Some(allocation) = observed.organization {
        window::settle(transaction, allocation, actual, now)
            .await
            .map_err(|error| settle_diagnostic("organization_window", error))?;
    }
    if let Some(allocation) = observed.team {
        window::settle(transaction, allocation, actual, now)
            .await
            .map_err(|error| settle_diagnostic("team_window", error))?;
    }
    if let Some(allocation) = observed.member {
        window::settle(transaction, allocation, actual, now)
            .await
            .map_err(|error| settle_diagnostic("member_window", error))?;
    }

    let current = lock_reservation(transaction, observed.id)
        .await
        .map_err(|error| settle_diagnostic("lock_reservation", error))?;
    let current = record(current).map_err(|error| settle_diagnostic("record", error))?;
    if !same_snapshot(&current, observed) {
        settle_diagnostic("snapshot", OrganizationBudgetRepositoryError::Invariant);
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    match current.status {
        OrganizationBillingReservationStatus::Settled if current.actual_quota == Some(actual) => {
            return Ok(OrganizationBillingMutationOutcome::Existing(
                OrganizationBillingReservationStatus::Settled,
            ));
        }
        OrganizationBillingReservationStatus::Reserved => {}
        OrganizationBillingReservationStatus::SettlementPending
            if current.actual_quota == Some(actual) => {}
        OrganizationBillingReservationStatus::SettlementPending
        | OrganizationBillingReservationStatus::Settled
        | OrganizationBillingReservationStatus::Released => {
            return Err(OrganizationBudgetRepositoryError::Conflict);
        }
    }
    let (balance_before, balance_after) =
        update_wallet_settle(transaction, &wallet, observed.reserved_quota, actual, now)
            .await
            .map_err(|error| settle_diagnostic("wallet", error))?;
    organization_wallet::append_usage_settlement_in_transaction(
        transaction,
        observed.principal.organization_id(),
        observed.id,
        actual,
        balance_before,
        balance_after,
        now,
    )
    .await
    .map_err(|error| {
        settle_diagnostic(
            "ledger",
            match error {
                crate::OrganizationWalletError::Query => OrganizationBudgetRepositoryError::Query,
                crate::OrganizationWalletError::InvalidInput
                | crate::OrganizationWalletError::Conflict
                | crate::OrganizationWalletError::InsufficientQuota
                | crate::OrganizationWalletError::Overflow
                | crate::OrganizationWalletError::OutcomeUnknown
                | crate::OrganizationWalletError::Timeout
                | crate::OrganizationWalletError::Invariant => {
                    OrganizationBudgetRepositoryError::Invariant
                }
            },
        )
    })?;
    let result = billing_organization_reservations::Entity::update_many()
        .filter(
            billing_organization_reservations::Column::IdempotencyKey
                .eq(observed.id.persistence_key()),
        )
        .filter(billing_organization_reservations::Column::Status.is_in([
            OrganizationBillingReservationStatus::Reserved.code(),
            OrganizationBillingReservationStatus::SettlementPending.code(),
        ]))
        .col_expr(
            billing_organization_reservations::Column::Status,
            Expr::value(OrganizationBillingReservationStatus::Settled.code()),
        )
        .col_expr(
            billing_organization_reservations::Column::ActualQuota,
            Expr::value(actual.units()),
        )
        .col_expr(
            billing_organization_reservations::Column::FinalizedAt,
            Expr::value(now),
        )
        .col_expr(
            billing_organization_reservations::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        settle_diagnostic(
            "reservation_update",
            OrganizationBudgetRepositoryError::Invariant,
        );
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    clear_protection_if_safe(transaction, observed.principal.organization_id(), now)
        .await
        .map_err(|error| settle_diagnostic("clear_protection", error))?;
    Ok(OrganizationBillingMutationOutcome::Applied(
        OrganizationBillingReservationStatus::Settled,
    ))
}

#[cfg(test)]
fn settle_diagnostic(
    stage: &'static str,
    error: OrganizationBudgetRepositoryError,
) -> OrganizationBudgetRepositoryError {
    eprintln!("[DEBUG-organization-budget-settle] stage={stage} error={error:?}");
    error
}

#[cfg(not(test))]
const fn settle_diagnostic(
    _stage: &'static str,
    error: OrganizationBudgetRepositoryError,
) -> OrganizationBudgetRepositoryError {
    error
}

pub(super) async fn release(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id()).await?;
    if let Some(allocation) = observed.organization {
        window::release(transaction, allocation, now).await?;
    }
    if let Some(allocation) = observed.team {
        window::release(transaction, allocation, now).await?;
    }
    if let Some(allocation) = observed.member {
        window::release(transaction, allocation, now).await?;
    }
    let current = record(lock_reservation(transaction, observed.id).await?)?;
    if !same_snapshot(&current, observed) {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    match current.status {
        OrganizationBillingReservationStatus::Released => {
            return Ok(OrganizationBillingMutationOutcome::Existing(
                OrganizationBillingReservationStatus::Released,
            ));
        }
        OrganizationBillingReservationStatus::Reserved => {}
        OrganizationBillingReservationStatus::SettlementPending
        | OrganizationBillingReservationStatus::Settled => {
            return Err(OrganizationBudgetRepositoryError::Conflict);
        }
    }
    update_wallet_release(transaction, &wallet, observed.reserved_quota, now).await?;
    let result = billing_organization_reservations::Entity::update_many()
        .filter(
            billing_organization_reservations::Column::IdempotencyKey
                .eq(observed.id.persistence_key()),
        )
        .filter(
            billing_organization_reservations::Column::Status
                .eq(OrganizationBillingReservationStatus::Reserved.code()),
        )
        .col_expr(
            billing_organization_reservations::Column::Status,
            Expr::value(OrganizationBillingReservationStatus::Released.code()),
        )
        .col_expr(
            billing_organization_reservations::Column::FinalizedAt,
            Expr::value(now),
        )
        .col_expr(
            billing_organization_reservations::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(OrganizationBillingMutationOutcome::Applied(
        OrganizationBillingReservationStatus::Released,
    ))
}

pub(super) async fn mark_pending_and_protect(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
    actual: Quota,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id()).await?;
    let current = record(lock_reservation(transaction, observed.id).await?)?;
    if !same_snapshot(&current, observed) {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    match current.status {
        OrganizationBillingReservationStatus::SettlementPending
            if current.actual_quota == Some(actual) =>
        {
            return Ok(());
        }
        OrganizationBillingReservationStatus::Reserved => {}
        _ => return Err(OrganizationBudgetRepositoryError::Conflict),
    }
    let reservation = billing_organization_reservations::Entity::update_many()
        .filter(
            billing_organization_reservations::Column::IdempotencyKey
                .eq(observed.id.persistence_key()),
        )
        .filter(
            billing_organization_reservations::Column::Status
                .eq(OrganizationBillingReservationStatus::Reserved.code()),
        )
        .col_expr(
            billing_organization_reservations::Column::Status,
            Expr::value(OrganizationBillingReservationStatus::SettlementPending.code()),
        )
        .col_expr(
            billing_organization_reservations::Column::ActualQuota,
            Expr::value(actual.units()),
        )
        .col_expr(
            billing_organization_reservations::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if reservation.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    let protected = organization_wallets::Entity::update_many()
        .filter(organization_wallets::Column::Id.eq(wallet.id))
        .filter(organization_wallets::Column::Version.eq(wallet.version))
        .col_expr(
            organization_wallets::Column::BillingProtected,
            Expr::value(true),
        )
        .col_expr(
            organization_wallets::Column::Version,
            Expr::value(
                wallet
                    .version
                    .checked_add(1)
                    .ok_or(OrganizationBudgetRepositoryError::Invariant)?,
            ),
        )
        .col_expr(organization_wallets::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if protected.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

pub(super) async fn load<C>(
    connection: &C,
    id: BillingReservationId,
) -> Result<Option<OrganizationBillingReservationRecord>, OrganizationBudgetRepositoryError>
where
    C: sea_orm::ConnectionTrait,
{
    billing_organization_reservations::Entity::find_by_id(
        BillingReservationKey::parse(&id.persistence_key())
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
    )
    .one(connection)
    .await
    .map_err(|_| OrganizationBudgetRepositoryError::Query)?
    .map(record)
    .transpose()
}

async fn reserve_scope(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    scope: OrganizationBudgetScope,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<Option<OrganizationBudgetAllocationRecord>, OrganizationBudgetRepositoryError> {
    match policy::active_policy(transaction, organization_id, scope, now).await? {
        Some(policy) => window::reserve(transaction, policy, amount, now).await,
        None => Ok(None),
    }
}

async fn validate_principal(
    transaction: &DatabaseTransaction,
    principal: OrganizationBillingPrincipal,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let membership = organization_memberships::Entity::find_by_id(principal.membership_id().get())
        .filter(
            organization_memberships::Column::OrganizationId.eq(principal.organization_id().get()),
        )
        .filter(organization_memberships::Column::UserId.eq(principal.user_id().get()))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::NotFound)?;
    let status = OrganizationMembershipStatus::try_from(membership.status)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    if !status.is_active() || membership.team_id != principal.team_id().map(OrganizationTeamId::get)
    {
        return Err(OrganizationBudgetRepositoryError::NotFound);
    }
    Ok(())
}

async fn lock_wallet(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organization_wallets::Model, OrganizationBudgetRepositoryError> {
    let organization_exists = organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16))
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .is_some();
    if !organization_exists {
        return Err(OrganizationBudgetRepositoryError::NotFound);
    }
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_wallets::Entity::update_many()
            .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
            .col_expr(
                organization_wallets::Column::Quota,
                Expr::col(organization_wallets::Column::Quota).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationBudgetRepositoryError::NotFound);
        }
    }
    let mut query = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let wallet = query
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::NotFound)?;
    if wallet.id <= 0
        || wallet.version <= 0
        || wallet.quota < 0
        || wallet.used_quota < 0
        || wallet.frozen_quota < 0
    {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(wallet)
}

async fn lock_reservation(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
) -> Result<billing_organization_reservations::Model, OrganizationBudgetRepositoryError> {
    let key = BillingReservationKey::parse(&id.persistence_key())
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let mut query = billing_organization_reservations::Entity::find_by_id(key);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .ok_or(OrganizationBudgetRepositoryError::NotFound)
}

async fn update_wallet_reserve(
    transaction: &DatabaseTransaction,
    wallet: &organization_wallets::Model,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let quota = wallet
        .quota
        .checked_sub(amount.units())
        .ok_or(OrganizationBudgetRepositoryError::WalletQuotaInsufficient)?;
    let frozen = wallet
        .frozen_quota
        .checked_add(amount.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    update_wallet(
        transaction,
        wallet,
        quota,
        wallet.used_quota,
        frozen,
        false,
        now,
    )
    .await
}

async fn update_wallet_settle(
    transaction: &DatabaseTransaction,
    wallet: &organization_wallets::Model,
    reserved: Quota,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(Quota, Quota), OrganizationBudgetRepositoryError> {
    let released = reserved
        .units()
        .checked_sub(actual.units())
        .unwrap_or_default();
    let supplemental = actual
        .units()
        .checked_sub(reserved.units())
        .unwrap_or_default();
    let quota = wallet
        .quota
        .checked_add(released)
        .and_then(|value| value.checked_sub(supplemental))
        .ok_or(OrganizationBudgetRepositoryError::WalletQuotaInsufficient)?;
    let used = wallet
        .used_quota
        .checked_add(actual.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let frozen = wallet
        .frozen_quota
        .checked_sub(reserved.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let balance_before = wallet
        .quota
        .checked_add(reserved.units())
        .and_then(|value| Quota::new(value).ok())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let balance_after =
        Quota::new(quota).map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    update_wallet(
        transaction,
        wallet,
        quota,
        used,
        frozen,
        wallet.billing_protected,
        now,
    )
    .await?;
    Ok((balance_before, balance_after))
}

async fn update_wallet_release(
    transaction: &DatabaseTransaction,
    wallet: &organization_wallets::Model,
    reserved: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let quota = wallet
        .quota
        .checked_add(reserved.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let frozen = wallet
        .frozen_quota
        .checked_sub(reserved.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    update_wallet(
        transaction,
        wallet,
        quota,
        wallet.used_quota,
        frozen,
        wallet.billing_protected,
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_wallet(
    transaction: &DatabaseTransaction,
    wallet: &organization_wallets::Model,
    quota: i64,
    used: i64,
    frozen: i64,
    protected: bool,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let next_version = wallet
        .version
        .checked_add(1)
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
    let result = organization_wallets::Entity::update_many()
        .filter(organization_wallets::Column::Id.eq(wallet.id))
        .filter(organization_wallets::Column::Version.eq(wallet.version))
        .filter(organization_wallets::Column::Quota.eq(wallet.quota))
        .filter(organization_wallets::Column::UsedQuota.eq(wallet.used_quota))
        .filter(organization_wallets::Column::FrozenQuota.eq(wallet.frozen_quota))
        .col_expr(organization_wallets::Column::Quota, Expr::value(quota))
        .col_expr(organization_wallets::Column::UsedQuota, Expr::value(used))
        .col_expr(
            organization_wallets::Column::FrozenQuota,
            Expr::value(frozen),
        )
        .col_expr(
            organization_wallets::Column::BillingProtected,
            Expr::value(protected),
        )
        .col_expr(
            organization_wallets::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(organization_wallets::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    if result.rows_affected != 1 {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(())
}

async fn clear_protection_if_safe(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let pending = billing_organization_reservations::Entity::find()
        .filter(billing_organization_reservations::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            billing_organization_reservations::Column::Status
                .eq(OrganizationBillingReservationStatus::SettlementPending.code()),
        )
        .one(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?
        .is_some();
    if pending {
        return Ok(());
    }
    organization_wallets::Entity::update_many()
        .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_wallets::Column::BillingProtected.eq(true))
        .col_expr(
            organization_wallets::Column::BillingProtected,
            Expr::value(false),
        )
        .col_expr(
            organization_wallets::Column::Version,
            Expr::col(organization_wallets::Column::Version).add(1_i64),
        )
        .col_expr(organization_wallets::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .await
        .map(|_| ())
        .map_err(|_| OrganizationBudgetRepositoryError::Query)
}

fn record(
    model: billing_organization_reservations::Model,
) -> Result<OrganizationBillingReservationRecord, OrganizationBudgetRepositoryError> {
    let id = BillingReservationId::from_persistence_key(model.idempotency_key.as_str())
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let principal = OrganizationBillingPrincipal::new(
        OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        OrganizationMembershipId::new(model.membership_id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        model
            .team_id
            .map(OrganizationTeamId::new)
            .transpose()
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
        UserId::new(model.user_id).map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
    );
    let status = OrganizationBillingReservationStatus::from_code(model.status)?;
    let reserved_quota = Quota::new(model.reserved_quota)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    if reserved_quota.is_zero() {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    let actual_quota = model
        .actual_quota
        .map(Quota::new)
        .transpose()
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let finalized = model.finalized_at.is_some();
    let valid_state = match status {
        OrganizationBillingReservationStatus::Reserved => actual_quota.is_none() && !finalized,
        OrganizationBillingReservationStatus::SettlementPending => {
            actual_quota.is_some() && !finalized
        }
        OrganizationBillingReservationStatus::Settled => actual_quota.is_some() && finalized,
        OrganizationBillingReservationStatus::Released => actual_quota.is_none() && finalized,
    };
    if !valid_state {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    Ok(OrganizationBillingReservationRecord {
        id,
        principal,
        status,
        reserved_quota,
        actual_quota,
        organization: allocation_record(
            model.organization_policy_id,
            model.organization_window_id,
            model.organization_policy_version,
            model.organization_window_version,
            model.organization_reserved_quota,
        )?,
        team: allocation_record(
            model.team_policy_id,
            model.team_window_id,
            model.team_policy_version,
            model.team_window_version,
            model.team_reserved_quota,
        )?,
        member: allocation_record(
            model.member_policy_id,
            model.member_window_id,
            model.member_policy_version,
            model.member_window_version,
            model.member_reserved_quota,
        )?,
    })
}

fn allocation_record(
    policy_id: Option<i64>,
    window_id: Option<i64>,
    policy_version: Option<i64>,
    window_version: Option<i64>,
    reserved_quota: Option<i64>,
) -> Result<Option<OrganizationBudgetAllocationRecord>, OrganizationBudgetRepositoryError> {
    match (
        policy_id,
        window_id,
        policy_version,
        window_version,
        reserved_quota,
    ) {
        (None, None, None, None, None) => Ok(None),
        (
            Some(policy_id),
            Some(window_id),
            Some(policy_version),
            Some(window_version),
            Some(quota),
        ) if policy_version > 0 && window_version > 0 && quota > 0 => {
            Ok(Some(OrganizationBudgetAllocationRecord {
                policy_id: OrganizationBudgetPolicyId::new(policy_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
                window_id: OrganizationBudgetWindowId::new(window_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
                policy_version,
                window_version,
                reserved_quota: Quota::new(quota)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            }))
        }
        _ => Err(OrganizationBudgetRepositoryError::Invariant),
    }
}

fn same_snapshot(
    left: &OrganizationBillingReservationRecord,
    right: &OrganizationBillingReservationRecord,
) -> bool {
    left.id == right.id
        && left.principal == right.principal
        && left.reserved_quota == right.reserved_quota
        && left.organization == right.organization
        && left.team == right.team
        && left.member == right.member
}
