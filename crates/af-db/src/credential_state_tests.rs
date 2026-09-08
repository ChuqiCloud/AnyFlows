use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status, UpstreamRetryAfter,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, EntityTrait, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
};

use crate::{
    CredentialStateChange, CredentialStateEvent, CredentialStateRepository,
    CredentialStateRepositoryError, DatabaseOptions, MigrationOptions,
    entity::{EncryptedJson, HeaderOverrides, SensitiveJson, channels, credentials},
};

#[tokio::test]
async fn repository_writes_closed_runtime_states_and_success_clears_cooling()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    let channel = test_channel("credential-state")
        .insert(pool.connection())
        .await?;
    let auth = test_credential(channel.id, 0x11)
        .insert(pool.connection())
        .await?;
    let limited = test_credential(channel.id, 0x22)
        .insert(pool.connection())
        .await?;
    let quota = test_credential(channel.id, 0x33)
        .insert(pool.connection())
        .await?;
    let overloaded = test_credential(channel.id, 0x44)
        .insert(pool.connection())
        .await?;
    let revoked = test_credential(channel.id, 0x55)
        .insert(pool.connection())
        .await?;
    let disabled = test_credential(channel.id, 0x56)
        .insert(pool.connection())
        .await?;
    let missing_refresh = test_credential_for_kind(channel.id, 0x57, CredentialKind::Oauth)
        .insert(pool.connection())
        .await?;
    let repository = CredentialStateRepository::new(pool.clone());
    let before = TimeDateTimeWithTimeZone::now_utc();

    let report = repository
        .apply(&[
            event(channel.id, auth.id, CredentialStateChange::AuthExpired),
            event(
                channel.id,
                limited.id,
                CredentialStateChange::RateLimited {
                    retry_after: Some(UpstreamRetryAfter::from_seconds(90).unwrap()),
                },
            ),
            event(channel.id, quota.id, CredentialStateChange::QuotaExhausted),
            event(
                channel.id,
                overloaded.id,
                CredentialStateChange::Overloaded {
                    retry_after: Some(UpstreamRetryAfter::from_seconds(45).unwrap()),
                },
            ),
            event(channel.id, revoked.id, CredentialStateChange::AuthRevoked),
            event(
                channel.id,
                disabled.id,
                CredentialStateChange::AccountDisabled,
            ),
            event_for_kind(
                channel.id,
                missing_refresh.id,
                CredentialKind::Oauth,
                CredentialStateChange::MissingRefreshToken,
            ),
        ])
        .await?;
    assert_eq!(report.event_count(), 7);
    assert_eq!(report.changed_count(), 7);

    let auth = credential(&pool, auth.id).await;
    assert!(
        auth.temp_unschedulable_until
            .is_some_and(|until| until > before)
    );
    assert_eq!(
        auth.temp_unschedulable_reason.as_deref(),
        Some("auth_expired")
    );
    let limited = credential(&pool, limited.id).await;
    assert_eq!(limited.oauth_revision, 0);
    assert!(limited.rate_limited_at.is_some());
    assert!(
        limited
            .rate_limit_reset_at
            .is_some_and(|until| until >= before + Duration::from_secs(90))
    );
    let quota = credential(&pool, quota.id).await;
    assert!(
        quota
            .temp_unschedulable_until
            .is_some_and(|until| until > before)
    );
    assert_eq!(
        quota.temp_unschedulable_reason.as_deref(),
        Some("quota_exhausted")
    );
    let overloaded = credential(&pool, overloaded.id).await;
    assert!(
        overloaded
            .overload_until
            .is_some_and(|until| until >= before + Duration::from_secs(45))
    );
    assert_eq!(
        credential(&pool, revoked.id).await.status,
        Status::AutoDisabled.code()
    );
    assert_eq!(
        credential(&pool, disabled.id).await.status,
        Status::AutoDisabled.code()
    );
    let missing_refresh = credential(&pool, missing_refresh.id).await;
    assert_eq!(missing_refresh.status, Status::AutoDisabled.code());
    assert!(missing_refresh.temp_unschedulable_until.is_none());
    assert!(missing_refresh.temp_unschedulable_reason.is_none());

    repository
        .apply(&[event(
            channel.id,
            limited.id,
            CredentialStateChange::Succeeded,
        )])
        .await?;
    let recovered = credential(&pool, limited.id).await;
    assert_eq!(recovered.oauth_revision, 0);
    assert!(recovered.last_used_at.is_some());
    assert!(recovered.rate_limited_at.is_none());
    assert!(recovered.rate_limit_reset_at.is_none());
    assert!(recovered.overload_until.is_none());
    assert!(recovered.temp_unschedulable_until.is_none());
    assert!(recovered.temp_unschedulable_reason.is_none());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_duplicate_events_and_rolls_back_invalid_membership()
