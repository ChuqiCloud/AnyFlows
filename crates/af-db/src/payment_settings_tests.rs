use std::{error::Error, time::Duration};

use sea_orm::{EntityTrait, PaginatorTrait};

use super::{
    DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions, PaymentSecretUpdate,
    PaymentSettingsRepository, PaymentSettingsRepositoryConfigError,
    PaymentSettingsRepositoryError, PaymentSettingsWriteRecord,
};
use crate::entity::payment_settings;

async fn installed_repository()
-> Result<(crate::DatabasePool, PaymentSettingsRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PaymentSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("payment-settings-test-key", [marker; 24], vec![marker; 32])
        .unwrap()
}

fn complete_write(
    expected_version: i64,
    stripe_secret_key: PaymentSecretUpdate,
    stripe_webhook_secret: PaymentSecretUpdate,
    epay_merchant_key: PaymentSecretUpdate,
) -> PaymentSettingsWriteRecord {
    PaymentSettingsWriteRecord::new(
        expected_version,
        true,
        Some("pk_test_payment_settings".to_owned()),
        stripe_secret_key,
        stripe_webhook_secret,
        300,
        true,
        Some("https://pay.example.com".to_owned()),
        Some("merchant_1001".to_owned()),
        epay_merchant_key,
        true,
        true,
        true,
        false,
        false,
        500_000,
    )
}

