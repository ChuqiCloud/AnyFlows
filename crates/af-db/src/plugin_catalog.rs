//! Persistent trust approvals and generation-checked plugin deployments.
//!
//! This repository stores host approval and deployment facts only. It does not
//! execute a plugin, verify a signature, or convert a manifest declaration into
//! authorization; callers must verify provenance and supply host-approved grants.

use std::{collections::BTreeSet, time::Duration};

use af_domain::{PlatformPermission, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::Json,
    sea_query::{Alias, Expr, LockType, Query},
};
use serde_json::Value;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, PlatformAuditOutcome,
    entity::{
        platform_audit_logs, plugin_artifact_approvals, plugin_artifact_revocations,
        plugin_artifacts, plugin_deployment_revisions, plugin_deployments,
    },
};

const ARTIFACTS: &str = "plugin_artifacts";
const APPROVALS: &str = "plugin_artifact_approvals";
const DEPLOYMENTS: &str = "plugin_deployments";
const REVISIONS: &str = "plugin_deployment_revisions";
pub const MAX_PLUGIN_MANIFEST_BYTES: usize = 65_536;
pub const MAX_PLUGIN_CONFIG_BYTES: usize = 65_536;
pub const MAX_PLUGIN_CAPABILITIES: usize = 128;
pub const MAX_PLUGIN_DEPLOYMENT_HISTORY: u32 = 100;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Origin class is descriptive metadata; an approval is still required before deployment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginArtifactSourceKind {
    Builtin,
    LocalProcess,
    RemoteEndpoint,
}

impl PluginArtifactSourceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::LocalProcess => "local_process",
            Self::RemoteEndpoint => "remote_endpoint",
        }
    }

    fn parse(value: &str) -> Result<Self, PluginCatalogError> {
        match value {
            "builtin" => Ok(Self::Builtin),
            "local_process" => Ok(Self::LocalProcess),
            "remote_endpoint" => Ok(Self::RemoteEndpoint),
            _ => Err(PluginCatalogError::Invariant),
        }
    }
}

/// Immutable candidate artifact metadata. Signature verification is performed by the host before
/// this record is inserted; administrative approval is a separate immutable operation.
#[derive(Clone, PartialEq)]
pub struct PluginArtifactCreate {
    pub plugin_id: String,
    pub plugin_version: String,
    pub digest: String,
    pub manifest: Value,
    pub source_kind: PluginArtifactSourceKind,
    pub source_reference: String,
    pub signer_key_id: String,
    pub signature: String,
    pub created_at: i64,
}

impl std::fmt::Debug for PluginArtifactCreate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginArtifactCreate")
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("digest", &self.digest)
            .field("manifest", &"<redacted>")
            .field("source_kind", &self.source_kind)
            .field("source_reference", &"<redacted>")
            .field("signer_key_id", &self.signer_key_id)
            .field("signature", &"<redacted>")
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl PluginArtifactCreate {
    fn validate(&self) -> Result<(), PluginCatalogError> {
        if !valid_plugin_id(&self.plugin_id)
            || !valid_semver(&self.plugin_version)
            || !is_lower_hex(&self.digest, 64)
            || !self.manifest.is_object()
            || serde_json::to_vec(&self.manifest)
                .map_err(|_| PluginCatalogError::InvalidInput)?
                .len()
                > MAX_PLUGIN_MANIFEST_BYTES
            || self.manifest.get("pluginId").and_then(Value::as_str)
                != Some(self.plugin_id.as_str())
            || self.manifest.get("pluginVersion").and_then(Value::as_str)
                != Some(self.plugin_version.as_str())
            || !self
                .manifest
                .get("capabilities")
                .is_some_and(Value::is_array)
            || !self
                .manifest
                .get("configSchema")
                .is_some_and(Value::is_object)
            || serde_json::to_vec(
                self.manifest
                    .get("configSchema")
                    .ok_or(PluginCatalogError::InvalidInput)?,
            )
            .map_err(|_| PluginCatalogError::InvalidInput)?
            .len()
                > MAX_PLUGIN_CONFIG_BYTES
            || self.source_reference.is_empty()
            || self.source_reference.len() > 2048
            || self.source_reference.chars().any(char::is_control)
            || !valid_source_reference(self.source_kind, &self.source_reference)
            || self.signer_key_id.is_empty()
            || self.signer_key_id.len() > 128
            || !self
                .signer_key_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            || !is_lower_hex(&self.signature, 128)
            || self.created_at <= 0
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        validate_manifest_shell(&self.manifest)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginCapabilityGrant {
    pub id: String,
    pub version: u16,
}

/// Explicit admin approval of a verified candidate, separate from artifact registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginArtifactApproval {
    pub artifact_id: i64,
    pub approved_by: UserId,
    pub approved_at: i64,
    pub request_id: String,
}

/// One-way administrative revocation of an artifact approval.
#[derive(Clone, Eq, PartialEq)]
pub struct PluginArtifactRevocation {
    pub artifact_id: i64,
    pub revoked_by: UserId,
    pub revoked_at: i64,
    pub request_id: String,
    pub reason: String,
}

impl std::fmt::Debug for PluginArtifactRevocation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginArtifactRevocation")
            .field("artifact_id", &self.artifact_id)
            .field("revoked_by", &self.revoked_by)
            .field("revoked_at", &self.revoked_at)
            .field("request_id", &self.request_id)
            .field("reason", &"<redacted>")
            .finish()
    }
}

