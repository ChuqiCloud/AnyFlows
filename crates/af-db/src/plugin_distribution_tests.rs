use std::error::Error;

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set,
    sea_query::{Alias, Expr, Query},
};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PLUGIN_NODE_LEASE_SECONDS,
    PLUGIN_PACKAGE_CHUNK_BYTES, PluginArtifactApproval, PluginArtifactCleanupRequest,
    PluginArtifactCreate, PluginArtifactRevocation, PluginArtifactSourceKind,
    PluginCapabilityGrant, PluginCatalogRepository, PluginDeploymentWrite, PluginDistributionError,
    PluginDistributionRepository, PluginRuntimeErrorCode, PluginRuntimeStateWrite,
    PluginRuntimeStatus, PluginUninstallRequest,
    entity::{groups, platform_audit_logs, users},
};

const NOW: i64 = 1_800_000_000;
const PLUGIN: &str = "org.example.distribution";

struct Fixture {
    pool: DatabasePool,
    catalog: PluginCatalogRepository,
    distribution: PluginDistributionRepository,
    operator: UserId,
    artifact_id: i64,
    digest: String,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = groups::ActiveModel {
            name: Set("plugin-distribution".to_owned()),
            display_name: Set("Plugin distribution".to_owned()),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let user = users::ActiveModel {
            username: Set("plugin-distribution-operator".to_owned()),
            role: Set(1),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set("plugin-distribution-aff".to_owned()),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let catalog = PluginCatalogRepository::new(pool.clone());
        let artifact = catalog
            .register_artifact(artifact(PLUGIN, "1.0.0", "ab".repeat(32)))
            .await?;
        Ok(Self {
            distribution: PluginDistributionRepository::new(pool.clone()),
            catalog,
            pool,
            operator: UserId::new(user.id)?,
            artifact_id: artifact.id,
            digest: artifact.digest,
        })
    }

    async fn deploy(&self, enabled: bool, expected: Option<i64>) -> Result<i64, Box<dyn Error>> {
        if expected.is_none() {
            self.catalog
                .approve_artifact(PluginArtifactApproval {
                    artifact_id: self.artifact_id,
                    approved_by: self.operator,
                    approved_at: NOW,
                    request_id: "approve-distribution".to_owned(),
                })
                .await?;
        }
        let deployment = self
            .catalog
            .deploy(PluginDeploymentWrite {
                plugin_id: PLUGIN.to_owned(),
                artifact_digest: self.digest.clone(),
                expected_generation: expected,
                enabled,
                granted_capabilities: vec![PluginCapabilityGrant {
                    id: "catalog.models.read".to_owned(),
                    version: 1,
                }],
                config: serde_json::json!({}),
                updated_by: self.operator,
                updated_at: NOW,
                request_id: format!("deploy-distribution-{}", expected.unwrap_or_default()),
            })
            .await?;
        Ok(deployment.generation)
    }

    async fn close(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await?;
        Ok(())
    }
}

fn artifact(plugin_id: &str, version: &str, digest: String) -> PluginArtifactCreate {
    PluginArtifactCreate {
        plugin_id: plugin_id.to_owned(),
        plugin_version: version.to_owned(),
        digest,
        manifest: serde_json::json!({
            "pluginId":plugin_id,"pluginVersion":version,
            "configSchema":{"type":"object","additionalProperties":false},
            "contractRange":{"minimum":{"major":1,"minor":0},"maximum":{"major":1,"minor":2}},
            "authenticationModes":["none"],
            "limits":{"maxConcurrency":1,"requestTimeoutMs":1000,"maxResponseBytes":4096,"maxPageSize":20},
            "capabilities":[{"id":"catalog.models.read","version":1,"access":"read","sideEffect":"none"}]
        }),
        source_kind: PluginArtifactSourceKind::LocalProcess,
        source_reference: "distribution-fixture".to_owned(),
        signer_key_id: String::new(),
        signature: String::new(),
        created_at: NOW,
    }
}

fn state(node_id: &str, generation: i64, now: i64) -> PluginRuntimeStateWrite {
    PluginRuntimeStateWrite {
        node_id: node_id.to_owned(),
        plugin_id: PLUGIN.to_owned(),
        generation,
        digest: "ab".repeat(32),
        status: PluginRuntimeStatus::Ready,
        isolation: "process".to_owned(),
        error_code: None,
        updated_at: now,
    }
}

fn uninstall_command(fixture: &Fixture, generation: i64) -> PluginDeploymentWrite {
    PluginDeploymentWrite {
        plugin_id: PLUGIN.to_owned(),
        artifact_digest: fixture.digest.clone(),
        expected_generation: Some(generation),
        enabled: false,
        granted_capabilities: vec![PluginCapabilityGrant {
            id: "catalog.models.read".to_owned(),
            version: 1,
        }],
        config: serde_json::json!({}),
        updated_by: fixture.operator,
        updated_at: NOW,
        request_id: "atomic-plugin-uninstall".to_owned(),
    }
}

/// Exercises real binary/state/transaction DML in each isolated migration database.
/// Keep the caller's pool open and preserve immutable history for later inspection.
pub(crate) async fn check_distribution_storage(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let mut entropy = [0_u8; 8];
    getrandom::fill(&mut entropy).map_err(|_| "plugin smoke entropy unavailable")?;
    let suffix = format!("{:016x}", u64::from_be_bytes(entropy));
    // Earlier upgrade checks insert explicit IDs without advancing PostgreSQL
    // sequences; keep this fixture independent of their sequence positions.
    let fixture_id = 9_900_000_000_i64
        + i64::from(u32::from_be_bytes([
            entropy[0], entropy[1], entropy[2], entropy[3],
        ]));
    let plugin_id = format!("org.example.smoke-{suffix}");
    let node_id = format!("plugin-smoke-{suffix}");
    let peer_id = format!("plugin-smoke-peer-{suffix}");
    let group = groups::ActiveModel {
        id: Set(fixture_id),
        name: Set(format!("plugin-smoke-{suffix}")),
        display_name: Set("Plugin migration smoke".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        id: Set(fixture_id),
        username: Set(format!("plugin-smoke-{suffix}")),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set(format!("plugin-smoke-{suffix}")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let operator = UserId::new(user.id)?;
    let catalog = PluginCatalogRepository::new(pool.clone());
    let distribution = PluginDistributionRepository::new(pool.clone());
    let artifact = catalog
        .register_artifact(artifact(&plugin_id, "1.0.0", "cd".repeat(32)))
        .await?;
    catalog
        .approve_artifact(PluginArtifactApproval {
            artifact_id: artifact.id,
            approved_by: operator,
            approved_at: NOW,
            request_id: format!("{suffix}-approve"),
        })
        .await?;

    // Includes NUL and 0xff on both sides of the 48 KiB BLOB boundary.
    let mut archive = (0..PLUGIN_PACKAGE_CHUNK_BYTES + 7)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    archive[PLUGIN_PACKAGE_CHUNK_BYTES + 6] = 0xff;
    let metadata = distribution
        .store_package(artifact.id, &artifact.digest, &archive, NOW)
        .await?;
    assert_eq!(metadata.artifact_id, artifact.id);
    assert_eq!(metadata.digest, artifact.digest);
    assert_eq!(metadata.total_bytes, archive.len());
    assert_eq!(metadata.chunk_count, 2);
    assert_eq!(metadata.created_at, NOW);
    assert_eq!(
        distribution.package_metadata(artifact.id).await?,
        Some(metadata.clone())
    );
    // Independent repository reads through the shared pool, as a second node does.
    let peer = PluginDistributionRepository::new(pool.clone());
    assert_eq!(peer.load_package(artifact.id).await?, Some(archive.clone()));
    assert_eq!(
        distribution
            .store_package(artifact.id, &artifact.digest, b"repacked", NOW + 1)
            .await?,
        metadata
    );
    assert_eq!(peer.load_package(artifact.id).await?, Some(archive.clone()));

    let command = PluginDeploymentWrite {
        plugin_id: plugin_id.clone(),
        artifact_digest: artifact.digest.clone(),
        expected_generation: None,
        enabled: true,
        granted_capabilities: vec![PluginCapabilityGrant {
            id: "catalog.models.read".to_owned(),
            version: 1,
        }],
        config: serde_json::json!({}),
        updated_by: operator,
        updated_at: NOW,
        request_id: format!("{suffix}-deploy"),
    };
    let enabled = catalog.deploy(command.clone()).await?;
    assert!(enabled.enabled);
    assert_eq!(enabled.generation, 1);
    assert_eq!(enabled.updated_at, NOW);
    distribution.upsert_node(&node_id, "worker-a", NOW).await?;
    distribution
        .upsert_node(&peer_id, "worker-b", NOW + 1)
        .await?;
    let ready = PluginRuntimeStateWrite {
        node_id: node_id.clone(),
        plugin_id: plugin_id.clone(),
        generation: enabled.generation,
        digest: artifact.digest.clone(),
        status: PluginRuntimeStatus::Ready,
        isolation: "resource_limited".to_owned(),
        error_code: None,
        updated_at: NOW + 2,
    };
    assert!(distribution.record_state(ready.clone()).await?);
    let mut failed = ready.clone();
    failed.node_id = peer_id.clone();
    failed.status = PluginRuntimeStatus::Failed;
    failed.error_code = Some(PluginRuntimeErrorCode::StartupFailed);
    failed.updated_at = NOW + 3;
    assert!(peer.record_state(failed).await?);
    let states = distribution.list_states(Some(&plugin_id), NOW + 3).await?;
    assert_eq!(states.len(), 2);
    let first = states
        .iter()
        .find(|state| state.node_id == node_id)
        .unwrap();
    assert_eq!(first.status, PluginRuntimeStatus::Ready);
    assert_eq!(first.isolation, "resource_limited");
    assert_eq!(first.error_code, None);
    assert_eq!(first.updated_at, NOW + 2);
    assert_eq!(first.node_last_seen_at, NOW);
    let second = states
        .iter()
        .find(|state| state.node_id == peer_id)
        .unwrap();
    assert_eq!(second.status, PluginRuntimeStatus::Failed);
    assert_eq!(
        second.error_code,
        Some(PluginRuntimeErrorCode::StartupFailed)
    );
    assert_eq!(second.node_last_seen_at, NOW + 1);
    let nodes = distribution.list_nodes(NOW + 3).await?;
    let first_node = nodes.iter().find(|node| node.node_id == node_id).unwrap();
    assert!(first_node.live);
    assert_eq!(first_node.host_label, "worker-a");
    assert_eq!(first_node.last_seen_at, NOW);
    assert_eq!(first_node.retired_at, None);

    let mut uninstall = command;
    uninstall.expected_generation = Some(enabled.generation);
    uninstall.enabled = false;
    uninstall.updated_at = NOW + 4;
    uninstall.request_id = format!("{suffix}-uninstall");
    // A reused intent request ID fails after the revision/pointer writes. Verify
    // their transaction rolls back on every backend without backend-specific DDL.
    let blocking_plugin = format!("org.example.blocked-{suffix}");
    let insert = Query::insert()
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
            blocking_plugin.clone().into(),
            1_i64.into(),
            operator.get().into(),
            NOW.into(),
            uninstall.request_id.clone().into(),
        ])
        .to_owned();
    let backend = pool.connection().get_database_backend();
    pool.connection().execute(backend.build(&insert)).await?;
    assert!(catalog.uninstall(uninstall.clone()).await.is_err());
    let current = catalog.get_deployment(&plugin_id).await?.unwrap();
    assert!(current.enabled);
    assert_eq!(current.generation, enabled.generation);
    assert_eq!(catalog.deployment_history(&plugin_id, 10).await?.len(), 1);
    assert!(
        !distribution
            .list_uninstall_requests()
            .await?
            .iter()
            .any(|request| request.plugin_id == plugin_id)
    );
    assert!(
        platform_audit_logs::Entity::find()
            .filter(platform_audit_logs::Column::RequestId.eq(&uninstall.request_id))
            .all(pool.connection())
            .await?
            .is_empty()
    );
    let delete = Query::delete()
        .from_table(Alias::new("plugin_uninstall_requests"))
        .and_where(Expr::col(Alias::new("plugin_id")).eq(blocking_plugin))
        .to_owned();
    pool.connection().execute(backend.build(&delete)).await?;
    let disabled = catalog.uninstall(uninstall.clone()).await?;
    assert!(!disabled.enabled);
    assert_eq!(disabled.generation, enabled.generation + 1);
    assert_eq!(disabled.updated_at, NOW + 4);
    assert_eq!(catalog.uninstall(uninstall.clone()).await?, disabled);
    let requests = distribution.list_uninstall_requests().await?;
    let request = requests
        .iter()
        .find(|request| request.plugin_id == plugin_id)
        .unwrap();
    assert_eq!(request.generation, disabled.generation);
    assert_eq!(request.requested_by, operator);
    assert_eq!(request.requested_at, NOW + 4);
    assert_eq!(request.request_id, uninstall.request_id);
    let audit = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::RequestId.eq(&uninstall.request_id))
        .filter(platform_audit_logs::Column::Operation.eq("catalog_uninstall"))
        .all(pool.connection())
        .await?;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].operator_user_id, operator.get());

    let mut stopped = ready.clone();
    stopped.generation = disabled.generation;
    stopped.status = PluginRuntimeStatus::Stopped;
    stopped.updated_at = NOW + 5;
    assert!(distribution.record_state(stopped).await?);
    assert!(!distribution.record_state(ready).await?);
    distribution.retire_node(&node_id, NOW + 6).await?;
    distribution.retire_node(&peer_id, NOW + 6).await?;
    let nodes = distribution.list_nodes(NOW + 6).await?;
    let retired = nodes.iter().find(|node| node.node_id == node_id).unwrap();
    assert!(!retired.live);
    assert_eq!(retired.last_seen_at, NOW);
    assert_eq!(retired.retired_at, Some(NOW + 6));
    assert!(
        distribution
            .list_states(Some(&plugin_id), NOW + 6)
            .await?
            .iter()
            .all(|state| state.status == PluginRuntimeStatus::Stale)
    );
    catalog
        .revoke_artifact(PluginArtifactRevocation {
            artifact_id: artifact.id,
            revoked_by: operator,
            revoked_at: NOW + 7,
            reason: "migration smoke cleanup".to_owned(),
            request_id: format!("{suffix}-revoke"),
        })
        .await?;
    let cleanup = PluginArtifactCleanupRequest {
        artifact_id: artifact.id,
        requested_by: operator,
        requested_at: NOW + 8,
        request_id: format!("{suffix}-cleanup"),
    };
    assert_eq!(
        distribution.record_cleanup_request(cleanup.clone()).await?,
        cleanup
    );
    assert!(
        distribution
            .list_cleanup_requests()
            .await?
            .contains(&cleanup)
    );
    assert!(distribution.remove_package(artifact.id).await?);
    assert!(!distribution.remove_package(artifact.id).await?);
    assert!(distribution.package_metadata(artifact.id).await?.is_none());
    assert!(peer.load_package(artifact.id).await?.is_none());
    assert!(catalog.get_artifact(artifact.id).await?.is_some());
    assert_eq!(catalog.deployment_history(&plugin_id, 10).await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn shared_package_round_trips_chunks_and_preserves_artifact_identity()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let archive = (0..PLUGIN_PACKAGE_CHUNK_BYTES * 2 + 17)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let record = fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, &archive, NOW)
        .await?;
    assert_eq!(record.chunk_count, 3);
    let other_node = PluginDistributionRepository::new(fixture.pool.clone());
    assert_eq!(
        other_node.load_package(fixture.artifact_id).await?,
        Some(archive.clone())
    );
    assert_eq!(
        fixture
            .distribution
            .store_package(fixture.artifact_id, &fixture.digest, &archive, NOW + 1)
            .await?,
        record
    );
    let mut altered_archive = archive.clone();
    altered_archive[0] ^= 1;
    assert_eq!(
        fixture
            .distribution
            .store_package(
                fixture.artifact_id,
                &fixture.digest,
                &altered_archive,
                NOW + 2
            )
            .await?,
        record
    );
    assert_eq!(
        other_node.load_package(fixture.artifact_id).await?,
        Some(archive.clone())
    );

    // Payload digest alone is insufficient to identify a ZIP with a different manifest.
    let second = fixture
        .catalog
        .register_artifact(artifact(
            "org.example.other",
            "2.0.0",
            fixture.digest.clone(),
        ))
        .await?;
    fixture
        .distribution
        .store_package(second.id, &second.digest, &altered_archive, NOW)
        .await?;
    assert_eq!(
        other_node.load_package(second.id).await?,
        Some(altered_archive)
    );
    assert_eq!(
        other_node.load_package(fixture.artifact_id).await?,
        Some(archive)
    );
    fixture.close().await
}

