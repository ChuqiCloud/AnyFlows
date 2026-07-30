use std::{error::Error, time::Duration};

use sea_orm::{EntityTrait, PaginatorTrait};

use super::{
    DatabaseOptions, EmailPasswordUpdate, EmailSettingsRepository,
    EmailSettingsRepositoryConfigError, EmailSettingsRepositoryError, EmailSettingsWriteRecord,
    EmailTlsMode, EncryptedCredentialEnvelope, MigrationOptions,
};
use crate::entity::email_settings;

async fn installed_repository()
-> Result<(crate::DatabasePool, EmailSettingsRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("email-settings-test-key", [marker; 24], vec![marker; 32])
        .unwrap()
}

fn settings_write(
    enabled: bool,
    username: Option<&str>,
    password_update: EmailPasswordUpdate,
) -> EmailSettingsWriteRecord {
    EmailSettingsWriteRecord::new(
        enabled,
        "smtp.example.com".to_owned(),
        587,
        EmailTlsMode::StartTls,
        username.map(str::to_owned),
        password_update,
        "from@example.com".to_owned(),
        Some("AnyFlows".to_owned()),
        Some("reply@example.com".to_owned()),
        10,
    )
}

#[tokio::test]
async fn migration_seeds_one_disabled_fixed_record() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;

    let settings = repository.settings().await?;
    assert!(!settings.enabled());
    assert_eq!(settings.host(), "");
    assert_eq!(settings.port(), 587);
    assert_eq!(settings.tls_mode(), EmailTlsMode::StartTls);
    assert_eq!(settings.username(), None);
    assert!(!settings.password_configured());
    assert_eq!(settings.from_address(), "");
    assert_eq!(settings.timeout_seconds(), 10);
    assert_eq!(settings.version(), 1);
    assert!(!settings.delivery_ready());
    assert_eq!(
        email_settings::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn password_replace_keep_and_clear_increment_versions() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let first_envelope = envelope(0x31);

    let replaced = repository
        .update(settings_write(
            true,
            Some("mailer@example.com"),
            EmailPasswordUpdate::Replace(first_envelope.clone()),
        ))
        .await?;
    assert_eq!(replaced.version(), 2);
    assert_eq!(replaced.password_secret(), Some(&first_envelope));
    let persisted = email_settings::Entity::find()
        .one(pool.connection())
        .await?
        .expect("固定邮件设置行必须存在");
    let rendered = format!("{persisted:?}");
    assert!(rendered.contains("<redacted>"));
    for private in [
        "smtp.example.com",
        "mailer@example.com",
        "from@example.com",
        "reply@example.com",
    ] {
        assert!(!rendered.contains(private));
    }

    let kept = repository
        .update(settings_write(
            true,
            Some("mailer@example.com"),
            EmailPasswordUpdate::Keep,
        ))
        .await?;
    assert_eq!(kept.version(), 3);
    assert_eq!(kept.password_secret(), Some(&first_envelope));

    let cleared = repository
        .update(settings_write(false, None, EmailPasswordUpdate::Clear))
        .await?;
    assert_eq!(cleared.version(), 4);
    assert_eq!(cleared.username(), None);
    assert!(!cleared.password_configured());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_authentication_password_rolls_back_the_whole_update() -> Result<(), Box<dyn Error>>
{
    let (pool, repository) = installed_repository().await?;

    assert_eq!(
        repository
            .update(settings_write(
                true,
                Some("mailer@example.com"),
                EmailPasswordUpdate::Keep,
            ))
            .await,
        Err(EmailSettingsRepositoryError::InvalidSettings)
    );
    let settings = repository.settings().await?;
    assert_eq!(settings.version(), 1);
    assert!(!settings.enabled());
    assert_eq!(settings.host(), "");

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    assert!(matches!(
        EmailSettingsRepository::new(pool.clone(), Duration::ZERO),
        Err(EmailSettingsRepositoryConfigError::ZeroOperationTimeout)
    ));
    pool.close().await?;
    Ok(())
}
