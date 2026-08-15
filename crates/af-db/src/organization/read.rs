use af_domain::{
    OrganizationId, OrganizationKey, OrganizationMembershipId, OrganizationMembershipStatus,
    OrganizationRole, OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus, UserId,
};

use crate::entity::{organization_memberships, organization_teams, organizations};

use super::{
    OrganizationMembershipRecord, OrganizationRecord, OrganizationRepositoryError,
    OrganizationTeamRecord, record_internal_error,
};

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
