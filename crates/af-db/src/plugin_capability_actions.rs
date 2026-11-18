//! Durable confirmation and idempotency records for plugin write capabilities.

use std::time::Duration;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::plugin_capability_actions};

const STATUS_PENDING: i16 = 1;
const STATUS_EXECUTING: i16 = 2;
const STATUS_SUCCEEDED: i16 = 3;
const STATUS_UNKNOWN: i16 = 4;
const STATUS_EXPIRED: i16 = 5;
const STATUS_REVOKED: i16 = 6;
const MAX_OUTCOME_CODE: usize = 128;
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginCapabilityActionCreate {
    pub proposal_key: String,
    pub idempotency_key: String,
    pub requested_by_user_id: i64,
    pub plugin_id: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub input_sha256: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginCapabilityActionRecord {
    pub id: i64,
    pub proposal_key: String,
    pub idempotency_key: String,
    pub requested_by_user_id: i64,
    pub confirmed_by_user_id: Option<i64>,
    pub plugin_id: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub input_sha256: String,
    pub status: PluginCapabilityActionStatus,
    pub expires_at: i64,
    pub created_at: i64,
    pub confirmed_at: Option<i64>,
    pub confirmation_key: Option<String>,
    pub outcome_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginCapabilityActionStatus {
    Pending,
    Executing,
    Succeeded,
    Unknown,
    Expired,
    Revoked,
}

impl PluginCapabilityActionStatus {
    fn from_code(value: i16) -> Result<Self, PluginCapabilityActionRepositoryError> {
        match value {
            STATUS_PENDING => Ok(Self::Pending),
            STATUS_EXECUTING => Ok(Self::Executing),
            STATUS_SUCCEEDED => Ok(Self::Succeeded),
            STATUS_UNKNOWN => Ok(Self::Unknown),
            STATUS_EXPIRED => Ok(Self::Expired),
            STATUS_REVOKED => Ok(Self::Revoked),
            _ => Err(PluginCapabilityActionRepositoryError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginCapabilityActionRepositoryError {
    #[error("plugin capability action query failed")]
    Query,
    #[error("plugin capability action operation timed out")]
    Timeout,
    #[error("plugin capability action invariant is invalid")]
    Invariant,
    #[error("plugin capability action conflicts with current state")]
    Conflict,
    #[error("plugin capability action was not found")]
    NotFound,
}

#[derive(Clone)]
pub struct PluginCapabilityActionRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PluginCapabilityActionRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    pub async fn create(
        &self,
        input: PluginCapabilityActionCreate,
    ) -> Result<(PluginCapabilityActionRecord, bool), PluginCapabilityActionRepositoryError> {
        validate_create(&input)?;
        let replay = input.clone();
        match timeout(self.operation_timeout, async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            if let Some(existing) = plugin_capability_actions::Entity::find()
                .filter(
                    plugin_capability_actions::Column::RequestedByUserId
                        .eq(input.requested_by_user_id),
                )
                .filter(
                    plugin_capability_actions::Column::IdempotencyKey
                        .eq(input.idempotency_key.clone()),
                )
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
            {
                verify_replay(&existing, &input)?;
                transaction.commit().await.map_err(map_db)?;
                return Ok((record(existing)?, false));
            }
            let row = plugin_capability_actions::ActiveModel {
                proposal_key: Set(input.proposal_key),
                idempotency_key: Set(input.idempotency_key),
                requested_by_user_id: Set(input.requested_by_user_id),
                confirmed_by_user_id: Set(None),
                plugin_id: Set(input.plugin_id),
                artifact_id: Set(input.artifact_id),
                artifact_digest: Set(input.artifact_digest),
                deployment_generation: Set(input.deployment_generation),
                capability_id: Set(input.capability_id),
                capability_version: Set(i32::from(input.capability_version)),
                input_sha256: Set(input.input_sha256),
                status: Set(STATUS_PENDING),
                expires_at: Set(input.expires_at),
                created_at: Set(input.created_at),
                confirmed_at: Set(None),
                confirmation_key: Set(None),
                outcome_code: Set(None),
                ..Default::default()
            };
            match row.insert(&transaction).await {
                Ok(inserted) => {
                    transaction.commit().await.map_err(map_db)?;
                    Ok((record(inserted)?, true))
                }
                Err(_) => {
                    transaction.rollback().await.map_err(map_db)?;
                    let existing = plugin_capability_actions::Entity::find()
                        .filter(
                            plugin_capability_actions::Column::RequestedByUserId
                                .eq(replay.requested_by_user_id),
                        )
                        .filter(
                            plugin_capability_actions::Column::IdempotencyKey
                                .eq(replay.idempotency_key.clone()),
                        )
                        .one(self.pool.connection())
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(map_db)?
                        .ok_or(PluginCapabilityActionRepositoryError::Query)?;
                    verify_replay(&existing, &replay)?;
                    Ok((record(existing)?, false))
                }
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginCapabilityActionRepositoryError::Timeout),
        }
    }

    pub async fn get_by_key(
        &self,
        proposal_key: &str,
    ) -> Result<PluginCapabilityActionRecord, PluginCapabilityActionRepositoryError> {
        if !valid_token(proposal_key, 36) {
            return Err(PluginCapabilityActionRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let row = plugin_capability_actions::Entity::find()
                .filter(plugin_capability_actions::Column::ProposalKey.eq(proposal_key))
                .one(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
                .ok_or(PluginCapabilityActionRepositoryError::NotFound)?;
            record(row)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginCapabilityActionRepositoryError::Timeout),
        }
    }

    /// Atomically confirms and claims one execution. Terminal states replay only with the same key.
    pub async fn claim_for_execution(
        &self,
        proposal_key: &str,
        confirmer_user_id: i64,
        confirmation_key: &str,
        input_sha256: &str,
        now: i64,
    ) -> Result<PluginCapabilityActionRecord, PluginCapabilityActionRepositoryError> {
        if !valid_token(proposal_key, 36)
            || !valid_token(confirmation_key, 128)
            || !valid_sha256(input_sha256)
            || confirmer_user_id <= 0
            || now <= 0
        {
            return Err(PluginCapabilityActionRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let Some(row) = plugin_capability_actions::Entity::find()
                .filter(plugin_capability_actions::Column::ProposalKey.eq(proposal_key))
                .lock_exclusive()
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
            else {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginCapabilityActionRepositoryError::NotFound);
            };
            if row.input_sha256 != input_sha256 {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginCapabilityActionRepositoryError::Conflict);
            }
            if matches!(row.status, STATUS_SUCCEEDED | STATUS_UNKNOWN) {
                if row.confirmation_key.as_deref() != Some(confirmation_key) {
                    transaction.rollback().await.map_err(map_db)?;
                    return Err(PluginCapabilityActionRepositoryError::Conflict);
                }
                transaction.commit().await.map_err(map_db)?;
                return record(row);
            }
            if row.status != STATUS_PENDING {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginCapabilityActionRepositoryError::Conflict);
            }
            if row.expires_at <= now {
                let mut update: plugin_capability_actions::ActiveModel = row.clone().into();
                update.status = Set(STATUS_EXPIRED);
                let updated = update.update(&transaction).await.map_err(map_db)?;
                transaction.commit().await.map_err(map_db)?;
                return record(updated);
            }
            let mut update: plugin_capability_actions::ActiveModel = row.clone().into();
            update.status = Set(STATUS_EXECUTING);
            update.confirmed_by_user_id = Set(Some(confirmer_user_id));
            update.confirmed_at = Set(Some(now));
            update.confirmation_key = Set(Some(confirmation_key.to_owned()));
            let updated = update.update(&transaction).await.map_err(map_db)?;
            transaction.commit().await.map_err(map_db)?;
            record(updated)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginCapabilityActionRepositoryError::Timeout),
        }
    }

    pub async fn finish(
        &self,
        proposal_key: &str,
        status: PluginCapabilityActionStatus,
        outcome_code: &str,
    ) -> Result<PluginCapabilityActionRecord, PluginCapabilityActionRepositoryError> {
        if !valid_token(proposal_key, 36)
            || !matches!(
                status,
                PluginCapabilityActionStatus::Succeeded | PluginCapabilityActionStatus::Unknown
            )
            || !valid_outcome_code(outcome_code)
        {
            return Err(PluginCapabilityActionRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let row = plugin_capability_actions::Entity::find()
                .filter(plugin_capability_actions::Column::ProposalKey.eq(proposal_key))
                .lock_exclusive()
                .one(&transaction)
                .await
                .map_err(map_db)?
                .ok_or(PluginCapabilityActionRepositoryError::NotFound)?;
            if row.status != STATUS_EXECUTING {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginCapabilityActionRepositoryError::Conflict);
            }
            let mut update: plugin_capability_actions::ActiveModel = row.into();
            update.status = Set(match status {
                PluginCapabilityActionStatus::Succeeded => STATUS_SUCCEEDED,
                _ => STATUS_UNKNOWN,
            });
            update.outcome_code = Set(Some(outcome_code.to_owned()));
            let updated = update.update(&transaction).await.map_err(map_db)?;
            transaction.commit().await.map_err(map_db)?;
            record(updated)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginCapabilityActionRepositoryError::Timeout),
        }
    }
}

fn validate_create(
    input: &PluginCapabilityActionCreate,
) -> Result<(), PluginCapabilityActionRepositoryError> {
    if !valid_token(&input.proposal_key, 36)
        || !valid_token(&input.idempotency_key, 128)
        || input.requested_by_user_id <= 0
        || input.plugin_id.is_empty()
        || input.plugin_id.len() > 128
        || input.artifact_id <= 0
        || !valid_sha256(&input.artifact_digest)
        || input.deployment_generation <= 0
        || input.capability_id.is_empty()
        || input.capability_id.len() > 128
        || input.capability_version == 0
        || !valid_sha256(&input.input_sha256)
        || input.created_at <= 0
        || input.expires_at <= input.created_at
        || input.expires_at - input.created_at > 600
    {
        return Err(PluginCapabilityActionRepositoryError::Invariant);
    }
    Ok(())
}
fn verify_replay(
    row: &plugin_capability_actions::Model,
    input: &PluginCapabilityActionCreate,
) -> Result<(), PluginCapabilityActionRepositoryError> {
    if row.proposal_key != input.proposal_key
        || row.plugin_id != input.plugin_id
        || row.artifact_id != input.artifact_id
        || row.artifact_digest != input.artifact_digest
        || row.deployment_generation != input.deployment_generation
        || row.capability_id != input.capability_id
        || row.capability_version != i32::from(input.capability_version)
        || row.input_sha256 != input.input_sha256
    {
        Err(PluginCapabilityActionRepositoryError::Conflict)
    } else {
        Ok(())
    }
}
fn record(
    row: plugin_capability_actions::Model,
) -> Result<PluginCapabilityActionRecord, PluginCapabilityActionRepositoryError> {
    Ok(PluginCapabilityActionRecord {
        id: row.id,
        proposal_key: row.proposal_key,
        idempotency_key: row.idempotency_key,
        requested_by_user_id: row.requested_by_user_id,
        confirmed_by_user_id: row.confirmed_by_user_id,
        plugin_id: row.plugin_id,
        artifact_id: row.artifact_id,
        artifact_digest: row.artifact_digest,
        deployment_generation: row.deployment_generation,
        capability_id: row.capability_id,
        capability_version: u16::try_from(row.capability_version)
            .map_err(|_| PluginCapabilityActionRepositoryError::Invariant)?,
        input_sha256: row.input_sha256,
        status: PluginCapabilityActionStatus::from_code(row.status)?,
        expires_at: row.expires_at,
        created_at: row.created_at,
        confirmed_at: row.confirmed_at,
        confirmation_key: row.confirmation_key,
        outcome_code: row.outcome_code,
    })
}
fn valid_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn valid_outcome_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OUTCOME_CODE
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}
fn map_db(_: sea_orm::DbErr) -> PluginCapabilityActionRepositoryError {
    PluginCapabilityActionRepositoryError::Query
}
