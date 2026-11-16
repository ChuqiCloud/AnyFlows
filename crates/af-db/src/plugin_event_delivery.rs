//! Durable host-owned plugin event deliveries.
//!
//! This repository only persists and leases events. It never starts plugin work, writes user
//! notifications, or decides that a plugin may observe a payload. The host worker must recheck
//! the active deployment generation and capability grant before invoking a Sidecar.

use std::fmt;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde_json::{Value, json};
use thiserror::Error;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::entity::plugin_event_deliveries;

mod repository;

pub use repository::{
    MAX_PLUGIN_EVENT_RESULT_PAGE_SIZE, PluginEventDeliveryClaimOutcome,
    PluginEventDeliveryCompletionOutcome, PluginEventDeliveryEnqueueOutcome,
    PluginEventDeliveryLease, PluginEventDeliveryRepository,
    PluginEventDeliveryRepositoryConfigError, PluginEventDeliveryRepositoryError,
    PluginEventDeliveryResultPage, PluginEventDeliveryResultQuery, PluginEventDeliveryResultRecord,
    PluginEventFailureKind,
};

const STATUS_PENDING: i16 = 1;
const STATUS_LEASED: i16 = 2;
const STATUS_DELIVERED: i16 = 3;
const STATUS_DEAD: i16 = 4;
const INITIAL_VERSION: i64 = 1;
const MAX_EVENT_PAYLOAD_BYTES: usize = 32 * 1024;
pub(crate) const MAX_EVENT_RESULT_BYTES: usize = 16 * 1024;
const MAX_EVENT_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_EVENT_ATTEMPTS: i16 = 12;

/// Closed event set. Adding a kind requires a host producer and an explicit safe projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginEventKind {
    /// A channel monitor changed its public health state.
    ChannelHealthChanged,
    /// A merchant service moved through a public lifecycle state.
    MerchantServiceChanged,
    /// The host scheduled an explicitly requested, bounded read-only report.
    ReportRequested,
}

impl PluginEventKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ChannelHealthChanged => "channel.health.changed",
            Self::MerchantServiceChanged => "merchant.service.changed",
            Self::ReportRequested => "report.requested",
        }
    }

    fn parse(value: &str) -> Result<Self, PluginEventDeliveryRepositoryError> {
        match value {
            "channel.health.changed" => Ok(Self::ChannelHealthChanged),
            "merchant.service.changed" => Ok(Self::MerchantServiceChanged),
            "report.requested" => Ok(Self::ReportRequested),
            _ => Err(PluginEventDeliveryRepositoryError::Invariant),
        }
    }

    const fn required_capability(self) -> &'static str {
        match self {
            Self::ChannelHealthChanged => "events.channel-health.changed.consume",
            Self::MerchantServiceChanged => "events.merchant-service.changed.consume",
            Self::ReportRequested => "events.report.requested.consume",
        }
    }

    fn validate_payload(self, payload: &Value) -> bool {
        let Some(object) = payload.as_object() else {
            return false;
        };
        let positive_integer = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_i64)
                .is_some_and(|value| value > 0)
        };
        let bounded_text = |key: &str, maximum: usize| {
            object
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    !value.is_empty()
                        && value.len() <= maximum
                        && !value.chars().any(char::is_control)
                })
        };
        match self {
            Self::ChannelHealthChanged => {
                only_keys(object, &["targetId", "channelId", "status", "observedAt"])
                    && positive_integer("targetId")
                    && positive_integer("channelId")
                    && positive_integer("observedAt")
                    && object
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| matches!(status, "down" | "recovered"))
            }
            Self::MerchantServiceChanged => {
                only_keys(object, &["serviceId", "revision", "status", "observedAt"])
                    && positive_integer("serviceId")
                    && positive_integer("revision")
                    && positive_integer("observedAt")
                    && object
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| matches!(status, "active" | "paused"))
            }
            Self::ReportRequested => {
                only_keys(object, &["requestId", "reportKind", "requestedAt"])
                    && bounded_text("requestId", 64)
                    && positive_integer("requestedAt")
                    && object
                        .get("reportKind")
                        .and_then(Value::as_str)
                        .is_some_and(|kind| matches!(kind, "channel-health" | "merchant-service"))
            }
        }
    }
}

fn only_keys(object: &serde_json::Map<String, Value>, expected: &[&str]) -> bool {
    !object.is_empty()
        && object.len() == expected.len()
        && object.keys().all(|key| expected.contains(&key.as_str()))
}

