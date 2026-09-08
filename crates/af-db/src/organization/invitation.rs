use std::collections::BTreeSet;

use af_domain::{
    GroupId, OrganizationId, OrganizationInvitationKey, OrganizationInvitationStatus,
    OrganizationMembershipStatus, OrganizationRole, OrganizationSsoDomainStatus,
    OrganizationSsoIdentityBindingMethod, OrganizationSsoIdentityStatus,
    OrganizationSsoProviderKey, OrganizationSsoProviderStatus, OrganizationStatus,
    OrganizationTeamId, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    SqlErr, entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};

use crate::{
    AdminUserCreateRecord, AdminUserRepositoryError, EncryptedCredentialEnvelope,
    admin_user_write::create_user_in_transaction,
    entity::{
        authentication_settings, organization_invitations, organization_memberships,
        organization_sso_domains, organization_sso_identities, organization_sso_providers,
        organization_teams, users,
    },
};

use super::{
    MAX_ORGANIZATION_INVITATION_BATCH_SIZE, OrganizationInvitationAcceptanceContext,
    OrganizationInvitationBatchRecord, OrganizationInvitationCreate,
    OrganizationInvitationPreviewItem, OrganizationInvitationPreviewRow,
    OrganizationInvitationPreviewSnapshot, OrganizationInvitationRecord, OrganizationRepository,
    OrganizationRepositoryError, mutation, read, record_internal_error,
};

const AUTHENTICATION_SETTINGS_ID: i16 = 1;
const ACTIVE_USER_STATUS: i16 = 1;
const STANDARD_USER_ROLE: i16 = 0;
const JIT_INITIAL_QUOTA: i64 = 0;
const JIT_USERNAME_PREFIX: &str = "sso_jit_";

pub(super) async fn create(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    write: OrganizationInvitationCreate,
) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
    if write.batch_key.is_some() {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    ensure_team_management(&entitlement, write.team_id)?;
    ensure_inviter(&transaction, write.invited_by_user_id).await?;
    validate_team(&transaction, organization_id, write.team_id).await?;
    ensure_no_existing_member_emails(
        &transaction,
        organization_id,
        vec![write.normalized_email.as_str().to_owned()],
    )
    .await?;
    ensure_no_pending_email(
        &transaction,
        organization_id,
        &hex(&write.email_fingerprint),
        now(),
    )
    .await?;
    ensure_capacity(&transaction, organization_id, &entitlement, 1, now()).await?;
    let inserted = insert_invitation(&transaction, organization_id, write).await?;
    let record = invitation_record(inserted)?;
    mutation::commit(transaction).await?;
    Ok(record)
}

