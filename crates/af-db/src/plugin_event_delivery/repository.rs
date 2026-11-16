use std::{fmt, time::Duration};

use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};
use serde_json::Value;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{
    STATUS_DEAD, STATUS_DELIVERED, STATUS_LEASED, STATUS_PENDING, completion_outcome,
    due_condition, record_internal_error, retry_delay,
};
use crate::{DatabasePool, entity::plugin_event_deliveries};

pub(super) fn serialize_event_result(
    result: &Value,
) -> Result<String, PluginEventDeliveryRepositoryError> {
    if !result.is_object() || super::contains_sensitive_fields(result) {
        return Err(PluginEventDeliveryRepositoryError::InvalidResult);
    }
    let encoded = serde_json::to_string(result)
        .map_err(|_| PluginEventDeliveryRepositoryError::InvalidResult)?;
    if encoded.len() > super::MAX_EVENT_RESULT_BYTES {
        return Err(PluginEventDeliveryRepositoryError::InvalidResult);
    }
    Ok(encoded)
}

fn result_record_from_model(
    row: plugin_event_deliveries::Model,
) -> Result<PluginEventDeliveryResultRecord, PluginEventDeliveryRepositoryError> {
    let result_json = row
        .result_json
        .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
    let result: Value = serde_json::from_str(&result_json)
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    serialize_event_result(&result)?;
    let capability_version = u16::try_from(row.capability_version)
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    let attempt_count = u16::try_from(row.attempt_count)
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    Ok(PluginEventDeliveryResultRecord {
        id: row.id,
        plugin_id: row.plugin_id,
        event_kind: row.event_kind,
        deployment_generation: row.deployment_generation,
        capability_id: row.capability_id,
        capability_version,
        attempt_count,
        completed_at: row
            .completed_at
            .ok_or(PluginEventDeliveryRepositoryError::Invariant)?,
        result,
    })
}

const CANDIDATE_LIMIT: u64 = 32;
const DELIVERY_LEASE_SECONDS: i64 = 60;
const MAX_EVENT_RESULT_BYTES: usize = 16 * 1024;
pub const MAX_PLUGIN_EVENT_RESULT_PAGE_SIZE: u32 = 100;

/// Cursor and bounded page size for completed plugin event results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginEventDeliveryResultQuery {
    before_id: Option<i64>,
    limit: u32,
}

impl PluginEventDeliveryResultQuery {
    pub fn new(
        before_id: Option<i64>,
        limit: u32,
    ) -> Result<Self, PluginEventDeliveryRepositoryError> {
        if before_id.is_some_and(|id| id <= 0)
            || limit == 0
            || limit > MAX_PLUGIN_EVENT_RESULT_PAGE_SIZE
        {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        Ok(Self { before_id, limit })
    }
}

/// A completed result without its event payload or any plugin connection details.
#[derive(Clone, PartialEq)]
pub struct PluginEventDeliveryResultRecord {
    pub id: i64,
    pub plugin_id: String,
    pub event_kind: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub attempt_count: u16,
    pub completed_at: i64,
    pub result: Value,
}

impl fmt::Debug for PluginEventDeliveryResultRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginEventDeliveryResultRecord")
            .field("id", &self.id)
            .field("plugin_id", &self.plugin_id)
            .field("event_kind", &self.event_kind)
            .field("deployment_generation", &self.deployment_generation)
            .field("capability_id", &self.capability_id)
            .field("capability_version", &self.capability_version)
            .field("attempt_count", &self.attempt_count)
            .field("completed_at", &self.completed_at)
            .field("result", &"<redacted>")
            .finish()
    }
}

/// Page of completed event results in descending delivery ID order.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginEventDeliveryResultPage {
    pub results: Vec<PluginEventDeliveryResultRecord>,
    pub next_before_id: Option<i64>,
}

/// A successfully stored event or a replay of its immutable idempotency key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginEventDeliveryEnqueueOutcome {
    Enqueued(i64),
    AlreadyEnqueued(i64),
}

/// Event outcome requested by the host after it has classified the Sidecar result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginEventFailureKind {
    /// Retry only after bounded backoff and while the event remains within its deadline.
    Transient,
    /// Invalid event, incompatible deployment, or non-retryable host decision.
    Permanent,
}

