use std::error::Error;

use af_domain::{PlatformPermission, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PluginArtifactApproval, PluginArtifactCreate,
    PluginArtifactRevocation, PluginArtifactSourceKind, PluginCapabilityGrant, PluginCatalogError,
    PluginCatalogRepository, PluginDeploymentQuery, PluginDeploymentWrite,
    entity::{groups, platform_audit_logs, users},
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
                {
                    "id": "catalog.models.write",
                    "version": 1,
                    "access": "write",
                    "sideEffect": "external_write",
                    "writeSemantics": {
                        "nativeIdempotencyKey": true,
                        "resultLookup": "by_request_id",
                        "reconciliation": true
                    }
                }
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

#[tokio::test]
async fn catalog_audit_failure_rolls_back_deployment_and_immutable_history()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (_, digest) = approved_artifact(&fixture).await?;
    let reject_catalog_audit =
        "CREATE TRIGGER reject_catalog_audit BEFORE INSERT ON platform_audit_logs
        WHEN NEW.operation = 'catalog_deploy'
        BEGIN SELECT RAISE(ABORT, 'injected catalog audit failure'); END";
    fixture
        .pool
        .connection()
        .execute_unprepared(reject_catalog_audit)
        .await?;
    assert_eq!(
        fixture
            .repo
            .deploy(deployment(
                fixture.operator,
                digest.clone(),
                None,
                "deploy-audit-failed-first",
            ))
            .await,
        Err(PluginCatalogError::Unavailable)
    );
    assert!(
        fixture
            .repo
            .get_deployment("org.example.catalog")
            .await?
            .is_none()
    );
    assert!(
        fixture
            .repo
            .deployment_history("org.example.catalog", 10)
            .await?
            .is_empty()
    );

    fixture
        .pool
        .connection()
        .execute_unprepared("DROP TRIGGER reject_catalog_audit")
        .await?;
    let first = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            digest.clone(),
            None,
            "deploy-audit-success",
        ))
        .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared(reject_catalog_audit)
        .await?;
    let mut disable = deployment(
        fixture.operator,
        digest,
        Some(first.generation),
        "deploy-audit-failed-update",
    );
    disable.enabled = false;
    assert_eq!(
        fixture.repo.deploy(disable).await,
        Err(PluginCatalogError::Unavailable)
    );
    let current = fixture
        .repo
        .get_deployment("org.example.catalog")
        .await?
        .ok_or("the previous deployment must survive the failed audit")?;
    assert_eq!(current.generation, first.generation);
    assert!(current.enabled);
    assert_eq!(
        fixture
            .repo
            .deployment_history("org.example.catalog", 10)
            .await?
            .len(),
        1
    );
    let facts = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::Operation.eq("catalog_deploy"))
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].request_id, "deploy-audit-success");
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn catalog_audit_correlates_approval_revocation_and_automatic_disable()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let (artifact_id, digest) = approved_artifact(&fixture).await?;
    let first = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            digest.clone(),
            None,
            "deploy-audit-correlated",
        ))
        .await?;
    fixture
        .repo
        .revoke_artifact(PluginArtifactRevocation {
            artifact_id,
            revoked_by: fixture.operator,
            revoked_at: 1_800_000_003,
            request_id: "revoke-audit-correlated".to_owned(),
            reason: "private revocation note".to_owned(),
        })
        .await?;
    let facts = platform_audit_logs::Entity::find()
        .order_by_asc(platform_audit_logs::Column::Id)
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(facts.len(), 4);
    assert_eq!(facts[0].operation, "catalog_approve");
    assert_eq!(facts[0].request_id, "approve-request-1");
    assert_eq!(facts[1].operation, "catalog_deploy");
    assert_eq!(facts[1].request_id, first.request_id);
    assert_eq!(facts[2].operation, "catalog_deploy");
    assert_eq!(facts[2].request_id, "revoke-audit-correlated");
    assert_eq!(facts[3].operation, "catalog_revoke");
    assert_eq!(facts[3].request_id, "revoke-audit-correlated");
    for fact in &facts {
        assert_eq!(fact.operator_user_id, fixture.operator.get());
        assert_eq!(
            fact.permission_code,
            PlatformPermission::PluginsManage.code()
        );
        assert_eq!(fact.outcome, crate::PlatformAuditOutcome::Succeeded as i16);
        assert!(fact.before_value.is_none());
        let metadata = fact
            .after_value
            .as_deref()
            .ok_or("catalog fact must have metadata")?;
        assert!(!metadata.contains("endpoint"));
        assert!(!metadata.contains("source_reference"));
        assert!(!metadata.contains("signature"));
        assert!(!metadata.contains("private revocation note"));
        let parsed: serde_json::Value = serde_json::from_str(metadata)?;
        assert_eq!(parsed["artifactId"], artifact_id);
        assert_eq!(parsed["artifactDigest"], digest);
    }
    let disabled: serde_json::Value = serde_json::from_str(
        facts[2]
            .after_value
            .as_deref()
            .ok_or("disable fact must have metadata")?,
    )?;
    assert_eq!(disabled["generation"], first.generation + 1);
    assert_eq!(disabled["enabled"], false);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn catalog_trust_mutations_roll_back_when_their_audit_cannot_commit()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let candidate = fixture
        .repo
        .register_artifact(artifact(&"ab".repeat(32)))
        .await?;
    let reject_trust_audit =
        "CREATE TRIGGER reject_trust_audit BEFORE INSERT ON platform_audit_logs
        WHEN NEW.operation IN ('catalog_approve', 'catalog_revoke')
        BEGIN SELECT RAISE(ABORT, 'injected trust audit failure'); END";
    fixture
        .pool
        .connection()
        .execute_unprepared(reject_trust_audit)
        .await?;
    let approval = PluginArtifactApproval {
        artifact_id: candidate.id,
        approved_by: fixture.operator,
        approved_at: 1_800_000_001,
        request_id: "approve-audit-rollback".to_owned(),
    };
    assert_eq!(
        fixture.repo.approve_artifact(approval.clone()).await,
        Err(PluginCatalogError::Unavailable)
    );
    assert!(
        fixture
            .repo
            .get_artifact(candidate.id)
            .await?
            .ok_or("artifact must exist")?
            .approval
            .is_none()
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("DROP TRIGGER reject_trust_audit")
        .await?;
    fixture.repo.approve_artifact(approval).await?;
    let active = fixture
        .repo
        .deploy(deployment(
            fixture.operator,
            candidate.digest,
            None,
            "deploy-before-audit-revoke-failure",
        ))
        .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared(reject_trust_audit)
        .await?;
    assert_eq!(
        fixture
            .repo
            .revoke_artifact(PluginArtifactRevocation {
                artifact_id: candidate.id,
                revoked_by: fixture.operator,
                revoked_at: 1_800_000_003,
                request_id: "revoke-audit-rollback".to_owned(),
                reason: "revoke for test".to_owned(),
            })
            .await,
        Err(PluginCatalogError::Unavailable)
    );
    assert!(
        fixture
            .repo
            .get_artifact(candidate.id)
            .await?
            .ok_or("artifact must exist")?
            .revocation
            .is_none()
    );
    let current = fixture
        .repo
        .get_deployment("org.example.catalog")
        .await?
        .ok_or("deployment must survive")?;
    assert_eq!(current.generation, active.generation);
    assert!(current.enabled);
    assert_eq!(
        fixture
            .repo
            .deployment_history("org.example.catalog", 10)
            .await?
            .len(),
        1
    );
    assert!(
        platform_audit_logs::Entity::find()
            .filter(platform_audit_logs::Column::RequestId.eq("revoke-audit-rollback"))
            .all(fixture.pool.connection())
            .await?
            .is_empty()
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn audited_artifact_registration_rolls_back_and_preserves_actor_on_retries()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let candidate = artifact(&"ab".repeat(32));
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "CREATE TRIGGER reject_registration_audit BEFORE INSERT ON platform_audit_logs
            WHEN NEW.operation = 'catalog_register'
            BEGIN SELECT RAISE(ABORT, 'injected registration audit failure'); END",
        )
        .await?;
    assert_eq!(
        fixture
            .repo
            .register_artifact_audited(candidate.clone(), fixture.operator, "register-atomic")
            .await,
        Err(PluginCatalogError::Unavailable)
    );
    assert!(
        fixture
            .repo
            .list_artifacts(crate::PluginArtifactQuery::new(None, 10)?)
            .await?
            .is_empty()
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("DROP TRIGGER reject_registration_audit")
        .await?;
    let first = fixture
        .repo
        .register_artifact_audited(candidate.clone(), fixture.operator, "register-atomic")
        .await?;
    let mut retry = candidate.clone();
    retry.created_at += 1;
    let repeated = fixture
        .repo
        .register_artifact_audited(retry, fixture.operator, "register-atomic")
        .await?;
    assert_eq!(repeated.id, first.id);
    assert_eq!(repeated.created_at, first.created_at);
    let facts = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::Operation.eq("catalog_register"))
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].operator_user_id, fixture.operator.get());
    assert_eq!(
        facts[0].permission_code,
        PlatformPermission::PluginsManage.code()
    );
    assert_eq!(facts[0].request_id, "register-atomic");
    assert_eq!(facts[0].resource_id, Some(first.id.to_string()));
    let metadata = facts[0]
        .after_value
        .as_deref()
        .ok_or("registration needs metadata")?;
    assert!(!metadata.contains(&candidate.source_reference));
    assert!(!metadata.contains(&candidate.signature));

    let group = groups::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .ok_or("fixture group must exist")?;
    let other_operator = users::ActiveModel {
        username: Set("plugin-catalog-other-operator".to_owned()),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("plugin-catalog-other-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    assert_eq!(
        fixture
            .repo
            .register_artifact_audited(
                candidate,
                UserId::new(other_operator.id)?,
                "register-atomic",
            )
            .await,
        Err(PluginCatalogError::Conflict)
    );

    // Reusing the request for a different candidate must not leave that
    // candidate behind when the audit identity cannot be reused.
    let mut other = artifact(&"cd".repeat(32));
    other.plugin_version = "2.0.0".to_owned();
    other.manifest["pluginVersion"] = serde_json::json!("2.0.0");
    assert_eq!(
        fixture
            .repo
            .register_artifact_audited(other, fixture.operator, "register-atomic")
            .await,
        Err(PluginCatalogError::Conflict)
    );
    assert_eq!(
        fixture
            .repo
            .list_artifacts(crate::PluginArtifactQuery::new(None, 10)?)
            .await?
            .len(),
        1
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_identical_audited_registrations_share_one_artifact_and_fact()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let candidate = artifact(&"ab".repeat(32));
    let (first, second) = tokio::join!(
        fixture.repo.register_artifact_audited(
            candidate.clone(),
            fixture.operator,
            "register-race"
        ),
        fixture
            .repo
            .register_artifact_audited(candidate, fixture.operator, "register-race"),
    );
    assert_eq!(first?.id, second?.id);
    assert_eq!(
        fixture
            .repo
            .list_artifacts(crate::PluginArtifactQuery::new(None, 10)?)
            .await?
            .len(),
        1
    );
    let facts = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::Operation.eq("catalog_register"))
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].request_id, "register-race");
    fixture.close().await?;
    Ok(())
}