#[tokio::test]
async fn package_write_is_atomic_and_wrong_artifact_digest_is_rejected()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let archive = vec![7; PLUGIN_PACKAGE_CHUNK_BYTES + 1];
    assert_eq!(
        fixture
            .distribution
            .store_package(fixture.artifact_id, &"cd".repeat(32), &archive, NOW)
            .await,
        Err(PluginDistributionError::Conflict)
    );
    fixture.pool.connection().execute_unprepared("CREATE TRIGGER fail_plugin_chunk BEFORE INSERT ON plugin_package_chunks WHEN NEW.chunk_index = 1 BEGIN SELECT RAISE(ABORT, 'chunk unavailable'); END").await?;
    assert_eq!(
        fixture
            .distribution
            .store_package(fixture.artifact_id, &fixture.digest, &archive, NOW)
            .await,
        Err(PluginDistributionError::Unavailable)
    );
    assert!(
        fixture
            .distribution
            .load_package(fixture.artifact_id)
            .await?
            .is_none()
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("DROP TRIGGER fail_plugin_chunk")
        .await?;
    fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, &archive, NOW)
        .await?;
    assert_eq!(
        fixture
            .distribution
            .load_package(fixture.artifact_id)
            .await?,
        Some(archive)
    );
    fixture.close().await
}