pub(super) async fn create_batch(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    writes: Vec<OrganizationInvitationCreate>,
) -> Result<OrganizationInvitationBatchRecord, OrganizationRepositoryError> {
    if writes.is_empty() || writes.len() > MAX_ORGANIZATION_INVITATION_BATCH_SIZE {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let Some(batch_key) = writes[0].batch_key else {
        return Err(OrganizationRepositoryError::InvalidInput);
    };
    let Some(batch_digest) = writes[0].batch_digest else {
        return Err(OrganizationRepositoryError::InvalidInput);
    };
    if writes.iter().any(|write| {
        write.batch_key != Some(batch_key)
            || write.batch_digest != Some(batch_digest)
            || write.batch_size != Some(writes.len())
    }) {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    if writes.iter().any(|write| write.team_id.is_some()) {
        ensure_team_management_enabled(&entitlement)?;
    }
    ensure_inviter(&transaction, writes[0].invited_by_user_id).await?;

    let existing = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_invitations::Column::BatchKey.eq(batch_key.persistence_key()))
        .order_by_asc(organization_invitations::Column::Id)
        .all(&transaction)
        .await
        .map_err(query_error)?;
    if !existing.is_empty() {
        if existing.len() != writes.len()
            || existing.iter().any(|model| {
                model.batch_digest.as_deref() != Some(hex(&batch_digest).as_str())
                    || model.batch_size != Some(i32::try_from(writes.len()).unwrap_or_default())
            })
        {
            return Err(OrganizationRepositoryError::InvitationBatchConflict);
        }
        let records = existing
            .into_iter()
            .map(invitation_record)
            .collect::<Result<Vec<_>, _>>()?;
        mutation::commit(transaction).await?;
        return Ok(OrganizationInvitationBatchRecord::new(records, true));
    }

    let mut fingerprints = std::collections::BTreeSet::new();
    let mut teams = std::collections::BTreeSet::new();
    for write in &writes {
        if !fingerprints.insert(hex(&write.email_fingerprint)) {
            return Err(OrganizationRepositoryError::Conflict);
        }
        if let Some(team_id) = write.team_id {
            teams.insert(team_id);
        }
    }
    for team_id in teams {
        validate_team(&transaction, organization_id, Some(team_id)).await?;
    }
    ensure_no_existing_member_emails(
        &transaction,
        organization_id,
        writes
            .iter()
            .map(|write| write.normalized_email.as_str().to_owned())
            .collect(),
    )
    .await?;
    let now = now();
    for write in &writes {
        ensure_no_pending_email(
            &transaction,
            organization_id,
            &hex(&write.email_fingerprint),
            now,
        )
        .await?;
    }
    ensure_capacity(
        &transaction,
        organization_id,
        &entitlement,
        i64::try_from(writes.len()).map_err(|_| OrganizationRepositoryError::InvalidInput)?,
        now,
    )
    .await?;

    let mut records = Vec::with_capacity(writes.len());
    for write in writes {
        records.push(invitation_record(
            insert_invitation(&transaction, organization_id, write).await?,
        )?);
    }
    mutation::commit(transaction).await?;
    Ok(OrganizationInvitationBatchRecord::new(records, false))
}

pub(super) async fn preview(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    items: Vec<OrganizationInvitationPreviewItem>,
) -> Result<OrganizationInvitationPreviewSnapshot, OrganizationRepositoryError> {
    if items.is_empty() || items.len() > MAX_ORGANIZATION_INVITATION_BATCH_SIZE {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    let entitlement_snapshot = read::entitlement_snapshot(entitlement.clone())?;

    let emails = items
        .iter()
        .map(|item| item.normalized_email.clone())
        .collect::<Vec<_>>();
    let existing_member_emails = organization_memberships::Entity::find()
        .inner_join(users::Entity)
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(users::Column::Email.is_in(emails))
        .select_only()
        .column(users::Column::Email)
        .into_tuple::<Option<String>>()
        .all(&transaction)
        .await
        .map_err(query_error)?
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();

    let fingerprint_values = items
        .iter()
        .map(|item| hex(&item.email_fingerprint))
        .collect::<Vec<_>>();
    let pending_fingerprints =
        organization_invitations::Entity::find()
            .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
            .filter(
                organization_invitations::Column::Status
                    .eq(OrganizationInvitationStatus::Pending.code()),
            )
            .filter(organization_invitations::Column::EmailFingerprint.is_in(fingerprint_values))
            .filter(organization_invitations::Column::ExpiresAt.gt(
                TimeDateTimeWithTimeZone::from_unix_timestamp(now()).map_err(|_| invariant())?,
            ))
            .select_only()
            .column(organization_invitations::Column::EmailFingerprint)
            .into_tuple::<String>()
            .all(&transaction)
            .await
            .map_err(query_error)?
            .into_iter()
            .collect::<BTreeSet<_>>();

    let team_ids = items
        .iter()
        .filter_map(|item| item.team_id.map(OrganizationTeamId::get))
        .collect::<BTreeSet<_>>();
    let valid_team_ids = if entitlement_snapshot.team_management_enabled() && !team_ids.is_empty() {
        organization_teams::Entity::find()
            .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
            .filter(
                organization_teams::Column::Status
                    .eq(af_domain::OrganizationTeamStatus::Active.code()),
            )
            .filter(organization_teams::Column::Id.is_in(team_ids))
            .select_only()
            .column(organization_teams::Column::Id)
            .into_tuple::<i64>()
            .all(&transaction)
            .await
            .map_err(query_error)?
            .into_iter()
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };

    let rows = items
        .iter()
        .map(|item| {
            OrganizationInvitationPreviewRow::new(
                existing_member_emails.contains(&item.normalized_email),
                pending_fingerprints.contains(&hex(&item.email_fingerprint)),
                item.team_id
                    .is_none_or(|team_id| valid_team_ids.contains(&team_id.get())),
            )
        })
        .collect();
    let (active, pending) = seat_usage(&transaction, organization_id, now()).await?;
    let available_seats = entitlement_snapshot
        .capacity()
        .seat_limit()
        .checked_sub(active)
        .and_then(|remaining| remaining.checked_sub(pending))
        .unwrap_or_default()
        .max(0);
    mutation::commit(transaction).await?;
    Ok(OrganizationInvitationPreviewSnapshot::new(
        rows,
        available_seats,
    ))
}

pub(super) async fn list(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
) -> Result<Vec<OrganizationInvitationRecord>, OrganizationRepositoryError> {
    let models = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .order_by_desc(organization_invitations::Column::Id)
        .all(repository.pool.connection())
        .await
        .map_err(query_error)?;
    models.into_iter().map(invitation_record).collect()
}

pub(super) async fn acceptance_context(
    repository: &OrganizationRepository,
    invitation_key: OrganizationInvitationKey,
) -> Result<OrganizationInvitationAcceptanceContext, OrganizationRepositoryError> {
    let model = organization_invitations::Entity::find()
        .filter(
            organization_invitations::Column::InvitationKey.eq(invitation_key.persistence_key()),
        )
        .one(repository.pool.connection())
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)?;
    let record = invitation_record(model.clone())?;
    let email_secret = envelope_from_json(model.email_secret)?;
    Ok(OrganizationInvitationAcceptanceContext {
        record,
        email_secret,
    })
}

pub(super) async fn accept(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    invitation_key: OrganizationInvitationKey,
    token_digest: [u8; 32],
    email_fingerprint: [u8; 32],
    user_id: UserId,
    normalized_email: String,
) -> Result<crate::organization::OrganizationMembershipRecord, OrganizationRepositoryError> {
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    let invitation = lock_invitation(&transaction, organization_id, invitation_key).await?;
    let invitation_status =
        OrganizationInvitationStatus::try_from(invitation.status).map_err(|_| invariant())?;
    if !invitation_status.is_pending() {
        return Err(OrganizationRepositoryError::InvitationNotPending);
    }
    let now = now();
    if invitation.expires_at.unix_timestamp() <= now {
        finalize_invitation(
            &transaction,
            invitation,
            OrganizationInvitationStatus::Expired,
            None,
            now,
        )
        .await?;
        mutation::commit(transaction).await?;
        return Err(OrganizationRepositoryError::InvitationExpired);
    }
    if invitation.token_digest != hex(&token_digest)
        || invitation.email_fingerprint != hex(&email_fingerprint)
    {
        return Err(OrganizationRepositoryError::InvitationEmailMismatch);
    }
    let user = mutation::lock_active_user_model(&transaction, user_id).await?;
    if user.email.as_deref() != Some(normalized_email.as_str()) {
        return Err(OrganizationRepositoryError::InvitationEmailMismatch);
    }
    let existing_membership = crate::entity::organization_memberships::Entity::find()
        .filter(
            crate::entity::organization_memberships::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(crate::entity::organization_memberships::Column::UserId.eq(user_id.get()))
        .one(&transaction)
        .await
        .map_err(query_error)?;
    if existing_membership.is_some() {
        return Err(OrganizationRepositoryError::Conflict);
    }
    let role = OrganizationRole::try_from(invitation.role).map_err(|_| invariant())?;
    let team_id = invitation
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| invariant())?;
    ensure_team_management(&entitlement, team_id)?;
    validate_team(&transaction, organization_id, team_id).await?;
    ensure_capacity(&transaction, organization_id, &entitlement, 0, now).await?;

    let membership = crate::entity::organization_memberships::ActiveModel {
        organization_id: Set(organization_id.get()),
        user_id: Set(user_id.get()),
        role: Set(role.code()),
        team_id: Set(team_id.map(OrganizationTeamId::get)),
        status: Set(OrganizationMembershipStatus::Active.code()),
        joined_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        suspended_at: Set(None),
        removed_at: Set(None),
        invited_by_user_id: Set(Some(invitation.invited_by_user_id)),
        version: Set(1),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_write_error)?;
    let membership_record = read::membership_record(membership)?;
    finalize_invitation(
        &transaction,
        invitation,
        OrganizationInvitationStatus::Accepted,
        Some(user_id),
        now,
    )
    .await?;
    mutation::commit(transaction).await?;
    Ok(membership_record)
}

/// 在同一企业事务内消费受邀请事实，并原子创建无密码的 JIT 成员与身份。
pub(super) async fn bind_invited_sso_identity(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
    subject_digest: String,
    invitation_email_fingerprint: [u8; 32],
) -> Result<crate::organization::OrganizationMembershipRecord, OrganizationRepositoryError> {
    validate_sso_digest(&subject_digest)?;
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    if !read::entitlement_snapshot(entitlement.clone())?.sso_enabled() {
        return Err(OrganizationRepositoryError::EntitlementRestricted);
    }

    let provider = lock_active_sso_provider(&transaction, organization_id, provider_key).await?;
    lock_verified_sso_domain(&transaction, organization_id, provider.id, now()).await?;

    // 同一身份的重试只恢复原成员；不能再次消费邀请或创建全局用户。
    if let Some(identity) =
        lock_identity_by_subject(&transaction, organization_id, provider.id, &subject_digest)
            .await?
    {
        let status =
            OrganizationSsoIdentityStatus::try_from(identity.status).map_err(|_| invariant())?;
        if !status.is_active() {
            return Err(OrganizationRepositoryError::Conflict);
        }
        let membership = lock_active_membership(
            &transaction,
            organization_id,
            identity.organization_membership_id,
        )
        .await?;
        let record = read::membership_record(membership)?;
        mutation::commit(transaction).await?;
        return Ok(record);
    }

    let now = now();
    let invitation = lock_pending_invitation_by_fingerprint(
        &transaction,
        organization_id,
        &hex(&invitation_email_fingerprint),
    )
    .await?;
    if invitation.expires_at.unix_timestamp() <= now {
        finalize_invitation(
            &transaction,
            invitation,
            OrganizationInvitationStatus::Expired,
            None,
            now,
        )
        .await?;
        mutation::commit(transaction).await?;
        return Err(OrganizationRepositoryError::InvitationExpired);
    }

    let settings = authentication_settings::Entity::find_by_id(AUTHENTICATION_SETTINGS_ID)
        .one(&transaction)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    let default_group_id = settings
        .registration_default_group_id
        .map(GroupId::new)
        .transpose()
        .map_err(|_| invariant())?
        .ok_or(OrganizationRepositoryError::InvalidInput)?;
    let role = OrganizationRole::try_from(invitation.role).map_err(|_| invariant())?;
    let team_id = invitation
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| invariant())?;
    ensure_team_management(&entitlement, team_id)?;
    validate_team(&transaction, organization_id, team_id).await?;
    ensure_capacity(&transaction, organization_id, &entitlement, 0, now).await?;

    let user = create_user_in_transaction(
        &transaction,
        AdminUserCreateRecord::new(
            jit_username(&subject_digest),
            None,
            None,
            STANDARD_USER_ROLE,
            ACTIVE_USER_STATUS,
            default_group_id,
            JIT_INITIAL_QUOTA,
            None,
            None,
        ),
    )
    .await
    .map_err(map_jit_user_error)?;

    let membership = organization_memberships::ActiveModel {
        organization_id: Set(organization_id.get()),
        user_id: Set(user.user_id().get()),
        role: Set(role.code()),
        team_id: Set(team_id.map(OrganizationTeamId::get)),
        custom_role_id: Set(None),
        status: Set(OrganizationMembershipStatus::Active.code()),
        joined_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        suspended_at: Set(None),
        removed_at: Set(None),
        invited_by_user_id: Set(Some(invitation.invited_by_user_id)),
        version: Set(1),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_write_error)?;
    let membership_record = read::membership_record(membership.clone())?;

    organization_sso_identities::ActiveModel {
        organization_id: Set(organization_id.get()),
        provider_id: Set(provider.id),
        subject_digest: Set(subject_digest.into()),
        organization_membership_id: Set(membership.id),
        status: Set(OrganizationSsoIdentityStatus::Active.code()),
        binding_method: Set(OrganizationSsoIdentityBindingMethod::Invited.code()),
        version: Set(1),
        created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        updated_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        disabled_at: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_write_error)?;
    finalize_invitation(
        &transaction,
        invitation,
        OrganizationInvitationStatus::Accepted,
        Some(user.user_id()),
        now,
    )
    .await?;
    mutation::commit(transaction).await?;
    Ok(membership_record)
}

