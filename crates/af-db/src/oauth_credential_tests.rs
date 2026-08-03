use std::{error::Error, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};

use super::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminCredentialCreateOutcome,
    AdminCredentialDeleteOutcome, AdminCredentialWriteRecord, DatabaseOptions, MigrationOptions,
    OAuthCredentialRepository, OAuthCredentialRepositoryError, OAuthCredentialTokenUpdateOutcome,
    entity::credentials,
};
use crate::EncryptedCredentialEnvelope;

#[tokio::test]
async fn oauth_token_update_preserves_scheduler_and_runtime_fields() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "primary").await?;
    let credential_id =
        create_credential(&fixture, channel_id, CredentialKind::Oauth, None, 0x11).await?;

    let mut active = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    active.status = Set(Status::AutoDisabled.code());
    active.rate_limited_at = Set(Some(now));
    active.rate_limit_reset_at = Set(Some(now + Duration::from_secs(60)));
    active.overload_until = Set(Some(now + Duration::from_secs(120)));
    active.temp_unschedulable_until = Set(Some(now + Duration::from_secs(180)));
    active.temp_unschedulable_reason = Set(Some("测试冷却".to_owned()));
    active.session_window_start = Set(Some(now));
    active.session_window_end = Set(Some(now + Duration::from_secs(3_600)));
    active.last_used_at = Set(Some(now));
    active.update(fixture.pool.connection()).await?;
    let before = credential_model(&fixture, credential_id).await?;

    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(channel_id, credential_id, "codex", envelope(0x22))
            .await?,
        OAuthCredentialTokenUpdateOutcome::Updated
    );
    let after = credential_model(&fixture, credential_id).await?;
    assert_ne!(after.secret, before.secret);
    assert_eq!(after.oauth_provider.as_deref(), Some("codex"));
    let (key_id, nonce, ciphertext) = after.secret.envelope_parts()?;
    assert_eq!(key_id, "oauth-write-test-key");
    assert_eq!(nonce, [0x22; 24]);
    assert_eq!(ciphertext, vec![0x22; 32]);

    // 除密文、provider 与审计更新时间外，OAuth 回调不得重写任何业务或运行字段。
    let mut normalized = after;
    normalized.secret = before.secret.clone();
    normalized.oauth_provider = before.oauth_provider.clone();
    normalized.updated_at = before.updated_at;
    assert_eq!(normalized, before);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn oauth_token_update_rejects_wrong_targets_without_changing_secrets()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_channel = create_channel(&fixture, "first").await?;
    let second_channel = create_channel(&fixture, "second").await?;
    let oauth_id =
        create_credential(&fixture, first_channel, CredentialKind::Oauth, None, 0x31).await?;
    let api_key_id =
        create_credential(&fixture, first_channel, CredentialKind::ApiKey, None, 0x32).await?;
    let provider_id = create_credential(
        &fixture,
        first_channel,
        CredentialKind::Oauth,
        Some("claude_code"),
        0x33,
    )
    .await?;
    let deleted_id =
        create_credential(&fixture, first_channel, CredentialKind::Oauth, None, 0x34).await?;
    assert_eq!(
        fixture
            .admin_repository
            .delete_credential(first_channel, deleted_id)
            .await?,
        AdminCredentialDeleteOutcome::Deleted
    );

    let oauth_secret = credential_model(&fixture, oauth_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(second_channel, oauth_id, "codex", envelope(0x41))
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, oauth_id).await?.secret,
        oauth_secret
    );

    let api_key_secret = credential_model(&fixture, api_key_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(first_channel, api_key_id, "codex", envelope(0x42))
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, api_key_id).await?.secret,
        api_key_secret
    );

    let deleted_secret = credential_model(&fixture, deleted_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(first_channel, deleted_id, "codex", envelope(0x43))
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, deleted_id).await?.secret,
        deleted_secret
    );

    let provider_secret = credential_model(&fixture, provider_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(first_channel, provider_id, "codex", envelope(0x44))
            .await?,
        OAuthCredentialTokenUpdateOutcome::ProviderMismatch
    );
    assert_eq!(
        credential_model(&fixture, provider_id).await?.secret,
        provider_secret
    );

    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                CredentialId::new(i64::MAX)?,
                "codex",
                envelope(0x45),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(first_channel, oauth_id, "Codex", envelope(0x46))
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidProvider
    );
    assert_eq!(
        credential_model(&fixture, oauth_id).await?.secret,
        oauth_secret
    );
    assert_eq!(
        OAuthCredentialRepository::with_operation_timeout(fixture.pool.clone(), Duration::ZERO,)
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidConfiguration
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    admin_repository: AdminChannelRepository,
    oauth_repository: OAuthCredentialRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        admin_repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?,
        oauth_repository: OAuthCredentialRepository::new(pool.clone()),
        pool,
    })
}

async fn create_channel(fixture: &Fixture, name: &str) -> Result<ChannelId, Box<dyn Error>> {
    Ok(fixture
        .admin_repository
        .create_channel(AdminChannelWriteRecord::new(
            name.to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://api.example.com/v1".to_owned()),
            Some(af_domain::ChannelTimeout::new(60)?),
            Status::Enabled,
            10,
            20,
            true,
            Vec::new(),
            Vec::new(),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            None,
        ))
        .await?
        .channel_id())
}

async fn create_credential(
    fixture: &Fixture,
    channel_id: ChannelId,
    kind: CredentialKind,
    oauth_provider: Option<&str>,
    marker: u8,
) -> Result<CredentialId, Box<dyn Error>> {
    let AdminCredentialCreateOutcome::Created(record) = fixture
        .admin_repository
        .create_credential(
            channel_id,
            AdminCredentialWriteRecord::new(
                kind,
                Status::Enabled,
                Some(2),
                30,
                40,
                Some(5),
                Some(1_250_000),
                Some(875_000),
                true,
                None,
                "global",
                Some(99),
                oauth_provider.map(str::to_owned),
                (kind == CredentialKind::Oauth).then(|| "account-key".to_owned()),
                (kind == CredentialKind::Oauth).then(|| "project-id".to_owned()),
            ),
            |_| Ok(envelope(marker)),
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    Ok(record.credential_id())
}

async fn credential_model(
    fixture: &Fixture,
    credential_id: CredentialId,
) -> Result<credentials::Model, Box<dyn Error>> {
    Ok(credentials::Entity::find_by_id(credential_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试凭据必须存在"))
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("oauth-write-test-key", [marker; 24], vec![marker; 32])
        .unwrap()
}
