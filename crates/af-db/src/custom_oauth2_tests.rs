use std::{error::Error, time::Duration};

use crate::{
    CustomOAuth2ProviderRepository, CustomOAuth2ProviderRepositoryError,
    CustomOAuth2ProviderSecretUpdate, CustomOAuth2ProviderWriteRecord, DatabaseOptions,
    EncryptedCredentialEnvelope, MigrationOptions, connect_and_migrate,
};

const AUTHORIZATION: &str = "https://login.example.test/oauth/authorize";
const TOKEN: &str = "https://login.example.test/oauth/token";
const USERINFO: &str = "https://login.example.test/oauth/userinfo";

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new(
        "primary-key",
        [marker; crate::CREDENTIAL_ENVELOPE_NONCE_BYTES],
        vec![marker; crate::CREDENTIAL_ENVELOPE_TAG_BYTES],
    )
    .expect("测试密文 envelope 必须有效")
}

fn write(
    expected_version: i64,
    enabled: bool,
    secret: CustomOAuth2ProviderSecretUpdate,
) -> CustomOAuth2ProviderWriteRecord {
    CustomOAuth2ProviderWriteRecord::new(
        expected_version,
        "企业登录".to_owned(),
        "client-id_1".to_owned(),
        AUTHORIZATION.to_owned(),
        TOKEN.to_owned(),
        USERINFO.to_owned(),
        "openid profile".to_owned(),
        "sub".to_owned(),
        enabled,
        secret,
    )
}

async fn fixture() -> Result<CustomOAuth2ProviderRepository, Box<dyn Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(CustomOAuth2ProviderRepository::new(
        pool,
        Duration::from_secs(2),
    )?)
}

#[tokio::test]
async fn custom_provider_persistence_uses_create_and_version_cas() -> Result<(), Box<dyn Error>> {
    let repository = fixture().await?;
    let saved = repository
        .save_provider(
            "custom_example",
            write(
                0,
                true,
                CustomOAuth2ProviderSecretUpdate::Replace(envelope(7)),
            ),
        )
        .await?;
    assert_eq!(saved.provider_key(), "custom_example");
    assert!(saved.available());
    assert!(saved.secret_configured());
    assert_eq!(saved.version(), 1);

    let stale = repository
        .save_provider(
            "custom_example",
            write(
                0,
                true,
                CustomOAuth2ProviderSecretUpdate::Replace(envelope(8)),
            ),
        )
        .await;
    assert_eq!(
        stale,
        Err(CustomOAuth2ProviderRepositoryError::ConcurrentUpdate)
    );

    let disabled = repository
        .save_provider(
            "custom_example",
            write(1, false, CustomOAuth2ProviderSecretUpdate::Clear),
        )
        .await?;
    assert!(!disabled.available());
    assert!(!disabled.secret_configured());
    assert_eq!(disabled.version(), 2);
    assert_eq!(
        repository.provider("custom_example").await?.unwrap(),
        disabled
    );

    let debug = format!("{saved:?}");
    for private in [
        "企业登录",
        "client-id_1",
        "login.example.test",
        "openid profile",
    ] {
        assert!(!debug.contains(private), "Debug 泄露了 {private}");
    }
    Ok(())
}

#[tokio::test]
async fn custom_provider_validation_rejects_unsafe_values() -> Result<(), Box<dyn Error>> {
    let repository = fixture().await?;
    assert_eq!(
        repository
            .save_provider(
                "github",
                write(0, false, CustomOAuth2ProviderSecretUpdate::Clear,),
            )
            .await,
        Err(CustomOAuth2ProviderRepositoryError::InvalidInput)
    );
    assert_eq!(
        repository
            .save_provider(
                "custom_unsafe",
                CustomOAuth2ProviderWriteRecord::new(
                    0,
                    "企业登录".to_owned(),
                    "client-id".to_owned(),
                    "http://login.example.test/oauth/authorize".to_owned(),
                    TOKEN.to_owned(),
                    USERINFO.to_owned(),
                    "openid profile".to_owned(),
                    "sub".to_owned(),
                    false,
                    CustomOAuth2ProviderSecretUpdate::Clear,
                ),
            )
            .await,
        Err(CustomOAuth2ProviderRepositoryError::InvalidInput)
    );
    assert_eq!(
        repository
            .save_provider(
                "custom_missing",
                write(0, false, CustomOAuth2ProviderSecretUpdate::Keep)
            )
            .await,
        Err(CustomOAuth2ProviderRepositoryError::InvalidInput)
    );
    Ok(())
}