pub(super) async fn revoke(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    invitation_key: OrganizationInvitationKey,
) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let invitation = lock_invitation(&transaction, organization_id, invitation_key).await?;
    let status =
        OrganizationInvitationStatus::try_from(invitation.status).map_err(|_| invariant())?;
    if !status.is_pending() {
        return Err(OrganizationRepositoryError::InvitationNotPending);
    }
    let updated = finalize_invitation(
        &transaction,
        invitation,
        OrganizationInvitationStatus::Revoked,
        None,
        now(),
    )
    .await?;
    let record = invitation_record(updated)?;
    mutation::commit(transaction).await?;
    Ok(record)
}

pub(super) async fn resend(
    repository: &OrganizationRepository,
    organization_id: OrganizationId,
    invitation_key: OrganizationInvitationKey,
    write: OrganizationInvitationCreate,
) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
    if write.batch_key.is_some() {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    let transaction = mutation::begin(repository).await?;
    let organization = mutation::lock_organization(&transaction, organization_id).await?;
    require_active_organization(&organization)?;
    let entitlement = mutation::lock_entitlement(&transaction, organization_id).await?;
    ensure_provisioning(&organization, &entitlement)?;
    ensure_team_management(&entitlement, write.team_id)?;
    ensure_inviter(&transaction, write.invited_by_user_id).await?;
    let old = lock_invitation(&transaction, organization_id, invitation_key).await?;
    if !OrganizationInvitationStatus::try_from(old.status)
        .map_err(|_| invariant())?
        .is_pending()
    {
        return Err(OrganizationRepositoryError::InvitationNotPending);
    }
    let old_fingerprint = old.email_fingerprint.clone();
    if old_fingerprint != hex(&write.email_fingerprint) {
        return Err(OrganizationRepositoryError::InvitationEmailMismatch);
    }
    finalize_invitation(
        &transaction,
        old,
        OrganizationInvitationStatus::Revoked,
        None,
        now(),
    )
    .await?;
    validate_team(&transaction, organization_id, write.team_id).await?;
    ensure_no_existing_member_emails(
        &transaction,
        organization_id,
        vec![write.normalized_email.as_str().to_owned()],
    )
    .await?;
    ensure_no_pending_email(&transaction, organization_id, &old_fingerprint, now()).await?;
    ensure_capacity(&transaction, organization_id, &entitlement, 1, now()).await?;
    let record = invitation_record(insert_invitation(&transaction, organization_id, write).await?)?;
    mutation::commit(transaction).await?;
    Ok(record)
}

