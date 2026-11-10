use super::*;
use crate::{
    DatabaseOptions, MigrationOptions,
    entity::{EncryptedJson, HeaderOverrides, SensitiveJson},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{DbBackend, Statement};
use std::error::Error;

async fn fixture() -> Result<(DatabasePool, CredentialModelsRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    channels::ActiveModel {
        id: Set(1),
        name: Set("model-policy-fixture".to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_responses".to_owned()),
        status: Set(1),
        model_mapping: Set(serde_json::json!({"public-a":"Vendor/A","public-b":"Vendor/B"})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({}))?),
        settings: Set(SensitiveJson::from(serde_json::json!({}))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    for id in [10, 11] {
        credentials::ActiveModel {
            id: Set(id),
            channel_id: Set(1),
            kind: Set("api_key".to_owned()),
            status: Set(1),
            schedulable: Set(true),
            secret: Set(secret("fixture-key")?),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }
    for model in ["public-a", "public-b"] {
        channel_models::ActiveModel {
            channel_id: Set(1),
            model: Set(model.to_owned()),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }
    Ok((
        pool.clone(),
        CredentialModelsRepository::new(pool, Duration::from_secs(5)),
    ))
}

fn secret(key: &str) -> Result<EncryptedJson, Box<dyn Error>> {
    Ok(EncryptedJson::from_envelope(
        serde_json::json!({"version":1,"algorithm":"xchacha20poly1305","key_id":key,
        "nonce":URL_SAFE_NO_PAD.encode([1;24]),"ciphertext":URL_SAFE_NO_PAD.encode([2;32])}),
    )?)
}

#[tokio::test]
async fn policies_are_versioned_scoped_and_cannot_publish_unknown_supply()
-> Result<(), Box<dyn Error>> {
    let (pool, repo) = fixture().await?;
    let channel = ChannelId::new(1)?;
    let key = CredentialId::new(10)?;
    let initial = repo.policy(channel, Some(key)).await?;
    assert_eq!(initial.version, 0);
    let write = UpstreamModelPolicySettings {
        policy: UpstreamModelPolicy {
            mode: UpstreamModelPolicyMode::Allowlist,
            models: vec!["Vendor/A".to_owned(), "Unpublished".to_owned()],
            disabled_models: vec![],
        },
        ..Default::default()
    };
    assert_eq!(
        repo.write_policy(channel, Some(key), write.clone())
            .await?
            .version,
        1
    );
    assert_eq!(
        repo.write_policy(channel, Some(key), write)
            .await
            .unwrap_err(),
        CredentialModelError::Conflict
    );
    assert_eq!(
        repo.policy(ChannelId::new(2)?, Some(key))
            .await
            .unwrap_err(),
        CredentialModelError::NotFound
    );
    let snapshot = repo.models(channel, key).await?;
    assert!(
        snapshot
            .items
            .iter()
            .find(|item| item.upstream_model == "Vendor/A")
            .unwrap()
            .enabled
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.upstream_model == "Unpublished")
            .unwrap()
            .blocked_reason
            .as_deref(),
        Some("channel_not_published")
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_leases_keep_last_success_reject_rotated_keys_and_distinguish_empty()
-> Result<(), Box<dyn Error>> {
    let (pool, repo) = fixture().await?;
    let channel = ChannelId::new(1)?;
    let key = CredentialId::new(10)?;
    let lease = repo.claim_refresh(channel, key, true).await?.unwrap();
    assert!(repo.claim_refresh(channel, key, true).await?.is_none());
    repo.complete_refresh(lease, Ok(vec!["Vendor/A".to_owned()]))
        .await?;
    pool.connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE credential_model_observations SET last_checked_at=0, next_refresh_at=0"
                .to_owned(),
        ))
        .await?;
    let lease = repo.claim_refresh(channel, key, true).await?.unwrap();
    repo.complete_refresh(lease, Err("timeout".to_owned()))
        .await?;
    let failed = repo.models(channel, key).await?;
    assert_eq!(failed.status, "unavailable");
    assert!(failed.last_success_at.is_some());
    assert!(
        failed
            .items
            .iter()
            .any(|item| item.source == "discovered" && item.upstream_model == "Vendor/A")
    );
    pool.connection()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE credential_model_observations SET last_checked_at=0, next_refresh_at=0"
                .to_owned(),
        ))
        .await?;
    let lease = repo.claim_refresh(channel, key, true).await?.unwrap();
    credentials::ActiveModel {
        id: Set(10),
        secret: Set(secret("rotated-key")?),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    assert_eq!(
        repo.complete_refresh(lease, Ok(vec!["stale".to_owned()]))
            .await
            .unwrap_err(),
        CredentialModelError::Conflict
    );
    assert_eq!(
        repo.models(channel, key).await?.error_code.as_deref(),
        Some("identity_changed")
    );
    let lease = repo.claim_refresh(channel, key, true).await?.unwrap();
    repo.complete_refresh(lease, Ok(vec![])).await?;
    assert_eq!(repo.models(channel, key).await?.status, "empty");
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn auto_add_preserves_manual_denials_and_background_skips_disabled_keys()
-> Result<(), Box<dyn Error>> {
    let (pool, repo) = fixture().await?;
    let channel = ChannelId::new(1)?;
    let key = CredentialId::new(10)?;
    repo.write_policy(
        channel,
        Some(key),
        UpstreamModelPolicySettings {
            policy: UpstreamModelPolicy {
                mode: UpstreamModelPolicyMode::Allowlist,
                models: vec![],
                disabled_models: vec!["Vendor/B".to_owned()],
            },
            auto_add_discovered: true,
            refresh_interval_secs: Some(300),
            version: 0,
        },
    )
    .await?;
    let lease = repo.claim_refresh(channel, key, true).await?.unwrap();
    repo.complete_refresh(
        lease,
        Ok(vec!["Vendor/A".to_owned(), "Vendor/B".to_owned()]),
    )
    .await?;
    let policy = repo.policy(channel, Some(key)).await?;
    assert!(policy.policy.allows("Vendor/A"));
    assert!(!policy.policy.allows("Vendor/B"));
    credentials::ActiveModel {
        id: Set(10),
        status: Set(2),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    assert!(repo.due_credentials(100).await?.is_empty());
    assert!(
        matches!(ModelSyncRepository::new(pool.clone()).load_discovery_target_for_credential(channel, key, true).await?, ModelDiscoveryTargetLookup::Found(target) if target.credential_id() == 10)
    );
    assert!(matches!(
        ModelSyncRepository::new(pool.clone())
            .load_discovery_target_for_credential(channel, key, false)
            .await?,
        ModelDiscoveryTargetLookup::NotFound
    ));
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn channel_refresh_defaults_and_explicit_null_overrides_are_distinct()
-> Result<(), Box<dyn Error>> {
    let (pool, repo) = fixture().await?;
    let channel = ChannelId::new(1)?;
    repo.write_policy(
        channel,
        None,
        UpstreamModelPolicySettings {
            refresh_interval_secs: Some(300),
            ..Default::default()
        },
    )
    .await?;
    let inherited = repo.policy(channel, Some(CredentialId::new(10)?)).await?;
    assert_eq!(inherited.version, 0);
    assert_eq!(inherited.refresh_interval_secs, Some(300));
    repo.write_policy(
        channel,
        Some(CredentialId::new(10)?),
        UpstreamModelPolicySettings::default(),
    )
    .await?;
    assert_eq!(
        repo.due_credentials(1).await?,
        vec![(channel, CredentialId::new(11)?)]
    );
    let lease = repo
        .claim_refresh(channel, CredentialId::new(11)?, true)
        .await?
        .unwrap();
    repo.complete_refresh(lease, Ok(vec![])).await?;
    assert!(repo.due_credentials(1).await?.is_empty());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn shadow_uses_parent_discovery_identity_and_oauth_revision_invalidates_the_lease()
-> Result<(), Box<dyn Error>> {
    let (pool, repo) = fixture().await?;
    let channel = ChannelId::new(1)?;
    credentials::ActiveModel {
        id: Set(10),
        kind: Set("oauth".to_owned()),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    credentials::ActiveModel {
        id: Set(20),
        channel_id: Set(1),
        kind: Set("oauth".to_owned()),
        parent_id: Set(Some(10)),
        quota_dimension: Set("spark".to_owned()),
        status: Set(1),
        schedulable: Set(true),
        secret: Set(secret("shadow-reference")?),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let sync = ModelSyncRepository::new(pool.clone());
    let ModelDiscoveryTargetLookup::Found(target) = sync
        .load_discovery_target_for_credential(channel, CredentialId::new(20)?, true)
        .await?
    else {
        panic!("shadow source must resolve")
    };
    assert_eq!(target.credential_id(), 10);
    let lease = repo
        .claim_refresh(channel, CredentialId::new(20)?, true)
        .await?
        .unwrap();
    assert!(
        repo.claim_refresh(channel, CredentialId::new(10)?, true)
            .await?
            .is_none()
    );
    credentials::ActiveModel {
        id: Set(10),
        oauth_revision: Set(1),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    assert_eq!(
        repo.complete_refresh(lease, Ok(vec!["old-account-model".to_owned()]))
            .await
            .unwrap_err(),
        CredentialModelError::Conflict
    );
    pool.close().await?;
    Ok(())
}

#[test]
fn runtime_policy_projection_preserves_shadow_independence_and_secret_identity()
-> Result<(), Box<dyn Error>> {
    use crate::{
        EncryptedCredentialEnvelope, SchedulerAbilityRecord, SchedulerRuntimeCredentialRecord,
        SchedulerRuntimeProjection, SchedulerRuntimeRecord, SchedulerRuntimeTargetRecord,
    };
    use af_domain::{ChannelType, CredentialKind, CredentialQuotaDimension, GroupId, Protocol};
    let parent = CredentialId::new(10)?;
    let shadow = CredentialId::new(20)?;
    let root = SchedulerRuntimeCredentialRecord::new(
        10,
        CredentialKind::Oauth,
        EncryptedCredentialEnvelope::new("policy-parent", [1; 24], vec![2; 32])?,
        false,
    )?;
    let spark = SchedulerRuntimeCredentialRecord::with_runtime_identity(
        shadow,
        parent,
        parent,
        parent,
        CredentialQuotaDimension::Spark,
        CredentialKind::Oauth,
        root.envelope().clone(),
        false,
        0,
        1,
        None,
    )?;
    let channel = ChannelId::new(1)?;
    let policy = |names: &[&str]| UpstreamModelPolicy {
        mode: UpstreamModelPolicyMode::Allowlist,
        models: names.iter().map(|name| (*name).to_owned()).collect(),
        disabled_models: vec![],
    };
    let target = SchedulerRuntimeTargetRecord::new_pool(
        channel,
        ChannelType::OpenAi,
        Protocol::OpenAiResponses,
        None,
        vec![root.clone(), spark.clone()],
        vec![],
    )?
    .with_model_policies(
        UpstreamModelPolicy::default(),
        BTreeMap::from([(10, policy(&["A", "Spark"])), (20, policy(&["Spark"]))]),
    )?;
    assert!(target.allows_credential_model(&root, "A"));
    assert!(!target.allows_credential_model(&spark, "A"));
    assert!(target.allows_credential_model(&spark, "Spark"));
    assert!(!target.allows_credential_model(&spark, "B"));
    let paused = target
        .clone()
        .with_usage_pauses(BTreeMap::from([(10, now_seconds() + 300)]));
    assert!(!paused.allows_credential_model(&root, "A"));
    assert!(
        paused.allows_credential_model(&spark, "Spark"),
        "global window exhaustion must not pause Spark"
    );
    assert_eq!(paused.credentials()[0], root);
    let record = SchedulerRuntimeRecord::new(
        SchedulerAbilityRecord::from_projection_parts(
            GroupId::new(1)?,
            "model".to_owned(),
            channel,
            1,
            1,
        )?,
        std::sync::Arc::new(target),
    );
    let bytes = SchedulerRuntimeProjection::new(
        1,
        SchedulerCatalogSubject::Channel(channel),
        vec![record],
    )?
    .encode()?;
    let decoded = SchedulerRuntimeProjection::decode(&bytes)?;
    assert!(
        !decoded.records()[0]
            .target()
            .allows_credential_model(&spark, "A")
    );
    let mut old: serde_json::Value = serde_json::from_slice(&bytes)?;
    old["wire_version"] = serde_json::json!(1);
    assert!(SchedulerRuntimeProjection::decode(&serde_json::to_vec(&old)?).is_err());
    let debug = format!("{:?}", decoded.records()[0].target());
    assert!(!debug.contains("Spark"));
    Ok(())
}

#[test]
fn credential_model_observation_insert_conflicts_are_portable_without_overwriting_evidence() {
    use sea_orm::{
        QueryTrait,
        sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder},
    };
    let insert = observations::Entity::insert(observations::ActiveModel {
        credential_id: Set(10),
        channel_id: Set(1),
        identity_fingerprint: Set("fingerprint".to_owned()),
        items: Set(serde_json::json!([])),
        status: Set("unknown".to_owned()),
        error_code: Set(None),
        last_success_at: Set(None),
        last_checked_at: Set(None),
        lease_token: Set(None),
        lease_until: Set(None),
        next_refresh_at: Set(0),
    })
    .on_conflict(
        OnConflict::column(observations::Column::CredentialId)
            .do_nothing_on([observations::Column::CredentialId])
            .to_owned(),
    );
    let query = insert.as_query();
    let mysql = query.to_string(MysqlQueryBuilder);
    assert!(mysql.contains("ON DUPLICATE KEY UPDATE `credential_id` = `credential_id`"));
    assert!(!mysql.contains("IGNORE"));
    for sql in [
        query.to_string(PostgresQueryBuilder),
        query.to_string(SqliteQueryBuilder),
    ] {
        assert!(sql.contains("ON CONFLICT (\"credential_id\") DO NOTHING"));
    }
}
