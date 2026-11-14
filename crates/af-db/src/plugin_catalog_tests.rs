use std::error::Error;

use af_domain::UserId;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PluginArtifactApproval, PluginArtifactCreate,
    PluginArtifactRevocation, PluginArtifactSourceKind, PluginCapabilityGrant, PluginCatalogError,
    PluginCatalogRepository, PluginDeploymentQuery, PluginDeploymentWrite,
    entity::{groups, users},
};

struct Fixture {
    pool: DatabasePool,
    repo: PluginCatalogRepository,
    operator: UserId,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = groups::ActiveModel {
            name: Set("plugin-catalog-test".to_owned()),
            display_name: Set("Plugin catalog test".to_owned()),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let user = users::ActiveModel {
            username: Set("plugin-catalog-operator".to_owned()),
            role: Set(1),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set("plugin-catalog-aff".to_owned()),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        Ok(Self {
            repo: PluginCatalogRepository::new(pool.clone()),
            pool,
            operator: UserId::new(user.id)?,
        })
    }

    async fn close(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await?;
        Ok(())
    }
}

fn artifact(digest: &str) -> PluginArtifactCreate {
    PluginArtifactCreate {
        plugin_id: "org.example.catalog".to_owned(),
        plugin_version: "1.0.0".to_owned(),
        digest: digest.to_owned(),
        manifest: serde_json::json!({
            "pluginId": "org.example.catalog",
            "pluginVersion": "1.0.0",
            "configSchema": { "type": "object", "additionalProperties": false },
            "contractRange": { "minimum": { "major": 1, "minor": 0 }, "maximum": { "major": 1, "minor": 2 } },
            "authenticationModes": ["none"],
            "limits": { "maxConcurrency": 2, "requestTimeoutMs": 1000, "maxResponseBytes": 4096, "maxPageSize": 20 },
            "capabilities": [
                { "id": "catalog.models.read", "version": 1, "access": "read", "sideEffect": "none" },
                { "id": "catalog.models.write", "version": 1, "access": "write", "sideEffect": "external_write" }
            ]
        }),
        source_kind: PluginArtifactSourceKind::LocalProcess,
        source_reference: "/opt/anyflows/plugins/catalog".to_owned(),
        signer_key_id: "release-2026".to_owned(),
        signature: "a5".repeat(64),
        created_at: 1_800_000_000,
    }
}

async fn approved_artifact(fixture: &Fixture) -> Result<(i64, String), Box<dyn Error>> {
    let artifact = fixture
        .repo
        .register_artifact(artifact(&"ab".repeat(32)))
        .await?;
    fixture
        .repo
        .approve_artifact(PluginArtifactApproval {
            artifact_id: artifact.id,
            approved_by: fixture.operator,
            approved_at: 1_800_000_001,
            request_id: "approve-request-1".to_owned(),
        })
        .await?;
    Ok((artifact.id, artifact.digest))
}

fn deployment(
    operator: UserId,
    digest: String,
    expected_generation: Option<i64>,
    request_id: &str,
) -> PluginDeploymentWrite {
    PluginDeploymentWrite {
        plugin_id: "org.example.catalog".to_owned(),
        artifact_digest: digest,
        expected_generation,
        enabled: true,
        granted_capabilities: vec![PluginCapabilityGrant {
            id: "catalog.models.read".to_owned(),
            version: 1,
        }],
        config: serde_json::json!({ "endpoint": "https://catalog.example" }),
        updated_by: operator,
        updated_at: 1_800_000_002,
        request_id: request_id.to_owned(),
    }
}

#[tokio::test]
async fn plugin_artifacts_are_immutable_and_require_explicit_approval_before_deployment()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (artifact_id, digest) = approved_artifact(&fixture).await?;
    let deployment = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            digest.clone(),
            None,
            "deploy-request-1",
        ))
        .await?;
    assert_eq!(deployment.generation, 1);
    assert!(deployment.artifact.approval.is_some());
    assert_eq!(deployment.granted_capabilities.len(), 1);

    let history = fixture
        .repo
        .deployment_history("org.example.catalog", 10)
        .await?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].request_id, "deploy-request-1");
    assert_eq!(
        fixture
            .repo
            .list_deployments(PluginDeploymentQuery::new(None, 10)?)
            .await?
            .len(),
        1
    );

    let repeated = fixture.repo.register_artifact(artifact(&digest)).await?;
    assert_eq!(repeated.id, artifact_id);
    assert!(repeated.approval.is_some());
    let mut later_retry = artifact(&digest);
    later_retry.created_at = later_retry.created_at.saturating_add(1);
    assert_eq!(
        fixture.repo.register_artifact(later_retry).await?.id,
        artifact_id
    );
    let pending = fixture
        .repo
        .list_artifacts(crate::PluginArtifactQuery::new(None, 10)?)
        .await?;
    assert_eq!(pending.len(), 1);
    assert!(pending[0].approval.is_some());
    let mut changed = artifact(&digest);
    changed.source_reference = "/opt/changed/artifact".to_owned();
    assert_eq!(
        fixture.repo.register_artifact(changed).await,
        Err(PluginCatalogError::Conflict)
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_identical_artifact_registration_is_idempotent() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let input = artifact(&"ef".repeat(32));
    let (left, right) = tokio::join!(
        fixture.repo.register_artifact(input.clone()),
        fixture.repo.register_artifact(input),
    );
    let left = left?;
    let right = right?;
    assert_eq!(left.id, right.id);
    assert_eq!(left.digest, right.digest);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn plugin_deployment_requires_granted_read_capabilities_and_generation_cas()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (_, digest) = approved_artifact(&fixture).await?;

    let mut write_grant = deployment(fixture.operator, digest.clone(), None, "deploy-invalid");
    write_grant.granted_capabilities = vec![PluginCapabilityGrant {
        id: "catalog.models.write".to_owned(),
        version: 1,
    }];
    assert_eq!(
        fixture.repo.deploy(write_grant).await,
        Err(PluginCatalogError::InvalidInput)
    );

    let first = fixture
        .repo
        .deploy(deployment(fixture.operator, digest, None, "deploy-one"))
        .await?;
    assert_eq!(
        fixture
            .repo
            .deploy(deployment(
                fixture.operator,
                first.artifact.digest.clone(),
                Some(0),
                "deploy-stale"
            ))
            .await,
        Err(PluginCatalogError::InvalidInput)
    );

    let second = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            first.artifact.digest.clone(),
            Some(first.generation),
            "deploy-two",
        ))
        .await?;
    assert_eq!(second.generation, 2);
    assert_eq!(
        fixture
            .repo
            .deploy(deployment(
                fixture.operator,
                first.artifact.digest.clone(),
                Some(first.generation),
                "deploy-stale-again"
            ))
            .await,
        Err(PluginCatalogError::Conflict)
    );
    let rollback = fixture
        .repo
        .rollback(
            "org.example.catalog",
            second.generation,
            first.generation,
            fixture.operator,
            1_800_000_003,
            "rollback-request".to_owned(),
        )
        .await?;
    assert_eq!(rollback.generation, 3);
    assert_eq!(
        fixture
            .repo
            .deployment_history("org.example.catalog", 10)
            .await?
            .iter()
            .map(|row| row.generation)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn unapproved_plugin_artifact_cannot_be_deployed() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let candidate = fixture
        .repo
        .register_artifact(artifact(&"cd".repeat(32)))
        .await?;
    assert!(candidate.approval.is_none());
    assert_eq!(
        fixture
            .repo
            .deploy(deployment(
                fixture.operator,
                candidate.digest,
                None,
                "deploy-unapproved"
            ))
            .await,
        Err(PluginCatalogError::NotFound)
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn revoked_plugin_artifact_cannot_be_reapproved_or_deployed() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (artifact_id, digest) = approved_artifact(&fixture).await?;
    let active = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            digest.clone(),
            None,
            "deploy-before-revoke",
        ))
        .await?;
    fixture
        .repo
        .revoke_artifact(PluginArtifactRevocation {
            artifact_id,
            revoked_by: fixture.operator,
            revoked_at: 1_800_000_003,
            request_id: "revoke-request-1".to_owned(),
            reason: "signature key compromised".to_owned(),
        })
        .await?;

    let artifact = fixture
        .repo
        .list_artifacts(crate::PluginArtifactQuery::new(None, 10)?)
        .await?
        .remove(0);
    assert!(artifact.revocation.is_some());
    let current = fixture
        .repo
        .get_deployment("org.example.catalog")
        .await?
        .ok_or("revocation must preserve a deployment record")?;
    assert_eq!(current.generation, active.generation + 1);
    assert!(!current.enabled);
    assert_eq!(
        fixture
            .repo
            .approve_artifact(PluginArtifactApproval {
                artifact_id,
                approved_by: fixture.operator,
                approved_at: 1_800_000_004,
                request_id: "approve-again".to_owned(),
            })
            .await,
        Err(PluginCatalogError::Conflict)
    );
    assert_eq!(
        fixture
            .repo
            .deploy(deployment(fixture.operator, digest, None, "deploy-revoked"))
            .await,
        Err(PluginCatalogError::Conflict)
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn plugin_catalog_immutable_records_reject_direct_updates_and_deletes()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (artifact_id, digest) = approved_artifact(&fixture).await?;
    let deployed = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            digest,
            None,
            "append-only-deploy",
        ))
        .await?;

    let artifact_update = fixture
        .pool
        .connection()
        .execute_unprepared(&format!(
            "UPDATE plugin_artifacts SET source_reference = 'mutated' WHERE id = {artifact_id}"
        ))
        .await;
    let approval_delete = fixture
        .pool
        .connection()
        .execute_unprepared(&format!(
            "DELETE FROM plugin_artifact_approvals WHERE artifact_id = {artifact_id}"
        ))
        .await;
    let revision_update = fixture
        .pool
        .connection()
        .execute_unprepared(&format!(
            "UPDATE plugin_deployment_revisions SET enabled = FALSE WHERE plugin_id = '{}' AND generation = {}",
            deployed.plugin_id, deployed.generation
        ))
        .await;
    assert!(artifact_update.is_err());
    assert!(approval_delete.is_err());
    assert!(revision_update.is_err());
    let revision_delete = fixture
        .pool
        .connection()
        .execute_unprepared(&format!(
            "DELETE FROM plugin_deployment_revisions WHERE plugin_id = '{}' AND generation = {}",
            deployed.plugin_id, deployed.generation
        ))
        .await;
    assert!(revision_delete.is_err());

    fixture.close().await?;
    Ok(())
}
