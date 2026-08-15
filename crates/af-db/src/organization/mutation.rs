use af_domain::{
    OrganizationId, OrganizationKey, OrganizationMembershipId, OrganizationMembershipStatus,
    OrganizationRole, OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};

use crate::entity::{organization_memberships, organization_teams, organizations, users};

use super::{
    MAX_ORGANIZATION_NAME_BYTES, MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES,
    MAX_ORGANIZATION_TEAM_NAME_BYTES, OrganizationCreate, OrganizationCreatedRecord,
    OrganizationMembershipChange, OrganizationMembershipCreate, OrganizationMembershipRecord,
    OrganizationOwnershipTransfer, OrganizationOwnershipTransferRecord, OrganizationRepository,
    OrganizationRepositoryError, OrganizationTeamCreate, OrganizationTeamRecord,
    record_internal_error, validate_name, validate_slug,
};

pub(super) async fn create(
    repository: &OrganizationRepository,
    write: OrganizationCreate,
) -> Result<OrganizationCreatedRecord, OrganizationRepositoryError> {
    validate_name(&write.name, MAX_ORGANIZATION_NAME_BYTES)?;
    validate_slug(&write.slug)?;
    let organization_key = generate_organization_key()?;
    let transaction = begin(repository).await?;
    if !lock_active_user(&transaction, write.owner_user_id).await? {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::UserUnavailable);
    }

    let now = TimeDateTimeWithTimeZone::now_utc();
    let organization = organizations::ActiveModel {
        organization_key: Set(organization_key.persistence_key()),
        name: Set(write.name),
        slug: Set(write.slug),
        status: Set(OrganizationStatus::Active.code()),
        created_by_user_id: Set(write.owner_user_id.get()),
        version: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        closed_at: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await;
    let organization = match organization {
        Ok(model) => model,
        Err(error) => {
            let mapped = map_write_error(&error);
            rollback(transaction).await?;
            return Err(mapped);
        }
    };

    let owner = organization_memberships::ActiveModel {
        organization_id: Set(organization.id),
        user_id: Set(write.owner_user_id.get()),
        role: Set(OrganizationRole::Owner.code()),
        team_id: Set(None),
        status: Set(OrganizationMembershipStatus::Active.code()),
        joined_at: Set(now),
        suspended_at: Set(None),
        removed_at: Set(None),
        invited_by_user_id: Set(None),
        version: Set(1),
        ..Default::default()
    }
    .insert(&transaction)
    .await;
    let owner = match owner {
        Ok(model) => model,
        Err(error) => {
            let mapped = map_write_error(&error);
            rollback(transaction).await?;
            return Err(mapped);
        }
    };

    let organization = super::read::organization_record(organization)?;
    let owner_membership = super::read::membership_record(owner)?;
    commit(transaction).await?;
    Ok(OrganizationCreatedRecord::new(
        organization,
        owner_membership,
    ))
}

pub(super) async fn add_membership(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    write: OrganizationMembershipCreate,
) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
    let transaction = begin(repository).await?;
    let organization = lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    if !lock_active_user(&transaction, write.user_id).await? {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::UserUnavailable);
    }
    if let Some(team_id) = write.team_id {
        lock_active_team(&transaction, organization_id, team_id).await?;
    }

    let now = TimeDateTimeWithTimeZone::now_utc();
    let inserted = organization_memberships::ActiveModel {
        organization_id: Set(organization_id.get()),
        user_id: Set(write.user_id.get()),
        role: Set(write.role.code()),
        team_id: Set(write.team_id.map(OrganizationTeamId::get)),
        status: Set(OrganizationMembershipStatus::Active.code()),
        joined_at: Set(now),
        suspended_at: Set(None),
        removed_at: Set(None),
        invited_by_user_id: Set(write.invited_by_user_id.map(UserId::get)),
        version: Set(1),
        ..Default::default()
    }
    .insert(&transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) => {
            let mapped = map_write_error(&error);
            rollback(transaction).await?;
            return Err(mapped);
        }
    };
    let record = super::read::membership_record(inserted)?;
    commit(transaction).await?;
    Ok(record)
}