/// Insert one host-projected event in the caller's business transaction.
///
/// The current deployment and exact capability grant are rechecked here. A stable event key is
/// idempotent per plugin generation and capability; conflicting replays are rejected. This
/// function does not invoke a plugin or create a user-visible notification.
pub async fn enqueue_plugin_event_delivery<C>(
    connection: &C,
    input: PluginEventDeliveryCreate,
) -> Result<PluginEventDeliveryEnqueueOutcome, PluginEventDeliveryRepositoryError>
where
    C: ConnectionTrait,
{
    input
        .validate()
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    let deployment = crate::entity::plugin_deployments::Entity::find_by_id(input.plugin_id.clone())
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
        .ok_or(PluginEventDeliveryRepositoryError::Conflict)?;
    if !deployment.enabled
        || deployment.generation != input.deployment_generation
        || deployment.artifact_id != input.artifact_id
        || !has_grant(
            &deployment.capability_grants,
            &input.capability_id,
            input.capability_version,
        )
    {
        return Err(PluginEventDeliveryRepositoryError::Conflict);
    }
    let artifact = crate::entity::plugin_artifacts::Entity::find_by_id(input.artifact_id)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
        .ok_or(PluginEventDeliveryRepositoryError::Conflict)?;
    if artifact.plugin_id != input.plugin_id
        || artifact.digest != input.artifact_digest
        || artifact.source_kind != "local_process"
    {
        return Err(PluginEventDeliveryRepositoryError::Conflict);
    }
    let approved = crate::entity::plugin_artifact_approvals::Entity::find_by_id(input.artifact_id)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
        .is_some();
    let revoked = crate::entity::plugin_artifact_revocations::Entity::find_by_id(input.artifact_id)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
        .is_some();
    if !approved
        || revoked
        || !artifact_declares_event_capability(
            &artifact.manifest,
            &input.capability_id,
            input.capability_version,
        )
    {
        return Err(PluginEventDeliveryRepositoryError::Conflict);
    }
    if let Some(existing) = find_idempotent(connection, &input).await? {
        return verify_duplicate(&existing, &input);
    }

    let payload_json = serde_json::to_string(&input.payload)
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    let row = plugin_event_deliveries::ActiveModel {
        plugin_id: Set(input.plugin_id.clone()),
        event_key: Set(input.event_key.clone()),
        event_kind: Set(input.event_kind.as_str().to_owned()),
        artifact_id: Set(input.artifact_id),
        artifact_digest: Set(input.artifact_digest.clone()),
        deployment_generation: Set(input.deployment_generation),
        capability_id: Set(input.capability_id.clone()),
        capability_version: Set(i32::from(input.capability_version)),
        payload_json: Set(payload_json),
        result_json: Set(None),
        status: Set(STATUS_PENDING),
        attempt_count: Set(0),
        max_attempts: Set(input.max_attempts),
        next_attempt_at: Set(input.created_at),
        lease_expires_at: Set(None),
        completed_at: Set(None),
        expires_at: Set(input.expires_at),
        version: Set(INITIAL_VERSION),
        created_at: Set(input.created_at),
        updated_at: Set(input.created_at),
        ..Default::default()
    };
    match row.insert(connection).await {
        Ok(inserted) => Ok(PluginEventDeliveryEnqueueOutcome::Enqueued(inserted.id)),
        Err(_) => {
            // A concurrent producer may have won the unique idempotency key.
            let existing = find_idempotent(connection, &input)
                .await?
                .ok_or(PluginEventDeliveryRepositoryError::Query)?;
            verify_duplicate(&existing, &input)
        }
    }
}