#[tokio::test]
async fn repacked_verified_upload_reuses_the_first_archive_without_overwriting_it()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    // The installer verifies payload/manifest identity before these opaque bytes
    // are published. Different ZIP encoding sizes must not change archive identity.
    let first_archive = b"verified artifact in first ZIP encoding";
    let first = fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, first_archive, NOW)
        .await?;
    let repacked = vec![42; PLUGIN_PACKAGE_CHUNK_BYTES + 5];
    let repeated = fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, &repacked, NOW + 60)
        .await?;
    assert_eq!(repeated, first);
    assert_eq!(repeated.chunk_count, 1);
    assert_eq!(repeated.created_at, NOW);
    assert_eq!(
        fixture
            .distribution
            .load_package(fixture.artifact_id)
            .await?,
        Some(first_archive.to_vec())
    );
    fixture.close().await
}

#[tokio::test]
async fn package_reads_reject_corrupted_or_missing_chunks() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let archive = vec![13; PLUGIN_PACKAGE_CHUNK_BYTES + 3];
    fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, &archive, NOW)
        .await?;
    let query = Query::update()
        .table(Alias::new("plugin_package_chunks"))
        .value(
            Alias::new("content"),
            vec![14_u8; PLUGIN_PACKAGE_CHUNK_BYTES],
        )
        .and_where(Expr::col(Alias::new("artifact_id")).eq(fixture.artifact_id))
        .and_where(Expr::col(Alias::new("chunk_index")).eq(0))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&query),
        )
        .await?;
    assert_eq!(
        fixture.distribution.load_package(fixture.artifact_id).await,
        Err(PluginDistributionError::Invariant)
    );
    assert_eq!(
        fixture
            .distribution
            .store_package(
                fixture.artifact_id,
                &fixture.digest,
                b"another verified encoding",
                NOW + 1
            )
            .await,
        Err(PluginDistributionError::Invariant)
    );
    let query = Query::delete()
        .from_table(Alias::new("plugin_package_chunks"))
        .and_where(Expr::col(Alias::new("artifact_id")).eq(fixture.artifact_id))
        .and_where(Expr::col(Alias::new("chunk_index")).eq(1))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&query),
        )
        .await?;
    assert_eq!(
        fixture.distribution.load_package(fixture.artifact_id).await,
        Err(PluginDistributionError::Invariant)
    );
    fixture.close().await
}