async fn insert_invitation(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    write: OrganizationInvitationCreate,
) -> Result<organization_invitations::Model, OrganizationRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let email_secret = encrypted_json(write.email_secret)?;
    organization_invitations::ActiveModel {
        invitation_key: Set(write.invitation_key.persistence_key()),
        organization_id: Set(organization_id.get()),
        email_fingerprint: Set(hex(&write.email_fingerprint)),
        email_secret: Set(email_secret),
        token_digest: Set(hex(&write.token_digest)),
        role: Set(write.role.code()),
        team_id: Set(write.team_id.map(OrganizationTeamId::get)),
        status: Set(OrganizationInvitationStatus::Pending.code()),
        invited_by_user_id: Set(write.invited_by_user_id.get()),
        accepted_by_user_id: Set(None),
        batch_key: Set(write
            .batch_key
            .map(OrganizationInvitationKey::persistence_key)),
        batch_digest: Set(write.batch_digest.map(|value| hex(&value))),
        batch_size: Set(write
            .batch_size
            .map(|value| i32::try_from(value).unwrap_or_default())),
        version: Set(1),
        created_at: Set(now),
        expires_at: Set(
            TimeDateTimeWithTimeZone::from_unix_timestamp(write.expires_at)
                .map_err(|_| OrganizationRepositoryError::InvalidInput)?,
        ),
        finalized_at: Set(None),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(map_write_error)
}

