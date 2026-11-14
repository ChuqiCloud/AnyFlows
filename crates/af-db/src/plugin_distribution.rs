//! Shared plugin archives and expiring per-process runtime observations.
//!
//! These host-only operations never execute plugin code. Archive identity is
//! bound to an artifact ID because equal payload digests may have different manifests.

use std::{future::Future, time::Duration};

use af_domain::{PlatformPermission, UserId};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseTransaction, DbBackend, QueryResult, Set, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Expr, LockType, Order, Query, SelectStatement},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, PlatformAuditOutcome, entity::platform_audit_logs};

pub const MAX_PLUGIN_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
pub const PLUGIN_PACKAGE_CHUNK_BYTES: usize = 48 * 1024;
pub const PLUGIN_NODE_LEASE_SECONDS: i64 = 120;
const MAX_RUNTIME_ROWS: u64 = 16_384;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PluginDistributionError {
    #[error("plugin distribution input is invalid")]
    InvalidInput,
    #[error("plugin distribution record was not found")]
    NotFound,
    #[error("plugin distribution record conflicts with current state")]
    Conflict,
    #[error("plugin archive or runtime state is inconsistent")]
    Invariant,
    #[error("plugin distribution service is unavailable")]
    Unavailable,
    #[error("plugin runtime observation limit was reached")]
    LimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginPackageRecord {
    pub artifact_id: i64,
    pub digest: String,
    pub archive_sha256: String,
    pub total_bytes: usize,
    pub chunk_count: u32,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginRuntimeStatus {
    Pending,
    Downloading,
    Starting,
    Ready,
    Stopped,
    Failed,
    Stale,
}

impl PluginRuntimeStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Stale => "stale",
        }
    }

    fn parse(value: &str) -> Result<Self, PluginDistributionError> {
        match value {
            "pending" => Ok(Self::Pending),
            "downloading" => Ok(Self::Downloading),
            "starting" => Ok(Self::Starting),
            "ready" => Ok(Self::Ready),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "stale" => Ok(Self::Stale),
            _ => Err(PluginDistributionError::Invariant),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginRuntimeErrorCode {
    ArtifactMissing,
    ArchiveInvalid,
    StartupFailed,
    HealthFailed,
    ConfigurationInvalid,
    PublisherUntrusted,
    Unavailable,
    UnsupportedPlatform,
}

impl PluginRuntimeErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactMissing => "artifact_missing",
            Self::ArchiveInvalid => "archive_invalid",
            Self::StartupFailed => "startup_failed",
            Self::HealthFailed => "health_failed",
            Self::ConfigurationInvalid => "configuration_invalid",
            Self::PublisherUntrusted => "publisher_untrusted",
            Self::Unavailable => "unavailable",
            Self::UnsupportedPlatform => "unsupported_platform",
        }
    }

    fn parse(value: &str) -> Result<Self, PluginDistributionError> {
        match value {
            "artifact_missing" => Ok(Self::ArtifactMissing),
            "archive_invalid" => Ok(Self::ArchiveInvalid),
            "startup_failed" => Ok(Self::StartupFailed),
            "health_failed" => Ok(Self::HealthFailed),
            "configuration_invalid" => Ok(Self::ConfigurationInvalid),
            "publisher_untrusted" => Ok(Self::PublisherUntrusted),
            "unavailable" => Ok(Self::Unavailable),
            "unsupported_platform" => Ok(Self::UnsupportedPlatform),
            _ => Err(PluginDistributionError::Invariant),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginRuntimeNode {
    pub node_id: String,
    pub host_label: String,
    pub last_seen_at: i64,
    pub retired_at: Option<i64>,
    pub live: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginRuntimeStateWrite {
    pub node_id: String,
    pub plugin_id: String,
    pub generation: i64,
    pub digest: String,
    pub status: PluginRuntimeStatus,
    pub isolation: String,
    pub error_code: Option<PluginRuntimeErrorCode>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginRuntimeState {
    pub node_id: String,
    pub plugin_id: String,
    pub generation: i64,
    pub digest: String,
    /// Effective status is stale after either the process or observation lease expires.
    pub status: PluginRuntimeStatus,
    pub isolation: String,
    pub error_code: Option<PluginRuntimeErrorCode>,
    pub updated_at: i64,
    pub node_last_seen_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginUninstallRequest {
    pub plugin_id: String,
    pub generation: i64,
    pub requested_by: UserId,
    pub requested_at: i64,
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginArtifactCleanupRequest {
    pub artifact_id: i64,
    pub requested_by: UserId,
    pub requested_at: i64,
    pub request_id: String,
}

#[derive(Clone)]
pub struct PluginDistributionRepository {
    pool: DatabasePool,
}

impl PluginDistributionRepository {
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    /// Check archive availability without materializing its contents.
    pub async fn package_metadata(
        &self,
        artifact_id: i64,
    ) -> Result<Option<PluginPackageRecord>, PluginDistributionError> {
        if artifact_id <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(package_metadata(self.pool.connection(), artifact_id)).await
    }

    /// Publish metadata and every archive chunk atomically after host verification.
    pub async fn store_package(
        &self,
        artifact_id: i64,
        digest: &str,
        archive: &[u8],
        now: i64,
    ) -> Result<PluginPackageRecord, PluginDistributionError> {
        if artifact_id <= 0
            || !valid_digest(digest)
            || archive.is_empty()
            || archive.len() > MAX_PLUGIN_ARCHIVE_BYTES
            || now <= 0
        {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let archive_sha256 = format!("{:x}", Sha256::digest(archive));
            let chunk_count = u32::try_from(archive.len().div_ceil(PLUGIN_PACKAGE_CHUNK_BYTES))
                .map_err(|_| PluginDistributionError::InvalidInput)?;
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let artifact = lock_artifact(&txn, artifact_id).await?;
            let expected_digest: String = artifact.try_get("", "digest").map_err(map_db)?;
            if expected_digest != digest {
                return Err(PluginDistributionError::Conflict);
            }
            if cleanup_requested(&txn, artifact_id).await? {
                return Err(PluginDistributionError::Conflict);
            }
            if let Some(existing) = package_metadata(&txn, artifact_id).await? {
                if existing.digest != digest {
                    return Err(PluginDistributionError::Conflict);
                }
                // The host verified this upload against the immutable manifest and
                // payload. ZIP timestamps/compression are not artifact identity;
                // retain and revalidate the first publication without overwriting it.
                drop(read_archive(&txn, &existing).await?);
                txn.commit().await.map_err(map_db)?;
                return Ok(existing);
            }
            let query = Query::insert()
                .into_table(Alias::new("plugin_packages"))
                .columns(
                    [
                        "artifact_id",
                        "digest",
                        "archive_sha256",
                        "total_bytes",
                        "chunk_count",
                        "created_at",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    artifact_id.into(),
                    digest.to_owned().into(),
                    archive_sha256.clone().into(),
                    i64::try_from(archive.len())
                        .map_err(|_| PluginDistributionError::InvalidInput)?
                        .into(),
                    i32::try_from(chunk_count)
                        .map_err(|_| PluginDistributionError::InvalidInput)?
                        .into(),
                    now.into(),
                ])
                .to_owned();
            txn.execute(txn.get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            // Keep each statement below 1 MiB while avoiding a network round trip per chunk.
            const CHUNKS_PER_INSERT: usize = 16;
            for (batch_index, batch) in archive
                .chunks(PLUGIN_PACKAGE_CHUNK_BYTES * CHUNKS_PER_INSERT)
                .enumerate()
            {
                let mut query = Query::insert();
                query
                    .into_table(Alias::new("plugin_package_chunks"))
                    .columns(["artifact_id", "chunk_index", "content"].map(Alias::new));
                for (local_index, chunk) in batch.chunks(PLUGIN_PACKAGE_CHUNK_BYTES).enumerate() {
                    let index = batch_index * CHUNKS_PER_INSERT + local_index;
                    query.values_panic([
                        artifact_id.into(),
                        i32::try_from(index)
                            .map_err(|_| PluginDistributionError::Invariant)?
                            .into(),
                        chunk.to_vec().into(),
                    ]);
                }
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            }
            txn.commit().await.map_err(map_db)?;
            Ok(PluginPackageRecord {
                artifact_id,
                digest: digest.to_owned(),
                archive_sha256,
                total_bytes: archive.len(),
                chunk_count,
                created_at: now,
            })
        })
        .await
    }

    /// Never return a partial, oversized, reordered or digest-mismatched archive.
    pub async fn load_package(
        &self,
        artifact_id: i64,
    ) -> Result<Option<Vec<u8>>, PluginDistributionError> {
        if artifact_id <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let Some(metadata) = package_metadata(&txn, artifact_id).await? else {
                txn.commit().await.map_err(map_db)?;
                return Ok(None);
            };
            let archive = read_archive(&txn, &metadata).await?;
            txn.commit().await.map_err(map_db)?;
            Ok(Some(archive))
        })
        .await
    }

    /// Explicit reclamation preserves immutable artifact/deployment audit rows.
    /// A live deployment can never lose its shared archive through this port.
    pub async fn remove_package(&self, artifact_id: i64) -> Result<bool, PluginDistributionError> {
        if artifact_id <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            lock_artifact(&txn, artifact_id).await?;
            ensure_no_enabled_deployment(&txn, artifact_id).await?;
            if !cleanup_requested(&txn, artifact_id).await? {
                return Err(PluginDistributionError::Conflict);
            }
            let query = Query::delete()
                .from_table(Alias::new("plugin_packages"))
                .and_where(Expr::col(Alias::new("artifact_id")).eq(artifact_id))
                .to_owned();
            let result = txn
                .execute(txn.get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            txn.commit().await.map_err(map_db)?;
            Ok(result.rows_affected() == 1)
        })
        .await
    }

    pub async fn upsert_node(
        &self,
        node_id: &str,
        host_label: &str,
        now: i64,
    ) -> Result<(), PluginDistributionError> {
        if !valid_node_id(node_id) || !valid_label(host_label) || now <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            if let Some(existing) = lock_node(&txn, node_id).await? {
                let old_label: String = existing.try_get("", "host_label").map_err(map_db)?;
                if old_label != host_label {
                    return Err(PluginDistributionError::Conflict);
                }
                if existing
                    .try_get::<Option<i64>>("", "retired_at")
                    .map_err(map_db)?
                    .is_some()
                {
                    return Err(PluginDistributionError::Conflict);
                }
                let query = Query::update()
                    .table(Alias::new("plugin_runtime_nodes"))
                    .value(Alias::new("updated_at"), now)
                    .and_where(Expr::col(Alias::new("node_id")).eq(node_id))
                    .and_where(Expr::col(Alias::new("updated_at")).lte(now))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            } else {
                let query = Query::insert()
                    .into_table(Alias::new("plugin_runtime_nodes"))
                    .columns(["node_id", "host_label", "updated_at"].map(Alias::new))
                    .values_panic([
                        node_id.to_owned().into(),
                        host_label.to_owned().into(),
                        now.into(),
                    ])
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            }
            // Expired process observations are operational state, not the immutable audit trail.
            // Reclaim them after seven days so process-scoped UUIDs do not grow without bound.
            let prune = Query::delete()
                .from_table(Alias::new("plugin_runtime_nodes"))
                .and_where(
                    Expr::col(Alias::new("updated_at")).lt(now.saturating_sub(7 * 24 * 60 * 60)),
                )
                .and_where(Expr::col(Alias::new("node_id")).ne(node_id))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&prune))
                .await
                .map_err(map_db)?;
            txn.commit().await.map_err(map_db)
        })
        .await
    }

    pub async fn list_nodes(
        &self,
        now: i64,
    ) -> Result<Vec<PluginRuntimeNode>, PluginDistributionError> {
        self.query_nodes(now, false).await
    }

    /// Mark this process deliberately stopped without falsifying its last heartbeat.
    /// Process IDs cannot be resurrected by an older in-flight reconciliation.
    pub async fn retire_node(
        &self,
        node_id: &str,
        now: i64,
    ) -> Result<(), PluginDistributionError> {
        if !valid_node_id(node_id) || now <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            if lock_node(&txn, node_id).await?.is_some() {
                let query = Query::update()
                    .table(Alias::new("plugin_runtime_nodes"))
                    .value(Alias::new("retired_at"), Some(now))
                    .and_where(Expr::col(Alias::new("node_id")).eq(node_id))
                    .and_where(Expr::col(Alias::new("retired_at")).is_null())
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            }
            txn.commit().await.map_err(map_db)
        })
        .await
    }

    pub async fn live_nodes(
        &self,
        now: i64,
    ) -> Result<Vec<PluginRuntimeNode>, PluginDistributionError> {
        self.query_nodes(now, true).await
    }

    async fn query_nodes(
        &self,
        now: i64,
        only_live: bool,
    ) -> Result<Vec<PluginRuntimeNode>, PluginDistributionError> {
        if now <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let cutoff = now.saturating_sub(PLUGIN_NODE_LEASE_SECONDS);
            let mut query = Query::select();
            query
                .columns(["node_id", "host_label", "updated_at", "retired_at"].map(Alias::new))
                .from(Alias::new("plugin_runtime_nodes"));
            if only_live {
                query
                    .and_where(Expr::col(Alias::new("updated_at")).gte(cutoff))
                    .and_where(Expr::col(Alias::new("retired_at")).is_null());
            }
            query
                .order_by(Alias::new("node_id"), Order::Asc)
                .limit(MAX_RUNTIME_ROWS + 1);
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            ensure_row_limit(rows.len())?;
            rows.into_iter()
                .map(|row| {
                    let last_seen_at = row.try_get("", "updated_at").map_err(map_db)?;
                    let retired_at: Option<i64> = row.try_get("", "retired_at").map_err(map_db)?;
                    Ok(PluginRuntimeNode {
                        node_id: row.try_get("", "node_id").map_err(map_db)?,
                        host_label: row.try_get("", "host_label").map_err(map_db)?,
                        last_seen_at,
                        retired_at,
                        live: last_seen_at >= cutoff && retired_at.is_none(),
                    })
                })
                .collect()
        })
        .await
    }

    /// Old process responses cannot overwrite a newer deployment generation.
    pub async fn record_state(
        &self,
        input: PluginRuntimeStateWrite,
    ) -> Result<bool, PluginDistributionError> {
        if !valid_node_id(&input.node_id)
            || !valid_plugin_id(&input.plugin_id)
            || input.generation <= 0
            || !valid_digest(&input.digest)
            || input.updated_at <= 0
            || input.status == PluginRuntimeStatus::Stale
            || !matches!(
                input.isolation.as_str(),
                "process" | "resource_limited" | "sandboxed" | "job_object" | "unknown"
            )
            || (input.status != PluginRuntimeStatus::Failed && input.error_code.is_some())
        {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let node = lock_node(&txn, &input.node_id)
                .await?
                .ok_or(PluginDistributionError::NotFound)?;
            if node
                .try_get::<Option<i64>>("", "retired_at")
                .map_err(map_db)?
                .is_some()
            {
                return Err(PluginDistributionError::Conflict);
            }
            let lookup = Query::select()
                .columns(["generation", "digest", "updated_at"].map(Alias::new))
                .from(Alias::new("plugin_runtime_states"))
                .and_where(Expr::col(Alias::new("node_id")).eq(&input.node_id))
                .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id))
                .to_owned();
            if let Some(existing) = txn
                .query_one(txn.get_database_backend().build(&lookup))
                .await
                .map_err(map_db)?
            {
                let old_generation: i64 = existing.try_get("", "generation").map_err(map_db)?;
                let old_time: i64 = existing.try_get("", "updated_at").map_err(map_db)?;
                let old_digest: String = existing.try_get("", "digest").map_err(map_db)?;
                if old_generation > input.generation
                    || (old_generation == input.generation && old_time > input.updated_at)
                {
                    txn.commit().await.map_err(map_db)?;
                    return Ok(false);
                }
                if old_generation == input.generation && old_digest != input.digest {
                    return Err(PluginDistributionError::Conflict);
                }
                let query = Query::update()
                    .table(Alias::new("plugin_runtime_states"))
                    .values([
                        (Alias::new("generation"), input.generation.into()),
                        (Alias::new("digest"), input.digest.clone().into()),
                        (Alias::new("status"), input.status.as_str().into()),
                        (Alias::new("isolation"), input.isolation.clone().into()),
                        (
                            Alias::new("error_code"),
                            input.error_code.map(|code| code.as_str().to_owned()).into(),
                        ),
                        (Alias::new("updated_at"), input.updated_at.into()),
                    ])
                    .and_where(Expr::col(Alias::new("node_id")).eq(&input.node_id))
                    .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            } else {
                let query = Query::insert()
                    .into_table(Alias::new("plugin_runtime_states"))
                    .columns(
                        [
                            "node_id",
                            "plugin_id",
                            "generation",
                            "digest",
                            "status",
                            "isolation",
                            "error_code",
                            "updated_at",
                        ]
                        .map(Alias::new),
                    )
                    .values_panic([
                        input.node_id.into(),
                        input.plugin_id.into(),
                        input.generation.into(),
                        input.digest.into(),
                        input.status.as_str().into(),
                        input.isolation.into(),
                        input.error_code.map(|code| code.as_str().to_owned()).into(),
                        input.updated_at.into(),
                    ])
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&query))
                    .await
                    .map_err(map_db)?;
            }
            txn.commit().await.map_err(map_db)?;
            Ok(true)
        })
        .await
    }

    pub async fn list_states(
        &self,
        plugin_id: Option<&str>,
        now: i64,
    ) -> Result<Vec<PluginRuntimeState>, PluginDistributionError> {
        if plugin_id.is_some_and(|id| !valid_plugin_id(id)) || now <= 0 {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let mut query = Query::select();
            query
                .columns(
                    [
                        "node_id",
                        "plugin_id",
                        "generation",
                        "digest",
                        "status",
                        "isolation",
                        "error_code",
                        "updated_at",
                    ]
                    .map(|column| (Alias::new("plugin_runtime_states"), Alias::new(column))),
                )
                .expr_as(
                    Expr::col((Alias::new("plugin_runtime_nodes"), Alias::new("updated_at"))),
                    Alias::new("node_last_seen_at"),
                )
                .expr_as(
                    Expr::col((Alias::new("plugin_runtime_nodes"), Alias::new("retired_at"))),
                    Alias::new("node_retired_at"),
                )
                .from(Alias::new("plugin_runtime_states"))
                .inner_join(
                    Alias::new("plugin_runtime_nodes"),
                    Expr::col((Alias::new("plugin_runtime_states"), Alias::new("node_id")))
                        .equals((Alias::new("plugin_runtime_nodes"), Alias::new("node_id"))),
                );
            if let Some(plugin_id) = plugin_id {
                query.and_where(
                    Expr::col((Alias::new("plugin_runtime_states"), Alias::new("plugin_id")))
                        .eq(plugin_id),
                );
            }
            query
                .order_by(
                    (Alias::new("plugin_runtime_states"), Alias::new("plugin_id")),
                    Order::Asc,
                )
                .order_by(
                    (Alias::new("plugin_runtime_states"), Alias::new("node_id")),
                    Order::Asc,
                )
                .limit(MAX_RUNTIME_ROWS + 1);
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            ensure_row_limit(rows.len())?;
            let cutoff = now.saturating_sub(PLUGIN_NODE_LEASE_SECONDS);
            rows.into_iter()
                .map(|row| {
                    let node_id: String = row.try_get("", "node_id").map_err(map_db)?;
                    let node_last_seen_at = row.try_get("", "node_last_seen_at").map_err(map_db)?;
                    let node_retired_at: Option<i64> =
                        row.try_get("", "node_retired_at").map_err(map_db)?;
                    let node_is_live = node_last_seen_at >= cutoff && node_retired_at.is_none();
                    let updated_at = row.try_get("", "updated_at").map_err(map_db)?;
                    let status: String = row.try_get("", "status").map_err(map_db)?;
                    let stored_status = PluginRuntimeStatus::parse(&status)?;
                    let error_code = row
                        .try_get::<Option<String>>("", "error_code")
                        .map_err(map_db)?
                        .map(|value| PluginRuntimeErrorCode::parse(&value))
                        .transpose()?;
                    Ok(PluginRuntimeState {
                        node_id,
                        plugin_id: row.try_get("", "plugin_id").map_err(map_db)?,
                        generation: row.try_get("", "generation").map_err(map_db)?,
                        digest: row.try_get("", "digest").map_err(map_db)?,
                        status: if updated_at < cutoff || !node_is_live {
                            PluginRuntimeStatus::Stale
                        } else {
                            stored_status
                        },
                        isolation: row.try_get("", "isolation").map_err(map_db)?,
                        error_code,
                        updated_at,
                        node_last_seen_at,
                    })
                })
                .collect()
        })
        .await
    }

    /// Persist uninstall intent only for the exact disabled deployment generation.
    /// Runtime cleanup happens independently on every node; history and archives remain.
    pub async fn record_uninstall(
        &self,
        input: PluginUninstallRequest,
    ) -> Result<PluginUninstallRequest, PluginDistributionError> {
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            let output = write_uninstall_in_transaction(&txn, input).await?;
            txn.commit().await.map_err(map_db)?;
            Ok(output)
        })
        .await
    }

    pub async fn list_uninstall_requests(
        &self,
    ) -> Result<Vec<PluginUninstallRequest>, PluginDistributionError> {
        bounded(async {
            let query = Query::select()
                .columns(
                    [
                        "plugin_id",
                        "generation",
                        "requested_by",
                        "requested_at",
                        "request_id",
                    ]
                    .map(Alias::new),
                )
                .from(Alias::new("plugin_uninstall_requests"))
                .order_by(Alias::new("plugin_id"), Order::Asc)
                .limit(MAX_RUNTIME_ROWS + 1)
                .to_owned();
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            ensure_row_limit(rows.len())?;
            rows.into_iter().map(uninstall_from_row).collect()
        })
        .await
    }

    /// Cleanup must be explicit, revoked and unreferenced by enabled deployments.
    pub async fn record_cleanup_request(
        &self,
        input: PluginArtifactCleanupRequest,
    ) -> Result<PluginArtifactCleanupRequest, PluginDistributionError> {
        if input.artifact_id <= 0 || input.requested_at <= 0 || !valid_request_id(&input.request_id)
        {
            return Err(PluginDistributionError::InvalidInput);
        }
        bounded(async {
            let txn = self.pool.connection().begin().await.map_err(map_db)?;
            lock_artifact(&txn, input.artifact_id).await?;
            ensure_no_enabled_deployment(&txn, input.artifact_id).await?;
            let revoked = Query::select()
                .column(Alias::new("artifact_id"))
                .from(Alias::new("plugin_artifact_revocations"))
                .and_where(Expr::col(Alias::new("artifact_id")).eq(input.artifact_id))
                .to_owned();
            if txn
                .query_one(txn.get_database_backend().build(&revoked))
                .await
                .map_err(map_db)?
                .is_none()
            {
                return Err(PluginDistributionError::Conflict);
            }
            let existing = Query::select()
                .columns(
                    ["artifact_id", "requested_by", "requested_at", "request_id"].map(Alias::new),
                )
                .from(Alias::new("plugin_artifact_cleanup_requests"))
                .and_where(Expr::col(Alias::new("artifact_id")).eq(input.artifact_id))
                .to_owned();
            if let Some(row) = txn
                .query_one(txn.get_database_backend().build(&existing))
                .await
                .map_err(map_db)?
            {
                let old = cleanup_from_row(row)?;
                if old.requested_by == input.requested_by && old.request_id == input.request_id {
                    txn.commit().await.map_err(map_db)?;
                    return Ok(old);
                }
                return Err(PluginDistributionError::Conflict);
            }
            let query = Query::insert()
                .into_table(Alias::new("plugin_artifact_cleanup_requests"))
                .columns(
                    ["artifact_id", "requested_by", "requested_at", "request_id"].map(Alias::new),
                )
                .values_panic([
                    input.artifact_id.into(),
                    input.requested_by.get().into(),
                    input.requested_at.into(),
                    input.request_id.clone().into(),
                ])
                .to_owned();
            txn.execute(txn.get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            write_audit(
                &txn,
                input.requested_by,
                &input.request_id,
                "catalog_cleanup",
                "plugin_artifact",
                input.artifact_id.to_string(),
                serde_json::json!({"artifactId":input.artifact_id}),
            )
            .await?;
            txn.commit().await.map_err(map_db)?;
            Ok(input)
        })
        .await
    }

    pub async fn list_cleanup_requests(
        &self,
    ) -> Result<Vec<PluginArtifactCleanupRequest>, PluginDistributionError> {
        bounded(async {
            let query = Query::select()
                .columns(
                    ["artifact_id", "requested_by", "requested_at", "request_id"].map(Alias::new),
                )
                .from(Alias::new("plugin_artifact_cleanup_requests"))
                .order_by(Alias::new("artifact_id"), Order::Asc)
                .limit(MAX_RUNTIME_ROWS + 1)
                .to_owned();
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&query))
                .await
                .map_err(map_db)?;
            ensure_row_limit(rows.len())?;
            rows.into_iter().map(cleanup_from_row).collect()
        })
        .await
    }
}