#[tokio::test]
async fn runtime_states_are_per_node_generation_checked_and_expire() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture
        .distribution
        .upsert_node("node-a", "worker-a", NOW)
        .await?;
    fixture
        .distribution
        .upsert_node("node-b", "worker-b", NOW)
        .await?;
    assert!(
        fixture
            .distribution
            .record_state(state("node-a", 2, NOW))
            .await?
    );
    assert!(
        fixture
            .distribution
            .record_state(state("node-b", 1, NOW))
            .await?
    );
    assert!(
        !fixture
            .distribution
            .record_state(state("node-a", 1, NOW + 10))
            .await?
    );
    let mut collision = state("node-a", 2, NOW + 1);
    collision.digest = "cd".repeat(32);
    assert_eq!(
        fixture.distribution.record_state(collision).await,
        Err(PluginDistributionError::Conflict)
    );
    let states = fixture.distribution.list_states(Some(PLUGIN), NOW).await?;
    assert_eq!(
        states
            .iter()
            .map(|row| (row.node_id.as_str(), row.generation))
            .collect::<Vec<_>>(),
        vec![("node-a", 2), ("node-b", 1)]
    );
    assert!(
        states
            .iter()
            .all(|row| row.status == PluginRuntimeStatus::Ready)
    );
    let later = NOW + PLUGIN_NODE_LEASE_SECONDS + 1;
    fixture
        .distribution
        .upsert_node("node-a", "worker-a", later)
        .await?;
    let mut failed = state("node-a", 2, later);
    failed.status = PluginRuntimeStatus::Failed;
    failed.error_code = Some(PluginRuntimeErrorCode::HealthFailed);
    fixture.distribution.record_state(failed).await?;
    let states = fixture.distribution.list_states(None, later).await?;
    assert_eq!(states[0].status, PluginRuntimeStatus::Failed);
    assert_eq!(
        states[0].error_code,
        Some(PluginRuntimeErrorCode::HealthFailed)
    );
    assert_eq!(states[1].status, PluginRuntimeStatus::Stale);
    assert_eq!(fixture.distribution.live_nodes(later).await?.len(), 1);
    // An out-of-order heartbeat cannot make the current process stale again.
    fixture
        .distribution
        .upsert_node("node-a", "worker-a", NOW)
        .await?;
    assert_eq!(fixture.distribution.live_nodes(later).await?.len(), 1);
    fixture.close().await
}