async fn finalize_invitation(
    transaction: &DatabaseTransaction,
    model: organization_invitations::Model,
    status: OrganizationInvitationStatus,
    accepted_by_user_id: Option<UserId>,
    finalized_at: i64,
) -> Result<organization_invitations::Model, OrganizationRepositoryError> {
    let next_version = model.version.checked_add(1).ok_or_else(invariant)?;
    let mut active = model.into_active_model();
    active.status = Set(status.code());
    active.accepted_by_user_id = Set(accepted_by_user_id.map(UserId::get));
    active.version = Set(next_version);
    active.finalized_at = Set(Some(
        TimeDateTimeWithTimeZone::from_unix_timestamp(finalized_at)
            .map_err(|_| OrganizationRepositoryError::InvalidInput)?,
    ));
    active.update(transaction).await.map_err(query_error)
}

async fn lock_invitation(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    invitation_key: OrganizationInvitationKey,
) -> Result<organization_invitations::Model, OrganizationRepositoryError> {
    let mut query = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            organization_invitations::Column::InvitationKey.eq(invitation_key.persistence_key()),
        );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)
}

async fn lock_active_sso_provider(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
) -> Result<organization_sso_providers::Model, OrganizationRepositoryError> {
    let mut query = organization_sso_providers::Entity::find()
        .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_providers::Column::ProviderKey.eq(provider_key.persistence_key()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let provider = query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::NotFound)?;
    if OrganizationSsoProviderStatus::try_from(provider.status).map_err(|_| invariant())?
        != OrganizationSsoProviderStatus::Active
    {
        return Err(OrganizationRepositoryError::InvalidTransition);
    }
    Ok(provider)
}

