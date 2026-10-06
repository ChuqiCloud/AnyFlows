use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, JsonValue, Set};

use crate::{
    CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepository, CustomOAuth2LoginRepositoryError,
    CustomOAuth2ProviderRepository, CustomOAuth2ProviderSecretUpdate,
    CustomOAuth2ProviderWriteRecord, DatabaseOptions, EncryptedCredentialEnvelope,
    MigrationOptions, connect_and_migrate,
    entity::{authentication_settings, groups, users},
};

const PROVIDER: &str = "custom_enterprise";
const SECOND_PROVIDER: &str = "custom_partner";
const STATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND_STATE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const THIRD_STATE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const FOURTH_STATE: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const TICKET: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const SECOND_TICKET: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const THIRD_TICKET: &str = "9999999999999999999999999999999999999999999999999999999999999999";
const FOURTH_TICKET: &str = "8888888888888888888888888888888888888888888888888888888888888888";

struct Fixture {
    pool: crate::DatabasePool,
    repository: CustomOAuth2LoginRepository,
    group_id: i64,
}

#[tokio::test]
async fn custom_state_binds_provider_version_and_is_single_use() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, STATE, 100)
        .await?;
    assert_eq!(
        fixture.repository.claim_state(PROVIDER, STATE, 100).await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    assert_eq!(
        fixture
            .repository
            .create_state(PROVIDER, 1, STATE, 200)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Conflict)
    );

    fixture
        .repository
        .create_state(PROVIDER, 1, SECOND_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .claim_state(SECOND_PROVIDER, SECOND_STATE, 100)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    let claim = fixture
        .repository
        .claim_state(PROVIDER, SECOND_STATE, 100)
        .await?;
    assert_eq!(claim.configuration_version(), 1);
    assert_eq!(
        fixture
            .repository
            .claim_state(PROVIDER, SECOND_STATE, 100)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "custom_enterprise_external_1",
                TICKET,
                150,
                110,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Rejected
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn custom_identity_obeys_registration_and_ticket_boundaries() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    enable_registration(&fixture, true).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, STATE, 200)
        .await?;
    let claim = fixture.repository.claim_state(PROVIDER, STATE, 100).await?;
    let user_id = match fixture
        .repository
        .complete_identity(
            claim,
            PROVIDER,
            "external-1",
            "custom_enterprise_external_1",
            TICKET,
            150,
            110,
        )
        .await?
    {
        CustomOAuth2IdentityCompletion::Issued(user_id) => user_id,
        CustomOAuth2IdentityCompletion::Rejected => return Err("已开启注册却拒绝未知身份".into()),
    };
    assert_eq!(
        fixture.repository.consume_ticket(TICKET, 120).await?,
        user_id
    );
    assert_eq!(
        fixture.repository.consume_ticket(TICKET, 120).await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    let user = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("自定义 OAuth2 新用户必须持久化");
    assert_eq!(user.password_hash, None);
    assert_eq!(user.email, None);

    enable_registration(&fixture, false).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, SECOND_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, SECOND_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "unused_existing_username",
                SECOND_TICKET,
                260,
                210,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Issued(user_id)
    );

    let mut disabled_user = user.into_active_model();
    disabled_user.status = Set(2);
    disabled_user.update(fixture.pool.connection()).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, THIRD_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, THIRD_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "unused_disabled_username",
                THIRD_TICKET,
                260,
                210,
            )
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );

    fixture
        .repository
        .create_state(PROVIDER, 1, FOURTH_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, FOURTH_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-2",
                "custom_enterprise_external_2",
                FOURTH_TICKET,
                260,
                210,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Rejected
    );
    fixture.pool.close().await?;
    Ok(())
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let providers = CustomOAuth2ProviderRepository::new(pool.clone(), Duration::from_secs(2))?;
    for (key, marker) in [(PROVIDER, 1_u8), (SECOND_PROVIDER, 2_u8)] {
        providers
            .save_provider(
                key,
                CustomOAuth2ProviderWriteRecord::new(
                    0,
                    "企业登录".to_owned(),
                    "client-id".to_owned(),
                    "https://login.example.test/oauth/authorize".to_owned(),
                    "https://login.example.test/oauth/token".to_owned(),
                    "https://login.example.test/oauth/userinfo".to_owned(),
                    "openid profile".to_owned(),
                    "sub".to_owned(),
                    true,
                    CustomOAuth2ProviderSecretUpdate::Replace(test_envelope(marker)?),
                ),
            )
            .await?;
    }
    let group = groups::ActiveModel {
        name: Set("custom-oauth2-users".to_owned()),
        display_name: Set("Custom OAuth2 Users".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: CustomOAuth2LoginRepository::new(pool.clone(), Duration::from_secs(2))?,
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

fn test_envelope(marker: u8) -> Result<EncryptedCredentialEnvelope, Box<dyn Error>> {
    Ok(EncryptedCredentialEnvelope::new(
        "primary-key",
        [marker; crate::CREDENTIAL_ENVELOPE_NONCE_BYTES],
        vec![marker; crate::CREDENTIAL_ENVELOPE_TAG_BYTES],
    )?)
}
