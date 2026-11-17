use std::error::Error;

use af_domain::{ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, entity::prelude::Json as DbJson,
    sea_query::Expr,
};

use crate::{
    ActiveChannelMonitorRepository, ActiveChannelMonitorTargetInput, DatabaseOptions,
    MigrationOptions, PluginActionProposalCreate, PluginActionProposalRepository,
    PluginActionProposalStatus, PluginArtifactApproval, PluginArtifactCreate,
    PluginArtifactSourceKind, PluginCapabilityGrant, PluginCatalogRepository,
    PluginDeploymentWrite,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channel_models,
        channel_monitor_targets, channels, credentials, groups, plugin_artifact_revocations,
        plugin_deployments, users,
    },
};

const NOW: i64 = 2_000_000_000;
const PLUGIN_ID: &str = "org.example.action-review";
const CAPABILITY_ID: &str = "channel.monitor.target.action-propose.read";
const ACTION_ID: &str = "active-channel-monitor.target.set-enabled";

struct Fixture {
    pool: crate::DatabasePool,
    proposals: PluginActionProposalRepository,
    requester: UserId,
    confirmer: UserId,
    artifact_id: i64,
    artifact_digest: String,
    deployment_generation: i64,
    target_id: i64,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = groups::ActiveModel {
            name: Set("plugin-action-test".to_owned()),
            display_name: Set("Plugin action test".to_owned()),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let requester = insert_admin(&pool, group.id, "plugin-action-requester").await?;
        let confirmer = insert_admin(&pool, group.id, "plugin-action-confirmer").await?;

        let catalog = PluginCatalogRepository::new(pool.clone());
        let artifact_digest = "ab".repeat(32);
        let artifact = catalog
            .register_artifact(PluginArtifactCreate {
                plugin_id: PLUGIN_ID.to_owned(),
                plugin_version: "1.0.0".to_owned(),
                digest: artifact_digest.clone(),
                manifest: serde_json::json!({
                    "pluginId": PLUGIN_ID,
                    "pluginVersion": "1.0.0",
                    "configSchema": { "type": "object", "additionalProperties": false },
                    "contractRange": {
                        "minimum": { "major": 1, "minor": 0 },
                        "maximum": { "major": 1, "minor": 2 }
                    },
                    "authenticationModes": ["none"],
                    "limits": {
                        "maxConcurrency": 1,
                        "requestTimeoutMs": 1000,
                        "maxResponseBytes": 4096,
                        "maxPageSize": 20
                    },
                    "capabilities": [{
                        "id": CAPABILITY_ID,
                        "version": 1,
                        "access": "read",
                        "sideEffect": "none"
                    }]
                }),
                source_kind: PluginArtifactSourceKind::LocalProcess,
                source_reference: "/opt/anyflows/plugins/action-review".to_owned(),
                signer_key_id: String::new(),
                signature: String::new(),
                created_at: NOW,
            })
            .await?;
        catalog
            .approve_artifact(PluginArtifactApproval {
                artifact_id: artifact.id,
                approved_by: requester,
                approved_at: NOW + 1,
                request_id: "approve-action-plugin".to_owned(),
            })
            .await?;
        let deployment = catalog
            .deploy(PluginDeploymentWrite {
                plugin_id: PLUGIN_ID.to_owned(),
                artifact_digest: artifact_digest.clone(),
                expected_generation: None,
                enabled: true,
                granted_capabilities: vec![PluginCapabilityGrant {
                    id: CAPABILITY_ID.to_owned(),
                    version: 1,
                }],
                config: serde_json::json!({}),
                updated_by: requester,
                updated_at: NOW + 2,
                request_id: "deploy-action-plugin".to_owned(),
            })
            .await?;

        let (channel_id, credential_id) = insert_monitor_channel(&pool).await?;
        let target = ActiveChannelMonitorRepository::new(pool.clone())
            .upsert_target(ActiveChannelMonitorTargetInput {
                channel_id,
                credential_id,
                model: "chat".to_owned(),
                enabled: true,
                interval_secs: 300,
                daily_budget: 12,
                timeout_secs: 15,
                alert_after_secs: 900,
                muted_until_epoch_seconds: None,
            })
            .await?;

        Ok(Self {
            proposals: PluginActionProposalRepository::new(pool.clone()),
            pool,
            requester,
            confirmer,
            artifact_id: artifact.id,
            artifact_digest,
            deployment_generation: deployment.generation,
            target_id: target.id,
        })
    }

