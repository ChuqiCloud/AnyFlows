use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationKey, OrganizationMembershipId, OrganizationMembershipStatus, OrganizationRole,
    OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus, UserId,
};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::entity::{
    organization_entitlements, organization_memberships, organization_teams, organizations,
};

use super::{
    MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE, OrganizationAccessRecord, OrganizationEntitlementRecord,
    OrganizationMembershipPageRecord, OrganizationMembershipRecord, OrganizationRecord,
    OrganizationRepository, OrganizationRepositoryError, OrganizationTeamPageRecord,
    OrganizationTeamRecord, record_internal_error,
};

/// 在企业路径内读取单个成员，禁止只凭全局成员 ID 查询。
pub(super) async fn membership(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    membership_id: af_domain::OrganizationMembershipId,
) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
    let model = organization_memberships::Entity::find_by_id(membership_id.get())
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?
        .ok_or(OrganizationRepositoryError::NotFound)?;
    membership_record(model)
}

/// 按稳定成员 ID 游标列出企业内全部生命周期记录。
pub(super) async fn list_memberships(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    after: Option<af_domain::OrganizationMembershipId>,
    limit: usize,
) -> Result<OrganizationMembershipPageRecord, OrganizationRepositoryError> {
    validate_directory_limit(limit)?;
    let mut query = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .order_by_asc(organization_memberships::Column::Id);
    if let Some(after) = after {
        query = query.filter(organization_memberships::Column::Id.gt(after.get()));
    }
    let fetch_limit =
        u64::try_from(limit.checked_add(1).ok_or_else(invariant)?).map_err(|_| invariant())?;
    let mut models = query
        .limit(fetch_limit)
        .all(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let has_more = models.len() > limit;
    if has_more {
        models.truncate(limit);
    }
    let records = models
        .into_iter()
        .map(membership_record)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = has_more
        .then(|| records.last().map(OrganizationMembershipRecord::id))
        .flatten();
    Ok(OrganizationMembershipPageRecord::new(records, next_cursor))
}

/// 按稳定团队 ID 游标列出企业内活动和已禁用团队。
pub(super) async fn list_teams(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    after: Option<OrganizationTeamId>,
    limit: usize,
) -> Result<OrganizationTeamPageRecord, OrganizationRepositoryError> {
    validate_directory_limit(limit)?;
    let mut query = organization_teams::Entity::find()
        .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
        .order_by_asc(organization_teams::Column::Id);
    if let Some(after) = after {
        query = query.filter(organization_teams::Column::Id.gt(after.get()));
    }
    let fetch_limit =
        u64::try_from(limit.checked_add(1).ok_or_else(invariant)?).map_err(|_| invariant())?;
    let mut models = query
        .limit(fetch_limit)
        .all(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let has_more = models.len() > limit;
    if has_more {
        models.truncate(limit);
    }
    let records = models
        .into_iter()
        .map(team_record)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = has_more
        .then(|| records.last().map(OrganizationTeamRecord::id))
        .flatten();
    Ok(OrganizationTeamPageRecord::new(records, next_cursor))
}

fn validate_directory_limit(limit: usize) -> Result<(), OrganizationRepositoryError> {
    if !(1..=MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE).contains(&limit) {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    Ok(())
}

/// 按企业路径和用户主体同时解析活动授权，避免跨企业复用成员标识。
pub(super) async fn authorized_access(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<OrganizationAccessRecord, OrganizationRepositoryError> {
    let result = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .find_also_related(organizations::Entity)
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let Some((membership, Some(organization))) = result else {
        return Err(OrganizationRepositoryError::NotFound);
    };
    if organization.status != OrganizationStatus::Active.code() {
        return Err(OrganizationRepositoryError::NotFound);
    }
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
        .one(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?
        .ok_or(OrganizationRepositoryError::Invariant)?;
    let (active_seats, active_teams) = capacity_counts(repository, organization_id).await?;
    Ok(OrganizationAccessRecord::new(
        organization_record(organization)?,
        membership_record(membership)?,
        entitlement_snapshot(entitlement)?,
        active_seats,
        active_teams,
    ))
}

/// 列出用户可进入的活动企业；每行都携带同一查询得到的成员角色。
pub(super) async fn list_authorized_access(
    repository: &OrganizationRepository,
    user_id: UserId,
) -> Result<Vec<OrganizationAccessRecord>, OrganizationRepositoryError> {
    let rows = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .find_also_related(organizations::Entity)
        .filter(organizations::Column::Status.eq(OrganizationStatus::Active.code()))
        .all(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let mut records = Vec::with_capacity(rows.len());
    for (membership, organization) in rows {
        let Some(organization) = organization else {
            return Err(record_internal_error(
                OrganizationRepositoryError::Invariant,
            ));
        };
        let organization_id = OrganizationId::new(organization.id).map_err(|_| invariant())?;
        let entitlement = organization_entitlements::Entity::find()
            .filter(organization_entitlements::Column::OrganizationId.eq(organization.id))
            .one(repository.pool.connection())
            .await
            .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?
            .ok_or(OrganizationRepositoryError::Invariant)?;
        let (active_seats, active_teams) = capacity_counts(repository, organization_id).await?;
        records.push(OrganizationAccessRecord::new(
            organization_record(organization)?,
            membership_record(membership)?,
            entitlement_snapshot(entitlement)?,
            active_seats,
            active_teams,
        ));
    }
    Ok(records)
}

async fn capacity_counts(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
) -> Result<(i64, i64), OrganizationRepositoryError> {
    let active_seats = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let active_seats = i64::try_from(active_seats).map_err(|_| invariant())?;
    let active_teams = organization_teams::Entity::find()
        .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_teams::Column::Status.eq(OrganizationTeamStatus::Active.code()))
        .count(repository.pool.connection())
        .await
        .map_err(|_| record_internal_error(OrganizationRepositoryError::Query))?;
    let active_teams = i64::try_from(active_teams).map_err(|_| invariant())?;
    Ok((active_seats, active_teams))
}

pub(super) fn entitlement_snapshot(
    model: organization_entitlements::Model,
) -> Result<OrganizationEntitlementSnapshot, OrganizationRepositoryError> {
    let capacity = OrganizationEntitlementCapacity::new(
        model.seat_limit,
        model.team_limit,
        model.organization_key_limit,
    )
    .map_err(|_| invariant())?;
    OrganizationEntitlementSnapshot::new(
        capacity,
        model.team_management_enabled,
        model.sso_enabled,
        model.custom_roles_enabled,
        model.audit_retention_days,
        model.valid_from.unix_timestamp(),
        model.valid_until.unix_timestamp(),
        model.grace_until.map(|value| value.unix_timestamp()),
    )
    .map_err(|_| invariant())
}

pub(super) fn entitlement_record(
    model: organization_entitlements::Model,
) -> Result<OrganizationEntitlementRecord, OrganizationRepositoryError> {
    if model.version <= 0
        || model.source.is_empty()
        || model
            .external_reference
            .as_ref()
            .is_some_and(String::is_empty)
    {
        return Err(invariant());
    }
    Ok(OrganizationEntitlementRecord {
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        snapshot: entitlement_snapshot(model.clone())?,
        source: model.source,
        external_reference: model.external_reference,
        version: model.version,
    })
}

pub(super) fn organization_record(
    model: organizations::Model,
) -> Result<OrganizationRecord, OrganizationRepositoryError> {
    let record = OrganizationRecord {
        id: OrganizationId::new(model.id).map_err(|_| invariant())?,
        organization_key: OrganizationKey::from_persistence_key(&model.organization_key)
            .map_err(|_| invariant())?,
        name: model.name,
        slug: model.slug,
        status: OrganizationStatus::try_from(model.status).map_err(|_| invariant())?,
        created_by_user_id: UserId::new(model.created_by_user_id).map_err(|_| invariant())?,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        closed_at: model.closed_at.map(|value| value.unix_timestamp()),
    };
    if record.version <= 0
        || record.name.is_empty()
        || record.slug.is_empty()
        || (record.status == OrganizationStatus::Closed) != record.closed_at.is_some()
    {
        return Err(invariant());
    }
    Ok(record)
}

pub(super) fn membership_record(
    model: organization_memberships::Model,
) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
    let status = OrganizationMembershipStatus::try_from(model.status).map_err(|_| invariant())?;
    let record = OrganizationMembershipRecord {
        id: OrganizationMembershipId::new(model.id).map_err(|_| invariant())?,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        user_id: UserId::new(model.user_id).map_err(|_| invariant())?,
        role: OrganizationRole::try_from(model.role).map_err(|_| invariant())?,
        team_id: model
            .team_id
            .map(OrganizationTeamId::new)
            .transpose()
            .map_err(|_| invariant())?,
        status,
        joined_at: model.joined_at.unix_timestamp(),
        suspended_at: model.suspended_at.map(|value| value.unix_timestamp()),
        removed_at: model.removed_at.map(|value| value.unix_timestamp()),
        invited_by_user_id: model
            .invited_by_user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| invariant())?,
        version: model.version,
    };
    let lifecycle_valid = match status {
        OrganizationMembershipStatus::Active => {
            record.suspended_at.is_none() && record.removed_at.is_none()
        }
        OrganizationMembershipStatus::Suspended => {
            record.suspended_at.is_some() && record.removed_at.is_none()
        }
        OrganizationMembershipStatus::Removed => record.removed_at.is_some(),
    };
    if record.version <= 0 || record.joined_at < 0 || !lifecycle_valid {
        return Err(invariant());
    }
    Ok(record)
}

pub(super) fn team_record(
    model: organization_teams::Model,
) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
    let status = OrganizationTeamStatus::try_from(model.status).map_err(|_| invariant())?;
    let record = OrganizationTeamRecord {
        id: OrganizationTeamId::new(model.id).map_err(|_| invariant())?,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        name: model.name,
        description: model.description,
        status,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        deleted_at: model.deleted_at.map(|value| value.unix_timestamp()),
    };
    if record.version <= 0
        || record.name.is_empty()
        || (status == OrganizationTeamStatus::Disabled) != record.deleted_at.is_some()
    {
        return Err(invariant());
    }
    Ok(record)
}

fn invariant() -> OrganizationRepositoryError {
    record_internal_error(OrganizationRepositoryError::Invariant)
}