impl PluginArtifactRevocation {
    fn validate(&self) -> Result<(), PluginCatalogError> {
        if self.artifact_id <= 0
            || self.revoked_at <= 0
            || !valid_request_id(&self.request_id)
            || self.reason.trim().is_empty()
            || self.reason.len() > 512
            || self.reason.chars().any(char::is_control)
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginArtifactQuery {
    before_id: Option<i64>,
    limit: u32,
}

impl PluginArtifactQuery {
    pub fn new(before_id: Option<i64>, limit: u32) -> Result<Self, PluginCatalogError> {
        if before_id.is_some_and(|id| id <= 0)
            || limit == 0
            || limit > MAX_PLUGIN_DEPLOYMENT_HISTORY
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        Ok(Self { before_id, limit })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginDeploymentQuery {
    before_plugin_id: Option<String>,
    limit: u32,
}

impl PluginDeploymentQuery {
    pub fn new(before_plugin_id: Option<String>, limit: u32) -> Result<Self, PluginCatalogError> {
        if before_plugin_id
            .as_deref()
            .is_some_and(|plugin_id| !valid_plugin_id(plugin_id))
            || limit == 0
            || limit > MAX_PLUGIN_DEPLOYMENT_HISTORY
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        Ok(Self {
            before_plugin_id,
            limit,
        })
    }
}

impl PluginArtifactApproval {
    fn validate(&self) -> Result<(), PluginCatalogError> {
        if self.artifact_id <= 0 || self.approved_at <= 0 || !valid_request_id(&self.request_id) {
            return Err(PluginCatalogError::InvalidInput);
        }
        Ok(())
    }
}

/// A desired deployment state. `expected_generation=None` creates the first generation;
/// otherwise compare-and-swap protects concurrent enable/upgrade/rollback requests.
#[derive(Clone, PartialEq)]
pub struct PluginDeploymentWrite {
    pub plugin_id: String,
    pub artifact_digest: String,
    pub expected_generation: Option<i64>,
    pub enabled: bool,
    pub granted_capabilities: Vec<PluginCapabilityGrant>,
    pub config: Value,
    pub updated_by: UserId,
    pub updated_at: i64,
    pub request_id: String,
}

impl std::fmt::Debug for PluginDeploymentWrite {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginDeploymentWrite")
            .field("plugin_id", &self.plugin_id)
            .field("artifact_digest", &self.artifact_digest)
            .field("expected_generation", &self.expected_generation)
            .field("enabled", &self.enabled)
            .field("granted_capabilities", &self.granted_capabilities)
            .field("config", &"<redacted>")
            .field("updated_by", &self.updated_by)
            .field("updated_at", &self.updated_at)
            .field("request_id", &self.request_id)
            .finish()
    }
}

impl PluginDeploymentWrite {
    fn validate(&self) -> Result<(), PluginCatalogError> {
        if !valid_plugin_id(&self.plugin_id)
            || !is_lower_hex(&self.artifact_digest, 64)
            || self
                .expected_generation
                .is_some_and(|generation| generation <= 0)
            || self.granted_capabilities.len() > MAX_PLUGIN_CAPABILITIES
            || !self.config.is_object()
            || serde_json::to_vec(&self.config)
                .map_err(|_| PluginCatalogError::InvalidInput)?
                .len()
                > MAX_PLUGIN_CONFIG_BYTES
            || self.updated_at <= 0
            || !valid_request_id(&self.request_id)
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        let mut seen = std::collections::BTreeSet::new();
        for grant in &self.granted_capabilities {
            if !valid_capability_id(&grant.id) || grant.version == 0 || !seen.insert(&grant.id) {
                return Err(PluginCatalogError::InvalidInput);
            }
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
pub struct PluginArtifactRecord {
    pub id: i64,
    pub plugin_id: String,
    pub plugin_version: String,
    pub digest: String,
    pub manifest: Value,
    pub source_kind: PluginArtifactSourceKind,
    pub source_reference: String,
    pub signer_key_id: String,
    pub signature: String,
    pub created_at: i64,
    pub approval: Option<PluginArtifactApproval>,
    pub revocation: Option<PluginArtifactRevocation>,
}

impl std::fmt::Debug for PluginArtifactRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginArtifactRecord")
            .field("id", &self.id)
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("digest", &self.digest)
            .field("manifest", &"<redacted>")
            .field("source_kind", &self.source_kind)
            .field("source_reference", &"<redacted>")
            .field("signer_key_id", &self.signer_key_id)
            .field("signature", &"<redacted>")
            .field("created_at", &self.created_at)
            .field("revoked", &self.revocation.is_some())
            .field("approval", &self.approval)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct PluginDeploymentRecord {
    pub plugin_id: String,
    pub generation: i64,
    pub artifact: PluginArtifactRecord,
    pub enabled: bool,
    pub granted_capabilities: Vec<PluginCapabilityGrant>,
    pub config: Value,
    pub updated_by: UserId,
    pub updated_at: i64,
    /// Correlates this immutable deployment revision with a platform audit event.
    pub request_id: String,
}

impl std::fmt::Debug for PluginDeploymentRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginDeploymentRecord")
            .field("plugin_id", &self.plugin_id)
            .field("generation", &self.generation)
            .field("artifact_digest", &self.artifact.digest)
            .field("enabled", &self.enabled)
            .field("granted_capabilities", &self.granted_capabilities)
            .field("config", &"<redacted>")
            .field("updated_by", &self.updated_by)
            .field("updated_at", &self.updated_at)
            .field("request_id", &self.request_id)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PluginCatalogError {
    #[error("plugin catalog input is invalid")]
    InvalidInput,
    #[error("plugin catalog record was not found")]
    NotFound,
    #[error("plugin catalog state conflicts with the requested generation")]
    Conflict,
    #[error("plugin catalog query failed")]
    Unavailable,
    #[error("plugin catalog operation timed out")]
    Timeout,
    #[error("plugin catalog persisted state violates an invariant")]
    Invariant,
}

#[derive(Clone)]
pub struct PluginCatalogRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl PluginCatalogRepository {
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            timeout: OPERATION_TIMEOUT,
        }
    }

    /// Register immutable metadata for a signature-verified artifact. Repeating the same digest
    /// and content is idempotent; reusing the digest with different metadata is rejected.
    pub async fn register_artifact(
        &self,
        input: PluginArtifactCreate,
    ) -> Result<PluginArtifactRecord, PluginCatalogError> {
        input.validate()?;
        let operation = async {
            if let Some(existing) = plugin_artifacts::Entity::find()
                .filter(plugin_artifacts::Column::PluginId.eq(&input.plugin_id))
                .filter(plugin_artifacts::Column::Digest.eq(&input.digest))
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
            {
                let existing = load_artifact(&self.pool, existing.id).await?;
                if artifact_matches(&existing, &input) {
                    return Ok(existing);
                }
                return Err(PluginCatalogError::Conflict);
            }
            let model = plugin_artifacts::ActiveModel {
                plugin_id: Set(input.plugin_id.clone()),
                plugin_version: Set(input.plugin_version.clone()),
                digest: Set(input.digest.clone()),
                manifest: Set(input.manifest.clone()),
                source_kind: Set(input.source_kind.as_str().to_owned()),
                source_reference: Set(input.source_reference.clone()),
                signer_key_id: Set(input.signer_key_id.clone()),
                signature: Set(input.signature.clone()),
                created_at: Set(input.created_at),
                ..Default::default()
            };
            match model.insert(self.pool.connection()).await {
                Ok(inserted) => artifact_from_model(inserted, None, None),
                Err(error)
                    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) =>
                {
                    if let Some(existing) = plugin_artifacts::Entity::find()
                        .filter(plugin_artifacts::Column::PluginId.eq(&input.plugin_id))
                        .filter(plugin_artifacts::Column::Digest.eq(&input.digest))
                        .one(self.pool.connection())
                        .await
                        .map_err(map_db)?
                    {
                        let existing = load_artifact(&self.pool, existing.id).await?;
                        if artifact_matches(&existing, &input) {
                            return Ok(existing);
                        }
                    }
                    Err(PluginCatalogError::Conflict)
                }
                Err(error) => Err(map_db(error)),
            }
        };
        self.bounded(operation).await
    }

    /// Register a verified candidate and its actor-attributed audit fact in one
    /// transaction. Matching retries preserve the artifact identity and audit
    /// fact; a reused request ID with different metadata or actor conflicts.
    pub async fn register_artifact_audited(
        &self,
        input: PluginArtifactCreate,
        operator: UserId,
        request_id: &str,
    ) -> Result<PluginArtifactRecord, PluginCatalogError> {
        input.validate()?;
        if !valid_request_id(request_id) {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            // A savepoint keeps PostgreSQL usable after a concurrent unique
            // collision. Inserting before reading also avoids a stale SQLite
            // read snapshot when two identical registrations race.
            let insertion = txn.begin().await.map_err(map_db)?;
            let inserted = plugin_artifacts::ActiveModel {
                plugin_id: Set(input.plugin_id.clone()),
                plugin_version: Set(input.plugin_version.clone()),
                digest: Set(input.digest.clone()),
                manifest: Set(input.manifest.clone()),
                source_kind: Set(input.source_kind.as_str().to_owned()),
                source_reference: Set(input.source_reference.clone()),
                signer_key_id: Set(input.signer_key_id.clone()),
                signature: Set(input.signature.clone()),
                created_at: Set(input.created_at),
                ..Default::default()
            }
            .insert(&insertion)
            .await;
            let artifact = match inserted {
                Ok(row) => {
                    insertion.commit().await.map_err(map_db)?;
                    artifact_from_model(row, None, None)?
                }
                Err(error)
                    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) =>
                {
                    insertion.rollback().await.map_err(map_db)?;
                    let existing = plugin_artifacts::Entity::find()
                        .filter(plugin_artifacts::Column::PluginId.eq(&input.plugin_id))
                        .filter(plugin_artifacts::Column::Digest.eq(&input.digest))
                        .one(&txn)
                        .await
                        .map_err(map_db)?
                        .ok_or(PluginCatalogError::Conflict)?;
                    let existing = load_artifact_on(&txn, existing.id).await?;
                    if !artifact_matches(&existing, &input) {
                        return Err(PluginCatalogError::Conflict);
                    }
                    existing
                }
                Err(error) => {
                    insertion.rollback().await.map_err(map_db)?;
                    return Err(map_db(error));
                }
            };
            write_catalog_audit(
                &txn,
                operator,
                request_id,
                "/admin/plugins/artifacts",
                "catalog_register",
                "plugin_artifact",
                artifact.id.to_string(),
                serde_json::json!({
                    "artifactId": artifact.id,
                    "pluginId": artifact.plugin_id,
                    "artifactDigest": artifact.digest,
                    "pluginVersion": artifact.plugin_version
                }),
            )
            .await?;
            txn.commit().await.map_err(map_db)?;
            Ok(artifact)
        };
        self.bounded(operation).await
    }

    /// Add a one-time explicit human approval. The immutable approval is the prerequisite for
    /// any deployment; manifest declarations alone never authorize capabilities.
    pub async fn approve_artifact(
        &self,
        input: PluginArtifactApproval,
    ) -> Result<(), PluginCatalogError> {
        input.validate()?;
        let operation = async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let mut artifact_query = plugin_artifacts::Entity::find_by_id(input.artifact_id);
            if txn.get_database_backend() != DbBackend::Sqlite {
                artifact_query = artifact_query.lock(LockType::Update);
            }
            let artifact = artifact_query
                .one(&txn)
                .await
                .map_err(map_db)?
                .ok_or(PluginCatalogError::NotFound)?;
            if plugin_artifact_revocations::Entity::find_by_id(input.artifact_id)
                .one(&txn)
                .await
                .map_err(map_db)?
                .is_some()
            {
                return Err(PluginCatalogError::Conflict);
            }
            if let Some(existing) = plugin_artifact_approvals::Entity::find_by_id(input.artifact_id)
                .one(&txn)
                .await
                .map_err(map_db)?
            {
                if existing.approved_by == input.approved_by.get()
                    && existing.request_id == input.request_id
                {
                    return Ok(());
                }
                return Err(PluginCatalogError::Conflict);
            }
            plugin_artifact_approvals::ActiveModel {
                artifact_id: Set(input.artifact_id),
                approved_by: Set(input.approved_by.get()),
                approved_at: Set(input.approved_at),
                request_id: Set(input.request_id.clone()),
            }
            .insert(&txn)
            .await
            .map_err(map_db)?;
            write_catalog_audit(
                &txn,
                input.approved_by,
                &input.request_id,
                "/admin/plugins/artifacts/{artifactId}/approve",
                "catalog_approve",
                "plugin_artifact",
                artifact.id.to_string(),
                serde_json::json!({
                    "artifactId": artifact.id,
                    "pluginId": artifact.plugin_id,
                    "artifactDigest": artifact.digest,
                    "approved": true
                }),
            )
            .await?;
            txn.commit().await.map_err(map_db)?;
            Ok(())
        };
        self.bounded(operation).await
    }

    /// Permanently revoke an artifact approval. Revocation is append-only and prevents any
    /// subsequent deployment or re-approval of this artifact digest.
    pub async fn revoke_artifact(
        &self,
        input: PluginArtifactRevocation,
    ) -> Result<(), PluginCatalogError> {
        input.validate()?;
        let operation = async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let mut artifact_query = plugin_artifacts::Entity::find_by_id(input.artifact_id);
            if txn.get_database_backend() != DbBackend::Sqlite {
                artifact_query = artifact_query.lock(LockType::Update);
            }
            let artifact = artifact_query
                .one(&txn)
                .await
                .map_err(map_db)?
                .ok_or(PluginCatalogError::NotFound)?;
            if plugin_artifact_approvals::Entity::find_by_id(input.artifact_id)
                .one(&txn)
                .await
                .map_err(map_db)?
                .is_none()
            {
                return Err(PluginCatalogError::Conflict);
            }
            if let Some(existing) =
                plugin_artifact_revocations::Entity::find_by_id(input.artifact_id)
                    .one(&txn)
                    .await
                    .map_err(map_db)?
            {
                return if existing.revoked_by == input.revoked_by.get()
                    && existing.request_id == input.request_id
                    && existing.reason == input.reason
                {
                    Ok(())
                } else {
                    Err(PluginCatalogError::Conflict)
                };
            }
            let inserted = plugin_artifact_revocations::ActiveModel {
                artifact_id: Set(artifact.id),
                revoked_by: Set(input.revoked_by.get()),
                revoked_at: Set(input.revoked_at),
                request_id: Set(input.request_id.clone()),
                reason: Set(input.reason.clone()),
            }
            .insert(&txn)
            .await;
            match inserted {
                Ok(_) => {
                    if let Some(current) =
                        plugin_deployments::Entity::find_by_id(artifact.plugin_id.clone())
                            .one(&txn)
                            .await
                            .map_err(map_db)?
                        && current.enabled
                        && current.artifact_id == artifact.id
                    {
                        let next_generation = current
                            .generation
                            .checked_add(1)
                            .ok_or(PluginCatalogError::Invariant)?;
                        let deployment = PluginDeploymentWrite {
                            plugin_id: current.plugin_id.clone(),
                            artifact_digest: artifact.digest.clone(),
                            expected_generation: Some(current.generation),
                            enabled: false,
                            granted_capabilities: parse_grants(current.capability_grants.clone())?,
                            config: current.config.clone(),
                            updated_by: input.revoked_by,
                            updated_at: input.revoked_at,
                            request_id: input.request_id.clone(),
                        };
                        write_revision(&txn, &deployment, artifact.id, next_generation).await?;
                        let update = Query::update()
                            .table(Alias::new(DEPLOYMENTS))
                            .value(Alias::new("generation"), next_generation)
                            .value(Alias::new("enabled"), false)
                            .value(Alias::new("updated_by"), input.revoked_by.get())
                            .value(Alias::new("updated_at"), input.revoked_at)
                            .value(Alias::new("request_id"), input.request_id.clone())
                            .and_where(Expr::col(Alias::new("plugin_id")).eq(&current.plugin_id))
                            .and_where(Expr::col(Alias::new("generation")).eq(current.generation))
                            .to_owned();
                        let result = txn
                            .execute(txn.get_database_backend().build(&update))
                            .await
                            .map_err(map_db)?;
                        if result.rows_affected() != 1 {
                            return Err(PluginCatalogError::Conflict);
                        }
                    }
                    write_catalog_audit(
                        &txn,
                        input.revoked_by,
                        &input.request_id,
                        "/admin/plugins/artifacts/{artifactId}/revoke",
                        "catalog_revoke",
                        "plugin_artifact",
                        artifact.id.to_string(),
                        serde_json::json!({
                            "artifactId": artifact.id,
                            "pluginId": artifact.plugin_id,
                            "artifactDigest": artifact.digest,
                            "revoked": true
                        }),
                    )
                    .await?;
                    txn.commit().await.map_err(map_db)?;
                    Ok(())
                }
                Err(error) => Err(map_db(error)),
            }
        };
        self.bounded(operation).await
    }

