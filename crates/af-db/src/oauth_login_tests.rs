use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, JsonValue, Set};

use super::{
    CREDENTIAL_ENVELOPE_NONCE_BYTES, DatabaseOptions, EncryptedCredentialEnvelope,
    MigrationOptions, OAUTH_LOGIN_DISCORD_PROVIDER, OAUTH_LOGIN_GITHUB_PROVIDER,
    OAUTH_LOGIN_GOOGLE_PROVIDER, OAUTH_LOGIN_LINUXDO_PROVIDER, OAUTH_LOGIN_OIDC_PROVIDER,
    OAUTH_LOGIN_TELEGRAM_PROVIDER, OAUTH_LOGIN_WECHAT_PROVIDER, OAuthIdentityCompletion,
    OAuthLoginProviderWriteRecord, OAuthLoginRepository, OAuthLoginRepositoryError,
    OAuthLoginSecretUpdate,
};
use crate::entity::{authentication_settings, groups, users};

const STATE_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND_STATE_DIGEST: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const THIRD_STATE_DIGEST: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const TICKET_DIGEST: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const SECOND_TICKET_DIGEST: &str =
    "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const THIRD_TICKET_DIGEST: &str =
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const FOURTH_STATE_DIGEST: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
const FOURTH_TICKET_DIGEST: &str =
    "2222222222222222222222222222222222222222222222222222222222222222";

#[tokio::test]
async fn provider_settings_use_version_cas_and_never_require_plaintext_reads()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let initial = fixture
        .repository
        .provider(OAUTH_LOGIN_GITHUB_PROVIDER)
        .await?;
    assert!(!initial.available());
    assert!(!initial.client_secret_configured());
    let discord = fixture
        .repository
        .provider(OAUTH_LOGIN_DISCORD_PROVIDER)
        .await?;
    assert!(!discord.available());
    assert_eq!(discord.version(), 1);
    let oidc = fixture
        .repository
        .provider(OAUTH_LOGIN_OIDC_PROVIDER)
        .await?;
    assert!(!oidc.available());
    assert_eq!(oidc.issuer_url(), None);
    let linuxdo = fixture
        .repository
        .provider(OAUTH_LOGIN_LINUXDO_PROVIDER)
        .await?;
    assert!(!linuxdo.available());
    assert_eq!(linuxdo.issuer_url(), Some("https://connect.linux.do"));
    let wechat = fixture
        .repository
        .provider(OAUTH_LOGIN_WECHAT_PROVIDER)
        .await?;
    assert!(!wechat.available());
    assert_eq!(wechat.issuer_url(), None);
    let telegram = fixture
        .repository
        .provider(OAUTH_LOGIN_TELEGRAM_PROVIDER)
        .await?;
    assert!(!telegram.available());
    assert_eq!(telegram.issuer_url(), Some("https://oauth.telegram.org"));
    let google = fixture
        .repository
        .provider(OAUTH_LOGIN_GOOGLE_PROVIDER)
        .await?;
    assert!(!google.available());
    assert_eq!(google.issuer_url(), Some("https://accounts.google.com"));

    assert_eq!(
        fixture
            .repository
            .update_provider(
                OAUTH_LOGIN_OIDC_PROVIDER,
                OAuthLoginProviderWriteRecord::new(
                    oidc.version(),
                    true,
                    Some("oidc-client-id".to_owned()),
                    None,
                    OAuthLoginSecretUpdate::Replace(test_envelope()?),
                ),
            )
            .await,
        Err(OAuthLoginRepositoryError::InvalidInput)
    );

    let saved = fixture
        .repository
        .update_provider(
            OAUTH_LOGIN_GITHUB_PROVIDER,
            OAuthLoginProviderWriteRecord::new(
                initial.version(),
                true,
                Some("github-client-id".to_owned()),
                None,
                OAuthLoginSecretUpdate::Replace(test_envelope()?),
            ),
        )
        .await?;
    assert!(saved.available());
    assert!(saved.client_secret_configured());
    assert_eq!(saved.version(), initial.version() + 1);
    assert_eq!(
        fixture
            .repository
            .provider(OAUTH_LOGIN_DISCORD_PROVIDER)
            .await?
            .version(),
        discord.version()
    );
    let debug = format!("{saved:?}");
    assert!(!debug.contains("github-client-id"));
    assert!(!debug.contains("oauth-test-key"));

    assert_eq!(
        fixture
            .repository
            .update_provider(
                OAUTH_LOGIN_GITHUB_PROVIDER,
                OAuthLoginProviderWriteRecord::new(
                    initial.version(),
                    false,
                    None,
                    None,
                    OAuthLoginSecretUpdate::Clear,
                ),
            )
            .await,
        Err(OAuthLoginRepositoryError::ConcurrentUpdate)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_and_ticket_are_single_use_and_expire_at_the_boundary() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, STATE_DIGEST, 100)
        .await?;
    assert_eq!(
        fixture
            .repository
            .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, STATE_DIGEST, 100)
            .await,
        Err(OAuthLoginRepositoryError::Rejected)
    );

    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, SECOND_STATE_DIGEST, 200)
        .await?;
    let claim = fixture
        .repository
        .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, SECOND_STATE_DIGEST, 100)
        .await?;
    assert_eq!(
        fixture
            .repository
            .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, SECOND_STATE_DIGEST, 100)
            .await,
        Err(OAuthLoginRepositoryError::Rejected)
    );
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                OAUTH_LOGIN_GITHUB_PROVIDER,
                "10001",
                "gh_test_10001",
                TICKET_DIGEST,
                150,
                110,
            )
            .await?,
        OAuthIdentityCompletion::Rejected
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_identity_requires_registration_but_bound_identity_does_not()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    enable_registration(&fixture, true).await?;
    let user_id = complete_new_identity(
        &fixture,
        STATE_DIGEST,
        TICKET_DIGEST,
        "20001",
        "gh_bound_20001",
    )
    .await?;
    assert_eq!(
        fixture
            .repository
            .consume_ticket(TICKET_DIGEST, 130)
            .await?,
        user_id
    );
    assert_eq!(
        fixture.repository.consume_ticket(TICKET_DIGEST, 130).await,
        Err(OAuthLoginRepositoryError::Rejected)
    );

    let user = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("OAuth 新用户必须持久化");
    assert_eq!(user.password_hash, None);
    assert_eq!(user.email, None);

    enable_registration(&fixture, false).await?;
    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, SECOND_STATE_DIGEST, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, SECOND_STATE_DIGEST, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                OAUTH_LOGIN_GITHUB_PROVIDER,
                "20001",
                "unused_existing_username",
                SECOND_TICKET_DIGEST,
                260,
                210,
            )
            .await?,
        OAuthIdentityCompletion::Issued(user_id)
    );
    assert_eq!(
        fixture
            .repository
            .consume_ticket(SECOND_TICKET_DIGEST, 260)
            .await,
        Err(OAuthLoginRepositoryError::Rejected)
    );

    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, THIRD_STATE_DIGEST, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, THIRD_STATE_DIGEST, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                OAUTH_LOGIN_GITHUB_PROVIDER,
                "20002",
                "gh_unknown_20002",
                THIRD_TICKET_DIGEST,
                260,
                210,
            )
            .await?,
        OAuthIdentityCompletion::Rejected
    );

    let mut disabled = user.into_active_model();
    disabled.status = Set(2);
    disabled.update(fixture.pool.connection()).await?;
    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, FOURTH_STATE_DIGEST, 400)
        .await?;
    let claim = fixture
        .repository
        .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, FOURTH_STATE_DIGEST, 300)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                OAUTH_LOGIN_GITHUB_PROVIDER,
                "20001",
                "unused_disabled_username",
                FOURTH_TICKET_DIGEST,
                360,
                310,
            )
            .await,
        Err(OAuthLoginRepositoryError::Rejected)
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: OAuthLoginRepository,
    group_id: i64,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("oauth-users".to_owned()),
        display_name: Set("OAuth Users".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: OAuthLoginRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        group_id: group.id,
    })
}

