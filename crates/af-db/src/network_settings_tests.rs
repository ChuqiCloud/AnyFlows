use std::{error::Error, time::Duration};

use sea_orm::{EntityTrait, PaginatorTrait};

use super::{
    DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions, NetworkSettingsMode,
    NetworkSettingsRepository, NetworkSettingsRepositoryConfigError,
    NetworkSettingsRepositoryError, NetworkSettingsWriteRecord, ProxyPasswordUpdate,
};
use crate::entity::network_settings;

async fn installed_repository()
-> Result<(crate::DatabasePool, NetworkSettingsRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = NetworkSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("network-settings-test-key", [marker; 24], vec![marker; 32])
        .unwrap()
}

#[tokio::test]
async fn migration_seeds_inherit_mode() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let settings = repository.settings().await?;
    assert_eq!(settings.mode(), NetworkSettingsMode::Inherit);
    assert_eq!(settings.proxy_host(), None);
    assert_eq!(settings.proxy_port(), None);
    assert!(!settings.password_configured());
    assert_eq!(settings.version(), 1);
    assert_eq!(
        network_settings::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn proxy_password_replace_keep_and_direct_clear() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let replaced = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Socks5h,
            Some("proxy.example.com".to_owned()),
            Some(1080),
            Some("proxy-user".to_owned()),
            ProxyPasswordUpdate::Replace(envelope(0x31)),
            true,
        ))
        .await?;
    assert_eq!(replaced.version(), 2);
    assert!(replaced.password_configured());
    assert!(replaced.trust_proxy_dns());

    let kept = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Socks5h,
            Some("proxy.example.com".to_owned()),
            Some(1080),
            Some("proxy-user".to_owned()),
            ProxyPasswordUpdate::Keep,
            true,
        ))
        .await?;
    assert_eq!(kept.version(), 3);
    assert!(kept.password_configured());

    let direct = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Direct,
            None,
            None,
            None,
            ProxyPasswordUpdate::Clear,
            false,
        ))
        .await?;
    assert_eq!(direct.mode(), NetworkSettingsMode::Direct);
    assert!(!direct.password_configured());
    assert!(!direct.trust_proxy_dns());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn restore_if_version_reinstates_previous_snapshot() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let previous = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Socks5h,
            Some("proxy.example.com".to_owned()),
            Some(1080),
            Some("proxy-user".to_owned()),
            ProxyPasswordUpdate::Replace(envelope(0x41)),
            true,
        ))
        .await?;
    let saved = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Direct,
            None,
            None,
            None,
            ProxyPasswordUpdate::Clear,
            false,
        ))
        .await?;

    repository
        .restore_if_version(saved.version(), &previous)
        .await?;
    let restored = repository.settings().await?;
    assert_eq!(restored.mode(), NetworkSettingsMode::Socks5h);
    assert_eq!(restored.proxy_host(), Some("proxy.example.com"));
    assert!(restored.password_configured());
    assert!(restored.trust_proxy_dns());
    assert_eq!(restored.version(), saved.version() + 1);

    assert_eq!(
        repository
            .restore_if_version(saved.version(), &previous)
            .await
            .unwrap_err(),
        NetworkSettingsRepositoryError::ConcurrentUpdate
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_proxy_shape_rolls_back() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let error = repository
        .update(NetworkSettingsWriteRecord::new(
            NetworkSettingsMode::Http,
            Some("http://proxy.example.com".to_owned()),
            Some(8080),
            None,
            ProxyPasswordUpdate::Keep,
            false,
        ))
        .await
        .unwrap_err();
    assert_eq!(error, NetworkSettingsRepositoryError::InvalidSettings);
    assert_eq!(repository.settings().await?.version(), 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    assert!(matches!(
        NetworkSettingsRepository::new(pool.clone(), Duration::ZERO),
        Err(NetworkSettingsRepositoryConfigError::ZeroOperationTimeout)
    ));
    pool.close().await?;
    Ok(())
}