    /// List candidate and approved artifacts in descending insertion order. This makes pending
    /// review durable across restarts without treating registration as approval.
    pub async fn list_artifacts(
        &self,
        query: PluginArtifactQuery,
    ) -> Result<Vec<PluginArtifactRecord>, PluginCatalogError> {
        let operation = async {
            let mut select = plugin_artifacts::Entity::find()
                .order_by_desc(plugin_artifacts::Column::Id)
                .limit(u64::from(query.limit));
            if let Some(before_id) = query.before_id {
                select = select.filter(plugin_artifacts::Column::Id.lt(before_id));
            }
            let rows = select.all(self.pool.connection()).await.map_err(map_db)?;
            let mut records = Vec::with_capacity(rows.len());
            for row in rows {
                records.push(load_artifact(&self.pool, row.id).await?);
            }
            Ok(records)
        };
        self.bounded(operation).await
    }

    /// Fetch one immutable artifact record by its stable database identity.
    pub async fn get_artifact(
        &self,
        artifact_id: i64,
    ) -> Result<Option<PluginArtifactRecord>, PluginCatalogError> {
        if artifact_id <= 0 {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            match plugin_artifacts::Entity::find_by_id(artifact_id)
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
            {
                Some(row) => load_artifact(&self.pool, row.id).await.map(Some),
                None => Ok(None),
            }
        };
        self.bounded(operation).await
    }