pub(crate) async fn write_uninstall_in_transaction(
    txn: &DatabaseTransaction,
    input: PluginUninstallRequest,
) -> Result<PluginUninstallRequest, PluginDistributionError> {
    if !valid_plugin_id(&input.plugin_id)
        || input.generation <= 0
        || input.requested_at <= 0
        || !valid_request_id(&input.request_id)
    {
        return Err(PluginDistributionError::InvalidInput);
    }
    let mut query = Query::select();
    query
        .columns(["generation", "enabled"].map(Alias::new))
        .from(Alias::new("plugin_deployments"))
        .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id));
    lock_select(txn, &mut query);
    let deployment = txn
        .query_one(txn.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .ok_or(PluginDistributionError::NotFound)?;
    if deployment
        .try_get::<i64>("", "generation")
        .map_err(map_db)?
        != input.generation
        || deployment.try_get::<bool>("", "enabled").map_err(map_db)?
    {
        return Err(PluginDistributionError::Conflict);
    }
    let existing = Query::select()
        .columns(
            [
                "plugin_id",
                "generation",
                "requested_by",
                "requested_at",
                "request_id",
            ]
            .map(Alias::new),
        )
        .from(Alias::new("plugin_uninstall_requests"))
        .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id))
        .to_owned();
    if let Some(row) = txn
        .query_one(txn.get_database_backend().build(&existing))
        .await
        .map_err(map_db)?
    {
        let old = uninstall_from_row(row)?;
        if old.generation == input.generation
            && old.requested_by == input.requested_by
            && old.request_id == input.request_id
        {
            return Ok(old);
        }
        if old.generation >= input.generation {
            return Err(PluginDistributionError::Conflict);
        }
        let query = Query::update()
            .table(Alias::new("plugin_uninstall_requests"))
            .values([
                (Alias::new("generation"), input.generation.into()),
                (Alias::new("requested_by"), input.requested_by.get().into()),
                (Alias::new("requested_at"), input.requested_at.into()),
                (Alias::new("request_id"), input.request_id.clone().into()),
            ])
            .and_where(Expr::col(Alias::new("plugin_id")).eq(&input.plugin_id))
            .to_owned();
        txn.execute(txn.get_database_backend().build(&query))
            .await
            .map_err(map_db)?;
    } else {
        let query = Query::insert()
            .into_table(Alias::new("plugin_uninstall_requests"))
            .columns(
                [
                    "plugin_id",
                    "generation",
                    "requested_by",
                    "requested_at",
                    "request_id",
                ]
                .map(Alias::new),
            )
            .values_panic([
                input.plugin_id.clone().into(),
                input.generation.into(),
                input.requested_by.get().into(),
                input.requested_at.into(),
                input.request_id.clone().into(),
            ])
            .to_owned();
        txn.execute(txn.get_database_backend().build(&query))
            .await
            .map_err(map_db)?;
    }
    write_audit(
        txn,
        input.requested_by,
        &input.request_id,
        "catalog_uninstall",
        "plugin",
        input.plugin_id.clone(),
        serde_json::json!({"generation":input.generation}),
    )
    .await?;
    Ok(input)
}