/// Persist a channel monitor transition for every currently eligible plugin deployment.
///
/// This is intentionally a closed producer rather than a general event API. The monitor event
/// ID is the stable idempotency key; delivery creation shares the caller's transaction with the
/// monitor event and state transition.
pub(crate) async fn enqueue_channel_health_changed<C>(
    connection: &C,
    event_id: i64,
    target_id: i64,
    channel_id: i64,
    status: ChannelHealthTransition,
    observed_at: i64,
) -> Result<(), PluginEventDeliveryRepositoryError>
where
    C: ConnectionTrait,
{
    if event_id <= 0 || target_id <= 0 || channel_id <= 0 || observed_at <= 0 {
        return Err(PluginEventDeliveryRepositoryError::Invariant);
    }

    let expires_at = observed_at
        .checked_add(MAX_EVENT_TTL_SECONDS)
        .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
    let payload = json!({
        "targetId": target_id,
        "channelId": channel_id,
        "status": status.as_str(),
        "observedAt": observed_at,
    });
    let mut after_plugin_id: Option<String> = None;
    loop {
        let mut query = crate::entity::plugin_deployments::Entity::find()
            .filter(crate::entity::plugin_deployments::Column::Enabled.eq(true))
            .order_by_asc(crate::entity::plugin_deployments::Column::PluginId)
            .limit(crate::MAX_PLUGIN_DEPLOYMENT_HISTORY as u64);
        if let Some(cursor) = after_plugin_id.as_ref() {
            query = query
                .filter(crate::entity::plugin_deployments::Column::PluginId.gt(cursor.clone()));
        }
        let deployments = query
            .all(connection)
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        let is_full_page = deployments.len() == crate::MAX_PLUGIN_DEPLOYMENT_HISTORY as usize;
        for deployment in &deployments {
            if !has_grant(
                &deployment.capability_grants,
                PluginEventKind::ChannelHealthChanged.required_capability(),
                1,
            ) {
                continue;
            }
            let artifact =
                crate::entity::plugin_artifacts::Entity::find_by_id(deployment.artifact_id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
            let Some(artifact) = artifact else {
                continue;
            };
            if artifact.plugin_id != deployment.plugin_id
                || artifact.source_kind != "local_process"
                || !artifact_declares_event_capability(
                    &artifact.manifest,
                    PluginEventKind::ChannelHealthChanged.required_capability(),
                    1,
                )
            {
                continue;
            }
            let approved =
                crate::entity::plugin_artifact_approvals::Entity::find_by_id(artifact.id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
                    .is_some();
            let revoked =
                crate::entity::plugin_artifact_revocations::Entity::find_by_id(artifact.id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
                    .is_some();
            if !approved || revoked {
                continue;
            }
            let input = PluginEventDeliveryCreate {
                plugin_id: deployment.plugin_id.clone(),
                event_key: format!("channel-monitor:{event_id}"),
                event_kind: PluginEventKind::ChannelHealthChanged,
                artifact_id: artifact.id,
                artifact_digest: artifact.digest.clone(),
                deployment_generation: deployment.generation,
                capability_id: PluginEventKind::ChannelHealthChanged
                    .required_capability()
                    .to_owned(),
                capability_version: 1,
                payload: payload.clone(),
                max_attempts: MAX_EVENT_ATTEMPTS,
                created_at: observed_at,
                expires_at,
            };
            enqueue_plugin_event_delivery(connection, input).await?;
        }
        if !is_full_page {
            break;
        }
        after_plugin_id = deployments
            .last()
            .map(|deployment| deployment.plugin_id.clone());
        if after_plugin_id.is_none() {
            break;
        }
    }
    Ok(())
}

/// Persist the minimal public lifecycle projection for a merchant service. The producer is
/// called from the service transaction, so an event can never describe a committed state that
/// the database did not commit. Service descriptions, URLs, models, prices and owner identities
/// are deliberately excluded from this event.
pub(crate) async fn enqueue_merchant_service_changed<C>(
    connection: &C,
    service_id: i64,
    revision: i64,
    event_key: &str,
    status: &str,
    observed_at: i64,
) -> Result<(), PluginEventDeliveryRepositoryError>
where
    C: ConnectionTrait,
{
    if service_id <= 0
        || revision <= 0
        || !valid_event_key(event_key)
        || observed_at <= 0
        || !matches!(status, "active" | "paused")
    {
        return Err(PluginEventDeliveryRepositoryError::Invariant);
    }

    let expires_at = observed_at
        .checked_add(MAX_EVENT_TTL_SECONDS)
        .ok_or(PluginEventDeliveryRepositoryError::Invariant)?;
    let payload = json!({
        "serviceId": service_id,
        "revision": revision,
        "status": status,
        "observedAt": observed_at,
    });
    let mut after_plugin_id: Option<String> = None;
    loop {
        let mut query = crate::entity::plugin_deployments::Entity::find()
            .filter(crate::entity::plugin_deployments::Column::Enabled.eq(true))
            .order_by_asc(crate::entity::plugin_deployments::Column::PluginId)
            .limit(crate::MAX_PLUGIN_DEPLOYMENT_HISTORY as u64);
        if let Some(cursor) = after_plugin_id.as_ref() {
            query = query
                .filter(crate::entity::plugin_deployments::Column::PluginId.gt(cursor.clone()));
        }
        let deployments = query
            .all(connection)
            .await
            .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
        let is_full_page = deployments.len() == crate::MAX_PLUGIN_DEPLOYMENT_HISTORY as usize;
        for deployment in &deployments {
            if !has_grant(
                &deployment.capability_grants,
                PluginEventKind::MerchantServiceChanged.required_capability(),
                1,
            ) {
                continue;
            }
            let artifact =
                crate::entity::plugin_artifacts::Entity::find_by_id(deployment.artifact_id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?;
            let Some(artifact) = artifact else {
                continue;
            };
            if artifact.plugin_id != deployment.plugin_id
                || artifact.source_kind != "local_process"
                || !artifact_declares_event_capability(
                    &artifact.manifest,
                    PluginEventKind::MerchantServiceChanged.required_capability(),
                    1,
                )
            {
                continue;
            }
            let approved =
                crate::entity::plugin_artifact_approvals::Entity::find_by_id(artifact.id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
                    .is_some();
            let revoked =
                crate::entity::plugin_artifact_revocations::Entity::find_by_id(artifact.id)
                    .one(connection)
                    .await
                    .map_err(|_| PluginEventDeliveryRepositoryError::Query)?
                    .is_some();
            if !approved || revoked {
                continue;
            }
            let input = PluginEventDeliveryCreate {
                plugin_id: deployment.plugin_id.clone(),
                event_key: event_key.to_owned(),
                event_kind: PluginEventKind::MerchantServiceChanged,
                artifact_id: artifact.id,
                artifact_digest: artifact.digest.clone(),
                deployment_generation: deployment.generation,
                capability_id: PluginEventKind::MerchantServiceChanged
                    .required_capability()
                    .to_owned(),
                capability_version: 1,
                payload: payload.clone(),
                max_attempts: MAX_EVENT_ATTEMPTS,
                created_at: observed_at,
                expires_at,
            };
            enqueue_plugin_event_delivery(connection, input).await?;
        }
        if !is_full_page {
            break;
        }
        after_plugin_id = deployments
            .last()
            .map(|deployment| deployment.plugin_id.clone());
        if after_plugin_id.is_none() {
            break;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChannelHealthTransition {
    Down,
    Recovered,
}

impl ChannelHealthTransition {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Recovered => "recovered",
        }
    }
}

fn artifact_declares_event_capability(manifest: &Value, capability_id: &str, version: u16) -> bool {
    manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|capabilities| {
            capabilities.iter().any(|capability| {
                capability.get("id").and_then(Value::as_str) == Some(capability_id)
                    && capability.get("version").and_then(Value::as_u64) == Some(u64::from(version))
                    && capability.get("access").and_then(Value::as_str) == Some("read")
                    && capability.get("sideEffect").and_then(Value::as_str) == Some("none")
            })
        })
}

fn has_grant(grants: &Value, capability_id: &str, version: u16) -> bool {
    grants.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.get("id").and_then(Value::as_str) == Some(capability_id)
                && item.get("version").and_then(Value::as_u64) == Some(u64::from(version))
        })
    })
}

async fn find_idempotent<C: ConnectionTrait>(
    connection: &C,
    input: &PluginEventDeliveryCreate,
) -> Result<Option<plugin_event_deliveries::Model>, PluginEventDeliveryRepositoryError> {
    plugin_event_deliveries::Entity::find()
        .filter(plugin_event_deliveries::Column::PluginId.eq(input.plugin_id.clone()))
        .filter(plugin_event_deliveries::Column::EventKey.eq(input.event_key.clone()))
        .filter(
            plugin_event_deliveries::Column::DeploymentGeneration.eq(input.deployment_generation),
        )
        .filter(plugin_event_deliveries::Column::CapabilityId.eq(input.capability_id.clone()))
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginEventDeliveryRepositoryError::Query)
}