async fn lock_verified_sso_domain(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
    now: i64,
) -> Result<organization_sso_domains::Model, OrganizationRepositoryError> {
    let mut query = organization_sso_domains::Entity::find()
        .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_domains::Column::ProviderId.eq(provider_id))
        .filter(
            organization_sso_domains::Column::Status
                .eq(OrganizationSsoDomainStatus::Verified.code()),
        )
        .filter(
            organization_sso_domains::Column::RecheckAt
                .gt(TimeDateTimeWithTimeZone::from_unix_timestamp(now).map_err(|_| invariant())?),
        )
        .order_by_asc(organization_sso_domains::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationRepositoryError::InvalidTransition)
}

async fn lock_identity_by_subject(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
    subject_digest: &str,
) -> Result<Option<organization_sso_identities::Model>, OrganizationRepositoryError> {
    let mut query = organization_sso_identities::Entity::find()
        .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_identities::Column::ProviderId.eq(provider_id))
        .filter(organization_sso_identities::Column::SubjectDigest.eq(subject_digest));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

async fn lock_active_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership_id: i64,
) -> Result<organization_memberships::Model, OrganizationRepositoryError> {
    let mut query = organization_memberships::Entity::find_by_id(membership_id)
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let membership = query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    if membership.status != OrganizationMembershipStatus::Active.code()
        || membership.suspended_at.is_some()
        || membership.removed_at.is_some()
    {
        return Err(OrganizationRepositoryError::Conflict);
    }
    let user_id = UserId::new(membership.user_id).map_err(|_| invariant())?;
    if !mutation::lock_active_user(transaction, user_id).await? {
        return Err(OrganizationRepositoryError::UserUnavailable);
    }
    Ok(membership)
}

