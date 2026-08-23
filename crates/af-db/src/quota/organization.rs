use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use sea_orm::{ConnectionTrait, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone};

use crate::organization_budget::{
    OrganizationBillingMutationOutcome, OrganizationBillingPrincipal,
    OrganizationBillingReservationRecord, OrganizationBillingReservationStatus,
    OrganizationBudgetRepositoryError, billing as organization_billing,
};

use super::{
    QuotaRepository, commit_applied, group_windows, load_reservation, lock_group, lock_token,
    lock_user, quota_from_db, rollback_with_error, sql,
    state::{
        QuotaMutationOutcome, QuotaRepositoryError, QuotaReservationKind, QuotaReservationStatus,
        ReservationState,
    },
    subscription, token_matches_principal, token_windows,
};

/// 在同一事务中冻结平台窗口、企业钱包、三层预算与企业 Key 额度。
pub(super) async fn precharge(
    repository: &QuotaRepository,
    id: BillingReservationId,
    principal: GatewayPrincipal,
    amount: Quota,
    reservation_kind: QuotaReservationKind,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    let organization = billing_principal(principal)?;
    let key = id.persistence_key();
    let (now, expires_at) = super::reservation_times(repository.reservation_ttl)?;
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    let backend = transaction.get_database_backend();

    let group_state = match lock_group(&transaction, principal.group_id().get()).await {
        Ok(state) => state,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    match organization_billing::reserve_gateway(
        &transaction,
        id,
        organization,
        principal.token_id(),
        principal.group_id(),
        amount,
        expires_at,
    )
    .await
    {
        Ok(OrganizationBillingMutationOutcome::Applied(
            OrganizationBillingReservationStatus::Reserved,
        )) => {}
        Ok(_) => {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        Err(OrganizationBudgetRepositoryError::Conflict) => {
            transaction
                .rollback()
                .await
                .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
            return existing_precharge(repository, id, principal, amount, reservation_kind).await;
        }
        Err(error) => {
            return rollback_with_error(transaction, map_organization_error(error)).await;
        }
    }

    let user_state = match lock_user(&transaction, principal.user_id().get()).await {
        Ok(state) => state,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let token_state = match lock_token(&transaction, principal.token_id().get()).await {
        Ok(state) if token_matches_principal(&state, principal) => state,
        Ok(_) => return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if user_state.used_quota < 0 {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }

    match transaction
        .execute(backend.build(&sql::insert_reservation(
            key.clone(),
            principal,
            amount,
            reservation_kind,
            now,
            expires_at,
        )))
        .await
    {
        Ok(result) if result.rows_affected() == 1 => {}
        Ok(_) => return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await,
        Err(error) if super::is_unique_violation(&error) => {
            transaction
                .rollback()
                .await
                .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
            return existing_precharge(repository, id, principal, amount, reservation_kind).await;
        }
        Err(_) => return rollback_with_error(transaction, QuotaRepositoryError::Query).await,
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

    let token_result = transaction
        .execute(backend.build(&sql::precharge_token(principal, amount, now)))
        .await;
    match token_result {
        Ok(result) if result.rows_affected() == 1 => {}
        Ok(_) => {
            let error = match super::classify_token_precharge(&transaction, principal, amount).await
            {
                Ok(error) => error,
                Err(error) => return rollback_with_error(transaction, error).await,
            };
            return rollback_with_error(transaction, error).await;
        }
        Err(_) => return rollback_with_error(transaction, QuotaRepositoryError::Query).await,
    }

    if reservation_kind.uses_token_windows()
        && let Err(error) =
            token_windows::reserve(&transaction, &key, &parent, token_state, amount, now).await
    {
        return rollback_with_error(transaction, error).await;
    }
    if !token_state.unlimited_quota {
        let shared = match transaction
            .execute(backend.build(&sql::snapshot_token_reservation(key.clone(), amount, now)))
            .await
        {
            Ok(shared) => shared,
            Err(_) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Query).await;
            }
        };
        if shared.rows_affected() != 1 {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        if let Err(error) =
            organization_billing::snapshot_gateway_token_reservation(&transaction, id, amount, now)
                .await
        {
            return rollback_with_error(transaction, map_organization_error(error)).await;
        }
    }
    commit_applied(transaction).await
}

/// 结算企业请求；任何可恢复的补扣不足都会同步固化双预留待结算状态。
pub(super) async fn settle(
    repository: &QuotaRepository,
    id: BillingReservationId,
    observed: ReservationState,
    actual: Quota,
    reservation_kind: QuotaReservationKind,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    let organization_observed = load_organization(repository, id).await?;
    ensure_pair(&observed, &organization_observed)?;
    match observed.status {
        QuotaReservationStatus::Settled => {
            if observed.matches_actual(actual) {
                return Ok(QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::Settled,
                ));
            }
            return Err(QuotaRepositoryError::Conflict);
        }
        QuotaReservationStatus::Refunded => {
            return Err(QuotaRepositoryError::Conflict);
        }
        QuotaReservationStatus::SettlementPending => {
            if !observed.matches_actual(actual) {
                return Err(QuotaRepositoryError::Conflict);
            }
        }
        QuotaReservationStatus::Reserved => {}
    }

    let key = id.persistence_key();
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    let backend = transaction.get_database_backend();
    let group_state = match lock_group(&transaction, observed.group_id).await {
        Ok(state) => state,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if let Err(error) =
        organization_billing::mark_pending_and_protect(&transaction, &organization_observed, actual)
            .await
    {
        return rollback_with_error(transaction, map_organization_error(error)).await;
    }
    let user_state = match lock_user(&transaction, observed.user_id).await {
        Ok(state) => state,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let principal = match gateway_principal(&organization_observed) {
        Ok(principal) => principal,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let token_state = match lock_token(&transaction, observed.token_id).await {
        Ok(state) if token_matches_principal(&state, principal) => state,
        Ok(_) => return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let mut state = match load_reservation(&transaction, key.clone(), true).await {
        Ok(Some(state)) if state.matches_immutable_snapshot(&observed) => state,
        Ok(Some(_)) | Ok(None) => {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if state.reservation_kind != reservation_kind {
        return rollback_with_error(transaction, QuotaRepositoryError::Conflict).await;
    }
    if state.status == QuotaReservationStatus::Reserved {
        let transition = transaction
            .execute(backend.build(&sql::begin_settlement(
                key.clone(),
                actual,
                TimeDateTimeWithTimeZone::now_utc(),
            )))
            .await;
        if !matches!(transition, Ok(ref result) if result.rows_affected() == 1) {
            let error = if transition.is_err() {
                QuotaRepositoryError::Query
            } else {
                QuotaRepositoryError::Invariant
            };
            return rollback_with_error(transaction, error).await;
        }
        state = match load_reservation(&transaction, key.clone(), true).await {
            Ok(Some(state)) => state,
            Ok(None) => {
                return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
            }
            Err(error) => return rollback_with_error(transaction, error).await,
        };
    }
    if state.status != QuotaReservationStatus::SettlementPending
        || !state.matches_actual(actual)
        || !state.matches_immutable_snapshot(&observed)
    {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }
    let organization_current = match organization_billing::load(&transaction, id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        Err(error) => {
            return rollback_with_error(transaction, map_organization_error(error)).await;
        }
    };
    if let Err(error) = ensure_pair(&state, &organization_current) {
        return rollback_with_error(transaction, error).await;
    }
    let subscription_allocation =
        match subscription::load_allocation(&transaction, &key, &state).await {
            Ok(allocation) => allocation,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
    if subscription_allocation.is_some() {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }
    let group_allocation = match group_windows::load_allocation(&transaction, &key, &state).await {
        Ok(allocation) => allocation,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let token_allocation = match token_windows::load_allocation(&transaction, &key, &state).await {
        Ok(allocation) => allocation,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let reserved = match quota_from_db(state.reserved_quota) {
        Ok(quota) => quota,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let token_reserved = match quota_from_db(state.token_reserved_quota) {
        Ok(quota) => quota,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let now = TimeDateTimeWithTimeZone::now_utc();
    let savepoint = match transaction.begin().await {
        Ok(savepoint) => savepoint,
        Err(_) => return rollback_with_error(transaction, QuotaRepositoryError::Query).await,
    };
    let adjustment = async {
        group_windows::apply_settlement(
            &savepoint,
            &state,
            group_allocation.as_ref(),
            group_state,
            actual,
            now,
        )
        .await?;
        organization_billing::settle(&savepoint, &organization_current, actual)
            .await
            .map_err(map_organization_error)?;
        let user = savepoint
            .execute(backend.build(&sql::settle_organization_user(state.user_id, actual, now)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if user.rows_affected() != 1
            || user_state.used_quota > i64::MAX - actual.units()
            || user_state.request_count == i64::MAX
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        let token = savepoint
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
        if token.rows_affected() != 1 {
            return Err(super::classify_token_settlement(
                &savepoint,
                &state,
                token_reserved,
                reserved,
                actual,
            )
            .await?);
        }
        token_windows::apply_settlement(
            &savepoint,
            &state,
            token_allocation.as_ref(),
            token_state,
            actual,
            now,
        )
        .await?;
        let finalized = savepoint
            .execute(backend.build(&sql::finalize_settlement(key.clone(), actual, now)))
            .await
            .map_err(|_| QuotaRepositoryError::Query)?;
        if finalized.rows_affected() != 1 {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(())
    }
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
            | QuotaRepositoryError::TokenQuotaInsufficient
            | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
            | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }),
        ) => {
            savepoint
                .rollback()
                .await
                .map_err(|_| QuotaRepositoryError::OutcomeUnknown)?;
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

/// 释放企业请求冻结额度；个人钱包从未参与，因此只退企业钱包与有限 Key。
pub(super) async fn refund(
    repository: &QuotaRepository,
    id: BillingReservationId,
    observed: ReservationState,
    reservation_kind: QuotaReservationKind,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    let organization_observed = load_organization(repository, id).await?;
    ensure_pair(&observed, &organization_observed)?;
    if observed.status == QuotaReservationStatus::Refunded {
        return Ok(QuotaMutationOutcome::Existing(
            QuotaReservationStatus::Refunded,
        ));
    }
    if observed.status != QuotaReservationStatus::Reserved {
        return Err(QuotaRepositoryError::Conflict);
    }

    let key = id.persistence_key();
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| QuotaRepositoryError::Query)?;
    let backend = transaction.get_database_backend();
    if let Err(error) = lock_group(&transaction, observed.group_id).await {
        return rollback_with_error(transaction, error).await;
    }
    match organization_billing::release(&transaction, &organization_observed).await {
        Ok(OrganizationBillingMutationOutcome::Applied(
            OrganizationBillingReservationStatus::Released,
        )) => {}
        Ok(_) => return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await,
        Err(error) => {
            return rollback_with_error(transaction, map_organization_error(error)).await;
        }
    }
    if let Err(error) = lock_user(&transaction, observed.user_id).await {
        return rollback_with_error(transaction, error).await;
    }
    let principal = match gateway_principal(&organization_observed) {
        Ok(principal) => principal,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if let Err(error) = lock_token(&transaction, observed.token_id)
        .await
        .and_then(|state| {
            token_matches_principal(&state, principal)
                .then_some(())
                .ok_or(QuotaRepositoryError::Invariant)
        })
    {
        return rollback_with_error(transaction, error).await;
    }
    let mut state = match load_reservation(&transaction, key.clone(), true).await {
        Ok(Some(state)) => state,
        Ok(None) => return rollback_with_error(transaction, QuotaRepositoryError::NotFound).await,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if !state.matches_immutable_snapshot(&observed) || state.reservation_kind != reservation_kind {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }
    let subscription_allocation =
        match subscription::load_allocation(&transaction, &key, &state).await {
            Ok(allocation) => allocation,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
    if subscription_allocation.is_some() {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }
    if let Err(error) = group_windows::load_allocation(&transaction, &key, &state).await {
        return rollback_with_error(transaction, error).await;
    }
    if let Err(error) = token_windows::load_allocation(&transaction, &key, &state).await {
        return rollback_with_error(transaction, error).await;
    }
    let transition = transaction
        .execute(backend.build(&sql::begin_refund(
            key.clone(),
            TimeDateTimeWithTimeZone::now_utc(),
        )))
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
            return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
        }
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    if !state.matches_immutable_snapshot(&observed)
        || state.status != QuotaReservationStatus::Refunded
    {
        return rollback_with_error(transaction, QuotaRepositoryError::Invariant).await;
    }
    let token_reserved = match quota_from_db(state.token_reserved_quota) {
        Ok(quota) => quota,
        Err(error) => return rollback_with_error(transaction, error).await,
    };
    let token = transaction
        .execute(backend.build(&sql::refund_token(
            state.token_id,
            state.user_id,
            token_reserved,
            TimeDateTimeWithTimeZone::now_utc(),
        )))
        .await;
    if !matches!(token, Ok(ref result) if result.rows_affected() == 1) {
        let error = if token.is_err() {
            QuotaRepositoryError::Query
        } else {
            QuotaRepositoryError::Invariant
        };
        return rollback_with_error(transaction, error).await;
    }
    commit_applied(transaction).await
}

async fn existing_precharge(
    repository: &QuotaRepository,
    id: BillingReservationId,
    principal: GatewayPrincipal,
    amount: Quota,
    reservation_kind: QuotaReservationKind,
) -> Result<QuotaMutationOutcome, QuotaRepositoryError> {
    let key = id.persistence_key();
    let state = load_reservation(repository.pool.connection(), key.clone(), false)
        .await?
        .ok_or(QuotaRepositoryError::Invariant)?;
    let organization = load_organization(repository, id).await?;
    ensure_pair(&state, &organization)?;
    if organization.principal() != billing_principal(principal)?
        || !state.matches_precharge(principal, amount, reservation_kind)
    {
        return Err(QuotaRepositoryError::Conflict);
    }
    if subscription::load_allocation(repository.pool.connection(), &key, &state)
        .await?
        .is_some()
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    group_windows::load_allocation(repository.pool.connection(), &key, &state).await?;
    token_windows::load_allocation(repository.pool.connection(), &key, &state).await?;
    Ok(QuotaMutationOutcome::Existing(state.status))
}

async fn load_organization(
    repository: &QuotaRepository,
    id: BillingReservationId,
) -> Result<OrganizationBillingReservationRecord, QuotaRepositoryError> {
    organization_billing::load(repository.pool.connection(), id)
        .await
        .map_err(map_organization_error)?
        .ok_or(QuotaRepositoryError::Invariant)
}

fn billing_principal(
    principal: GatewayPrincipal,
) -> Result<OrganizationBillingPrincipal, QuotaRepositoryError> {
    let organization = principal
        .organization_principal()
        .ok_or(QuotaRepositoryError::Invariant)?;
    Ok(OrganizationBillingPrincipal::new(
        organization.organization_id(),
        organization.membership_id(),
        organization.team_id(),
        principal.user_id(),
    ))
}

fn gateway_principal(
    reservation: &OrganizationBillingReservationRecord,
) -> Result<GatewayPrincipal, QuotaRepositoryError> {
    let gateway = reservation
        .gateway()
        .ok_or(QuotaRepositoryError::Invariant)?;
    let organization = reservation.principal();
    Ok(GatewayPrincipal::organization(
        gateway.token_id,
        organization.user_id(),
        gateway.group_id,
        af_domain::OrganizationGatewayPrincipal::new(
            organization.organization_id(),
            organization.membership_id(),
            organization.team_id(),
        ),
    ))
}

fn ensure_pair(
    shared: &ReservationState,
    organization: &OrganizationBillingReservationRecord,
) -> Result<(), QuotaRepositoryError> {
    let principal = organization.principal();
    let gateway = organization
        .gateway()
        .ok_or(QuotaRepositoryError::Invariant)?;
    let status_matches = matches!(
        (shared.status, organization.status()),
        (
            QuotaReservationStatus::Reserved,
            OrganizationBillingReservationStatus::Reserved
        ) | (
            QuotaReservationStatus::SettlementPending,
            OrganizationBillingReservationStatus::SettlementPending
        ) | (
            QuotaReservationStatus::Settled,
            OrganizationBillingReservationStatus::Settled
        ) | (
            QuotaReservationStatus::Refunded,
            OrganizationBillingReservationStatus::Released
        )
    );
    if shared.organization_id != Some(principal.organization_id().get())
        || shared.user_id != principal.user_id().get()
        || shared.token_id != gateway.token_id.get()
        || shared.group_id != gateway.group_id.get()
        || shared.reserved_quota != organization.reserved_quota().units()
        || shared.token_reserved_quota != gateway.token_reserved_quota.units()
        || shared.actual_quota != organization.actual_quota().map(Quota::units)
        || !status_matches
    {
        return Err(QuotaRepositoryError::Invariant);
    }
    Ok(())
}

fn map_organization_error(error: OrganizationBudgetRepositoryError) -> QuotaRepositoryError {
    match error {
        OrganizationBudgetRepositoryError::WalletQuotaInsufficient
        | OrganizationBudgetRepositoryError::BudgetQuotaInsufficient
        | OrganizationBudgetRepositoryError::BillingProtected
        | OrganizationBudgetRepositoryError::SettlementPending => {
            QuotaRepositoryError::OrganizationQuotaInsufficient
        }
        OrganizationBudgetRepositoryError::NotFound => QuotaRepositoryError::NotFound,
        OrganizationBudgetRepositoryError::Conflict => QuotaRepositoryError::Conflict,
        OrganizationBudgetRepositoryError::Query | OrganizationBudgetRepositoryError::Timeout => {
            QuotaRepositoryError::Query
        }
        OrganizationBudgetRepositoryError::OutcomeUnknown => QuotaRepositoryError::OutcomeUnknown,
        OrganizationBudgetRepositoryError::InvalidInput
        | OrganizationBudgetRepositoryError::PermissionDenied
        | OrganizationBudgetRepositoryError::Invariant => QuotaRepositoryError::Invariant,
    }
}