fn verify_duplicate(
    existing: &plugin_event_deliveries::Model,
    input: &PluginEventDeliveryCreate,
) -> Result<PluginEventDeliveryEnqueueOutcome, PluginEventDeliveryRepositoryError> {
    let payload = serde_json::to_string(&input.payload)
        .map_err(|_| PluginEventDeliveryRepositoryError::Invariant)?;
    if existing.event_kind != input.event_kind.as_str()
        || existing.artifact_id != input.artifact_id
        || existing.artifact_digest != input.artifact_digest
        || existing.capability_version != i32::from(input.capability_version)
        || existing.max_attempts != input.max_attempts
        || existing.expires_at != input.expires_at
        || existing.payload_json != payload
    {
        return Err(PluginEventDeliveryRepositoryError::Conflict);
    }
    Ok(PluginEventDeliveryEnqueueOutcome::AlreadyEnqueued(
        existing.id,
    ))
}

/// Host-created, immutable event delivery facts. `event_key` must be stable and contain no PII.
#[derive(Clone, PartialEq)]
pub struct PluginEventDeliveryCreate {
    pub plugin_id: String,
    pub event_key: String,
    pub event_kind: PluginEventKind,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: u16,
    pub payload: Value,
    pub max_attempts: i16,
    pub created_at: i64,
    pub expires_at: i64,
}