    async fn proposal(&self, suffix: &str, expires_at: i64) -> Result<String, Box<dyn Error>> {
        let proposal_key = format!("proposal-{suffix}");
        let input = serde_json::json!({
            "actionId": ACTION_ID,
            "actionVersion": 1,
            "input": { "enabled": false }
        });
        self.proposals
            .create(PluginActionProposalCreate {
                proposal_key: proposal_key.clone(),
                idempotency_key: format!("request-{suffix}"),
                requested_by_user_id: self.requester.get(),
                plugin_id: PLUGIN_ID.to_owned(),
                artifact_id: self.artifact_id,
                artifact_digest: self.artifact_digest.clone(),
                deployment_generation: self.deployment_generation,
                capability_version: 1,
                action_version: 1,
                input_json: input.to_string(),
                target_id: self.target_id,
                expected_target_version: 1,
                expected_enabled: true,
                requested_enabled: false,
                created_at: NOW + 10,
                expires_at,
            })
            .await?;
        Ok(proposal_key)
    }

    async fn close(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await?;
        Ok(())
    }
}

async fn insert_admin(
    pool: &crate::DatabasePool,
    group_id: i64,
    username: &str,
) -> Result<UserId, Box<dyn Error>> {
    let user = users::ActiveModel {
        username: Set(username.to_owned()),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(group_id),
        aff_code: Set(format!("{username}-aff")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(UserId::new(user.id)?)
}

async fn insert_monitor_channel(
    pool: &crate::DatabasePool,
) -> Result<(ChannelId, CredentialId), Box<dyn Error>> {
    let channel = channels::ActiveModel {
        name: Set("plugin-action-monitor".to_owned()),
        r#type: Set(ChannelType::OpenAi.as_str().to_owned()),
        protocol: Set(Protocol::OpenAiChat.as_str().to_owned()),
        base_url: Set(Some(ChannelBaseUrl::parse("https://monitor.example.com")?)),
        status: Set(Status::Enabled.code()),
        auto_ban: Set(false),
        model_mapping: Set(serde_json::json!({})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({}))?),
        settings: Set(SensitiveJson::from(serde_json::json!({}))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set("chat".to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let credential = credentials::ActiveModel {
        channel_id: Set(channel.id),
        kind: Set(CredentialKind::ApiKey.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_credential()).unwrap()),
        status: Set(Status::Enabled.code()),
        schedulable: Set(true),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok((
        ChannelId::new(channel.id)?,
        CredentialId::new(credential.id)?,
    ))
}

fn encrypted_credential() -> DbJson {
    DbJson::Object(
        [
            ("version".to_owned(), DbJson::from(1)),
            (
                "algorithm".to_owned(),
                DbJson::String("xchacha20poly1305".to_owned()),
            ),
            (
                "key_id".to_owned(),
                DbJson::String("plugin-action-test".to_owned()),
            ),
            (
                "nonce".to_owned(),
                DbJson::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                DbJson::String(URL_SAFE_NO_PAD.encode([0x42; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

async fn target_state(fixture: &Fixture) -> Result<(bool, i64), Box<dyn Error>> {
    let target = channel_monitor_targets::Entity::find_by_id(fixture.target_id)
        .one(fixture.pool.connection())
        .await?
        .ok_or("monitor target disappeared")?;
    Ok((target.enabled, target.config_version))
}

#[tokio::test]
async fn confirmed_action_is_idempotent_and_requires_the_same_confirmation_key()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let key = fixture.proposal("confirm", NOW + 300).await?;

    let confirmed = fixture
        .proposals
        .confirm(&key, fixture.confirmer.get(), "confirm-once", NOW + 20)
        .await?;
    assert_eq!(confirmed.status, PluginActionProposalStatus::Confirmed);
    assert_eq!(
        confirmed.confirmed_by_user_id,
        Some(fixture.confirmer.get())
    );
    assert_eq!(
        confirmed
            .outcome
            .as_ref()
            .and_then(|value| value["code"].as_str()),
        Some("confirmed")
    );

    let replay = fixture
        .proposals
        .confirm(&key, fixture.requester.get(), "confirm-once", NOW + 30)
        .await?;
    assert_eq!(replay, confirmed);
    assert_eq!(
        fixture
            .proposals
            .confirm(
                &key,
                fixture.requester.get(),
                "different-confirmation",
                NOW + 30
            )
            .await,
        Err(crate::PluginActionProposalRepositoryError::Conflict)
    );
    assert_eq!(target_state(&fixture).await?, (false, 2));
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn expired_and_stale_proposals_never_overwrite_monitor_configuration()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let expired_key = fixture.proposal("expired", NOW + 20).await?;
    let expired = fixture
        .proposals
        .confirm(
            &expired_key,
            fixture.confirmer.get(),
            "expired-confirmation",
            NOW + 20,
        )
        .await?;
    assert_eq!(expired.status, PluginActionProposalStatus::Expired);
    assert_eq!(target_state(&fixture).await?, (true, 1));

    let stale_key = fixture.proposal("stale", NOW + 300).await?;
    channel_monitor_targets::Entity::update_many()
        .col_expr(
            channel_monitor_targets::Column::ConfigVersion,
            Expr::value(2),
        )
        .filter(channel_monitor_targets::Column::Id.eq(fixture.target_id))
        .exec(fixture.pool.connection())
        .await?;
    let stale = fixture
        .proposals
        .confirm(
            &stale_key,
            fixture.confirmer.get(),
            "stale-confirmation",
            NOW + 20,
        )
        .await?;
    assert_eq!(stale.status, PluginActionProposalStatus::Stale);
    assert_eq!(target_state(&fixture).await?, (true, 2));
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn deployment_grant_and_artifact_revocation_invalidate_pending_actions()
-> Result<(), Box<dyn Error>> {
    for change in ["generation", "grant", "revocation"] {
        let fixture = Fixture::new().await?;
        let key = fixture.proposal(change, NOW + 300).await?;
        match change {
            "generation" => {
                plugin_deployments::Entity::update_many()
                    .col_expr(plugin_deployments::Column::Generation, Expr::value(2))
                    .filter(plugin_deployments::Column::PluginId.eq(PLUGIN_ID))
                    .exec(fixture.pool.connection())
                    .await?;
            }
            "grant" => {
                plugin_deployments::Entity::update_many()
                    .col_expr(
                        plugin_deployments::Column::CapabilityGrants,
                        Expr::value(serde_json::json!([])),
                    )
                    .filter(plugin_deployments::Column::PluginId.eq(PLUGIN_ID))
                    .exec(fixture.pool.connection())
                    .await?;
            }
            "revocation" => {
                plugin_artifact_revocations::ActiveModel {
                    artifact_id: Set(fixture.artifact_id),
                    revoked_by: Set(fixture.confirmer.get()),
                    revoked_at: Set(NOW + 15),
                    request_id: Set("revoke-action-plugin".to_owned()),
                    reason: Set("integration test".to_owned()),
                }
                .insert(fixture.pool.connection())
                .await?;
            }
            _ => unreachable!(),
        }
        let result = fixture
            .proposals
            .confirm(
                &key,
                fixture.confirmer.get(),
                "revoked-confirmation",
                NOW + 20,
            )
            .await?;
        assert_eq!(
            result.status,
            PluginActionProposalStatus::Revoked,
            "change={change}"
        );
        assert_eq!(target_state(&fixture).await?, (true, 1));
        fixture.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_administrator_confirmations_change_the_target_only_once()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let key = fixture.proposal("concurrent", NOW + 300).await?;
    let (first, second) = tokio::join!(
        fixture
            .proposals
            .confirm(&key, fixture.requester.get(), "administrator-one", NOW + 20),
        fixture
            .proposals
            .confirm(&key, fixture.confirmer.get(), "administrator-two", NOW + 20)
    );
    let confirmed = [first, second]
        .into_iter()
        .filter_map(Result::ok)
        .filter(|record| record.status == PluginActionProposalStatus::Confirmed)
        .count();
    assert_eq!(confirmed, 1);
    assert_eq!(target_state(&fixture).await?, (false, 2));
    assert_eq!(
        fixture.proposals.get_by_key(&key).await?.status,
        PluginActionProposalStatus::Confirmed
    );
    fixture.close().await?;
    Ok(())
}