#[tokio::test]
async fn migration_seeds_one_uninitialized_fixed_record() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let settings = repository.settings().await?;

    assert!(!settings.initialized());
    assert!(!settings.stripe_enabled());
    assert!(!settings.epay_enabled());
    assert!(!settings.epay_qr_enabled());
    assert!(!settings.epay_refund_enabled());
    assert!(!settings.refund_auto_submit_enabled());
    assert_eq!(settings.stripe_signature_tolerance_seconds(), 300);
    assert_eq!(settings.epay_quota_per_cny(), 500_000);
    assert_eq!(settings.version(), 1);
    assert_eq!(
        payment_settings::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn complete_update_keeps_secrets_and_redacts_debug() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let stripe_key = envelope(0x31);
    let stripe_webhook = envelope(0x32);
    let epay_key = envelope(0x33);

    let replaced = repository
        .update(complete_write(
            1,
            PaymentSecretUpdate::Replace(stripe_key.clone()),
            PaymentSecretUpdate::Replace(stripe_webhook.clone()),
            PaymentSecretUpdate::Replace(epay_key.clone()),
        ))
        .await?;
    assert!(replaced.initialized());
    assert_eq!(replaced.version(), 2);
    assert_eq!(replaced.stripe_secret_key(), Some(&stripe_key));
    assert_eq!(replaced.stripe_webhook_secret(), Some(&stripe_webhook));
    assert_eq!(replaced.epay_merchant_key(), Some(&epay_key));
    assert!(replaced.epay_qr_enabled());

    let kept = repository
        .update(complete_write(
            2,
            PaymentSecretUpdate::Keep,
            PaymentSecretUpdate::Keep,
            PaymentSecretUpdate::Keep,
        ))
        .await?;
    assert_eq!(kept.version(), 3);
    assert_eq!(kept.stripe_secret_key(), Some(&stripe_key));
    assert_eq!(kept.stripe_webhook_secret(), Some(&stripe_webhook));
    assert_eq!(kept.epay_merchant_key(), Some(&epay_key));
    assert!(kept.epay_qr_enabled());

    let rendered = format!("{kept:?}");
    for private in [
        "pk_test_payment_settings",
        "pay.example.com",
        "merchant_1001",
        "payment-settings-test-key",
    ] {
        assert!(!rendered.contains(private));
    }

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_enabled_shapes_and_stale_versions_roll_back() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;

    let missing_stripe_secret = PaymentSettingsWriteRecord::new(
        1,
        true,
        Some("pk_test_payment_settings".to_owned()),
        PaymentSecretUpdate::Keep,
        PaymentSecretUpdate::Keep,
        300,
        false,
        None,
        None,
        PaymentSecretUpdate::Keep,
        true,
        true,
        false,
        false,
        false,
        500_000,
    );
    assert_eq!(
        repository.update(missing_stripe_secret).await.unwrap_err(),
        PaymentSettingsRepositoryError::InvalidSettings
    );

    let invalid_gateway = PaymentSettingsWriteRecord::new(
        1,
        false,
        None,
        PaymentSecretUpdate::Keep,
        PaymentSecretUpdate::Keep,
        300,
        true,
        Some("http://pay.example.com".to_owned()),
        Some("merchant_1001".to_owned()),
        PaymentSecretUpdate::Replace(envelope(0x41)),
        true,
        false,
        false,
        false,
        false,
        500_000,
    );
    assert_eq!(
        repository.update(invalid_gateway).await.unwrap_err(),
        PaymentSettingsRepositoryError::InvalidSettings
    );
    assert_eq!(repository.settings().await?.version(), 1);

    let refund_without_epay = PaymentSettingsWriteRecord::new(
        1,
        false,
        None,
        PaymentSecretUpdate::Keep,
        PaymentSecretUpdate::Keep,
        300,
        false,
        None,
        None,
        PaymentSecretUpdate::Keep,
        true,
        true,
        false,
        true,
        false,
        500_000,
    );
    assert_eq!(
        repository.update(refund_without_epay).await.unwrap_err(),
        PaymentSettingsRepositoryError::InvalidSettings
    );

    let auto_without_refund = PaymentSettingsWriteRecord::new(
        1,
        false,
        None,
        PaymentSecretUpdate::Keep,
        PaymentSecretUpdate::Keep,
        300,
        true,
        Some("https://pay.example.com".to_owned()),
        Some("merchant_1001".to_owned()),
        PaymentSecretUpdate::Replace(envelope(0x40)),
        true,
        true,
        false,
        false,
        true,
        500_000,
    );
    assert_eq!(
        repository.update(auto_without_refund).await.unwrap_err(),
        PaymentSettingsRepositoryError::InvalidSettings
    );

    let saved = repository
        .update(complete_write(
            1,
            PaymentSecretUpdate::Replace(envelope(0x42)),
            PaymentSecretUpdate::Replace(envelope(0x43)),
            PaymentSecretUpdate::Replace(envelope(0x44)),
        ))
        .await?;
    assert_eq!(saved.version(), 2);
    assert_eq!(
        repository
            .update(complete_write(
                1,
                PaymentSecretUpdate::Keep,
                PaymentSecretUpdate::Keep,
                PaymentSecretUpdate::Keep,
            ))
            .await
            .unwrap_err(),
        PaymentSettingsRepositoryError::ConcurrentUpdate
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn restore_if_version_reinstates_previous_snapshot() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let previous = repository.settings().await?;
    let saved = repository
        .update(complete_write(
            previous.version(),
            PaymentSecretUpdate::Replace(envelope(0x51)),
            PaymentSecretUpdate::Replace(envelope(0x52)),
            PaymentSecretUpdate::Replace(envelope(0x53)),
        ))
        .await?;

    repository
        .restore_if_version(saved.version(), &previous)
        .await?;
    let restored = repository.settings().await?;
    assert!(!restored.initialized());
    assert!(!restored.stripe_enabled());
    assert!(!restored.epay_enabled());
    assert_eq!(restored.version(), saved.version() + 1);
    assert_eq!(
        repository
            .restore_if_version(saved.version(), &previous)
            .await
            .unwrap_err(),
        PaymentSettingsRepositoryError::ConcurrentUpdate
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    assert!(matches!(
        PaymentSettingsRepository::new(pool.clone(), Duration::ZERO),
        Err(PaymentSettingsRepositoryConfigError::ZeroOperationTimeout)
    ));
    pool.close().await?;
    Ok(())
}