#[tokio::test]
async fn uninstall_requires_exact_disabled_generation_and_keeps_archive_and_audit()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, b"archive", NOW)
        .await?;
    let generation = fixture.deploy(true, None).await?;
    let mut request = PluginUninstallRequest {
        plugin_id: PLUGIN.to_owned(),
        generation,
        requested_by: fixture.operator,
        requested_at: NOW,
        request_id: "uninstall-distribution".to_owned(),
    };
    assert_eq!(
        fixture.distribution.record_uninstall(request.clone()).await,
        Err(PluginDistributionError::Conflict)
    );
    assert_eq!(
        fixture
            .distribution
            .remove_package(fixture.artifact_id)
            .await,
        Err(PluginDistributionError::Conflict)
    );
    let disabled = fixture.deploy(false, Some(generation)).await?;
    assert_eq!(
        fixture.distribution.record_uninstall(request.clone()).await,
        Err(PluginDistributionError::Conflict)
    );
    request.generation = disabled;
    assert_eq!(
        fixture
            .distribution
            .record_uninstall(request.clone())
            .await?,
        request
    );
    assert_eq!(
        fixture
            .distribution
            .record_uninstall(request.clone())
            .await?,
        request
    );
    let mut retry = request.clone();
    retry.requested_at += 10;
    assert_eq!(fixture.distribution.record_uninstall(retry).await?, request);
    assert_eq!(
        fixture.distribution.list_uninstall_requests().await?,
        vec![request]
    );
    assert_eq!(
        fixture
            .distribution
            .load_package(fixture.artifact_id)
            .await?,
        Some(b"archive".to_vec())
    );
    let logs = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::Operation.eq("catalog_uninstall"))
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].operator_user_id, fixture.operator.get());
    assert_eq!(
        fixture.catalog.deployment_history(PLUGIN, 10).await?.len(),
        2
    );
    fixture.close().await
}