async fn package_metadata<C: ConnectionTrait>(
    connection: &C,
    artifact_id: i64,
) -> Result<Option<PluginPackageRecord>, PluginDistributionError> {
    let query = Query::select()
        .columns(
            [
                "artifact_id",
                "digest",
                "archive_sha256",
                "total_bytes",
                "chunk_count",
                "created_at",
            ]
            .map(Alias::new),
        )
        .from(Alias::new("plugin_packages"))
        .and_where(Expr::col(Alias::new("artifact_id")).eq(artifact_id))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .map(|row| {
            let total_bytes =
                usize::try_from(row.try_get::<i64>("", "total_bytes").map_err(map_db)?)
                    .map_err(|_| PluginDistributionError::Invariant)?;
            let chunk_count = u32::try_from(row.try_get::<i32>("", "chunk_count").map_err(map_db)?)
                .map_err(|_| PluginDistributionError::Invariant)?;
            let digest: String = row.try_get("", "digest").map_err(map_db)?;
            let archive_sha256: String = row.try_get("", "archive_sha256").map_err(map_db)?;
            if total_bytes == 0
                || total_bytes > MAX_PLUGIN_ARCHIVE_BYTES
                || !valid_digest(&digest)
                || !valid_digest(&archive_sha256)
                || usize::try_from(chunk_count).map_err(|_| PluginDistributionError::Invariant)?
                    != total_bytes.div_ceil(PLUGIN_PACKAGE_CHUNK_BYTES)
            {
                return Err(PluginDistributionError::Invariant);
            }
            Ok(PluginPackageRecord {
                artifact_id: row.try_get("", "artifact_id").map_err(map_db)?,
                digest,
                archive_sha256,
                total_bytes,
                chunk_count,
                created_at: row.try_get("", "created_at").map_err(map_db)?,
            })
        })
        .transpose()
}