pub(super) async fn change_membership(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    change: OrganizationMembershipChange,
) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
    if change.expected_version <= 0 {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = begin(repository).await?;
    let organization = lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let current = lock_membership(&transaction, organization_id, membership_id).await?;
    if current.version != change.expected_version {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::Conflict);
    }

    let current_role = OrganizationRole::try_from(current.role).map_err(|_| invariant())?;
    let current_status =
        OrganizationMembershipStatus::try_from(current.status).map_err(|_| invariant())?;
    if !membership_transition_allowed(current_status, change.status) {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::InvalidTransition);
    }
    if let Some(team_id) = change.team_id {
        lock_active_team(&transaction, organization_id, team_id).await?;
    }

    let target_is_active_owner = change.role.is_owner() && change.status.is_active();
    if current_role.is_owner() && current_status.is_active() && !target_is_active_owner {
        let other_owners = active_owner_count(&transaction, organization_id, membership_id).await?;
        if other_owners == 0 {
            rollback(transaction).await?;
            return Err(OrganizationRepositoryError::LastOwner);
        }
    }

    let target_team_id = change.team_id.map(OrganizationTeamId::get);
    if current.role == change.role.code()
        && current.status == change.status.code()
        && current.team_id == target_team_id
    {
        let record = super::read::membership_record(current)?;
        commit(transaction).await?;
        return Ok(record);
    }

    let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let (suspended_at, removed_at) = membership_lifecycle_times(&current, change.status, now);
    let mut active = current.into_active_model();
    active.role = Set(change.role.code());
    active.team_id = Set(target_team_id);
    active.status = Set(change.status.code());
    active.suspended_at = Set(suspended_at);
    active.removed_at = Set(removed_at);
    active.version = Set(next_version);
    let updated = active.update(&transaction).await.map_err(query_error)?;
    let record = super::read::membership_record(updated)?;
    commit(transaction).await?;
    Ok(record)
}

pub(super) async fn transfer_ownership(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    transfer: OrganizationOwnershipTransfer,
) -> Result<OrganizationOwnershipTransferRecord, OrganizationRepositoryError> {
    if transfer.from_membership_id == transfer.to_membership_id
        || transfer.from_expected_version <= 0
        || transfer.to_expected_version <= 0
    {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = begin(repository).await?;
    let organization = lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;

    // 固定按成员主键加锁，避免并发转移在不同数据库上形成反向锁序。
    let (from, to) = if transfer.from_membership_id < transfer.to_membership_id {
        let from =
            lock_membership(&transaction, organization_id, transfer.from_membership_id).await?;
        let to = lock_membership(&transaction, organization_id, transfer.to_membership_id).await?;
        (from, to)
    } else {
        let to = lock_membership(&transaction, organization_id, transfer.to_membership_id).await?;
        let from =
            lock_membership(&transaction, organization_id, transfer.from_membership_id).await?;
        (from, to)
    };

    if from.version != transfer.from_expected_version || to.version != transfer.to_expected_version
    {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::Conflict);
    }
    let from_role = OrganizationRole::try_from(from.role).map_err(|_| invariant())?;
    let from_status =
        OrganizationMembershipStatus::try_from(from.status).map_err(|_| invariant())?;
    let to_status = OrganizationMembershipStatus::try_from(to.status).map_err(|_| invariant())?;
    if !from_role.is_owner() || !from_status.is_active() || !to_status.is_active() {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::InvalidTransition);
    }

    // 先提升目标、再降级原 Owner；任一步失败都会回滚整个事务。
    let current_owner = update_membership_role(&transaction, to, OrganizationRole::Owner).await?;
    let previous_owner =
        update_membership_role(&transaction, from, OrganizationRole::Admin).await?;
    let current_owner = super::read::membership_record(current_owner)?;
    let previous_owner = super::read::membership_record(previous_owner)?;
    commit(transaction).await?;
    Ok(OrganizationOwnershipTransferRecord::new(
        previous_owner,
        current_owner,
    ))
}

pub(super) async fn create_team(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    write: OrganizationTeamCreate,
) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
    validate_name(&write.name, MAX_ORGANIZATION_TEAM_NAME_BYTES)?;
    if write.description.as_ref().is_some_and(|description| {
        description.is_empty()
            || description.len() > MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES
            || description.trim() != description
            || description.contains('\0')
    }) {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = begin(repository).await?;
    let organization = lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let inserted = organization_teams::ActiveModel {
        organization_id: Set(organization_id.get()),
        name: Set(write.name),
        description: Set(write.description),
        status: Set(OrganizationTeamStatus::Active.code()),
        version: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) => {
            let mapped = map_write_error(&error);
            rollback(transaction).await?;
            return Err(mapped);
        }
    };
    let record = super::read::team_record(inserted)?;
    commit(transaction).await?;
    Ok(record)
}

pub(super) async fn disable_team(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    team_id: OrganizationTeamId,
    expected_version: i64,
) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
    if expected_version <= 0 {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = begin(repository).await?;
    let organization = lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let team = lock_active_team(&transaction, organization_id, team_id).await?;
    if team.version != expected_version {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::Conflict);
    }
    let active_members = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::TeamId.eq(team_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(&transaction)
        .await
        .map_err(query_error)?;
    if active_members != 0 {
        rollback(transaction).await?;
        return Err(OrganizationRepositoryError::TeamHasActiveMembers);
    }

    let next_version = team.version.checked_add(1).ok_or_else(invariant)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let mut active = team.into_active_model();
    active.status = Set(OrganizationTeamStatus::Disabled.code());
    active.version = Set(next_version);
    active.updated_at = Set(now);
    active.deleted_at = Set(Some(now));
    let updated = active.update(&transaction).await.map_err(query_error)?;
    let record = super::read::team_record(updated)?;
    commit(transaction).await?;
    Ok(record)
}

