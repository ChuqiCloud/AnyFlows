use std::{error::Error, time::Duration};

use super::{
    DatabaseOptions, MigrationOptions, SiteSettingsRepository, SiteSettingsRepositoryError,
    SiteSettingsWriteRecord,
};

#[tokio::test]
async fn fixed_site_settings_record_updates_with_a_monotonic_version() -> Result<(), Box<dyn Error>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;

    let defaults = repository.settings().await?;
    assert_eq!(defaults.site_name(), "AnyFlows");
    assert_eq!(defaults.public_base_url(), None);
    assert_eq!(defaults.version(), 1);

    let saved = repository
        .update(SiteSettingsWriteRecord::new(
            "AnyFlows Cloud".to_owned(),
            Some("https://example.com/gateway".to_owned()),
            Some("/brand.svg".to_owned()),
            Some("统一访问模型".to_owned()),
            Some("面向团队的模型 API 工作台。".to_owned()),
        ))
        .await?;
    assert_eq!(saved.site_name(), "AnyFlows Cloud");
    assert_eq!(saved.version(), 2);
    assert_eq!(repository.settings().await?, saved);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_script_logo_and_credential_base_urls() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;

    assert_eq!(
        repository
            .update(SiteSettingsWriteRecord::new(
                "AnyFlows".to_owned(),
                Some("https://admin:secret@example.com".to_owned()),
                None,
                None,
                None,
            ))
            .await,
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
    assert_eq!(
        repository
            .update(SiteSettingsWriteRecord::new(
                "AnyFlows".to_owned(),
                None,
                Some("javascript:alert(1)".to_owned()),
                None,
                None,
            ))
            .await,
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );

    pool.close().await?;
    Ok(())
}