async fn read_archive(
    connection: &DatabaseTransaction,
    metadata: &PluginPackageRecord,
) -> Result<Vec<u8>, PluginDistributionError> {
    let query = Query::select()
        .columns(["chunk_index", "content"].map(Alias::new))
        .from(Alias::new("plugin_package_chunks"))
        .and_where(Expr::col(Alias::new("artifact_id")).eq(metadata.artifact_id))
        .order_by(Alias::new("chunk_index"), Order::Asc)
        .limit(u64::from(metadata.chunk_count) + 1)
        .to_owned();
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?;
    if rows.len() != metadata.chunk_count as usize {
        return Err(PluginDistributionError::Invariant);
    }
    let mut archive = Vec::with_capacity(metadata.total_bytes);
    for (index, row) in rows.into_iter().enumerate() {
        let chunk_index = usize::try_from(row.try_get::<i32>("", "chunk_index").map_err(map_db)?)
            .map_err(|_| PluginDistributionError::Invariant)?;
        let content: Vec<u8> = row.try_get("", "content").map_err(map_db)?;
        let expected_size = (metadata.total_bytes - archive.len()).min(PLUGIN_PACKAGE_CHUNK_BYTES);
        if chunk_index != index || content.len() != expected_size {
            return Err(PluginDistributionError::Invariant);
        }
        archive.extend_from_slice(&content);
    }
    if archive.len() != metadata.total_bytes
        || format!("{:x}", Sha256::digest(&archive)) != metadata.archive_sha256
    {
        return Err(PluginDistributionError::Invariant);
    }
    Ok(archive)
}