    /// Fetch an immutable artifact by its plugin identity and content digest.
    /// This is used for preflight validation before writing a deployment
    /// generation; deploy() repeats approval checks transactionally.
    pub async fn get_artifact_by_digest(
        &self,
        plugin_id: &str,
        digest: &str,
    ) -> Result<Option<PluginArtifactRecord>, PluginCatalogError> {
        if !valid_plugin_id(plugin_id) || !is_lower_hex(digest, 64) {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            match plugin_artifacts::Entity::find()
                .filter(plugin_artifacts::Column::PluginId.eq(plugin_id))
                .filter(plugin_artifacts::Column::Digest.eq(digest))
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
            {
                Some(row) => load_artifact(&self.pool, row.id).await.map(Some),
                None => Ok(None),
            }
        };
        self.bounded(operation).await
    }

    /// Atomically create or advance a deployment generation, after intersecting requested grants
    /// with the approved manifest's read capabilities. Each generation is immutable history.
    pub async fn deploy(
        &self,
        input: PluginDeploymentWrite,
    ) -> Result<PluginDeploymentRecord, PluginCatalogError> {
        input.validate()?;
        let operation = async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let (artifact_id, manifest) =
                approved_artifact(&txn, &input.plugin_id, &input.artifact_digest).await?;
            validate_grants(&manifest, &input.granted_capabilities)?;
            let current = plugin_deployments::Entity::find_by_id(input.plugin_id.clone())
                .one(&txn)
                .await
                .map_err(map_db)?;
            let next_generation = match current.as_ref() {
                None if input.expected_generation.is_none() => 1,
                None => return Err(PluginCatalogError::Conflict),
                Some(current) if input.expected_generation == Some(current.generation) => current
                    .generation
                    .checked_add(1)
                    .ok_or(PluginCatalogError::Invariant)?,
                Some(_) => return Err(PluginCatalogError::Conflict),
            };
            write_revision(&txn, &input, artifact_id, next_generation).await?;
            match current {
                None => {
                    plugin_deployments::ActiveModel {
                        plugin_id: Set(input.plugin_id.clone()),
                        generation: Set(next_generation),
                        artifact_id: Set(artifact_id),
                        enabled: Set(input.enabled),
                        capability_grants: Set(grants_json(&input.granted_capabilities)),
                        config: Set(input.config.clone()),
                        updated_by: Set(input.updated_by.get()),
                        updated_at: Set(input.updated_at),
                        request_id: Set(input.request_id.clone()),
                    }
                    .insert(&txn)
                    .await
                    .map_err(map_db)?;
                }
                Some(_) => {
                    let update = Query::update()
                        .table(Alias::new(DEPLOYMENTS))
                        .value(Alias::new("generation"), next_generation)
                        .value(Alias::new("artifact_id"), artifact_id)
                        .value(Alias::new("enabled"), input.enabled)
                        .value(
                            Alias::new("capability_grants"),
                            grants_json(&input.granted_capabilities),
                        )
                        .value(Alias::new("config"), input.config.clone())
                        .value(Alias::new("updated_by"), input.updated_by.get())
                        .value(Alias::new("updated_at"), input.updated_at)
                        .value(Alias::new("request_id"), input.request_id.clone())
                        .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id))
                        .and_where(
                            Expr::col(Alias::new("generation"))
                                .eq(input.expected_generation.unwrap_or(0)),
                        )
                        .to_owned();
                    let result = txn
                        .execute(txn.get_database_backend().build(&update))
                        .await
                        .map_err(map_db)?;
                    if result.rows_affected() != 1 {
                        return Err(PluginCatalogError::Conflict);
                    }
                }
            }
            txn.commit().await.map_err(map_db)?;
            self.get_deployment(&input.plugin_id)
                .await?
                .ok_or(PluginCatalogError::Invariant)
        };
        self.bounded(operation).await
    }

    /// Re-apply a previous immutable generation as a new generation, preserving a linear audit
    /// history and using the same expected-generation compare-and-swap as normal deploys.
    pub async fn rollback(
        &self,
        plugin_id: &str,
        expected_generation: i64,
        target_generation: i64,
        updated_by: UserId,
        updated_at: i64,
        request_id: String,
    ) -> Result<PluginDeploymentRecord, PluginCatalogError> {
        if !valid_plugin_id(plugin_id) || expected_generation <= 0 || target_generation <= 0 {
            return Err(PluginCatalogError::InvalidInput);
        }
        let target = self
            .get_revision(plugin_id, target_generation)
            .await?
            .ok_or(PluginCatalogError::NotFound)?;
        self.deploy(PluginDeploymentWrite {
            plugin_id: plugin_id.to_owned(),
            artifact_digest: target.artifact.digest,
            expected_generation: Some(expected_generation),
            enabled: target.enabled,
            granted_capabilities: target.granted_capabilities,
            config: target.config,
            updated_by,
            updated_at,
            request_id,
        })
        .await
    }

    pub async fn get_deployment(
        &self,
        plugin_id: &str,
    ) -> Result<Option<PluginDeploymentRecord>, PluginCatalogError> {
        if !valid_plugin_id(plugin_id) {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            let Some(model) = plugin_deployments::Entity::find_by_id(plugin_id.to_owned())
                .one(self.pool.connection())
                .await
                .map_err(map_db)?
            else {
                return Ok(None);
            };
            deployment_from_model(&self.pool, model).await.map(Some)
        };
        self.bounded(operation).await
    }

    pub async fn list_deployments(
        &self,
        query: PluginDeploymentQuery,
    ) -> Result<Vec<PluginDeploymentRecord>, PluginCatalogError> {
        let operation = async {
            let mut select = plugin_deployments::Entity::find()
                .order_by_asc(plugin_deployments::Column::PluginId)
                .limit(u64::from(query.limit));
            if let Some(before_plugin_id) = query.before_plugin_id {
                select = select.filter(plugin_deployments::Column::PluginId.gt(before_plugin_id));
            }
            let rows = select.all(self.pool.connection()).await.map_err(map_db)?;
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                result.push(deployment_from_model(&self.pool, row).await?);
            }
            Ok(result)
        };
        self.bounded(operation).await
    }

    pub async fn deployment_history(
        &self,
        plugin_id: &str,
        limit: u32,
    ) -> Result<Vec<PluginDeploymentRecord>, PluginCatalogError> {
        if !valid_plugin_id(plugin_id) || limit == 0 || limit > MAX_PLUGIN_DEPLOYMENT_HISTORY {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            let rows = plugin_deployment_revisions::Entity::find()
                .filter(plugin_deployment_revisions::Column::PluginId.eq(plugin_id))
                .order_by_desc(plugin_deployment_revisions::Column::Generation)
                .limit(u64::from(limit))
                .all(self.pool.connection())
                .await
                .map_err(map_db)?;
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                result.push(revision_from_model(&self.pool, row).await?);
            }
            Ok(result)
        };
        self.bounded(operation).await
    }

    async fn get_revision(
        &self,
        plugin_id: &str,
        generation: i64,
    ) -> Result<Option<PluginDeploymentRecord>, PluginCatalogError> {
        if !valid_plugin_id(plugin_id) || generation <= 0 {
            return Err(PluginCatalogError::InvalidInput);
        }
        let operation = async {
            let row = plugin_deployment_revisions::Entity::find()
                .filter(plugin_deployment_revisions::Column::PluginId.eq(plugin_id))
                .filter(plugin_deployment_revisions::Column::Generation.eq(generation))
                .one(self.pool.connection())
                .await
                .map_err(map_db)?;
            match row {
                Some(row) => revision_from_model(&self.pool, row).await.map(Some),
                None => Ok(None),
            }
        };
        self.bounded(operation).await
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, PluginCatalogError>>,
    ) -> Result<T, PluginCatalogError> {
        match timeout(self.timeout, operation)
            .with_subscriber(NoSubscriber::default())
            .await
        {
            Ok(result) => result,
            Err(_) => Err(PluginCatalogError::Timeout),
        }
    }
}