-> Result<(), Box<dyn Error>> {
    let pool = test_pool().await?;
    assert_eq!(
        CredentialStateRepository::with_mutation_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        CredentialStateRepositoryError::InvalidConfiguration
    );
    let channel = test_channel("credential-rollback")
        .insert(pool.connection())
        .await?;
    let first = test_credential(channel.id, 0x66)
        .insert(pool.connection())
        .await?;
    let second = test_credential(channel.id, 0x77)
        .insert(pool.connection())
        .await?;
    let repository = CredentialStateRepository::new(pool.clone());
    let duplicate = event(channel.id, first.id, CredentialStateChange::Succeeded);
    assert_eq!(
        CredentialStateEvent::new(
            ChannelId::new(channel.id).unwrap(),
            CredentialId::new(first.id).unwrap(),
            CredentialKind::ApiKey,
            CredentialStateChange::MissingRefreshToken,
        ),
        Err(CredentialStateRepositoryError::InvalidEvent)
    );
    assert_eq!(
        repository.apply(&[duplicate, duplicate]).await,
        Err(CredentialStateRepositoryError::InvalidBatch)
    );

    let invalid_channel = ChannelId::new(channel.id.checked_add(1).unwrap()).unwrap();
    let invalid = CredentialStateEvent::new(
        invalid_channel,
        CredentialId::new(second.id).unwrap(),
        CredentialKind::ApiKey,
        CredentialStateChange::RateLimited { retry_after: None },
    )?;
    assert_eq!(
        repository.apply(&[duplicate, invalid]).await,
        Err(CredentialStateRepositoryError::Invariant)
    );
    assert!(credential(&pool, first.id).await.last_used_at.is_none());

    pool.close().await?;
    Ok(())
}

async fn test_pool() -> Result<crate::DatabasePool, Box<dyn Error>> {
    Ok(crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?)
}

fn event(
    channel_id: i64,
    credential_id: i64,
    change: CredentialStateChange,
) -> CredentialStateEvent {
    event_for_kind(channel_id, credential_id, CredentialKind::ApiKey, change)
}

fn event_for_kind(
    channel_id: i64,
    credential_id: i64,
    credential_kind: CredentialKind,
    change: CredentialStateChange,
) -> CredentialStateEvent {
    CredentialStateEvent::new(
        ChannelId::new(channel_id).unwrap(),
        CredentialId::new(credential_id).unwrap(),
        credential_kind,
        change,
    )
    .unwrap()
}

async fn credential(pool: &crate::DatabasePool, credential_id: i64) -> credentials::Model {
    credentials::Entity::find_by_id(credential_id)
        .one(pool.connection())
        .await
        .unwrap()
        .unwrap()
}

fn test_channel(name: &str) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set(ChannelType::OpenAi.as_str().to_owned()),
        protocol: Set(Protocol::OpenAiChat.as_str().to_owned()),
        status: Set(Status::Enabled.code()),
        model_mapping: Set(empty_object()),
        param_override: Set(empty_object()),
        header_override: Set(HeaderOverrides::validate(empty_object()).unwrap()),
        settings: Set(SensitiveJson::from(empty_object())),
        ..Default::default()
    }
}

fn test_credential(channel_id: i64, marker: u8) -> credentials::ActiveModel {
    test_credential_for_kind(channel_id, marker, CredentialKind::ApiKey)
}

fn test_credential_for_kind(
    channel_id: i64,
    marker: u8,
    credential_kind: CredentialKind,
) -> credentials::ActiveModel {
    let envelope = Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String("state-test".to_owned())),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    );
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(credential_kind.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(envelope).unwrap()),
        status: Set(Status::Enabled.code()),
        priority: Set(0),
        weight: Set(10),
        schedulable: Set(true),
        ..Default::default()
    }
}

fn empty_object() -> Json {
    Json::Object(Default::default())
}