async fn lock_artifact(
    connection: &DatabaseTransaction,
    artifact_id: i64,
) -> Result<QueryResult, PluginDistributionError> {
    let mut query = Query::select();
    query
        .columns(["id", "digest"].map(Alias::new))
        .from(Alias::new("plugin_artifacts"))
        .and_where(Expr::col(Alias::new("id")).eq(artifact_id));
    lock_select(connection, &mut query);
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .ok_or(PluginDistributionError::NotFound)
}

async fn lock_node(
    connection: &DatabaseTransaction,
    node_id: &str,
) -> Result<Option<QueryResult>, PluginDistributionError> {
    let mut query = Query::select();
    query
        .columns(["node_id", "host_label", "updated_at", "retired_at"].map(Alias::new))
        .from(Alias::new("plugin_runtime_nodes"))
        .and_where(Expr::col(Alias::new("node_id")).eq(node_id));
    lock_select(connection, &mut query);
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)
}

fn lock_select(connection: &DatabaseTransaction, query: &mut SelectStatement) {
    if connection.get_database_backend() != DbBackend::Sqlite {
        query.lock(LockType::Update);
    }
}

async fn ensure_no_enabled_deployment(
    connection: &DatabaseTransaction,
    artifact_id: i64,
) -> Result<(), PluginDistributionError> {
    let query = Query::select()
        .column(Alias::new("plugin_id"))
        .from(Alias::new("plugin_deployments"))
        .and_where(Expr::col(Alias::new("artifact_id")).eq(artifact_id))
        .and_where(Expr::col(Alias::new("enabled")).eq(true))
        .limit(1)
        .to_owned();
    if connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .is_some()
    {
        return Err(PluginDistributionError::Conflict);
    }
    Ok(())
}