async fn approved_artifact(
    txn: &DatabaseTransaction,
    plugin_id: &str,
    digest: &str,
) -> Result<(i64, Value), PluginCatalogError> {
    let mut query = Query::select()
        .columns([
            (Alias::new(ARTIFACTS), Alias::new("id")),
            (Alias::new(ARTIFACTS), Alias::new("manifest")),
        ])
        .from(Alias::new(ARTIFACTS))
        .inner_join(
            Alias::new(APPROVALS),
            Expr::col((Alias::new(ARTIFACTS), Alias::new("id")))
                .equals((Alias::new(APPROVALS), Alias::new("artifact_id"))),
        )
        .and_where(Expr::col((Alias::new(ARTIFACTS), Alias::new("plugin_id"))).eq(plugin_id))
        .and_where(Expr::col((Alias::new(ARTIFACTS), Alias::new("digest"))).eq(digest))
        .to_owned();
    if txn.get_database_backend() != DbBackend::Sqlite {
        query.lock(LockType::Update);
    }
    let query = query.to_owned();
    let row = txn
        .query_one(txn.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .ok_or(PluginCatalogError::NotFound)?;
    let artifact_id = row.try_get("", "id").map_err(map_db)?;
    if plugin_artifact_revocations::Entity::find_by_id(artifact_id)
        .one(txn)
        .await
        .map_err(map_db)?
        .is_some()
    {
        return Err(PluginCatalogError::Conflict);
    }
    Ok((
        artifact_id,
        row.try_get::<Json>("", "manifest").map_err(map_db)?,
    ))
}

async fn write_revision(
    txn: &DatabaseTransaction,
    input: &PluginDeploymentWrite,
    artifact_id: i64,
    generation: i64,
) -> Result<(), PluginCatalogError> {
    plugin_deployment_revisions::ActiveModel {
        plugin_id: Set(input.plugin_id.clone()),
        generation: Set(generation),
        artifact_id: Set(artifact_id),
        enabled: Set(input.enabled),
        capability_grants: Set(grants_json(&input.granted_capabilities)),
        config: Set(input.config.clone()),
        updated_by: Set(input.updated_by.get()),
        updated_at: Set(input.updated_at),
        request_id: Set(input.request_id.clone()),
        ..Default::default()
    }
    .insert(txn)
    .await
    .map_err(map_db)?;
    write_catalog_audit(
        txn,
        input.updated_by,
        &input.request_id,
        "/admin/plugins/{pluginId}/deployment",
        "catalog_deploy",
        "plugin_deployment",
        input.plugin_id.clone(),
        serde_json::json!({
            "artifactId": artifact_id,
            "artifactDigest": input.artifact_digest,
            "generation": generation,
            "enabled": input.enabled,
            "grantedCapabilityCount": input.granted_capabilities.len()
        }),
    )
    .await
}

/// Append the committed catalog fact within the state-changing transaction.
/// Service outcome audits remain separate because runtime activation can fail
/// after commit. Only host-selected, non-sensitive identifiers are accepted at
/// these private call sites; configuration, sources and signatures are omitted.
#[allow(clippy::too_many_arguments)]
async fn write_catalog_audit(
    txn: &DatabaseTransaction,
    operator: UserId,
    request_id: &str,
    route: &str,
    operation: &str,
    resource: &str,
    resource_id: String,
    after: Value,
) -> Result<(), PluginCatalogError> {
    let after_value = serde_json::to_string(&after).map_err(|_| PluginCatalogError::Invariant)?;
    let insertion = txn.begin().await.map_err(map_db)?;
    let inserted = platform_audit_logs::ActiveModel {
        operator_user_id: Set(operator.get()),
        actor_credential_kind: Set(None),
        actor_credential_id: Set(None),
        permission_code: Set(PlatformPermission::PluginsManage.code().to_owned()),
        route: Set(route.to_owned()),
        operation: Set(operation.to_owned()),
        resource: Set(resource.to_owned()),
        resource_id: Set(Some(resource_id.clone())),
        outcome: Set(PlatformAuditOutcome::Succeeded as i16),
        before_value: Set(None),
        after_value: Set(Some(after_value.clone())),
        audit_info: Set(None),
        request_id: Set(request_id.to_owned()),
        created_at: Set(time::OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .insert(&insertion)
    .await;
    match inserted {
        Ok(_) => insertion.commit().await.map_err(map_db),
        Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            insertion.rollback().await.map_err(map_db)?;
            let existing = platform_audit_logs::Entity::find()
                .filter(platform_audit_logs::Column::RequestId.eq(request_id))
                .filter(platform_audit_logs::Column::Route.eq(route))
                .filter(platform_audit_logs::Column::Operation.eq(operation))
                .one(txn)
                .await
                .map_err(map_db)?
                .ok_or(PluginCatalogError::Conflict)?;
            if existing.operator_user_id == operator.get()
                && existing.actor_credential_kind.is_none()
                && existing.actor_credential_id.is_none()
                && existing.permission_code == PlatformPermission::PluginsManage.code()
                && existing.resource == resource
                && existing.resource_id.as_deref() == Some(resource_id.as_str())
                && existing.outcome == PlatformAuditOutcome::Succeeded as i16
                && existing.before_value.is_none()
                && existing.after_value.as_deref() == Some(after_value.as_str())
                && existing.audit_info.is_none()
            {
                Ok(())
            } else {
                Err(PluginCatalogError::Conflict)
            }
        }
        Err(error) => {
            insertion.rollback().await.map_err(map_db)?;
            Err(map_db(error))
        }
    }
}

async fn deployment_from_model(
    pool: &DatabasePool,
    row: plugin_deployments::Model,
) -> Result<PluginDeploymentRecord, PluginCatalogError> {
    let artifact = load_artifact(pool, row.artifact_id).await?;
    Ok(PluginDeploymentRecord {
        plugin_id: row.plugin_id,
        generation: row.generation,
        artifact,
        enabled: row.enabled,
        granted_capabilities: parse_grants(row.capability_grants)?,
        config: row.config,
        updated_by: UserId::new(row.updated_by).map_err(|_| PluginCatalogError::Invariant)?,
        updated_at: row.updated_at,
        request_id: row.request_id,
    })
}

async fn revision_from_model(
    pool: &DatabasePool,
    row: plugin_deployment_revisions::Model,
) -> Result<PluginDeploymentRecord, PluginCatalogError> {
    let artifact = load_artifact(pool, row.artifact_id).await?;
    Ok(PluginDeploymentRecord {
        plugin_id: row.plugin_id,
        generation: row.generation,
        artifact,
        enabled: row.enabled,
        granted_capabilities: parse_grants(row.capability_grants)?,
        config: row.config,
        updated_by: UserId::new(row.updated_by).map_err(|_| PluginCatalogError::Invariant)?,
        updated_at: row.updated_at,
        request_id: row.request_id,
    })
}

async fn load_artifact(
    pool: &DatabasePool,
    id: i64,
) -> Result<PluginArtifactRecord, PluginCatalogError> {
    load_artifact_on(pool.connection(), id).await
}

async fn load_artifact_on<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<PluginArtifactRecord, PluginCatalogError> {
    let artifact = plugin_artifacts::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(map_db)?
        .ok_or(PluginCatalogError::Invariant)?;
    let approval = plugin_artifact_approvals::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(map_db)?;
    let approval = approval
        .map(|row| {
            Ok(PluginArtifactApproval {
                artifact_id: row.artifact_id,
                approved_by: UserId::new(row.approved_by)
                    .map_err(|_| PluginCatalogError::Invariant)?,
                approved_at: row.approved_at,
                request_id: row.request_id,
            })
        })
        .transpose()?;
    let revocation = plugin_artifact_revocations::Entity::find_by_id(id)
        .one(connection)
        .await
        .map_err(map_db)?
        .map(|row| {
            Ok(PluginArtifactRevocation {
                artifact_id: row.artifact_id,
                revoked_by: UserId::new(row.revoked_by)
                    .map_err(|_| PluginCatalogError::Invariant)?,
                revoked_at: row.revoked_at,
                request_id: row.request_id,
                reason: row.reason,
            })
        })
        .transpose()?;
    artifact_from_model(artifact, approval, revocation)
}

fn artifact_from_model(
    row: plugin_artifacts::Model,
    approval: Option<PluginArtifactApproval>,
    revocation: Option<PluginArtifactRevocation>,
) -> Result<PluginArtifactRecord, PluginCatalogError> {
    Ok(PluginArtifactRecord {
        id: row.id,
        plugin_id: row.plugin_id,
        plugin_version: row.plugin_version,
        digest: row.digest,
        manifest: row.manifest,
        source_kind: PluginArtifactSourceKind::parse(&row.source_kind)?,
        source_reference: row.source_reference,
        signer_key_id: row.signer_key_id,
        signature: row.signature,
        created_at: row.created_at,
        approval,
        revocation,
    })
}

fn validate_grants(
    manifest: &Value,
    grants: &[PluginCapabilityGrant],
) -> Result<(), PluginCatalogError> {
    let declared = manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or(PluginCatalogError::Invariant)?;
    for grant in grants {
        let matches = declared.iter().any(|capability| {
            capability.get("id").and_then(Value::as_str) == Some(grant.id.as_str())
                && capability.get("version").and_then(Value::as_u64)
                    == Some(u64::from(grant.version))
                && capability.get("access").and_then(Value::as_str) == Some("read")
                && capability.get("sideEffect").and_then(Value::as_str) == Some("none")
        });
        if !matches {
            return Err(PluginCatalogError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_manifest_shell(manifest: &Value) -> Result<(), PluginCatalogError> {
    let contract_range = manifest
        .get("contractRange")
        .and_then(Value::as_object)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let minimum = contract_range
        .get("minimum")
        .and_then(Value::as_object)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let maximum = contract_range
        .get("maximum")
        .and_then(Value::as_object)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let min_major = minimum
        .get("major")
        .and_then(Value::as_u64)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let max_major = maximum
        .get("major")
        .and_then(Value::as_u64)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let min_minor = minimum
        .get("minor")
        .and_then(Value::as_u64)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let max_minor = maximum
        .get("minor")
        .and_then(Value::as_u64)
        .ok_or(PluginCatalogError::InvalidInput)?;
    if min_major == 0 || min_major != max_major || (min_major, min_minor) > (max_major, max_minor) {
        return Err(PluginCatalogError::InvalidInput);
    }
    let limits = manifest
        .get("limits")
        .and_then(Value::as_object)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let limit_in = |key: &str, min: u64, max: u64| {
        limits
            .get(key)
            .and_then(Value::as_u64)
            .is_some_and(|value| (min..=max).contains(&value))
    };
    if !limit_in("maxConcurrency", 1, 1024)
        || !limit_in("requestTimeoutMs", 1, 300_000)
        || !limit_in("maxResponseBytes", 1, 16 * 1024 * 1024)
        || !limit_in("maxPageSize", 1, 10_000)
    {
        return Err(PluginCatalogError::InvalidInput);
    }
    if let Some(display_name) = manifest.get("displayName")
        && (!display_name.is_string()
            || display_name
                .as_str()
                .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control)))
    {
        return Err(PluginCatalogError::InvalidInput);
    }
    let auth_modes = manifest
        .get("authenticationModes")
        .and_then(Value::as_array)
        .filter(|modes| !modes.is_empty() && modes.len() <= 16)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let mut auth_seen = BTreeSet::new();
    for mode in auth_modes {
        let mode = mode.as_str().ok_or(PluginCatalogError::InvalidInput)?;
        if mode.is_empty()
            || mode.len() > 128
            || !auth_seen.insert(mode)
            || !mode.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'_')
                    })
            })
        {
            return Err(PluginCatalogError::InvalidInput);
        }
    }

    let capabilities = manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_PLUGIN_CAPABILITIES)
        .ok_or(PluginCatalogError::InvalidInput)?;
    let mut ids = BTreeSet::new();
    for capability in capabilities {
        if !capability.is_object() {
            return Err(PluginCatalogError::InvalidInput);
        }
        let id = capability
            .get("id")
            .and_then(Value::as_str)
            .ok_or(PluginCatalogError::InvalidInput)?;
        let version = capability
            .get("version")
            .and_then(Value::as_u64)
            .ok_or(PluginCatalogError::InvalidInput)?;
        let access = capability
            .get("access")
            .and_then(Value::as_str)
            .ok_or(PluginCatalogError::InvalidInput)?;
        let side_effect = capability
            .get("sideEffect")
            .and_then(Value::as_str)
            .ok_or(PluginCatalogError::InvalidInput)?;
        if !valid_capability_id(id)
            || version == 0
            || version > u64::from(u16::MAX)
            || !matches!(access, "read" | "write")
            || !matches!(side_effect, "none" | "external_read" | "external_write")
            || (access == "read" && side_effect == "external_write")
            || !ids.insert(id)
            || capability.get("inputSchema").is_some() != capability.get("outputSchema").is_some()
        {
            return Err(PluginCatalogError::InvalidInput);
        }
        for schema in [
            capability.get("inputSchema"),
            capability.get("outputSchema"),
        ]
        .into_iter()
        .flatten()
        {
            if !schema.is_object() {
                return Err(PluginCatalogError::InvalidInput);
            }
        }
        if side_effect == "external_write" {
            let semantics = capability
                .get("writeSemantics")
                .and_then(Value::as_object)
                .ok_or(PluginCatalogError::InvalidInput)?;
            if !semantics
                .get("nativeIdempotencyKey")
                .is_some_and(Value::is_boolean)
                || !matches!(
                    semantics.get("resultLookup").and_then(Value::as_str),
                    Some("none" | "by_request_id" | "by_external_reference")
                )
                || !semantics
                    .get("reconciliation")
                    .is_some_and(Value::is_boolean)
            {
                return Err(PluginCatalogError::InvalidInput);
            }
        } else if capability.get("writeSemantics").is_some() {
            return Err(PluginCatalogError::InvalidInput);
        }
    }
    Ok(())
}

