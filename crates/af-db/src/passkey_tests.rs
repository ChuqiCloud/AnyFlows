use std::{error::Error, time::Duration};

use af_domain::UserId;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
use serde_json::json;

use super::{
    CREDENTIAL_ENVELOPE_NONCE_BYTES, DatabaseOptions, DatabasePool, EncryptedCredentialEnvelope,
    InitialSetupRecord, InitialSetupRepository, MigrationOptions, PasskeyAuthenticationOutcome,
    PasskeyRepository, PasskeyRepositoryError,
};
use crate::entity::{passkey_authentication_challenges, passkeys, users};

const REGISTRATION_DIGEST: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
const AUTHENTICATION_DIGEST_A: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const AUTHENTICATION_DIGEST_B: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AUTHENTICATION_DIGEST_C: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const AUTHENTICATION_DIGEST_D: &str =
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const CREDENTIAL_ID: &str = "cGFzc2tleS10ZXN0LWNyZWRlbnRpYWw";

struct Fixture {
    pool: DatabasePool,
    repository: PasskeyRepository,
    user_id: UserId,
    session_version: i64,
}

#[tokio::test]
async fn authentication_challenge_expires_and_can_only_be_consumed_once()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(0).await?;
    let now = super::DatabaseTimestamp::now_utc();
    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_A, now, 300).await?;

    let challenge = fixture
        .repository
        .load_authentication_challenge(AUTHENTICATION_DIGEST_A, now)
        .await?;
    assert_eq!(challenge.user_id(), fixture.user_id);
    assert_eq!(challenge.session_version(), fixture.session_version);

    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_A,
            0,
            now + Duration::from_secs(1)
        )
        .await?,
        PasskeyAuthenticationOutcome::Authenticated
    );
    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_A,
            0,
            now + Duration::from_secs(2)
        )
        .await,
        Err(PasskeyRepositoryError::Consumed)
    );

    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_B, now, 10).await?;
    assert!(matches!(
        fixture
            .repository
            .load_authentication_challenge(AUTHENTICATION_DIGEST_B, now + Duration::from_secs(10),)
            .await,
        Err(PasskeyRepositoryError::Expired)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn zero_counter_is_valid_but_positive_counter_regression_marks_anomaly()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(0).await?;
    let now = super::DatabaseTimestamp::now_utc();

    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_A, now, 300).await?;
    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_A,
            0,
            now + Duration::from_secs(1)
        )
        .await?,
        PasskeyAuthenticationOutcome::Authenticated
    );
    let zero_counter = passkey(&fixture).await?;
    assert_eq!(zero_counter.sign_count, 0);
    assert_eq!(
        zero_counter.last_used_at,
        Some(now + Duration::from_secs(1))
    );
    assert_eq!(zero_counter.anomaly_at, None);

    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_B, now, 300).await?;
    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_B,
            4,
            now + Duration::from_secs(2)
        )
        .await?,
        PasskeyAuthenticationOutcome::Authenticated
    );
    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_C, now, 300).await?;
    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_C,
            3,
            now + Duration::from_secs(3)
        )
        .await?,
        PasskeyAuthenticationOutcome::AnomalyDetected
    );
    let anomalous = passkey(&fixture).await?;
    assert_eq!(anomalous.sign_count, 4);
    assert_eq!(anomalous.anomaly_at, Some(now + Duration::from_secs(3)));
    assert!(
        fixture
            .repository
            .authentication_target_for_user(fixture.user_id)
            .await?
            .is_none()
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn session_change_and_terminal_credential_state_consume_the_challenge()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(0).await?;
    let now = super::DatabaseTimestamp::now_utc();
    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_A, now, 300).await?;

    let user = users::Entity::find_by_id(fixture.user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在");
    let mut changed = user.into_active_model();
    changed.session_version = Set(fixture.session_version + 1);
    changed.update(fixture.pool.connection()).await?;
    assert_eq!(
        consume(
            &fixture,
            AUTHENTICATION_DIGEST_A,
            0,
            now + Duration::from_secs(1)
        )
        .await?,
        PasskeyAuthenticationOutcome::Rejected
    );
    assert!(challenge_consumed(&fixture, AUTHENTICATION_DIGEST_A).await?);

    create_authentication_challenge_with_version(
        &fixture,
        AUTHENTICATION_DIGEST_B,
        fixture.session_version + 1,
        now,
        300,
    )
    .await?;
    let user = users::Entity::find_by_id(fixture.user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在");
    let mut disabled = user.into_active_model();
    disabled.status = Set(2);
    disabled.update(fixture.pool.connection()).await?;
    assert_eq!(
        consume_with_version(
            &fixture,
            AUTHENTICATION_DIGEST_B,
            fixture.session_version + 1,
            0,
            now + Duration::from_secs(2),
        )
        .await?,
        PasskeyAuthenticationOutcome::Rejected
    );
    assert!(challenge_consumed(&fixture, AUTHENTICATION_DIGEST_B).await?);

    let user = users::Entity::find_by_id(fixture.user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在");
    let mut enabled = user.into_active_model();
    enabled.status = Set(1);
    enabled.update(fixture.pool.connection()).await?;
    create_authentication_challenge_with_version(
        &fixture,
        AUTHENTICATION_DIGEST_C,
        fixture.session_version + 1,
        now,
        300,
    )
    .await?;
    let mut revoked = passkey(&fixture).await?.into_active_model();
    revoked.revoked_at = Set(Some(now + Duration::from_secs(3)));
    revoked.update(fixture.pool.connection()).await?;
    assert_eq!(
        consume_with_version(
            &fixture,
            AUTHENTICATION_DIGEST_C,
            fixture.session_version + 1,
            0,
            now + Duration::from_secs(4),
        )
        .await?,
        PasskeyAuthenticationOutcome::Rejected
    );
    assert!(challenge_consumed(&fixture, AUTHENTICATION_DIGEST_C).await?);

    let mut anomalous = passkey(&fixture).await?.into_active_model();
    anomalous.revoked_at = Set(None);
    anomalous.anomaly_at = Set(Some(now + Duration::from_secs(5)));
    anomalous.update(fixture.pool.connection()).await?;
    create_authentication_challenge_with_version(
        &fixture,
        AUTHENTICATION_DIGEST_D,
        fixture.session_version + 1,
        now,
        300,
    )
    .await?;
    assert_eq!(
        consume_with_version(
            &fixture,
            AUTHENTICATION_DIGEST_D,
            fixture.session_version + 1,
            0,
            now + Duration::from_secs(6),
        )
        .await?,
        PasskeyAuthenticationOutcome::Rejected
    );
    assert!(challenge_consumed(&fixture, AUTHENTICATION_DIGEST_D).await?);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn verified_counter_compromise_uses_the_dedicated_anomaly_transaction()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(8).await?;
    let now = super::DatabaseTimestamp::now_utc();
    create_authentication_challenge(&fixture, AUTHENTICATION_DIGEST_A, now, 300).await?;

    assert_eq!(
        fixture
            .repository
            .consume_authentication_anomaly(
                AUTHENTICATION_DIGEST_A,
                fixture.user_id,
                fixture.session_version,
                CREDENTIAL_ID.to_owned(),
                now + Duration::from_secs(1),
            )
            .await?,
        PasskeyAuthenticationOutcome::AnomalyDetected
    );
    let anomalous = passkey(&fixture).await?;
    assert_eq!(anomalous.sign_count, 8);
    assert_eq!(anomalous.last_used_at, None);
    assert_eq!(anomalous.anomaly_at, Some(now + Duration::from_secs(1)));
    assert!(challenge_consumed(&fixture, AUTHENTICATION_DIGEST_A).await?);

    fixture.pool.close().await?;
    Ok(())
}

async fn fixture(sign_count: i64) -> Result<Fixture, Box<dyn Error>> {
    let pool = super::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
        .initialize(InitialSetupRecord::new(
            "passkey_owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?;
    let user = users::Entity::find()
        .filter(users::Column::Username.eq("passkey_owner"))
        .one(pool.connection())
        .await?
        .expect("首次安装用户必须存在");
    let user_id = UserId::new(user.id)?;
    let repository = PasskeyRepository::new(pool.clone(), Duration::from_secs(5));
    let now = super::DatabaseTimestamp::now_utc();
    repository
        .replace_registration_challenge(
            user_id,
            REGISTRATION_DIGEST.to_owned(),
            test_envelope(1)?,
            now + Duration::from_secs(300),
            now,
        )
        .await?;
    repository
        .consume_registration_challenge(
            user_id,
            REGISTRATION_DIGEST,
            CREDENTIAL_ID.to_owned(),
            json!({"counter": sign_count}),
            "测试 Passkey".to_owned(),
            sign_count,
            now,
        )
        .await?;
    Ok(Fixture {
        pool,
        repository,
        user_id,
        session_version: user.session_version,
    })
}

async fn create_authentication_challenge(
    fixture: &Fixture,
    digest: &str,
    now: super::DatabaseTimestamp,
    ttl_seconds: u64,
) -> Result<(), PasskeyRepositoryError> {
    create_authentication_challenge_with_version(
        fixture,
        digest,
        fixture.session_version,
        now,
        ttl_seconds,
    )
    .await
}

async fn create_authentication_challenge_with_version(
    fixture: &Fixture,
    digest: &str,
    session_version: i64,
    now: super::DatabaseTimestamp,
    ttl_seconds: u64,
) -> Result<(), PasskeyRepositoryError> {
    fixture
        .repository
        .replace_authentication_challenge(
            fixture.user_id,
            session_version,
            digest.to_owned(),
            test_envelope(2).expect("测试密文封套必须有效"),
            now + Duration::from_secs(ttl_seconds),
            now,
        )
        .await
}

async fn consume(
    fixture: &Fixture,
    digest: &str,
    sign_count: i64,
    now: super::DatabaseTimestamp,
) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
    consume_with_version(fixture, digest, fixture.session_version, sign_count, now).await
}

async fn consume_with_version(
    fixture: &Fixture,
    digest: &str,
    session_version: i64,
    sign_count: i64,
    now: super::DatabaseTimestamp,
) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
    fixture
        .repository
        .consume_authentication_challenge(
            digest,
            fixture.user_id,
            session_version,
            CREDENTIAL_ID.to_owned(),
            json!({"counter": sign_count}),
            sign_count,
            now,
        )
        .await
}

async fn passkey(fixture: &Fixture) -> Result<passkeys::Model, Box<dyn Error>> {
    Ok(passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(fixture.user_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("测试 Passkey 必须存在"))
}

async fn challenge_consumed(fixture: &Fixture, digest: &str) -> Result<bool, Box<dyn Error>> {
    Ok(passkey_authentication_challenges::Entity::find()
        .filter(passkey_authentication_challenges::Column::ChallengeDigest.eq(digest))
        .one(fixture.pool.connection())
        .await?
        .expect("测试挑战必须存在")
        .consumed_at
        .is_some())
}

fn test_envelope(seed: u8) -> Result<EncryptedCredentialEnvelope, Box<dyn Error>> {
    Ok(EncryptedCredentialEnvelope::new(
        format!("passkey-test-key-{seed}"),
        [seed; CREDENTIAL_ENVELOPE_NONCE_BYTES],
        vec![seed; 48],
    )?)
}
