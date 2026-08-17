use std::{error::Error, time::Duration};

use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminCredentialCreateOutcome,
    AdminCredentialMutationOutcome, AdminCredentialWriteRecord, DatabaseOptions, MigrationOptions,
    OAuthCredentialRepository,
};
use af_domain::{
    ChannelId, ChannelType, CredentialId, CredentialKind, CredentialQuotaDimension, Protocol,
    Status, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use super::*;
use crate::{
    PlainCredentialSecret, PlainOAuthCredential,
    oauth::{
        OAuthAuthorizationContext, OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService,
        OAuthTokenSet,
    },
};

#[tokio::test]
async fn prepares_refresh_candidates_and_persists_with_stale_cas() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("完整 OAuth 凭据必须成为到期候选");
    let stale_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("并发读取必须观察同一候选事实");
    assert_eq!(first_candidate.channel_id(), fixture.channel_id);
    assert_eq!(first_candidate.credential_id(), fixture.credential_id);
    assert_eq!(first_candidate.provider(), UpstreamOAuthProvider::Codex);
    assert_eq!(first_candidate.expected_revision(), 1);
    let rendered = format!("{first_candidate:?}");
    for private in [
        "initial-access-private",
        "initial-refresh-private",
        "openid profile",
        "test-key",
    ] {
        assert!(!rendered.contains(private));
    }

    let (guard, request) = first_candidate.into_parts();
    assert_eq!(request.refresh_token(), "initial-refresh-private");
    assert_eq!(request.scope(), Some("openid profile"));
    assert_eq!(guard.expected_revision(), 1);
    assert_eq!(
        fixture
            .service
            .persist_at(
                guard,
                refreshed_token_set("new-access-private", "new-refresh-private"),
                std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            )
            .await?,
        OAuthRefreshPersistenceOutcome::Stored
    );

    let (stale_guard, _) = stale_candidate.into_parts();
    assert_eq!(
        fixture
            .service
            .persist_at(
                stale_guard,
                refreshed_token_set("stale-access-private", "stale-refresh-private"),
                std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_100),
            )
            .await?,
        OAuthRefreshPersistenceOutcome::Stale
    );

    let current = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("写回后的凭据仍可按新到期投影读取");
    assert_eq!(current.expected_revision(), 2);
    assert_eq!(current.expires_at_epoch_seconds(), 1_700_000_120);
    let (_, current_request) = current.into_parts();
    assert_eq!(current_request.refresh_token(), "new-refresh-private");
    assert_eq!(current_request.scope(), Some("openid email"));
    let current_debug = format!("{current_request:?}");
    for private in ["new-refresh-private", "openid email"] {
        assert!(!current_debug.contains(private));
    }

    assert_eq!(
        fixture.service.due_candidates(-1, 1).await.unwrap_err(),
        OAuthRefreshPersistenceError::InvalidCandidateQuery
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn backfills_legacy_expiration_without_promoting_access_only_credentials()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let admin = AdminChannelRepository::new(fixture.pool.clone(), Duration::from_secs(2))?;
    let legacy_complete = PlainOAuthCredential::new(
        "legacy-access-private".to_owned(),
        Some("legacy-refresh-private".to_owned()),
        Some(1_700_000_000),
        Some("openid profile".to_owned()),
    )?;
    let legacy_envelope = CredentialEncryptor::new(&settings())?.encrypt_oauth(
        fixture.channel_id,
        fixture.credential_id.get(),
        &legacy_complete,
    )?;
    drop(legacy_complete);
    let AdminCredentialMutationOutcome::Mutated(_) = admin
        .update_credential(
            fixture.channel_id,
            fixture.credential_id,
            oauth_credential_record(),
            Some(legacy_envelope),
        )
        .await?
    else {
        panic!("旧完整 OAuth 凭据必须可以模拟为空投影");
    };

    let access_only = PlainCredentialSecret::new(
        CredentialKind::Oauth,
        "legacy-access-only-private".to_owned(),
    )?;
    let access_only_encryptor = CredentialEncryptor::new(&settings())?;
    let AdminCredentialCreateOutcome::Created(access_only_record) = admin
        .create_credential(
            fixture.channel_id,
            oauth_credential_record(),
            |credential_id| {
                access_only_encryptor
                    .encrypt(fixture.channel_id, credential_id.get(), &access_only)
                    .map_err(|_| ())
            },
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };

    let batch = fixture
        .service
        .backfill_missing_expiration_projections(None, 2)
        .await?;
    assert_eq!(batch.scanned(), 2);
    assert_eq!(batch.projected(), 1);
    assert_eq!(batch.incomplete(), 1);
    assert_eq!(batch.conflicted(), 0);
    assert_eq!(
        batch.last_credential_id(),
        Some(access_only_record.credential_id())
    );

    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("回填后的完整凭据必须进入到期查询");
    assert_eq!(candidate.credential_id(), fixture.credential_id);
    assert_eq!(candidate.expected_revision(), 2);
    assert_eq!(candidate.expires_at_epoch_seconds(), 1_700_000_000);

    let remaining = fixture
        .service
        .backfill_missing_expiration_projections(None, 2)
        .await?;
    assert_eq!(remaining.scanned(), 1);
    assert_eq!(remaining.projected(), 0);
    assert_eq!(remaining.incomplete(), 1);
    assert_eq!(
        remaining.last_credential_id(),
        Some(access_only_record.credential_id())
    );

    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn provider_parser_accepts_only_closed_internal_identifiers() {
    for (value, expected) in [
        ("claude_code", UpstreamOAuthProvider::ClaudeCode),
        ("codex", UpstreamOAuthProvider::Codex),
        ("gemini", UpstreamOAuthProvider::Gemini),
        ("antigravity", UpstreamOAuthProvider::Antigravity),
    ] {
        assert_eq!(parse_provider(value), Some(expected));
    }
    for invalid in ["Codex", " codex", "custom", ""] {
        assert_eq!(parse_provider(invalid), None);
    }
}

struct Fixture {
    pool: af_db::DatabasePool,
    channel_id: ChannelId,
    credential_id: CredentialId,
    service: OAuthRefreshPersistenceService,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let admin = AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?;
    let channel_id = admin
        .create_channel(AdminChannelWriteRecord::new(
            "oauth-refresh".to_owned(),
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
        .channel_id();
    let bootstrap_encryptor = CredentialEncryptor::new(&settings())?;
    let bootstrap_secret =
        PlainCredentialSecret::new(CredentialKind::Oauth, "bootstrap-access".to_owned())?;
    let AdminCredentialCreateOutcome::Created(credential) = admin
        .create_credential(channel_id, oauth_credential_record(), |credential_id| {
            bootstrap_encryptor
                .encrypt(channel_id, credential_id.get(), &bootstrap_secret)
                .map_err(|_| ())
        })
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    let credential_id = credential.credential_id();
    let repository = OAuthCredentialRepository::new(pool.clone());
    let initial_persistence = OAuthTokenPersistenceService::new(
        CredentialEncryptor::new(&settings())?,
        repository.clone(),
    );
    assert_eq!(
        initial_persistence
            .persist(OAuthTokenSet::for_test(
                UpstreamOAuthProvider::Codex,
                OAuthAuthorizationContext::new(
                    UserId::new(11)?,
                    Some(channel_id),
                    Some(credential_id),
                )?,
                "initial-access-private".to_owned(),
                Some("initial-refresh-private".to_owned()),
                Some(Duration::from_secs(60)),
                Some("openid profile".to_owned()),
            ))
            .await?,
        OAuthTokenPersistenceOutcome::Stored
    );
    let service = OAuthRefreshPersistenceService::new(
        CredentialEncryptor::new(&settings())?,
        CredentialDecryptor::new(&settings())?,
        repository,
    );
    Ok(Fixture {
        pool,
        channel_id,
        credential_id,
        service,
    })
}

fn refreshed_token_set(access_token: &str, refresh_token: &str) -> OAuthRefreshedTokenSet {
    OAuthRefreshedTokenSet::for_test(
        UpstreamOAuthProvider::Codex,
        access_token.to_owned(),
        refresh_token.to_owned(),
        Some(Duration::from_secs(120)),
        Some("openid email".to_owned()),
    )
}

fn oauth_credential_record() -> AdminCredentialWriteRecord {
    AdminCredentialWriteRecord::new(
        CredentialKind::Oauth,
        Status::Enabled,
        Some(2),
        30,
        40,
        Some(5),
        Some(1_250_000),
        Some(875_000),
        true,
        None,
        CredentialQuotaDimension::Global,
        None,
        Some("codex".to_owned()),
        Some("account-key".to_owned()),
        Some("project-id".to_owned()),
    )
}

fn settings() -> CredentialEncryptionSettings {
    serde_json::from_value(serde_json::json!({
        "key_id": "test-key",
        "key": URL_SAFE_NO_PAD.encode([0x5a; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap()
}