/// A lease-bound host-side view of one delivery. Payload formatting is omitted from Debug.
#[derive(Clone, Eq, PartialEq)]
pub struct PluginEventDeliveryLease {
    id: i64,
    plugin_id: String,
    event_key: String,
    event_kind: String,
    artifact_id: i64,
    artifact_digest: String,
    deployment_generation: i64,
    capability_id: String,
    capability_version: u16,
    payload: Value,
    attempt_count: i16,
    max_attempts: i16,
    expires_at: i64,
    version: i64,
}

impl PluginEventDeliveryLease {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
    #[must_use]
    pub fn event_key(&self) -> &str {
        &self.event_key
    }
    #[must_use]
    pub fn event_kind(&self) -> &str {
        &self.event_kind
    }
    #[must_use]
    pub const fn artifact_id(&self) -> i64 {
        self.artifact_id
    }
    #[must_use]
    pub fn artifact_digest(&self) -> &str {
        &self.artifact_digest
    }
    #[must_use]
    pub const fn deployment_generation(&self) -> i64 {
        self.deployment_generation
    }
    #[must_use]
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    #[must_use]
    pub const fn capability_version(&self) -> u16 {
        self.capability_version
    }
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }
    #[must_use]
    pub const fn attempt_count(&self) -> i16 {
        self.attempt_count
    }
    #[must_use]
    pub const fn max_attempts(&self) -> i16 {
        self.max_attempts
    }
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

impl fmt::Debug for PluginEventDeliveryLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginEventDeliveryLease")
            .field("id", &self.id)
            .field("plugin_id", &self.plugin_id)
            .field("event_key", &self.event_key)
            .field("event_kind", &self.event_kind)
            .field("artifact_id", &self.artifact_id)
            .field("artifact_digest", &self.artifact_digest)
            .field("deployment_generation", &self.deployment_generation)
            .field("capability_id", &self.capability_id)
            .field("capability_version", &self.capability_version)
            .field("payload", &"<redacted>")
            .field("attempt_count", &self.attempt_count)
            .field("expires_at", &self.expires_at)
            .field("version", &self.version)
            .finish()
    }
}

/// Result of attempting to atomically claim the next queued delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginEventDeliveryClaimOutcome {
    Claimed(PluginEventDeliveryLease),
    Empty,
}

/// Result of completing the lease currently owned by a worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginEventDeliveryCompletionOutcome {
    Completed,
    Stale,
    Expired,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginEventDeliveryRepositoryConfigError {
    #[error("plugin event repository operation timeout must be non-zero")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginEventDeliveryRepositoryError {
    #[error("plugin event repository operation failed")]
    Query,
    #[error("plugin event repository operation timed out")]
    Timeout,
    #[error("plugin event queue invariant is invalid")]
    Invariant,
    #[error("plugin event idempotency key conflicts with existing delivery")]
    Conflict,
    #[error("plugin event output is invalid or exceeds its size limit")]
    InvalidResult,
}