async fn lock_pending_invitation_by_fingerprint(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    email_fingerprint: &str,
) -> Result<organization_invitations::Model, OrganizationRepositoryError> {
    let mut query = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_invitations::Column::EmailFingerprint.eq(email_fingerprint))
        .filter(
            organization_invitations::Column::Status
                .eq(OrganizationInvitationStatus::Pending.code()),
        )
        .order_by_asc(organization_invitations::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let models = query.all(transaction).await.map_err(query_error)?;
    match models.as_slice() {
        [] => Err(OrganizationRepositoryError::NotFound),
        [model] => Ok(model.clone()),
        _ => Err(invariant()),
    }
}

async fn ensure_no_pending_email(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    email_fingerprint: &str,
    now: i64,
) -> Result<(), OrganizationRepositoryError> {
    let exists = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_invitations::Column::EmailFingerprint.eq(email_fingerprint))
        .filter(
            organization_invitations::Column::Status
                .eq(OrganizationInvitationStatus::Pending.code()),
        )
        .filter(
            organization_invitations::Column::ExpiresAt
                .gt(TimeDateTimeWithTimeZone::from_unix_timestamp(now).map_err(|_| invariant())?),
        )
        .one(transaction)
        .await
        .map_err(query_error)?
        .is_some();
    if exists {
        return Err(OrganizationRepositoryError::Conflict);
    }
    Ok(())
}

async fn ensure_no_existing_member_emails(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    normalized_emails: Vec<String>,
) -> Result<(), OrganizationRepositoryError> {
    let existing = organization_memberships::Entity::find()
        .inner_join(users::Entity)
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(users::Column::Email.is_in(normalized_emails))
        .one(transaction)
        .await
        .map_err(query_error)?
        .is_some();
    if existing {
        return Err(OrganizationRepositoryError::Conflict);
    }
    Ok(())
}

async fn ensure_capacity(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    entitlement: &crate::entity::organization_entitlements::Model,
    additional: i64,
    now: i64,
) -> Result<(), OrganizationRepositoryError> {
    let (active, pending) = seat_usage(transaction, organization_id, now).await?;
    let used = active
        .checked_add(pending)
        .and_then(|used| used.checked_add(additional))
        .ok_or_else(invariant)?;
    if used > entitlement.seat_limit {
        return Err(OrganizationRepositoryError::SeatLimitReached);
    }
    Ok(())
}