async fn cleanup_requested(
    connection: &DatabaseTransaction,
    artifact_id: i64,
) -> Result<bool, PluginDistributionError> {
    let query = Query::select()
        .column(Alias::new("artifact_id"))
        .from(Alias::new("plugin_artifact_cleanup_requests"))
        .and_where(Expr::col(Alias::new("artifact_id")).eq(artifact_id))
        .to_owned();
    Ok(connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(map_db)?
        .is_some())
}

fn uninstall_from_row(row: QueryResult) -> Result<PluginUninstallRequest, PluginDistributionError> {
    Ok(PluginUninstallRequest {
        plugin_id: row.try_get("", "plugin_id").map_err(map_db)?,
        generation: row.try_get("", "generation").map_err(map_db)?,
        requested_by: UserId::new(row.try_get("", "requested_by").map_err(map_db)?)
            .map_err(|_| PluginDistributionError::Invariant)?,
        requested_at: row.try_get("", "requested_at").map_err(map_db)?,
        request_id: row.try_get("", "request_id").map_err(map_db)?,
    })
}

fn cleanup_from_row(
    row: QueryResult,
) -> Result<PluginArtifactCleanupRequest, PluginDistributionError> {
    Ok(PluginArtifactCleanupRequest {
        artifact_id: row.try_get("", "artifact_id").map_err(map_db)?,
        requested_by: UserId::new(row.try_get("", "requested_by").map_err(map_db)?)
            .map_err(|_| PluginDistributionError::Invariant)?,
        requested_at: row.try_get("", "requested_at").map_err(map_db)?,
        request_id: row.try_get("", "request_id").map_err(map_db)?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn write_audit(
    connection: &DatabaseTransaction,
    operator: UserId,
    request_id: &str,
    operation: &str,
    resource: &str,
    resource_id: String,
    after: serde_json::Value,
) -> Result<(), PluginDistributionError> {
    platform_audit_logs::ActiveModel {
        operator_user_id: Set(operator.get()),
        actor_credential_kind: Set(None),
        actor_credential_id: Set(None),
        permission_code: Set(PlatformPermission::PluginsManage.code().to_owned()),
        route: Set("/admin/plugins/distribution".to_owned()),
        operation: Set(operation.to_owned()),
        resource: Set(resource.to_owned()),
        resource_id: Set(Some(resource_id)),
        outcome: Set(PlatformAuditOutcome::Succeeded as i16),
        before_value: Set(None),
        after_value: Set(Some(
            serde_json::to_string(&after).map_err(|_| PluginDistributionError::Invariant)?,
        )),
        audit_info: Set(None),
        request_id: Set(request_id.to_owned()),
        created_at: Set(time::OffsetDateTime::now_utc()),
        ..Default::default()
    }
    .insert(connection)
    .await
    .map_err(map_db)?;
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_node_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn valid_plugin_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && part.as_bytes()[0].is_ascii_alphanumeric()
                && part.as_bytes()[part.len() - 1].is_ascii_alphanumeric()
        })
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn ensure_row_limit(count: usize) -> Result<(), PluginDistributionError> {
    if count > MAX_RUNTIME_ROWS as usize {
        Err(PluginDistributionError::LimitReached)
    } else {
        Ok(())
    }
}

fn map_db(error: sea_orm::DbErr) -> PluginDistributionError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        PluginDistributionError::Conflict
    } else {
        PluginDistributionError::Unavailable
    }
}

async fn bounded<T>(
    operation: impl Future<Output = Result<T, PluginDistributionError>>,
) -> Result<T, PluginDistributionError> {
    timeout(OPERATION_TIMEOUT, operation)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| PluginDistributionError::Unavailable)?
}