/// Database lease and lifecycle operations for the host-owned plugin delivery queue.
#[derive(Clone)]
pub struct PluginEventDeliveryRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PluginEventDeliveryRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, PluginEventDeliveryRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(PluginEventDeliveryRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// Atomically claim the oldest due event; expired leases are eligible for takeover.
    pub async fn claim_next(
        &self,
        now: i64,
    ) -> Result<PluginEventDeliveryClaimOutcome, PluginEventDeliveryRepositoryError> {
        if now <= 0 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            self.retire_expired(now).await?;
            self.purge_expired_inner(now).await?;
            self.claim_next_inner(now).await
        })
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                PluginEventDeliveryRepositoryError::Timeout,
            )),
        }
    }

    /// Remove unleased delivery rows after their TTL so queue retention does not block artifact
    /// collection. Expired leases are first retired by the normal lease lifecycle.
    pub async fn purge_expired(&self, now: i64) -> Result<u64, PluginEventDeliveryRepositoryError> {
        if now <= 0 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            self.retire_expired(now).await?;
            self.purge_expired_inner(now).await
        })
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                PluginEventDeliveryRepositoryError::Timeout,
            )),
        }
    }

    async fn purge_expired_inner(
        &self,
        now: i64,
    ) -> Result<u64, PluginEventDeliveryRepositoryError> {
        const PURGE_BATCH: u64 = 256;
        let rows = plugin_event_deliveries::Entity::find()
            .filter(plugin_event_deliveries::Column::ExpiresAt.lte(now))
            .filter(plugin_event_deliveries::Column::Status.ne(STATUS_LEASED))
            .order_by_asc(plugin_event_deliveries::Column::Id)
            .limit(PURGE_BATCH)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        if rows.is_empty() {
            return Ok(0);
        }
        let ids = rows.into_iter().map(|row| row.id).collect::<Vec<_>>();
        let deleted = plugin_event_deliveries::Entity::delete_many()
            .filter(plugin_event_deliveries::Column::Id.is_in(ids))
            .filter(plugin_event_deliveries::Column::ExpiresAt.lte(now))
            .filter(plugin_event_deliveries::Column::Status.ne(STATUS_LEASED))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        Ok(deleted.rows_affected)
    }

    /// Complete only if the lease version still belongs to this worker.
    pub async fn mark_delivered(
        &self,
        lease: &PluginEventDeliveryLease,
        now: i64,
        result: &Value,
    ) -> Result<PluginEventDeliveryCompletionOutcome, PluginEventDeliveryRepositoryError> {
        if now <= 0 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        let result_json = serialize_event_result(result)?;
        match timeout(
            self.operation_timeout,
            self.mark_delivered_inner(lease, now, result_json),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                PluginEventDeliveryRepositoryError::Timeout,
            )),
        }
    }

    /// Return only completed deliveries with a validated output; event input is never selected.
    pub async fn list_results(
        &self,
        query: PluginEventDeliveryResultQuery,
        now: i64,
    ) -> Result<PluginEventDeliveryResultPage, PluginEventDeliveryRepositoryError> {
        if now <= 0 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        match timeout(self.operation_timeout, async {
            let mut select = plugin_event_deliveries::Entity::find()
                .filter(plugin_event_deliveries::Column::Status.eq(STATUS_DELIVERED))
                .filter(plugin_event_deliveries::Column::ResultJson.is_not_null())
                .filter(plugin_event_deliveries::Column::CompletedAt.is_not_null())
                .filter(plugin_event_deliveries::Column::ExpiresAt.gt(now))
                .order_by_desc(plugin_event_deliveries::Column::Id)
                .limit(u64::from(query.limit) + 1);
            if let Some(before_id) = query.before_id {
                select = select.filter(plugin_event_deliveries::Column::Id.lt(before_id));
            }
            let mut rows = select
                .all(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
            let has_more = rows.len() > query.limit as usize;
            rows.truncate(query.limit as usize);
            let next_before_id = has_more.then(|| rows.last().map(|row| row.id)).flatten();
            let results = rows
                .into_iter()
                .map(result_record_from_model)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PluginEventDeliveryResultPage {
                results,
                next_before_id,
            })
        })
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                PluginEventDeliveryRepositoryError::Timeout,
            )),
        }
    }

    /// Record a host-classified transient or permanent failure using bounded retry policy.
    pub async fn record_failure(
        &self,
        lease: &PluginEventDeliveryLease,
        now: i64,
        kind: super::PluginEventFailureKind,
    ) -> Result<PluginEventDeliveryCompletionOutcome, PluginEventDeliveryRepositoryError> {
        if now <= 0 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        match timeout(
            self.operation_timeout,
            self.record_failure_inner(lease, now, kind),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                PluginEventDeliveryRepositoryError::Timeout,
            )),
        }
    }

    async fn retire_expired(&self, now: i64) -> Result<(), PluginEventDeliveryRepositoryError> {
        let result = plugin_event_deliveries::Entity::update_many()
            .col_expr(
                plugin_event_deliveries::Column::Status,
                Expr::value(STATUS_DEAD),
            )
            .col_expr(
                plugin_event_deliveries::Column::LeaseExpiresAt,
                Expr::value(Option::<i64>::None),
            )
            .col_expr(
                plugin_event_deliveries::Column::CompletedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                plugin_event_deliveries::Column::Version,
                Expr::col(plugin_event_deliveries::Column::Version).add(1_i64),
            )
            .col_expr(plugin_event_deliveries::Column::UpdatedAt, Expr::value(now))
            .filter(
                Condition::any()
                    .add(plugin_event_deliveries::Column::ExpiresAt.lte(now))
                    .add(
                        Condition::all()
                            .add(plugin_event_deliveries::Column::Status.eq(STATUS_PENDING))
                            .add(
                                Expr::col(plugin_event_deliveries::Column::AttemptCount)
                                    .gte(Expr::col(plugin_event_deliveries::Column::MaxAttempts)),
                            ),
                    )
                    .add(
                        Condition::all()
                            .add(plugin_event_deliveries::Column::Status.eq(STATUS_LEASED))
                            .add(plugin_event_deliveries::Column::LeaseExpiresAt.lte(now))
                            .add(
                                Expr::col(plugin_event_deliveries::Column::AttemptCount)
                                    .gte(Expr::col(plugin_event_deliveries::Column::MaxAttempts)),
                            ),
                    ),
            )
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        if result.rows_affected > i64::MAX as u64 {
            return Err(PluginEventDeliveryRepositoryError::Invariant);
        }
        Ok(())
    }

    async fn claim_next_inner(
        &self,
        now: i64,
    ) -> Result<PluginEventDeliveryClaimOutcome, PluginEventDeliveryRepositoryError> {
        let candidates = plugin_event_deliveries::Entity::find()
            .filter(due_condition(now))
            .filter(plugin_event_deliveries::Column::ExpiresAt.gt(now))
            .filter(
                Expr::col(plugin_event_deliveries::Column::AttemptCount)
                    .lt(Expr::col(plugin_event_deliveries::Column::MaxAttempts)),
            )
            .order_by_asc(plugin_event_deliveries::Column::NextAttemptAt)
            .order_by_asc(plugin_event_deliveries::Column::Id)
            .limit(CANDIDATE_LIMIT)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;

        for candidate in candidates {
            let attempt_count = candidate
                .attempt_count
                .checked_add(1)
                .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
            let version = candidate
                .version
                .checked_add(1)
                .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
            let lease_expires_at = now
                .saturating_add(DELIVERY_LEASE_SECONDS)
                .min(candidate.expires_at);
            if lease_expires_at <= now {
                continue;
            }
            let update = plugin_event_deliveries::Entity::update_many()
                .col_expr(
                    plugin_event_deliveries::Column::Status,
                    Expr::value(STATUS_LEASED),
                )
                .col_expr(
                    plugin_event_deliveries::Column::AttemptCount,
                    Expr::value(attempt_count),
                )
                .col_expr(
                    plugin_event_deliveries::Column::NextAttemptAt,
                    Expr::value(lease_expires_at),
                )
                .col_expr(
                    plugin_event_deliveries::Column::LeaseExpiresAt,
                    Expr::value(Some(lease_expires_at)),
                )
                .col_expr(
                    plugin_event_deliveries::Column::Version,
                    Expr::value(version),
                )
                .col_expr(plugin_event_deliveries::Column::UpdatedAt, Expr::value(now))
                .filter(plugin_event_deliveries::Column::Id.eq(candidate.id))
                .filter(plugin_event_deliveries::Column::Version.eq(candidate.version))
                .filter(due_condition(now))
                .exec(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
            if update.rows_affected == 0 {
                continue;
            }
            if update.rows_affected != 1 {
                return Err(PluginEventDeliveryRepositoryError::Invariant);
            }
            let event_kind = super::PluginEventKind::parse(&candidate.event_kind)?;
            if candidate.capability_id != event_kind.required_capability() {
                return Err(PluginEventDeliveryRepositoryError::Invariant);
            }
            let capability_version = u16::try_from(candidate.capability_version)
                .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
            let payload: Value = serde_json::from_str(&candidate.payload_json)
                .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
            return Ok(PluginEventDeliveryClaimOutcome::Claimed(
                PluginEventDeliveryLease {
                    id: candidate.id,
                    plugin_id: candidate.plugin_id,
                    event_key: candidate.event_key,
                    event_kind: candidate.event_kind,
                    artifact_id: candidate.artifact_id,
                    artifact_digest: candidate.artifact_digest,
                    deployment_generation: candidate.deployment_generation,
                    capability_id: candidate.capability_id,
                    capability_version,
                    payload,
                    attempt_count,
                    max_attempts: candidate.max_attempts,
                    expires_at: candidate.expires_at,
                    version,
                },
            ));
        }
        Ok(PluginEventDeliveryClaimOutcome::Empty)
    }

    async fn mark_delivered_inner(
        &self,
        lease: &PluginEventDeliveryLease,
        now: i64,
        result_json: String,
    ) -> Result<PluginEventDeliveryCompletionOutcome, PluginEventDeliveryRepositoryError> {
        if now >= lease.expires_at {
            let update = plugin_event_deliveries::Entity::update_many()
                .col_expr(
                    plugin_event_deliveries::Column::Status,
                    Expr::value(STATUS_DEAD),
                )
                .col_expr(
                    plugin_event_deliveries::Column::LeaseExpiresAt,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    plugin_event_deliveries::Column::CompletedAt,
                    Expr::value(Some(now)),
                )
                .col_expr(
                    plugin_event_deliveries::Column::Version,
                    Expr::col(plugin_event_deliveries::Column::Version).add(1_i64),
                )
                .col_expr(plugin_event_deliveries::Column::UpdatedAt, Expr::value(now))
                .filter(plugin_event_deliveries::Column::Id.eq(lease.id))
                .filter(plugin_event_deliveries::Column::Status.eq(STATUS_LEASED))
                .filter(plugin_event_deliveries::Column::Version.eq(lease.version))
                .exec(self.pool.connection())
                .await
                .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
            return match completion_outcome(update.rows_affected)? {
                PluginEventDeliveryCompletionOutcome::Completed => {
                    Ok(PluginEventDeliveryCompletionOutcome::Expired)
                }
                stale => Ok(stale),
            };
        }
        let version = lease
            .version
            .checked_add(1)
            .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
        let update = plugin_event_deliveries::Entity::update_many()
            .col_expr(
                plugin_event_deliveries::Column::Status,
                Expr::value(STATUS_DELIVERED),
            )
            .col_expr(
                plugin_event_deliveries::Column::LeaseExpiresAt,
                Expr::value(Option::<i64>::None),
            )
            .col_expr(
                plugin_event_deliveries::Column::CompletedAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                plugin_event_deliveries::Column::ResultJson,
                Expr::value(Some(result_json)),
            )
            .col_expr(
                plugin_event_deliveries::Column::Version,
                Expr::value(version),
            )
            .col_expr(plugin_event_deliveries::Column::UpdatedAt, Expr::value(now))
            .filter(plugin_event_deliveries::Column::Id.eq(lease.id))
            .filter(plugin_event_deliveries::Column::Status.eq(STATUS_LEASED))
            .filter(plugin_event_deliveries::Column::Version.eq(lease.version))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        completion_outcome(update.rows_affected)
    }

    async fn record_failure_inner(
        &self,
        lease: &PluginEventDeliveryLease,
        now: i64,
        kind: super::PluginEventFailureKind,
    ) -> Result<PluginEventDeliveryCompletionOutcome, PluginEventDeliveryRepositoryError> {
        let version = lease
            .version
            .checked_add(1)
            .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
        let is_dead = kind == super::PluginEventFailureKind::Permanent
            || lease.attempt_count >= lease.max_attempts
            || now >= lease.expires_at;
        let mut update = plugin_event_deliveries::Entity::update_many()
            .col_expr(
                plugin_event_deliveries::Column::Status,
                Expr::value(if is_dead { STATUS_DEAD } else { STATUS_PENDING }),
            )
            .col_expr(
                plugin_event_deliveries::Column::LeaseExpiresAt,
                Expr::value(Option::<i64>::None),
            )
            .col_expr(
                plugin_event_deliveries::Column::Version,
                Expr::value(version),
            )
            .col_expr(plugin_event_deliveries::Column::UpdatedAt, Expr::value(now))
            .filter(plugin_event_deliveries::Column::Id.eq(lease.id))
            .filter(plugin_event_deliveries::Column::Status.eq(STATUS_LEASED))
            .filter(plugin_event_deliveries::Column::Version.eq(lease.version));
        if is_dead {
            update = update.col_expr(
                plugin_event_deliveries::Column::CompletedAt,
                Expr::value(Some(now)),
            );
        } else {
            let delay = retry_delay(lease.attempt_count)?;
            update = update.col_expr(
                plugin_event_deliveries::Column::NextAttemptAt,
                Expr::value(now.saturating_add(delay).min(lease.expires_at)),
            );
        }
        let result = update
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        completion_outcome(result.rows_affected)
    }
}