async fn seat_usage(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    now: i64,
) -> Result<(i64, i64), OrganizationRepositoryError> {
    let active = crate::entity::organization_memberships::Entity::find()
        .filter(
            crate::entity::organization_memberships::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(
            crate::entity::organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(transaction)
        .await
        .map_err(query_error)?;
    let pending = organization_invitations::Entity::find()
        .filter(organization_invitations::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            organization_invitations::Column::Status
                .eq(OrganizationInvitationStatus::Pending.code()),
        )
        .filter(
            organization_invitations::Column::ExpiresAt
                .gt(TimeDateTimeWithTimeZone::from_unix_timestamp(now).map_err(|_| invariant())?),
        )
        .count(transaction)
        .await
        .map_err(query_error)?;
    Ok((
        i64::try_from(active).map_err(|_| invariant())?,
        i64::try_from(pending).map_err(|_| invariant())?,
    ))
}

async fn validate_team(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    team_id: Option<OrganizationTeamId>,
) -> Result<(), OrganizationRepositoryError> {
    if let Some(team_id) = team_id {
        mutation::lock_active_team(transaction, organization_id, team_id).await?;
    }
    Ok(())
}

async fn ensure_inviter(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<(), OrganizationRepositoryError> {
    if mutation::lock_active_user(transaction, user_id).await? {
        Ok(())
    } else {
        Err(OrganizationRepositoryError::UserUnavailable)
    }
}

fn ensure_provisioning(
    organization: &crate::entity::organizations::Model,
    entitlement: &crate::entity::organization_entitlements::Model,
) -> Result<(), OrganizationRepositoryError> {
    let status = OrganizationStatus::try_from(organization.status).map_err(|_| invariant())?;
    let snapshot = read::entitlement_snapshot(entitlement.clone())?;
    if !snapshot.status(status, now()).allows_provisioning() {
        return Err(OrganizationRepositoryError::EntitlementRestricted);
    }
    Ok(())
}

fn ensure_team_management(
    entitlement: &crate::entity::organization_entitlements::Model,
    team_id: Option<OrganizationTeamId>,
) -> Result<(), OrganizationRepositoryError> {
    if team_id.is_some()
        && !read::entitlement_snapshot(entitlement.clone())?.team_management_enabled()
    {
        return Err(OrganizationRepositoryError::EntitlementRestricted);
    }
    Ok(())
}

fn ensure_team_management_enabled(
    entitlement: &crate::entity::organization_entitlements::Model,
) -> Result<(), OrganizationRepositoryError> {
    if !read::entitlement_snapshot(entitlement.clone())?.team_management_enabled() {
        return Err(OrganizationRepositoryError::EntitlementRestricted);
    }
    Ok(())
}

fn require_active_organization(
    organization: &crate::entity::organizations::Model,
) -> Result<(), OrganizationRepositoryError> {
    let status = OrganizationStatus::try_from(organization.status).map_err(|_| invariant())?;
    if !status.is_active() || organization.closed_at.is_some() {
        return Err(OrganizationRepositoryError::InvalidTransition);
    }
    Ok(())
}

fn invitation_record(
    model: organization_invitations::Model,
) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
    let invitation_key = OrganizationInvitationKey::from_persistence_key(&model.invitation_key)
        .map_err(|_| invariant())?;
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let role = OrganizationRole::try_from(model.role).map_err(|_| invariant())?;
    let team_id = model
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let status = OrganizationInvitationStatus::try_from(model.status).map_err(|_| invariant())?;
    let invited_by_user_id = UserId::new(model.invited_by_user_id).map_err(|_| invariant())?;
    let accepted_by_user_id = model
        .accepted_by_user_id
        .map(UserId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let batch_key = model
        .batch_key
        .as_deref()
        .map(OrganizationInvitationKey::from_persistence_key)
        .transpose()
        .map_err(|_| invariant())?;
    if model.version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationInvitationRecord {
        id: model.id,
        invitation_key,
        organization_id,
        role,
        team_id,
        status,
        invited_by_user_id,
        accepted_by_user_id,
        batch_key,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        expires_at: model.expires_at.unix_timestamp(),
        finalized_at: model.finalized_at.map(|value| value.unix_timestamp()),
    })
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<crate::entity::EncryptedJson, OrganizationRepositoryError> {
    crate::entity::EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| OrganizationRepositoryError::InvalidInput)
}

fn envelope_from_json(
    encrypted: crate::entity::EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, OrganizationRepositoryError> {
    let (key_id, nonce, ciphertext) = encrypted.envelope_parts().map_err(|_| invariant())?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext).map_err(|_| invariant())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn now() -> i64 {
    TimeDateTimeWithTimeZone::now_utc().unix_timestamp()
}

fn map_write_error(error: DbErr) -> OrganizationRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationRepositoryError::Conflict
    } else {
        query_error(error)
    }
}

fn validate_sso_digest(value: &str) -> Result<(), OrganizationRepositoryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Err(OrganizationRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}

fn jit_username(subject_digest: &str) -> String {
    // 用户名只由不透明摘要组成，避免把 subject、邮箱或显示名带入全局用户表。
    format!("{JIT_USERNAME_PREFIX}{}", &subject_digest[..56])
}

fn map_jit_user_error(error: AdminUserRepositoryError) -> OrganizationRepositoryError {
    match error {
        AdminUserRepositoryError::Conflict => OrganizationRepositoryError::Conflict,
        AdminUserRepositoryError::InvalidReference => OrganizationRepositoryError::Invariant,
        AdminUserRepositoryError::Query | AdminUserRepositoryError::Timeout => {
            OrganizationRepositoryError::Query
        }
        AdminUserRepositoryError::Invariant | AdminUserRepositoryError::Entropy => {
            OrganizationRepositoryError::Invariant
        }
    }
}

fn query_error(_error: impl std::fmt::Debug) -> OrganizationRepositoryError {
    record_internal_error(OrganizationRepositoryError::Query)
}

fn invariant() -> OrganizationRepositoryError {
    record_internal_error(OrganizationRepositoryError::Invariant)
}