fn grants_json(grants: &[PluginCapabilityGrant]) -> Value {
    Value::Array(
        grants
            .iter()
            .map(|grant| serde_json::json!({"id": grant.id, "version": grant.version}))
            .collect(),
    )
}

fn parse_grants(value: Value) -> Result<Vec<PluginCapabilityGrant>, PluginCatalogError> {
    let rows = value.as_array().ok_or(PluginCatalogError::Invariant)?;
    let grants = rows
        .iter()
        .map(|row| {
            Ok(PluginCapabilityGrant {
                id: row
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(PluginCatalogError::Invariant)?
                    .to_owned(),
                version: u16::try_from(
                    row.get("version")
                        .and_then(Value::as_u64)
                        .ok_or(PluginCatalogError::Invariant)?,
                )
                .map_err(|_| PluginCatalogError::Invariant)?,
            })
        })
        .collect::<Result<Vec<_>, PluginCatalogError>>()?;
    let mut ids = BTreeSet::new();
    if grants.iter().any(|grant| {
        !valid_capability_id(&grant.id) || grant.version == 0 || !ids.insert(grant.id.as_str())
    }) {
        return Err(PluginCatalogError::Invariant);
    }
    Ok(grants)
}

fn artifact_matches(record: &PluginArtifactRecord, input: &PluginArtifactCreate) -> bool {
    record.plugin_version == input.plugin_version
        && record.digest == input.digest
        && record.manifest == input.manifest
        && record.source_kind == input.source_kind
        && record.source_reference == input.source_reference
        && record.signer_key_id == input.signer_key_id
        && record.signature == input.signature
}