async fn begin(
    repository: &OrganizationRepository,
) -> Result<DatabaseTransaction, OrganizationRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(query_error)
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), OrganizationRepositoryError> {
    transaction.commit().await.map_err(query_error)
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), OrganizationRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

async fn lock_active_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<bool, OrganizationRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，通过无值自更新先取得写锁，再读取用户有效状态。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            return Ok(false);
        }
    }
    let mut query = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::Id.eq(user_id.get()))
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map(|row| row.is_some())
        .map_err(query_error)
}

async fn lock_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organizations::Model, OrganizationRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // 企业行是所有组织写操作的统一串行化边界。
        let result = organizations::Entity::update_many()
            .filter(organizations::Column::Id.eq(organization_id.get()))
            .col_expr(
                organizations::Column::Version,
                Expr::col(organizations::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            return Err(OrganizationRepositoryError::NotFound);
        }
    }
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)
}

async fn lock_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
) -> Result<organization_memberships::Model, OrganizationRepositoryError> {
    let mut query = organization_memberships::Entity::find_by_id(membership_id.get())
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)
}

async fn lock_active_team(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    team_id: OrganizationTeamId,
) -> Result<organization_teams::Model, OrganizationRepositoryError> {
    let mut query = organization_teams::Entity::find_by_id(team_id.get())
        .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_teams::Column::Status.eq(OrganizationTeamStatus::Active.code()))
        .filter(organization_teams::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)
}

async fn active_owner_count(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    excluded_membership_id: OrganizationMembershipId,
) -> Result<u64, OrganizationRepositoryError> {
    organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::Id.ne(excluded_membership_id.get()))
        .filter(organization_memberships::Column::Role.eq(OrganizationRole::Owner.code()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(transaction)
        .await
        .map_err(query_error)
}

async fn update_membership_role(
    transaction: &DatabaseTransaction,
    model: organization_memberships::Model,
    role: OrganizationRole,
) -> Result<organization_memberships::Model, OrganizationRepositoryError> {
    let next_version = model.version.checked_add(1).ok_or_else(invariant)?;
    let mut active = model.into_active_model();
    active.role = Set(role.code());
    active.version = Set(next_version);
    active.update(transaction).await.map_err(query_error)
}

fn require_active_organization(
    organization: &organizations::Model,
) -> Result<(), OrganizationRepositoryError> {
    let status = OrganizationStatus::try_from(organization.status).map_err(|_| invariant())?;
    if !status.is_active() || organization.closed_at.is_some() {
        return Err(OrganizationRepositoryError::InvalidTransition);
    }
    Ok(())
}

fn membership_transition_allowed(
    current: OrganizationMembershipStatus,
    target: OrganizationMembershipStatus,
) -> bool {
    current == target
        || matches!(
            (current, target),
            (
                OrganizationMembershipStatus::Active,
                OrganizationMembershipStatus::Suspended | OrganizationMembershipStatus::Removed
            ) | (
                OrganizationMembershipStatus::Suspended,
                OrganizationMembershipStatus::Active | OrganizationMembershipStatus::Removed
            )
        )
}

fn membership_lifecycle_times(
    current: &organization_memberships::Model,
    target: OrganizationMembershipStatus,
    now: TimeDateTimeWithTimeZone,
) -> (
    Option<TimeDateTimeWithTimeZone>,
    Option<TimeDateTimeWithTimeZone>,
) {
    match target {
        OrganizationMembershipStatus::Active => (None, None),
        OrganizationMembershipStatus::Suspended => (current.suspended_at.or(Some(now)), None),
        OrganizationMembershipStatus::Removed => (current.suspended_at, Some(now)),
    }
}

fn generate_organization_key() -> Result<OrganizationKey, OrganizationRepositoryError> {
    for _ in 0..4 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| OrganizationRepositoryError::EntropyUnavailable)?;
        if let Ok(key) = OrganizationKey::new(bytes) {
            return Ok(key);
        }
    }
    Err(OrganizationRepositoryError::EntropyUnavailable)
}

fn map_write_error(error: &DbErr) -> OrganizationRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationRepositoryError::Conflict
    } else {
        query_error(error)
    }
}

fn query_error(_error: impl std::fmt::Debug) -> OrganizationRepositoryError {
    record_internal_error(OrganizationRepositoryError::Query)
}

fn invariant() -> OrganizationRepositoryError {
    record_internal_error(OrganizationRepositoryError::Invariant)
}

#[cfg(test)]
mod tests {
    use super::membership_transition_allowed;
    use af_domain::OrganizationMembershipStatus::{Active, Removed, Suspended};

    #[test]
    fn removed_membership_is_terminal() {
        assert!(membership_transition_allowed(Active, Suspended));
        assert!(membership_transition_allowed(Suspended, Active));
        assert!(membership_transition_allowed(Active, Removed));
        assert!(membership_transition_allowed(Removed, Removed));
        assert!(!membership_transition_allowed(Removed, Active));
        assert!(!membership_transition_allowed(Removed, Suspended));
    }
}
