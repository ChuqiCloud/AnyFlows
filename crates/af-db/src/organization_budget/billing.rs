use std::collections::{HashMap, HashSet};

use af_domain::{
    BillingReservationId, GroupId, OrganizationBudgetPolicyId, OrganizationBudgetWindowId,
    OrganizationDepartmentBudgetPath, OrganizationDepartmentId,
    OrganizationDepartmentMembershipStatus, OrganizationDepartmentNode,
    OrganizationDepartmentStatus, OrganizationId, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationTeamId, Quota, TokenId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};

use crate::entity::{
    BillingReservationKey, billing_organization_reservation_departments,
    billing_organization_reservations, organization_department_memberships,
    organization_departments, organization_memberships, organization_wallets, organizations,
};
use crate::organization_wallet;

use super::{
    policy,
    types::{
        OrganizationBillingGatewayRecord, OrganizationBillingMutationOutcome,
        OrganizationBillingPrincipal, OrganizationBillingReservationRecord,
        OrganizationBillingReservationStatus, OrganizationBudgetAllocationRecord,
        OrganizationBudgetRepositoryError, OrganizationBudgetScope,
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
    reserve_with_gateway(transaction, id, principal, amount, expires_at, None).await
}

/// 在网关统一事务内冻结企业钱包与全部适用预算层，并固化 Key 和平台分组主体。
pub(crate) async fn reserve_gateway(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
    principal: OrganizationBillingPrincipal,
    token_id: TokenId,
    group_id: GroupId,
    amount: Quota,
    expires_at: TimeDateTimeWithTimeZone,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    reserve_with_gateway(
        transaction,
        id,
        principal,
        amount,
        expires_at,
        Some((token_id, group_id)),
    )
    .await
}

async fn reserve_with_gateway(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
    principal: OrganizationBillingPrincipal,
    amount: Quota,
    expires_at: TimeDateTimeWithTimeZone,
    gateway: Option<(TokenId, GroupId)>,
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
    let (department_path, departments) = match principal.department_id() {
        Some(department_id) => {
            let path =
                load_department_path(transaction, principal.organization_id(), department_id)
                    .await?;
            let mut allocations = Vec::with_capacity(path.nodes().len());
            for department_id in path.department_ids() {
                if let Some(allocation) = reserve_scope(
                    transaction,
                    principal.organization_id(),
                    OrganizationBudgetScope::Department(department_id),
                    amount,
                    now,
                )
                .await?
                {
                    allocations.push(allocation);
                }
            }
            (Some(path), allocations)
        }
        None => (None, Vec::new()),
    };
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
        idempotency_key: Set(key.clone()),
        organization_id: Set(principal.organization_id().get()),
        membership_id: Set(principal.membership_id().get()),
        team_id: Set(principal.team_id().map(OrganizationTeamId::get)),
        user_id: Set(principal.user_id().get()),
        department_path: Set(department_path.as_ref().map(encode_department_path)),
        token_id: Set(gateway.map(|(token_id, _)| token_id.get())),
        group_id: Set(gateway.map(|(_, group_id)| group_id.get())),
        reservation_kind: Set(if gateway.is_some() { 2 } else { 1 }),
        status: Set(OrganizationBillingReservationStatus::Reserved.code()),
        reserved_quota: Set(amount.units()),
        token_reserved_quota: Set(0),
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
    for allocation in &departments {
        let department_id = allocation
            .department_id()
            .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
        let path_index = department_path
            .as_ref()
            .and_then(|path| {
                path.department_ids()
                    .position(|candidate| candidate == department_id)
            })
            .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
        billing_organization_reservation_departments::ActiveModel {
            idempotency_key: Set(key.clone()),
            path_index: Set(i16::try_from(path_index)
                .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?),
            organization_id: Set(principal.organization_id().get()),
            department_id: Set(department_id.get()),
            policy_id: Set(allocation.policy_id().get()),
            window_id: Set(allocation.window_id().get()),
            policy_version: Set(allocation.policy_version()),
            window_version: Set(allocation.window_version()),
            reserved_quota: Set(allocation.reserved_quota().units()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    }
    Ok(OrganizationBillingMutationOutcome::Applied(
        OrganizationBillingReservationStatus::Reserved,
    ))
}

/// 有限 Key 预扣成功后同步企业预留快照，保证双表可独立校验恢复。
pub(crate) async fn snapshot_gateway_token_reservation(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let result = billing_organization_reservations::Entity::update_many()
        .filter(billing_organization_reservations::Column::IdempotencyKey.eq(id.persistence_key()))
        .filter(billing_organization_reservations::Column::ReservationKind.eq(2_i16))
        .filter(
            billing_organization_reservations::Column::Status
                .eq(OrganizationBillingReservationStatus::Reserved.code()),
        )
        .filter(billing_organization_reservations::Column::ReservedQuota.eq(amount.units()))
        .filter(billing_organization_reservations::Column::TokenReservedQuota.eq(0_i64))
        .col_expr(
            billing_organization_reservations::Column::TokenReservedQuota,
            Expr::value(amount.units()),
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
    Ok(())
}

pub(crate) async fn settle(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
    actual: Quota,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id()).await?;
    // 先锁定并核对预留状态，避免重放请求先改写预算窗口再返回冲突。
    let current = lock_reservation_record(transaction, observed.id).await?;
    if !same_snapshot(&current, observed) {
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
    let supplemental = actual
        .units()
        .checked_sub(observed.reserved_quota.units())
        .unwrap_or_default();
    if supplemental > wallet.quota {
        return Err(OrganizationBudgetRepositoryError::SettlementPending);
    }

    if let Some(allocation) = observed.organization {
        window::settle(transaction, allocation, actual, now).await?;
    }
    for allocation in &observed.departments {
        window::settle(transaction, *allocation, actual, now).await?;
    }
    if let Some(allocation) = observed.team {
        window::settle(transaction, allocation, actual, now).await?;
    }
    if let Some(allocation) = observed.member {
        window::settle(transaction, allocation, actual, now).await?;
    }

    let (balance_before, balance_after) =
        update_wallet_settle(transaction, &wallet, observed.reserved_quota, actual, now).await?;
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
    .map_err(|error| match error {
        crate::OrganizationWalletError::Query => OrganizationBudgetRepositoryError::Query,
        crate::OrganizationWalletError::InvalidInput
        | crate::OrganizationWalletError::Conflict
        | crate::OrganizationWalletError::InsufficientQuota
        | crate::OrganizationWalletError::Overflow
        | crate::OrganizationWalletError::OutcomeUnknown
        | crate::OrganizationWalletError::Timeout
        | crate::OrganizationWalletError::Invariant => OrganizationBudgetRepositoryError::Invariant,
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
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    clear_protection_if_safe(transaction, observed.principal.organization_id(), now).await?;
    Ok(OrganizationBillingMutationOutcome::Applied(
        OrganizationBillingReservationStatus::Settled,
    ))
}

pub(crate) async fn release(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
) -> Result<OrganizationBillingMutationOutcome, OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id()).await?;
    // 释放是终态操作，必须先确认预留仍处于可释放状态。
    let current = lock_reservation_record(transaction, observed.id).await?;
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
    if let Some(allocation) = observed.organization {
        window::release(transaction, allocation, now).await?;
    }
    for allocation in &observed.departments {
        window::release(transaction, *allocation, now).await?;
    }
    if let Some(allocation) = observed.team {
        window::release(transaction, allocation, now).await?;
    }
    if let Some(allocation) = observed.member {
        window::release(transaction, allocation, now).await?;
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

pub(crate) async fn mark_pending_and_protect(
    transaction: &DatabaseTransaction,
    observed: &OrganizationBillingReservationRecord,
    actual: Quota,
) -> Result<(), OrganizationBudgetRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let wallet = lock_wallet(transaction, observed.principal.organization_id()).await?;
    let current = lock_reservation_record(transaction, observed.id).await?;
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

pub(crate) async fn load<C>(
    connection: &C,
    id: BillingReservationId,
) -> Result<Option<OrganizationBillingReservationRecord>, OrganizationBudgetRepositoryError>
where
    C: sea_orm::ConnectionTrait,
{
    let model = billing_organization_reservations::Entity::find_by_id(
        BillingReservationKey::parse(&id.persistence_key())
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
    )
    .one(connection)
    .await
    .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    let Some(model) = model else {
        return Ok(None);
    };
    let departments = billing_organization_reservation_departments::Entity::find()
        .filter(
            billing_organization_reservation_departments::Column::IdempotencyKey
                .eq(model.idempotency_key.clone()),
        )
        .order_by_asc(billing_organization_reservation_departments::Column::PathIndex)
        .all(connection)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    record(model, departments).map(Some)
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

/// 在预留事务开始时读取并校验成员的根到叶部门路径。
async fn load_department_path(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    leaf_id: OrganizationDepartmentId,
) -> Result<OrganizationDepartmentBudgetPath, OrganizationBudgetRepositoryError> {
    let mut query = organization_departments::Entity::find()
        .filter(organization_departments::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let models = query
        .all(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    let mut nodes = HashMap::with_capacity(models.len());
    for model in &models {
        let department_id = OrganizationDepartmentId::new(model.id)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        let parent_id = model
            .parent_department_id
            .map(OrganizationDepartmentId::new)
            .transpose()
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        let depth =
            u8::try_from(model.depth).map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        let status = OrganizationDepartmentStatus::try_from(model.status)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        let node = OrganizationDepartmentNode::new(
            organization_id,
            department_id,
            parent_id,
            depth,
            status,
            model.version,
        )
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        if nodes.insert(department_id, node).is_some() {
            return Err(OrganizationBudgetRepositoryError::Invariant);
        }
    }
    let mut current = leaf_id;
    let mut visited = HashSet::new();
    let mut path = Vec::new();
    loop {
        if !visited.insert(current) || path.len() >= 8 {
            return Err(OrganizationBudgetRepositoryError::Invariant);
        }
        let node = nodes
            .get(&current)
            .copied()
            .ok_or(OrganizationBudgetRepositoryError::NotFound)?;
        if !node.status().is_active() {
            return Err(OrganizationBudgetRepositoryError::NotFound);
        }
        path.push(node);
        let Some(parent_id) = node.parent_id() else {
            break;
        };
        current = parent_id;
    }
    path.reverse();
    OrganizationDepartmentBudgetPath::new(organization_id, path)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)
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
    if let Some(department_id) = principal.department_id() {
        let binding = organization_department_memberships::Entity::find()
            .filter(
                organization_department_memberships::Column::OrganizationId
                    .eq(principal.organization_id().get()),
            )
            .filter(
                organization_department_memberships::Column::MembershipId
                    .eq(principal.membership_id().get()),
            )
            .filter(
                organization_department_memberships::Column::DepartmentId.eq(department_id.get()),
            )
            .filter(
                organization_department_memberships::Column::Status
                    .eq(OrganizationDepartmentMembershipStatus::Active.code()),
            )
            .one(transaction)
            .await
            .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
        if binding.is_none() {
            return Err(OrganizationBudgetRepositoryError::NotFound);
        }
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

async fn lock_reservation_record(
    transaction: &DatabaseTransaction,
    id: BillingReservationId,
) -> Result<OrganizationBillingReservationRecord, OrganizationBudgetRepositoryError> {
    let model = lock_reservation(transaction, id).await?;
    let departments = billing_organization_reservation_departments::Entity::find()
        .filter(
            billing_organization_reservation_departments::Column::IdempotencyKey
                .eq(model.idempotency_key.clone()),
        )
        .order_by_asc(billing_organization_reservation_departments::Column::PathIndex)
        .all(transaction)
        .await
        .map_err(|_| OrganizationBudgetRepositoryError::Query)?;
    record(model, departments)
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
    // 结算只在预留与实际用量的差额方向上调整钱包，另一方向必须归零。
    let released = reserved
        .units()
        .checked_sub(actual.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?
        .max(0);
    let supplemental = actual
        .units()
        .checked_sub(reserved.units())
        .ok_or(OrganizationBudgetRepositoryError::Invariant)?
        .max(0);
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
    department_models: Vec<billing_organization_reservation_departments::Model>,
) -> Result<OrganizationBillingReservationRecord, OrganizationBudgetRepositoryError> {
    let id = BillingReservationId::from_persistence_key(model.idempotency_key.as_str())
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let membership_id = OrganizationMembershipId::new(model.membership_id)
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let team_id = model
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let user_id =
        UserId::new(model.user_id).map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
    let department_path = decode_department_path(model.department_path.as_deref())?;
    let principal = match department_path.last().copied() {
        Some(department_id) => OrganizationBillingPrincipal::with_department(
            organization_id,
            membership_id,
            team_id,
            department_id,
            user_id,
        ),
        None => OrganizationBillingPrincipal::new(organization_id, membership_id, team_id, user_id),
    };
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
    let gateway = match (
        model.reservation_kind,
        model.token_id,
        model.group_id,
        model.token_reserved_quota,
    ) {
        (1, None, None, 0) => None,
        (2, Some(token_id), Some(group_id), token_reserved)
            if token_reserved == 0 || token_reserved == model.reserved_quota =>
        {
            Some(OrganizationBillingGatewayRecord {
                token_id: TokenId::new(token_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
                group_id: GroupId::new(group_id)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
                token_reserved_quota: Quota::new(token_reserved)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?,
            })
        }
        _ => return Err(OrganizationBudgetRepositoryError::Invariant),
    };
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
        gateway,
        organization: allocation_record(
            None,
            model.organization_policy_id,
            model.organization_window_id,
            model.organization_policy_version,
            model.organization_window_version,
            model.organization_reserved_quota,
        )?,
        departments: {
            let mut snapshots = Vec::with_capacity(department_models.len());
            let mut previous_index = None;
            for department in department_models {
                if previous_index.is_some_and(|previous| department.path_index <= previous)
                    || department.path_index < 0
                {
                    return Err(OrganizationBudgetRepositoryError::Invariant);
                }
                let index = usize::try_from(department.path_index)
                    .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
                let department_id = *department_path
                    .get(index)
                    .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
                if department.organization_id != model.organization_id
                    || department.department_id != department_id.get()
                {
                    return Err(OrganizationBudgetRepositoryError::Invariant);
                }
                let allocation = allocation_record(
                    Some(department_id),
                    Some(department.policy_id),
                    Some(department.window_id),
                    Some(department.policy_version),
                    Some(department.window_version),
                    Some(department.reserved_quota),
                )?
                .ok_or(OrganizationBudgetRepositoryError::Invariant)?;
                snapshots.push(allocation);
                previous_index = Some(department.path_index);
            }
            snapshots
        },
        department_path,
        team: allocation_record(
            None,
            model.team_policy_id,
            model.team_window_id,
            model.team_policy_version,
            model.team_window_version,
            model.team_reserved_quota,
        )?,
        member: allocation_record(
            None,
            model.member_policy_id,
            model.member_window_id,
            model.member_policy_version,
            model.member_window_version,
            model.member_reserved_quota,
        )?,
    })
}

fn allocation_record(
    department_id: Option<OrganizationDepartmentId>,
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
                department_id,
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

fn encode_department_path(path: &OrganizationDepartmentBudgetPath) -> String {
    path.department_ids()
        .map(|department_id| department_id.get().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_department_path(
    encoded: Option<&str>,
) -> Result<Vec<OrganizationDepartmentId>, OrganizationBudgetRepositoryError> {
    let Some(encoded) = encoded else {
        return Ok(Vec::new());
    };
    if encoded.is_empty() {
        return Err(OrganizationBudgetRepositoryError::Invariant);
    }
    let mut ids = Vec::new();
    for segment in encoded.split(',') {
        if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(OrganizationBudgetRepositoryError::Invariant);
        }
        let value = segment
            .parse::<i64>()
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        let id = OrganizationDepartmentId::new(value)
            .map_err(|_| OrganizationBudgetRepositoryError::Invariant)?;
        if ids.contains(&id) || ids.len() >= 8 {
            return Err(OrganizationBudgetRepositoryError::Invariant);
        }
        ids.push(id);
    }
    Ok(ids)
}

fn same_snapshot(
    left: &OrganizationBillingReservationRecord,
    right: &OrganizationBillingReservationRecord,
) -> bool {
    left.id == right.id
        && left.principal == right.principal
        && left.reserved_quota == right.reserved_quota
        && left.gateway == right.gateway
        && left.organization == right.organization
        && left.department_path == right.department_path
        && left.departments == right.departments
        && left.team == right.team
        && left.member == right.member
}