#[tokio::test]
async fn retiring_a_process_keeps_its_last_observation_and_blocks_late_heartbeats()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture
        .distribution
        .upsert_node("retired-node", "worker", NOW)
        .await?;
    fixture
        .distribution
        .record_state(state("retired-node", 1, NOW))
        .await?;
    fixture
        .distribution
        .retire_node("retired-node", NOW + 1)
        .await?;
    fixture
        .distribution
        .retire_node("retired-node", NOW + 2)
        .await?;
    assert!(fixture.distribution.live_nodes(NOW + 2).await?.is_empty());
    let nodes = fixture.distribution.list_nodes(NOW + 2).await?;
    assert_eq!(nodes[0].last_seen_at, NOW);
    assert_eq!(nodes[0].retired_at, Some(NOW + 1));
    assert!(!nodes[0].live);
    assert_eq!(
        fixture.distribution.list_states(None, NOW + 2).await?[0].status,
        PluginRuntimeStatus::Stale
    );
    assert_eq!(
        fixture
            .distribution
            .upsert_node("retired-node", "worker", NOW + 3)
            .await,
        Err(PluginDistributionError::Conflict)
    );
    assert_eq!(
        fixture
            .distribution
            .record_state(state("retired-node", 1, NOW + 3))
            .await,
        Err(PluginDistributionError::Conflict)
    );
    fixture.close().await
}

