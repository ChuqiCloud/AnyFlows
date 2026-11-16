//! Durable confirmation records for host-registered, non-financial plugin actions.

use std::{fmt, time::Duration};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionTrait, sea_query::Expr,
};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{channel_monitor_targets, plugin_action_proposals},
};

const STATUS_PENDING: i16 = 1;
const STATUS_CONFIRMED: i16 = 2;
const STATUS_EXPIRED: i16 = 3;
const STATUS_STALE: i16 = 4;
const STATUS_REVOKED: i16 = 5;
const ACTION_ID: &str = "active-channel-monitor.target.set-enabled";
const CAPABILITY_ID: &str = "channel.monitor.target.action-propose.read";
const MAX_INPUT_BYTES: usize = 4 * 1024;
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Eq, PartialEq)]
pub struct PluginActionProposalCreate {
    pub proposal_key: String,
    pub idempotency_key: String,
    pub requested_by_user_id: i64,
    pub plugin_id: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_version: u16,
    pub action_version: u16,
    pub input_json: String,
    pub target_id: i64,
    pub expected_target_version: i64,
    pub expected_enabled: bool,
    pub requested_enabled: bool,
    pub created_at: i64,
    pub expires_at: i64,
}

impl PluginActionProposalCreate {
    fn validate(&self) -> Result<(), PluginActionProposalRepositoryError> {
        if !valid_token(&self.proposal_key, 36)
            || !valid_token(&self.idempotency_key, 128)
            || self.requested_by_user_id <= 0
            || self.plugin_id.is_empty()
            || self.plugin_id.len() > 128
            || self.artifact_id <= 0
            || !valid_digest(&self.artifact_digest)
            || self.deployment_generation <= 0
            || self.capability_version != 1
            || self.action_version != 1
            || self.target_id <= 0
            || self.expected_target_version <= 0
            || self.expected_enabled == self.requested_enabled
            || self.created_at <= 0
            || self.expires_at <= self.created_at
            || self.expires_at - self.created_at > 600
            || self.input_json.len() > MAX_INPUT_BYTES
            || !valid_proposal_input(&self.input_json, self.requested_enabled)
        {
            return Err(PluginActionProposalRepositoryError::Invariant);
        }
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PluginActionProposalRecord {
    pub id: i64,
    pub proposal_key: String,
    pub requested_by_user_id: i64,
    pub confirmed_by_user_id: Option<i64>,
    pub plugin_id: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub action_id: String,
    pub action_version: u16,
    pub input_json: String,
    pub target_id: i64,
    pub expected_target_version: i64,
    pub expected_enabled: bool,
    pub requested_enabled: bool,
    pub status: PluginActionProposalStatus,
    pub expires_at: i64,
    pub created_at: i64,
    pub confirmed_at: Option<i64>,
    pub confirmation_key: Option<String>,
    pub outcome: Option<Value>,
}

impl fmt::Debug for PluginActionProposalRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginActionProposalRecord")
            .field("id", &self.id)
            .field("proposal_key", &self.proposal_key)
            .field("requested_by_user_id", &self.requested_by_user_id)
            .field("confirmed_by_user_id", &self.confirmed_by_user_id)
            .field("plugin_id", &self.plugin_id)
            .field("deployment_generation", &self.deployment_generation)
            .field("action_id", &self.action_id)
            .field("target_id", &self.target_id)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginActionProposalStatus {
    Pending,
    Confirmed,
    Expired,
    Stale,
    Revoked,
}

impl PluginActionProposalStatus {
    fn from_code(value: i16) -> Result<Self, PluginActionProposalRepositoryError> {
        match value {
            STATUS_PENDING => Ok(Self::Pending),
            STATUS_CONFIRMED => Ok(Self::Confirmed),
            STATUS_EXPIRED => Ok(Self::Expired),
            STATUS_STALE => Ok(Self::Stale),
            STATUS_REVOKED => Ok(Self::Revoked),
            _ => Err(PluginActionProposalRepositoryError::Invariant),
        }
    }

    const fn code(self) -> i16 {
        match self {
            Self::Pending => STATUS_PENDING,
            Self::Confirmed => STATUS_CONFIRMED,
            Self::Expired => STATUS_EXPIRED,
            Self::Stale => STATUS_STALE,
            Self::Revoked => STATUS_REVOKED,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginActionProposalRepositoryError {
    #[error("plugin action proposal query failed")]
    Query,
    #[error("plugin action proposal operation timed out")]
    Timeout,
    #[error("plugin action proposal invariant is invalid")]
    Invariant,
    #[error("plugin action proposal conflicts with current state")]
    Conflict,
    #[error("plugin action proposal was not found")]
    NotFound,
}

#[derive(Clone)]
pub struct PluginActionProposalRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PluginActionProposalRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    pub async fn create(
        &self,
        input: PluginActionProposalCreate,
    ) -> Result<(PluginActionProposalRecord, bool), PluginActionProposalRepositoryError> {
        input.validate()?;
        let replay_input = input.clone();
        match timeout(self.operation_timeout, async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            if let Some(existing) = plugin_action_proposals::Entity::find()
                .filter(
                    plugin_action_proposals::Column::RequestedByUserId
                        .eq(input.requested_by_user_id),
                )
                .filter(
                    plugin_action_proposals::Column::IdempotencyKey
                        .eq(input.idempotency_key.clone()),
                )
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
            {
                verify_idempotent_replay(&existing, &input)?;
                transaction.commit().await.map_err(map_db)?;
                return Ok((record(existing)?, false));
            }
            let row = plugin_action_proposals::ActiveModel {
                proposal_key: Set(input.proposal_key),
                idempotency_key: Set(input.idempotency_key),
                requested_by_user_id: Set(input.requested_by_user_id),
                confirmed_by_user_id: Set(None),
                plugin_id: Set(input.plugin_id),
                artifact_id: Set(input.artifact_id),
                artifact_digest: Set(input.artifact_digest),
                deployment_generation: Set(input.deployment_generation),
                capability_id: Set(CAPABILITY_ID.to_owned()),
                capability_version: Set(i32::from(input.capability_version)),
                action_id: Set(ACTION_ID.to_owned()),
                action_version: Set(i32::from(input.action_version)),
                input_json: Set(input.input_json),
                target_id: Set(input.target_id),
                expected_target_version: Set(input.expected_target_version),
                expected_enabled: Set(input.expected_enabled),
                requested_enabled: Set(input.requested_enabled),
                status: Set(STATUS_PENDING),
                expires_at: Set(input.expires_at),
                created_at: Set(input.created_at),
                confirmed_at: Set(None),
                confirmation_key: Set(None),
                outcome_json: Set(None),
                ..Default::default()
            };
            match row.insert(&transaction).await {
                Ok(row) => {
                    transaction.commit().await.map_err(map_db)?;
                    Ok((record(row)?, true))
                }
                Err(_) => {
                    // Another request may have won the unique requester/idempotency key.
                    transaction.rollback().await.map_err(map_db)?;
                    let existing = plugin_action_proposals::Entity::find()
                        .filter(
                            plugin_action_proposals::Column::RequestedByUserId
                                .eq(replay_input.requested_by_user_id),
                        )
                        .filter(
                            plugin_action_proposals::Column::IdempotencyKey
                                .eq(replay_input.idempotency_key.clone()),
                        )
                        .one(self.pool.connection())
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(map_db)?
                        .ok_or(PluginActionProposalRepositoryError::Query)?;
                    verify_idempotent_replay(&existing, &replay_input)?;
                    Ok((record(existing)?, false))
                }
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginActionProposalRepositoryError::Timeout),
        }
    }

    pub async fn get_by_key(
        &self,
        proposal_key: &str,
    ) -> Result<PluginActionProposalRecord, PluginActionProposalRepositoryError> {
        if !valid_token(proposal_key, 36) {
            return Err(PluginActionProposalRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let row = plugin_action_proposals::Entity::find()
                .filter(plugin_action_proposals::Column::ProposalKey.eq(proposal_key))
                .one(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
                .ok_or(PluginActionProposalRepositoryError::NotFound)?;
            record(row)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginActionProposalRepositoryError::Timeout),
        }
    }

    /// Revalidates the plugin deployment and target under locks, then writes the target change
    /// and immutable confirmation outcome in the same database transaction.
    pub async fn confirm(
        &self,
        proposal_key: &str,
        confirmer_user_id: i64,
        confirmation_key: &str,
        now: i64,
    ) -> Result<PluginActionProposalRecord, PluginActionProposalRepositoryError> {
        if !valid_token(proposal_key, 36)
            || !valid_token(confirmation_key, 128)
            || confirmer_user_id <= 0
            || now <= 0
        {
            return Err(PluginActionProposalRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let transaction = self.pool.connection().begin().await.map_err(map_db)?;
            let Some(proposal) = plugin_action_proposals::Entity::find()
                .filter(plugin_action_proposals::Column::ProposalKey.eq(proposal_key))
                .lock_exclusive()
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?
            else {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginActionProposalRepositoryError::NotFound);
            };
            if proposal.status == STATUS_CONFIRMED {
                if proposal.confirmation_key.as_deref() != Some(confirmation_key) {
                    transaction.rollback().await.map_err(map_db)?;
                    return Err(PluginActionProposalRepositoryError::Conflict);
                }
                transaction.commit().await.map_err(map_db)?;
                return record(proposal);
            }
            if proposal.status != STATUS_PENDING {
                transaction.rollback().await.map_err(map_db)?;
                return Err(PluginActionProposalRepositoryError::Conflict);
            }
            if proposal.expires_at <= now {
                let updated = finish_proposal(
                    &transaction,
                    proposal.id,
                    STATUS_EXPIRED,
                    None,
                    None,
                    json!({"code":"expired"}),
                )
                .await?;
                transaction.commit().await.map_err(map_db)?;
                return record(updated);
            }
            if !deployment_is_current(&transaction, &proposal).await? {
                let updated = finish_proposal(
                    &transaction,
                    proposal.id,
                    STATUS_REVOKED,
                    None,
                    None,
                    json!({"code":"plugin_deployment_changed"}),
                )
                .await?;
                transaction.commit().await.map_err(map_db)?;
                return record(updated);
            }
            let target = channel_monitor_targets::Entity::find_by_id(proposal.target_id)
                .lock_exclusive()
                .one(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db)?;
            let Some(target) = target.filter(|target| {
                !target.delete_after_lease
                    && target.config_version == proposal.expected_target_version
                    && target.enabled == proposal.expected_enabled
            }) else {
                let updated = finish_proposal(
                    &transaction,
                    proposal.id,
                    STATUS_STALE,
                    None,
                    None,
                    json!({"code":"target_changed"}),
                )
                .await?;
                transaction.commit().await.map_err(map_db)?;
                return record(updated);
            };
            let next_version = target
                .config_version
                .checked_add(1)
                .ok_or(PluginActionProposalRepositoryError::Invariant)?;
            let target_update = channel_monitor_targets::Entity::update_many()
                .col_expr(
                    channel_monitor_targets::Column::Enabled,
                    Expr::value(proposal.requested_enabled),
                )
                .col_expr(
                    channel_monitor_targets::Column::ConfigVersion,
                    Expr::value(next_version),
                )
                .col_expr(
                    channel_monitor_targets::Column::UpdatedAt,
                    Expr::value(time::OffsetDateTime::now_utc()),
                )
                .filter(channel_monitor_targets::Column::Id.eq(proposal.target_id))
                .filter(
                    channel_monitor_targets::Column::ConfigVersion
                        .eq(proposal.expected_target_version),
                )
                .filter(channel_monitor_targets::Column::Enabled.eq(proposal.expected_enabled))
                .filter(channel_monitor_targets::Column::DeleteAfterLease.eq(false))
                .exec(&transaction)
                .await
                .map_err(map_db)?;
            if target_update.rows_affected != 1 {
                let updated = finish_proposal(
                    &transaction,
                    proposal.id,
                    STATUS_STALE,
                    None,
                    None,
                    json!({"code":"target_changed"}),
                )
                .await?;
                transaction.commit().await.map_err(map_db)?;
                return record(updated);
            }
            let outcome = json!({
                "code":"confirmed",
                "targetId":proposal.target_id,
                "previousEnabled":proposal.expected_enabled,
                "enabled":proposal.requested_enabled,
                "targetConfigVersion":next_version
            });
            let updated = finish_proposal(
                &transaction,
                proposal.id,
                STATUS_CONFIRMED,
                Some(confirmer_user_id),
                Some((now, confirmation_key)),
                outcome,
            )
            .await?;
            transaction.commit().await.map_err(map_db)?;
            record(updated)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginActionProposalRepositoryError::Timeout),
        }
    }
}

async fn deployment_is_current(
    transaction: &DatabaseTransaction,
    proposal: &plugin_action_proposals::Model,
) -> Result<bool, PluginActionProposalRepositoryError> {
    let Some(deployment) =
        crate::entity::plugin_deployments::Entity::find_by_id(proposal.plugin_id.clone())
            .lock_exclusive()
            .one(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db)?
    else {
        return Ok(false);
    };
    if !deployment.enabled
        || deployment.generation != proposal.deployment_generation
        || deployment.artifact_id != proposal.artifact_id
        || !has_capability_grant(
            &deployment.capability_grants,
            &proposal.capability_id,
            u16::try_from(proposal.capability_version)
                .map_err(|_| PluginActionProposalRepositoryError::Invariant)?,
        )
    {
        return Ok(false);
    }
    let Some(artifact) = crate::entity::plugin_artifacts::Entity::find_by_id(proposal.artifact_id)
        .lock_exclusive()
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db)?
    else {
        return Ok(false);
    };
    if artifact.plugin_id != proposal.plugin_id
        || artifact.digest != proposal.artifact_digest
        || artifact.source_kind != "local_process"
        || !manifest_declares_action_capability(&artifact.manifest, &proposal.capability_id)
    {
        return Ok(false);
    }
    let approved =
        crate::entity::plugin_artifact_approvals::Entity::find_by_id(proposal.artifact_id)
            .one(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db)?
            .is_some();
    let revoked =
        crate::entity::plugin_artifact_revocations::Entity::find_by_id(proposal.artifact_id)
            .one(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db)?
            .is_some();
    Ok(approved && !revoked)
}

async fn finish_proposal(
    transaction: &DatabaseTransaction,
    id: i64,
    status: i16,
    confirmer_user_id: Option<i64>,
    confirmation: Option<(i64, &str)>,
    outcome: Value,
) -> Result<plugin_action_proposals::Model, PluginActionProposalRepositoryError> {
    let outcome_json = serde_json::to_string(&outcome)
        .map_err(|_| PluginActionProposalRepositoryError::Invariant)?;
    let mut update = plugin_action_proposals::Entity::update_many()
        .col_expr(plugin_action_proposals::Column::Status, Expr::value(status))
        .col_expr(
            plugin_action_proposals::Column::OutcomeJson,
            Expr::value(Some(outcome_json)),
        );
    if let Some(confirmer) = confirmer_user_id {
        update = update.col_expr(
            plugin_action_proposals::Column::ConfirmedByUserId,
            Expr::value(Some(confirmer)),
        );
    }
    if let Some((confirmed_at, confirmation_key)) = confirmation {
        update = update
            .col_expr(
                plugin_action_proposals::Column::ConfirmedAt,
                Expr::value(Some(confirmed_at)),
            )
            .col_expr(
                plugin_action_proposals::Column::ConfirmationKey,
                Expr::value(Some(confirmation_key.to_owned())),
            );
    }
    let result = update
        .filter(plugin_action_proposals::Column::Id.eq(id))
        .filter(plugin_action_proposals::Column::Status.eq(STATUS_PENDING))
        .exec(transaction)
        .await
        .map_err(map_db)?;
    if result.rows_affected != 1 {
        return Err(PluginActionProposalRepositoryError::Conflict);
    }
    plugin_action_proposals::Entity::find_by_id(id)
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db)?
        .ok_or(PluginActionProposalRepositoryError::NotFound)
}

fn verify_idempotent_replay(
    existing: &plugin_action_proposals::Model,
    input: &PluginActionProposalCreate,
) -> Result<(), PluginActionProposalRepositoryError> {
    if existing.plugin_id != input.plugin_id
        || existing.artifact_id != input.artifact_id
        || existing.artifact_digest != input.artifact_digest
        || existing.deployment_generation != input.deployment_generation
        || existing.capability_version != i32::from(input.capability_version)
        || existing.action_version != i32::from(input.action_version)
        || existing.input_json != input.input_json
        || existing.target_id != input.target_id
        || existing.expected_target_version != input.expected_target_version
        || existing.expected_enabled != input.expected_enabled
        || existing.requested_enabled != input.requested_enabled
    {
        return Err(PluginActionProposalRepositoryError::Conflict);
    }
    Ok(())
}

fn record(
    row: plugin_action_proposals::Model,
) -> Result<PluginActionProposalRecord, PluginActionProposalRepositoryError> {
    let outcome = row
        .outcome_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| PluginActionProposalRepositoryError::Invariant)?;
    Ok(PluginActionProposalRecord {
        id: row.id,
        proposal_key: row.proposal_key,
        requested_by_user_id: row.requested_by_user_id,
        confirmed_by_user_id: row.confirmed_by_user_id,
        plugin_id: row.plugin_id,
        artifact_id: row.artifact_id,
        artifact_digest: row.artifact_digest,
        deployment_generation: row.deployment_generation,
        capability_id: row.capability_id,
        capability_version: u16::try_from(row.capability_version)
            .map_err(|_| PluginActionProposalRepositoryError::Invariant)?,
        action_id: row.action_id,
        action_version: u16::try_from(row.action_version)
            .map_err(|_| PluginActionProposalRepositoryError::Invariant)?,
        input_json: row.input_json,
        target_id: row.target_id,
        expected_target_version: row.expected_target_version,
        expected_enabled: row.expected_enabled,
        requested_enabled: row.requested_enabled,
        status: PluginActionProposalStatus::from_code(row.status)?,
        expires_at: row.expires_at,
        created_at: row.created_at,
        confirmed_at: row.confirmed_at,
        confirmation_key: row.confirmation_key,
        outcome,
    })
}

fn valid_proposal_input(json: &str, enabled: bool) -> bool {
    serde_json::from_str::<Value>(json)
        .ok()
        .is_some_and(|proposal| {
            proposal.is_object()
                && proposal.as_object().is_some_and(|object| {
                    object.len() == 3
                        && object.get("actionId").and_then(Value::as_str) == Some(ACTION_ID)
                        && object.get("actionVersion").and_then(Value::as_u64) == Some(1)
                        && object
                            .get("input")
                            .and_then(Value::as_object)
                            .is_some_and(|input| {
                                input.len() == 1
                                    && input.get("enabled").and_then(Value::as_bool)
                                        == Some(enabled)
                            })
                })
        })
}

fn has_capability_grant(grants: &Value, id: &str, version: u16) -> bool {
    grants.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.get("id").and_then(Value::as_str) == Some(id)
                && item.get("version").and_then(Value::as_u64) == Some(u64::from(version))
        })
    })
}