impl fmt::Debug for PluginEventDeliveryCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginEventDeliveryCreate")
            .field("plugin_id", &self.plugin_id)
            .field("event_key", &self.event_key)
            .field("event_kind", &self.event_kind)
            .field("artifact_id", &self.artifact_id)
            .field("artifact_digest", &self.artifact_digest)
            .field("deployment_generation", &self.deployment_generation)
            .field("capability_id", &self.capability_id)
            .field("capability_version", &self.capability_version)
            .field("payload", &"<redacted>")
            .field("max_attempts", &self.max_attempts)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl PluginEventDeliveryCreate {
    #[must_use]
    pub fn required_capability(&self) -> &'static str {
        self.event_kind.required_capability()
    }

    /// Validate bounded host-projected data before it enters persistent storage.
    pub fn validate(&self) -> Result<(), PluginEventDeliveryInputError> {
        if !valid_plugin_id(&self.plugin_id)
            || !valid_event_key(&self.event_key)
            || self.artifact_id <= 0
            || !valid_lower_hex(&self.artifact_digest, 64)
            || self.deployment_generation <= 0
            || !valid_capability_id(&self.capability_id)
            || self.capability_id != self.event_kind.required_capability()
            || self.capability_version == 0
            || !self.payload.is_object()
            || !self.event_kind.validate_payload(&self.payload)
            || contains_sensitive_fields(&self.payload)
            || self.max_attempts <= 0
            || self.max_attempts > MAX_EVENT_ATTEMPTS
            || self.created_at <= 0
            || self.expires_at <= self.created_at
            || self.expires_at.saturating_sub(self.created_at) > MAX_EVENT_TTL_SECONDS
        {
            return Err(PluginEventDeliveryInputError::InvalidInput);
        }
        if serde_json::to_vec(&self.payload)
            .map_err(|_| PluginEventDeliveryInputError::InvalidInput)?
            .len()
            > MAX_EVENT_PAYLOAD_BYTES
        {
            return Err(PluginEventDeliveryInputError::InvalidInput);
        }
        Ok(())
    }
}

/// Error returned when host event data is invalid. It never contains the payload.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PluginEventDeliveryInputError {
    #[error("plugin event delivery input is invalid")]
    InvalidInput,
}