#[tokio::test]
async fn uninstall_audit_failure_rolls_back_intent() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let generation = fixture.deploy(false, None).await?;
    fixture.pool.connection().execute_unprepared("CREATE TRIGGER fail_plugin_uninstall_audit BEFORE INSERT ON platform_audit_logs WHEN NEW.operation = 'catalog_uninstall' BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END").await?;
    let request = PluginUninstallRequest {
        plugin_id: PLUGIN.to_owned(),
        generation,
        requested_by: fixture.operator,
        requested_at: NOW,
        request_id: "uninstall-audit-failure".to_owned(),
    };
    assert_eq!(
        fixture.distribution.record_uninstall(request).await,
        Err(PluginDistributionError::Unavailable)
    );
    assert!(
        fixture
            .distribution
            .list_uninstall_requests()
            .await?
            .is_empty()
    );
    fixture.close().await
}

#[tokio::test]
async fn cleanup_requires_revocation_and_never_removes_deployment_history()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture
        .distribution
        .store_package(fixture.artifact_id, &fixture.digest, b"archive", NOW)
        .await?;
    fixture.deploy(false, None).await?;
    let request = PluginArtifactCleanupRequest {
        artifact_id: fixture.artifact_id,
        requested_by: fixture.operator,
        requested_at: NOW,
        request_id: "cleanup-distribution".to_owned(),
    };
    assert_eq!(
        fixture
            .distribution
            .record_cleanup_request(request.clone())
            .await,
        Err(PluginDistributionError::Conflict)
    );
    fixture
        .catalog
        .revoke_artifact(PluginArtifactRevocation {
            artifact_id: fixture.artifact_id,
            revoked_by: fixture.operator,
            revoked_at: NOW,
            reason: "retired package".to_owned(),
            request_id: "revoke-distribution".to_owned(),
        })
        .await?;
    assert_eq!(
        fixture
            .distribution
            .record_cleanup_request(request.clone())
            .await?,
        request
    );
    assert_eq!(
        fixture.distribution.list_cleanup_requests().await?,
        vec![request]
    );
    assert!(
        fixture
            .distribution
            .remove_package(fixture.artifact_id)
            .await?
    );
    assert_eq!(
        fixture
            .distribution
            .store_package(fixture.artifact_id, &fixture.digest, b"archive", NOW + 10)
            .await,
        Err(PluginDistributionError::Conflict)
    );
    assert!(
        !fixture
            .distribution
            .remove_package(fixture.artifact_id)
            .await?
    );
    assert!(
        fixture
            .distribution
            .load_package(fixture.artifact_id)
            .await?
            .is_none()
    );
    assert!(
        fixture
            .catalog
            .get_artifact(fixture.artifact_id)
            .await?
            .is_some()
    );
    assert_eq!(
        fixture.catalog.deployment_history(PLUGIN, 10).await?.len(),
        1
    );
    fixture.close().await
}