async fn enable_registration(fixture: &Fixture, enabled: bool) -> Result<(), Box<dyn Error>> {
    let model = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .expect("认证设置固定行必须存在");
    let mut active = model.into_active_model();
    active.registration_enabled = Set(enabled);
    active.registration_default_group_id = Set(Some(fixture.group_id));
    active.registration_initial_quota = Set(123);
    active.update(fixture.pool.connection()).await?;
    Ok(())
}

async fn complete_new_identity(
    fixture: &Fixture,
    state_digest: &str,
    ticket_digest: &str,
    subject: &str,
    username: &str,
) -> Result<af_domain::UserId, Box<dyn Error>> {
    fixture
        .repository
        .create_state(OAUTH_LOGIN_GITHUB_PROVIDER, state_digest, 200)
        .await?;
    let claim = fixture
        .repository
        .claim_state(OAUTH_LOGIN_GITHUB_PROVIDER, state_digest, 100)
        .await?;
    match fixture
        .repository
        .complete_identity(
            claim,
            OAUTH_LOGIN_GITHUB_PROVIDER,
            subject,
            username,
            ticket_digest,
            150,
            110,
        )
        .await?
    {
        OAuthIdentityCompletion::Issued(user_id) => Ok(user_id),
        OAuthIdentityCompletion::Rejected => Err("已启用注册时未知身份不应被拒绝".into()),
    }
}

fn test_envelope() -> Result<EncryptedCredentialEnvelope, Box<dyn Error>> {
    Ok(EncryptedCredentialEnvelope::new(
        "oauth-test-key".to_owned(),
        [7_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES],
        vec![8_u8; 48],
    )?)
}
