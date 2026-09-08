use std::str::FromStr;

use af_domain::{
    AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestFingerprint,
    AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal, GroupId, Protocol, TokenId,
    UpstreamTaskId, UserId,
};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    async_task::AsyncTaskRepositoryError,
    entity::{SensitiveString, async_task_submission_claims},
};

use super::types::{
    AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionRecord,
    AsyncTaskSubmissionState, AsyncTaskVideoResolution,
};
use crate::async_task::status::status_from_persistence;

pub(super) async fn load_claim_collision<C>(
    connection: &C,
    write: &AsyncTaskSubmissionClaim,
) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let models = async_task_submission_claims::Entity::find()
        .filter(
            Condition::any()
                .add(
                    async_task_submission_claims::Column::TaskKey
                        .eq(SensitiveString::from(write.task_id.persistence_key())),
                )
                .add(
                    Condition::all()
                        .add(
                            async_task_submission_claims::Column::UserId
                                .eq(write.principal.user_id().get()),
                        )
                        .add(
                            async_task_submission_claims::Column::IdempotencyKey
                                .eq(SensitiveString::from(write.request_id.persistence_key())),
                        ),
                ),
        )
        .all(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if models.len() > 1 {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    models.into_iter().next().map(claim_record).transpose()
}

pub(super) fn classify_claim_collision(
    existing: AsyncTaskSubmissionRecord,
    write: &AsyncTaskSubmissionClaim,
) -> Result<AsyncTaskSubmissionClaimOutcome, AsyncTaskRepositoryError> {
    if existing.matches_claim(write) {
        Ok(AsyncTaskSubmissionClaimOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

pub(super) async fn load_by_owner<C>(
    connection: &C,
    user_id: UserId,
    task_id: AsyncTaskId,
) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_task_submission_claims::Entity::find()
        .filter(
            async_task_submission_claims::Column::TaskKey
                .eq(SensitiveString::from(task_id.persistence_key())),
        )
        .filter(async_task_submission_claims::Column::UserId.eq(user_id.get()))
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(claim_record)
        .transpose()
}

pub(super) async fn load_by_request<C>(
    connection: &C,
    user_id: UserId,
    request_id: AsyncTaskRequestId,
) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_task_submission_claims::Entity::find()
        .filter(async_task_submission_claims::Column::UserId.eq(user_id.get()))
        .filter(
            async_task_submission_claims::Column::IdempotencyKey
                .eq(SensitiveString::from(request_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(claim_record)
        .transpose()
}

pub(super) async fn load_by_database_id<C>(
    connection: &C,
    database_id: i64,
) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_task_submission_claims::Entity::find_by_id(database_id)
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(claim_record)
        .transpose()
}

pub(super) fn claim_record(
    model: async_task_submission_claims::Model,
) -> Result<AsyncTaskSubmissionRecord, AsyncTaskRepositoryError> {
    let task_id = AsyncTaskId::from_persistence_key(model.task_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let request_id = AsyncTaskRequestId::from_persistence_key(model.idempotency_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let principal = GatewayPrincipal::new(
        TokenId::new(model.token_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
        UserId::new(model.user_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
        GroupId::new(model.group_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
    );
    let protocol =
        Protocol::from_str(&model.protocol).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let requested_model = model.requested_model.as_str().to_owned();
    validate_model(&requested_model)?;
    let request_fingerprint =
        AsyncTaskRequestFingerprint::from_persistence_key(model.request_fingerprint.as_str())
            .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let video_duration_seconds = model
        .video_duration_seconds
        .map(u8::try_from)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if video_duration_seconds.is_some_and(|value| !(1..=15).contains(&value)) {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    let state = claim_state_from_code(model.state)?;
    let attempt_id = model
        .attempt_key
        .as_ref()
        .map(|value| AsyncTaskAttemptId::from_persistence_key(value.as_str()))
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let target_group_id = model
        .target_group_id
        .map(GroupId::new)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let upstream_model = model
        .upstream_model
        .as_ref()
        .map(|value| value.as_str().to_owned());
    if upstream_model
        .as_deref()
        .is_some_and(|value| validate_model(value).is_err())
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    let channel_id = model
        .channel_id
        .map(ChannelId::new)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let credential_id = model
        .credential_id
        .map(CredentialId::new)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let credential_revision = model
        .credential_revision
        .as_ref()
        .map(|value| parse_credential_revision(value.as_str()))
        .transpose()?;
    let upstream_task_id = model
        .upstream_task_id
        .as_ref()
        .map(|value| UpstreamTaskId::new(value.as_str().to_owned()))
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let binding_fingerprint = model
        .binding_fingerprint
        .as_ref()
        .map(|value| AsyncTaskBindingFingerprint::from_persistence_key(value.as_str()))
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let attempt_timeout_millis = model
        .attempt_timeout_millis
        .map(u64::try_from)
        .transpose()
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let video_resolution = model
        .video_resolution
        .map(|value| {
            AsyncTaskVideoResolution::from_database(value)
                .ok_or(AsyncTaskRepositoryError::Invariant)
        })
        .transpose()?;
    let status = match (model.status, model.progress_basis_points) {
        (Some(status), Some(progress)) => Some(status_from_persistence(
            status,
            progress,
            model.failure_kind,
        )?),
        (None, None) if model.failure_kind.is_none() => None,
        _ => return Err(AsyncTaskRepositoryError::Invariant),
    };
    let version = u64::try_from(model.version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let accepted_at = optional_unix_seconds(model.accepted_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let binding_present = target_group_id.is_some()
        && upstream_model.is_some()
        && channel_id.is_some()
        && credential_id.is_some()
        && credential_revision.is_some()
        && upstream_task_id.is_some()
        && binding_fingerprint.is_some()
        && attempt_timeout_millis.is_some_and(|value| (1..=900_000).contains(&value))
        && status.is_some()
        && accepted_at.is_some();
    let shape_valid = match state {
        AsyncTaskSubmissionState::Claimed => attempt_id.is_none() && !binding_present,
        AsyncTaskSubmissionState::Submitting => attempt_id.is_some() && !binding_present,
        AsyncTaskSubmissionState::Accepted => attempt_id.is_some() && binding_present,
    };
    let no_partial_binding = binding_present
        || (target_group_id.is_none()
            && upstream_model.is_none()
            && channel_id.is_none()
            && credential_id.is_none()
            && credential_revision.is_none()
            && upstream_task_id.is_none()
            && binding_fingerprint.is_none()
            && attempt_timeout_millis.is_none()
            && video_resolution.is_none()
            && status.is_none()
            && accepted_at.is_none());
    if model.id <= 0
        || version == 0
        || updated_at < created_at
        || accepted_at.is_some_and(|value| value < created_at || value > updated_at)
        || !shape_valid
        || !no_partial_binding
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    Ok(AsyncTaskSubmissionRecord {
        database_id: model.id,
        task_id,
        request_id,
        principal,
        protocol,
        requested_model,
        request_fingerprint,
        video_duration_seconds,
        state,
        attempt_id,
        target_group_id,
        upstream_model,
        channel_id,
        credential_id,
        credential_revision,
        upstream_task_id,
        binding_fingerprint,
        attempt_timeout_millis,
        video_resolution,
        status,
        version,
        accepted_at,
        created_at,
        updated_at,
    })
}

fn validate_model(value: &str) -> Result<(), AsyncTaskRepositoryError> {
    if value.is_empty()
        || value.len() > af_domain::MAX_MODEL_NAME_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(AsyncTaskRepositoryError::Invariant)
    } else {
        Ok(())
    }
}

pub(super) const fn state_code_for_claim(state: AsyncTaskSubmissionState) -> i16 {
    match state {
        AsyncTaskSubmissionState::Claimed => 1,
        AsyncTaskSubmissionState::Submitting => 2,
        AsyncTaskSubmissionState::Accepted => 3,
    }
}

fn claim_state_from_code(code: i16) -> Result<AsyncTaskSubmissionState, AsyncTaskRepositoryError> {
    match code {
        1 => Ok(AsyncTaskSubmissionState::Claimed),
        2 => Ok(AsyncTaskSubmissionState::Submitting),
        3 => Ok(AsyncTaskSubmissionState::Accepted),
        _ => Err(AsyncTaskRepositoryError::Invariant),
    }
}

pub(super) fn credential_revision_key(revision: u64) -> String {
    format!("{revision:016x}")
}

fn parse_credential_revision(value: &str) -> Result<u64, AsyncTaskRepositoryError> {
    if value.len() != 16
        || !value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    u64::from_str_radix(value, 16).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

pub(super) fn expected_version(value: i64) -> Result<u64, AsyncTaskRepositoryError> {
    u64::try_from(value).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

pub(super) fn next_version(value: i64) -> Result<i64, AsyncTaskRepositoryError> {
    value
        .checked_add(1)
        .filter(|value| *value > 1)
        .ok_or(AsyncTaskRepositoryError::Invariant)
}

pub(super) fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, AsyncTaskRepositoryError> {
    let value = i64::try_from(value).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, AsyncTaskRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, AsyncTaskRepositoryError> {
    value.map(unix_seconds).transpose()
}