fn manifest_declares_action_capability(manifest: &Value, id: &str) -> bool {
    manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("id").and_then(Value::as_str) == Some(id)
                    && item.get("version").and_then(Value::as_u64) == Some(1)
                    && item.get("access").and_then(Value::as_str) == Some("read")
                    && item.get("sideEffect").and_then(Value::as_str) == Some("none")
            })
        })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> PluginActionProposalCreate {
        PluginActionProposalCreate {
            proposal_key: "7b9be48c95ac4ba487d412f7f442bc49".to_owned(),
            idempotency_key: "request-1".to_owned(),
            requested_by_user_id: 7,
            plugin_id: "org.example.monitor".to_owned(),
            artifact_id: 12,
            artifact_digest: "a".repeat(64),
            deployment_generation: 3,
            capability_version: 1,
            action_version: 1,
            input_json: r#"{"actionId":"active-channel-monitor.target.set-enabled","actionVersion":1,"input":{"enabled":false}}"#.to_owned(),
            target_id: 18,
            expected_target_version: 4,
            expected_enabled: true,
            requested_enabled: false,
            created_at: 100,
            expires_at: 400,
        }
    }

    #[test]
    fn action_proposal_accepts_only_the_registered_enabled_flag() {
        proposal().validate().unwrap();

        let mut altered_action = proposal();
        altered_action.input_json =
            r#"{"actionId":"wallet.credit","actionVersion":1,"input":{"enabled":false}}"#
                .to_owned();
        assert_eq!(
            altered_action.validate(),
            Err(PluginActionProposalRepositoryError::Invariant)
        );

        let mut extra_input = proposal();
        extra_input.input_json = r#"{"actionId":"active-channel-monitor.target.set-enabled","actionVersion":1,"input":{"enabled":false,"targetId":19}}"#.to_owned();
        assert_eq!(
            extra_input.validate(),
            Err(PluginActionProposalRepositoryError::Invariant)
        );
    }

    #[test]
    fn action_proposal_rejects_stale_state_and_unbounded_expiry() {
        let mut no_change = proposal();
        no_change.requested_enabled = true;
        assert_eq!(
            no_change.validate(),
            Err(PluginActionProposalRepositoryError::Invariant)
        );

        let mut long_expiry = proposal();
        long_expiry.expires_at = long_expiry.created_at + 601;
        assert_eq!(
            long_expiry.validate(),
            Err(PluginActionProposalRepositoryError::Invariant)
        );
    }
}

fn map_db(_: sea_orm::DbErr) -> PluginActionProposalRepositoryError {
    PluginActionProposalRepositoryError::Query
}