#[tokio::test]
async fn atomic_uninstall_rolls_back_disabled_pointer_revision_and_intent_on_audit_failure()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let generation = fixture.deploy(true, None).await?;
    fixture.pool.connection().execute_unprepared("CREATE TRIGGER fail_atomic_uninstall_audit BEFORE INSERT ON platform_audit_logs WHEN NEW.operation = 'catalog_uninstall' BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END").await?;
    let command = uninstall_command(&fixture, generation);
    assert!(matches!(
        fixture.catalog.uninstall(command.clone()).await,
        Err(crate::PluginCatalogError::Unavailable)
    ));
    let current = fixture
        .catalog
        .get_deployment(PLUGIN)
        .await?
        .ok_or("missing deployment")?;
    assert_eq!(current.generation, generation);
    assert!(current.enabled);
    assert_eq!(
        fixture.catalog.deployment_history(PLUGIN, 10).await?.len(),
        1
    );
    assert!(
        fixture
            .distribution
            .list_uninstall_requests()
            .await?
            .is_empty()
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("DROP TRIGGER fail_atomic_uninstall_audit")
        .await?;
    let disabled = fixture.catalog.uninstall(command.clone()).await?;
    assert_eq!(disabled.generation, generation + 1);
    assert!(!disabled.enabled);
    assert_eq!(
        fixture.distribution.list_uninstall_requests().await?[0].generation,
        disabled.generation
    );
    let mut retry = command;
    retry.updated_at += 10;
    assert_eq!(
        fixture.catalog.uninstall(retry).await?.generation,
        disabled.generation
    );
    assert_eq!(
        fixture.catalog.deployment_history(PLUGIN, 10).await?.len(),
        2
    );
    fixture.close().await
}

#[tokio::test]
async fn atomic_uninstall_rolls_back_all_state_when_intent_insert_fails()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let generation = fixture.deploy(true, None).await?;
    fixture.pool.connection().execute_unprepared("CREATE TRIGGER fail_atomic_uninstall_intent BEFORE INSERT ON plugin_uninstall_requests BEGIN SELECT RAISE(ABORT, 'intent unavailable'); END").await?;
    assert!(matches!(
        fixture
            .catalog
            .uninstall(uninstall_command(&fixture, generation))
            .await,
        Err(crate::PluginCatalogError::Unavailable)
    ));
    let current = fixture
        .catalog
        .get_deployment(PLUGIN)
        .await?
        .ok_or("missing deployment")?;
    assert_eq!(current.generation, generation);
    assert!(current.enabled);
    assert_eq!(
        fixture.catalog.deployment_history(PLUGIN, 10).await?.len(),
        1
    );
    assert!(
        fixture
            .distribution
            .list_uninstall_requests()
            .await?
            .is_empty()
    );
    fixture.close().await
}

#[tokio::test]
async fn atomic_uninstall_accepts_revoked_current_artifacts_without_reactivating_them()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture.deploy(true, None).await?;
    fixture
        .catalog
        .revoke_artifact(PluginArtifactRevocation {
            artifact_id: fixture.artifact_id,
            revoked_by: fixture.operator,
            revoked_at: NOW,
            request_id: "revoke-before-uninstall".to_owned(),
            reason: "retired".to_owned(),
        })
        .await?;
    let current = fixture
        .catalog
        .get_deployment(PLUGIN)
        .await?
        .ok_or("missing deployment")?;
    assert!(!current.enabled);
    let disabled = fixture
        .catalog
        .uninstall(uninstall_command(&fixture, current.generation))
        .await?;
    assert!(!disabled.enabled);
    assert!(disabled.artifact.revocation.is_some());
    assert_eq!(disabled.generation, current.generation + 1);
    fixture.close().await
}