fn valid_plugin_id(value: &str) -> bool {
    valid_namespaced_id(value, false)
}

fn valid_namespaced_id(value: &str, lowercase_prefix: bool) -> bool {
    value.len() <= 128
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            let valid_prefix = if lowercase_prefix {
                segment
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_lowercase)
            } else {
                segment
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
            };
            !segment.is_empty()
                && valid_prefix
                && segment
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_capability_id(value: &str) -> bool {
    valid_namespaced_id(value, true)
}

fn valid_semver(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return false;
    }
    let (without_build, build) = match value.split_once('+') {
        Some((left, right)) if !right.is_empty() && !right.contains('+') => (left, Some(right)),
        Some(_) => return false,
        None => (value, None),
    };
    let (core, prerelease) = match without_build.split_once('-') {
        Some((left, right)) if !right.is_empty() => (left, Some(right)),
        Some(_) => return false,
        None => (without_build, None),
    };
    let numbers = core.split('.').collect::<Vec<_>>();
    if numbers.len() != 3
        || numbers.iter().any(|number| {
            number.is_empty()
                || !number.bytes().all(|byte| byte.is_ascii_digit())
                || (number.len() > 1 && number.starts_with('0'))
        })
    {
        return false;
    }
    valid_semver_identifiers(prerelease.unwrap_or(""), true)
        && valid_semver_identifiers(build.unwrap_or(""), false)
}

fn valid_semver_identifiers(value: &str, reject_leading_zero: bool) -> bool {
    if value.is_empty() {
        return true;
    }
    value.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !(reject_leading_zero
                && part.len() > 1
                && part.starts_with('0')
                && part.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_source_reference(kind: PluginArtifactSourceKind, value: &str) -> bool {
    if kind != PluginArtifactSourceKind::RemoteEndpoint {
        return true;
    }
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn map_db(error: sea_orm::DbErr) -> PluginCatalogError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        PluginCatalogError::Conflict
    } else {
        PluginCatalogError::Unavailable
    }
}