fn valid_plugin_id(value: &str) -> bool {
    value.len() <= 128
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_alphanumeric()
                && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_event_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn valid_capability_id(value: &str) -> bool {
    value.len() <= 128
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_lower_hex(value: &str, expected: usize) -> bool {
    value.len() == expected
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn contains_sensitive_fields(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| sensitive_key(key) || contains_sensitive_fields(value)),
        Value::Array(items) => items.iter().any(contains_sensitive_fields),
        _ => false,
    }
}

fn sensitive_key(value: &str) -> bool {
    let normalized = value
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "password",
        "secret",
        "credential",
        "authorization",
        "apikey",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "privatekey",
        "cookie",
        "bearer",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
        || normalized.contains("token")
}

fn completion_outcome(
    rows_affected: u64,
) -> Result<PluginEventDeliveryCompletionOutcome, PluginEventDeliveryRepositoryError> {
    match rows_affected {
        0 => Ok(PluginEventDeliveryCompletionOutcome::Stale),
        1 => Ok(PluginEventDeliveryCompletionOutcome::Completed),
        _ => Err(PluginEventDeliveryRepositoryError::Invariant),
    }
}

fn record_internal_error(
    error: PluginEventDeliveryRepositoryError,
) -> PluginEventDeliveryRepositoryError {
    let error_kind = match error {
        PluginEventDeliveryRepositoryError::Query => "plugin_event_delivery_query",
        PluginEventDeliveryRepositoryError::Timeout => "plugin_event_delivery_timeout",
        PluginEventDeliveryRepositoryError::Invariant => "plugin_event_delivery_invariant",
        PluginEventDeliveryRepositoryError::Conflict => "plugin_event_delivery_conflict",
        PluginEventDeliveryRepositoryError::InvalidResult => "plugin_event_delivery_invalid_result",
    };
    tracing::error!(target: "af_db::plugin_event_delivery", error_kind, "插件事件队列仓储发生内部错误");
    error
}

fn retry_delay(attempt_count: i16) -> Result<i64, PluginEventDeliveryRepositoryError> {
    let seconds = match attempt_count {
        1 => 1,
        2 => 2,
        3 => 5,
        4 => 10,
        5 => 30,
        6 => 60,
        _ if attempt_count > 6 => 300,
        _ => return Err(PluginEventDeliveryRepositoryError::Invariant),
    };
    Ok(seconds)
}

fn due_condition(now: i64) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(plugin_event_deliveries::Column::Status.eq(STATUS_PENDING))
                .add(plugin_event_deliveries::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(plugin_event_deliveries::Column::Status.eq(STATUS_LEASED))
                .add(plugin_event_deliveries::Column::LeaseExpiresAt.lte(now)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivery(
        kind: PluginEventKind,
        payload: Value,
        capability: &str,
    ) -> PluginEventDeliveryCreate {
        PluginEventDeliveryCreate {
            plugin_id: "org.example.events".to_owned(),
            event_key: "channel:12:revision:4".to_owned(),
            event_kind: kind,
            artifact_id: 8,
            artifact_digest: "ab".repeat(32),
            deployment_generation: 3,
            capability_id: capability.to_owned(),
            capability_version: 1,
            payload,
            max_attempts: 5,
            created_at: 100,
            expires_at: 1_000,
        }
    }

    #[test]
    fn event_projection_is_closed_and_capability_bound() {
        assert!(
            delivery(
                PluginEventKind::ChannelHealthChanged,
                serde_json::json!({"targetId":1,"channelId":12,"status":"down","observedAt":100}),
                "events.channel-health.changed.consume"
            )
            .validate()
            .is_ok()
        );
        assert!(delivery(
            PluginEventKind::ChannelHealthChanged,
            serde_json::json!({"targetId":1,"channelId":12,"status":"down","observedAt":100,"apiKey":"secret"}),
            "events.channel-health.changed.consume"
        ).validate().is_err());
        assert!(
            delivery(
                PluginEventKind::ChannelHealthChanged,
                serde_json::json!({"targetId":1,"channelId":12,"status":"down","observedAt":100}),
                "events.report.requested.consume"
            )
            .validate()
            .is_err()
        );
        assert!(PluginEventKind::parse("*").is_err());
    }

    #[test]
    fn merchant_service_event_projection_contains_only_public_lifecycle_facts() {
        let capability = PluginEventKind::MerchantServiceChanged.required_capability();
        for status in ["active", "paused"] {
            assert!(
                delivery(
                    PluginEventKind::MerchantServiceChanged,
                    json!({"serviceId":8,"revision":3,"status":status,"observedAt":100}),
                    capability,
                )
                .validate()
                .is_ok()
            );
        }
        for status in ["draft", "pending", "rejected", "suspended"] {
            assert!(
                delivery(
                    PluginEventKind::MerchantServiceChanged,
                    json!({"serviceId":8,"revision":3,"status":status,"observedAt":100}),
                    capability,
                )
                .validate()
                .is_err()
            );
        }
        assert!(delivery(
            PluginEventKind::MerchantServiceChanged,
            json!({"serviceId":8,"revision":3,"status":"active","observedAt":100,"baseUrl":"https://private.invalid"}),
            capability,
        )
        .validate()
        .is_err());
    }

    #[test]
    fn report_requests_reject_unrecognized_fields_and_overlong_retention() {
        let mut input = delivery(
            PluginEventKind::ReportRequested,
            serde_json::json!({"requestId":"request-1","reportKind":"channel-health","requestedAt":100}),
            "events.report.requested.consume",
        );
        input.expires_at = input.created_at + MAX_EVENT_TTL_SECONDS + 1;
        assert!(input.validate().is_err());
        input.expires_at = 1_000;
        input.payload["principalEmail"] = Value::String("private@example.invalid".to_owned());
        assert!(input.validate().is_err());
    }

    #[test]
    fn retry_delay_is_bounded_and_monotonic_after_early_attempts() {
        assert_eq!(retry_delay(1).unwrap(), 1);
        assert_eq!(retry_delay(2).unwrap(), 2);
        assert_eq!(retry_delay(3).unwrap(), 5);
        assert_eq!(retry_delay(6).unwrap(), 60);
        assert_eq!(retry_delay(12).unwrap(), 300);
        assert!(retry_delay(0).is_err());
    }

    #[test]
    fn event_results_require_a_bounded_object_without_sensitive_field_names() {
        assert!(
            repository::serialize_event_result(&json!({ "summary": "checked", "checks": [] }))
                .is_ok()
        );
        assert!(
            repository::serialize_event_result(
                &json!({ "summary": "checked", "apiKey": "secret" })
            )
            .is_err()
        );
        assert!(
            repository::serialize_event_result(
                &json!({ "details": [{ "access_token": "secret" }] })
            )
            .is_err()
        );
        assert!(repository::serialize_event_result(&json!({ "sessionToken": "secret" })).is_err());
        assert!(
            repository::serialize_event_result(
                &json!({ "summary": "x".repeat(MAX_EVENT_RESULT_BYTES) })
            )
            .is_err()
        );
        assert!(repository::serialize_event_result(&json!(["not-an-object"])).is_err());
    }

    #[tokio::test]
    async fn channel_health_producer_is_idempotent_and_skips_disabled_deployments() {
        use af_domain::UserId;
        use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};

        let pool = crate::connect_and_migrate(
            &crate::DatabaseOptions::new("sqlite::memory:").unwrap(),
            crate::MigrationOptions::default(),
        )
        .await
        .unwrap();
        let group = crate::entity::groups::ActiveModel {
            name: Set("plugin-event-test".to_owned()),
            display_name: Set("Plugin event test".to_owned()),
            flags: Set(json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await
        .unwrap();
        let user = crate::entity::users::ActiveModel {
            username: Set("plugin-event-operator".to_owned()),
            role: Set(1),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set("plugin-event-aff".to_owned()),
            settings: Set(json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await
        .unwrap();
        let operator = UserId::new(user.id).unwrap();
        let plugin_id = "org.example.channel-events";
        let capability_id = PluginEventKind::ChannelHealthChanged.required_capability();
        let merchant_capability_id = PluginEventKind::MerchantServiceChanged.required_capability();
        let digest = "ab".repeat(32);
        let repository = crate::PluginCatalogRepository::new(pool.clone());
        let artifact = repository
            .register_artifact(crate::PluginArtifactCreate {
                plugin_id: plugin_id.to_owned(),
                plugin_version: "1.0.0".to_owned(),
                digest: digest.clone(),
                manifest: json!({
                    "pluginId": plugin_id,
                    "pluginVersion": "1.0.0",
                    "configSchema": { "type": "object", "additionalProperties": false },
                    "contractRange": { "minimum": { "major": 1, "minor": 0 }, "maximum": { "major": 1, "minor": 2 } },
                    "authenticationModes": ["none"],
                    "limits": { "maxConcurrency": 1, "requestTimeoutMs": 1000, "maxResponseBytes": 4096, "maxPageSize": 20 },
                    "capabilities": [
                        { "id": capability_id, "version": 1, "access": "read", "sideEffect": "none" },
                        { "id": merchant_capability_id, "version": 1, "access": "read", "sideEffect": "none" }
                    ]
                }),
                source_kind: crate::PluginArtifactSourceKind::LocalProcess,
                source_reference: "/opt/anyflows/plugins/channel-events".to_owned(),
                signer_key_id: String::new(),
                signature: String::new(),
                created_at: 1_800_000_000,
            })
            .await
            .unwrap();
        repository
            .approve_artifact(crate::PluginArtifactApproval {
                artifact_id: artifact.id,
                approved_by: operator,
                approved_at: 1_800_000_001,
                request_id: "event-approval".to_owned(),
            })
            .await
            .unwrap();
        let write_deployment =
            |expected_generation, enabled, request_id: &str| crate::PluginDeploymentWrite {
                plugin_id: plugin_id.to_owned(),
                artifact_digest: digest.clone(),
                expected_generation,
                enabled,
                granted_capabilities: vec![
                    crate::PluginCapabilityGrant {
                        id: capability_id.to_owned(),
                        version: 1,
                    },
                    crate::PluginCapabilityGrant {
                        id: merchant_capability_id.to_owned(),
                        version: 1,
                    },
                ],
                config: json!({}),
                updated_by: operator,
                updated_at: 1_800_000_002,
                request_id: request_id.to_owned(),
            };
        repository
            .deploy(write_deployment(None, true, "event-deploy"))
            .await
            .unwrap();

        enqueue_channel_health_changed(
            pool.connection(),
            501,
            8,
            12,
            ChannelHealthTransition::Down,
            1_800_000_010,
        )
        .await
        .unwrap();
        enqueue_channel_health_changed(
            pool.connection(),
            501,
            8,
            12,
            ChannelHealthTransition::Down,
            1_800_000_010,
        )
        .await
        .unwrap();
        let deliveries = plugin_event_deliveries::Entity::find()
            .all(pool.connection())
            .await
            .unwrap();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].event_key, "channel-monitor:501");
        assert_eq!(deliveries[0].deployment_generation, 1);
        assert_eq!(deliveries[0].artifact_digest, digest);
        assert_eq!(
            serde_json::from_str::<Value>(&deliveries[0].payload_json).unwrap(),
            json!({ "targetId": 8, "channelId": 12, "status": "down", "observedAt": 1_800_000_010 })
        );

        let queue = crate::PluginEventDeliveryRepository::new(
            pool.clone(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        let lease = match queue.claim_next(1_800_000_011).await.unwrap() {
            crate::PluginEventDeliveryClaimOutcome::Claimed(lease) => lease,
            crate::PluginEventDeliveryClaimOutcome::Empty => panic!("expected queued event"),
        };
        let result = json!({ "summary": "Channel recovered", "checks": [{ "id": "state", "status": "ok" }] });
        queue
            .mark_delivered(&lease, 1_800_000_012, &result)
            .await
            .unwrap();
        let result_page = queue
            .list_results(
                crate::PluginEventDeliveryResultQuery::new(None, 20).unwrap(),
                1_800_000_013,
            )
            .await
            .unwrap();
        assert_eq!(result_page.results.len(), 1);
        assert_eq!(result_page.results[0].result, result);
        assert_eq!(result_page.results[0].id, deliveries[0].id);
        let expired_page = queue
            .list_results(
                crate::PluginEventDeliveryResultQuery::new(None, 20).unwrap(),
                1_800_000_010 + MAX_EVENT_TTL_SECONDS,
            )
            .await
            .unwrap();
        assert!(expired_page.results.is_empty());
        assert_eq!(
            queue
                .purge_expired(1_800_000_010 + MAX_EVENT_TTL_SECONDS)
                .await
                .unwrap(),
            1
        );
        assert!(
            plugin_event_deliveries::Entity::find()
                .all(pool.connection())
                .await
                .unwrap()
                .is_empty()
        );

        repository
            .deploy(write_deployment(Some(1), false, "event-disable"))
            .await
            .unwrap();
        enqueue_channel_health_changed(
            pool.connection(),
            502,
            8,
            12,
            ChannelHealthTransition::Recovered,
            1_800_000_020,
        )
        .await
        .unwrap();
        assert_eq!(
            plugin_event_deliveries::Entity::find()
                .count(pool.connection())
                .await
                .unwrap(),
            0
        );
        repository
            .deploy(write_deployment(Some(2), true, "merchant-event-enable"))
            .await
            .unwrap();
        enqueue_merchant_service_changed(
            pool.connection(),
            88,
            7,
            "service:88:service_restored:resume:88:1800000020000000000",
            "active",
            1_800_000_020,
        )
        .await
        .unwrap();
        enqueue_merchant_service_changed(
            pool.connection(),
            88,
            7,
            "service:88:service_restored:resume:88:1800000020000000000",
            "active",
            1_800_000_020,
        )
        .await
        .unwrap();
        let merchant_deliveries = plugin_event_deliveries::Entity::find()
            .all(pool.connection())
            .await
            .unwrap();
        assert_eq!(merchant_deliveries.len(), 1);
        assert_eq!(
            merchant_deliveries[0].event_key,
            "service:88:service_restored:resume:88:1800000020000000000"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&merchant_deliveries[0].payload_json).unwrap(),
            json!({ "serviceId": 88, "revision": 7, "status": "active", "observedAt": 1_800_000_020 })
        );
        pool.close().await.unwrap();
    }
}
