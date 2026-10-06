use std::{error::Error, time::Duration};

use super::{
    BalanceDisplayModeRecord, BalanceDisplayPolicyRecord, BalanceSymbolPositionRecord,
    DatabaseOptions, MigrationOptions, SiteNavigationGroupRecord, SiteNavigationLinkRecord,
    SiteNavigationRecord, SiteSettingsRepository, SiteSettingsRepositoryError,
    SiteSettingsWriteRecord, SiteSidebarLinkRecord,
};

fn custom_balance_display() -> BalanceDisplayPolicyRecord {
    BalanceDisplayPolicyRecord::new(
        BalanceDisplayModeRecord::CustomUnit,
        "算力积分".to_owned(),
        "积分".to_owned(),
        10_000,
        BalanceSymbolPositionRecord::Suffix,
        2,
    )
    .unwrap()
}

#[tokio::test]
async fn navigation_updates_preserve_brand_and_survive_template_changes()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let original = repository.settings().await?;
    assert_eq!(original.navigation(), &SiteNavigationRecord::default());

    let navigation = SiteNavigationRecord {
        header_links: vec![SiteNavigationLinkRecord {
            label: "文档".to_owned(),
            label_en: Some("Docs".to_owned()),
            url: "/api".to_owned(),
        }],
        footer_groups: vec![SiteNavigationGroupRecord {
            title: "资源".to_owned(),
            title_en: None,
            links: vec![SiteNavigationLinkRecord {
                label: "状态页".to_owned(),
                label_en: None,
                url: "https://status.example.com".to_owned(),
            }],
        }],
        sidebar_links: vec![SiteSidebarLinkRecord {
            label: "支持".to_owned(),
            label_en: Some("Support".to_owned()),
            url: "https://support.example.com".to_owned(),
            icon: "headphones".to_owned(),
            kind: "link".to_owned(),
            level: 1,
            style: "accent".to_owned(),
            audience: "all".to_owned(),
        }],
    };
    let saved = repository
        .update_navigation(navigation.clone(), original.version())
        .await?;
    assert_eq!(saved.navigation(), &navigation);
    assert_eq!(saved.version(), original.version() + 1);
    assert_eq!(
        repository
            .update_navigation(navigation.clone(), original.version())
            .await,
        Err(SiteSettingsRepositoryError::Conflict)
    );

    let updated = repository
        .update(SiteSettingsWriteRecord::new(
            "AnyFlows Cloud".to_owned(),
            None,
            None,
            None,
            None,
            custom_balance_display(),
            saved.version(),
        ))
        .await?;
    assert_eq!(updated.navigation(), &navigation);
    let switched = repository
        .set_frontend_template_id(Some("classic".to_owned()))
        .await?;
    assert_eq!(switched.navigation(), &navigation);
    assert_eq!(switched.site_name(), "AnyFlows Cloud");

    for invalid_url in [
        "javascript:alert(1)",
        "http://example.com",
        "//example.com",
        "https://user@example.com",
        "/path?token=secret",
    ] {
        let mut invalid = navigation.clone();
        invalid.header_links[0].url = invalid_url.to_owned();
        assert_eq!(
            repository
                .update_navigation(invalid, switched.version())
                .await,
            Err(SiteSettingsRepositoryError::InvalidSettings)
        );
    }
    for invalid_icon in ["javascript", "../../shield", ""] {
        let mut invalid = navigation.clone();
        invalid.sidebar_links[0].icon = invalid_icon.to_owned();
        assert_eq!(
            repository
                .update_navigation(invalid, switched.version())
                .await,
            Err(SiteSettingsRepositoryError::InvalidSettings)
        );
    }
    let mut grouped = navigation.clone();
    grouped.sidebar_links[0].kind = "group".to_owned();
    grouped.sidebar_links[0].url.clear();
    assert_eq!(
        grouped.validate(),
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
    let mut child = navigation.sidebar_links[0].clone();
    child.level = 2;
    grouped.sidebar_links.push(child);
    assert!(grouped.validate().is_ok());
    grouped.sidebar_links[1].url = "javascript:alert(1)".to_owned();
    assert_eq!(
        grouped.validate(),
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
    let mut full = navigation.clone();
    full.sidebar_links = vec![navigation.sidebar_links[0].clone(); 48];
    assert!(full.validate().is_ok());
    full.sidebar_links.push(navigation.sidebar_links[0].clone());
    assert_eq!(
        full.validate(),
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
    pool.close().await?;
    Ok(())
}

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
    assert_eq!(
        defaults.balance_display().mode(),
        BalanceDisplayModeRecord::Quota
    );
    assert_eq!(
        defaults.balance_display().quota_units_per_display_unit(),
        10_000
    );

    let saved = repository
        .update(SiteSettingsWriteRecord::new(
            "AnyFlows Cloud".to_owned(),
            Some("https://example.com/gateway".to_owned()),
            Some("/brand.svg".to_owned()),
            Some("统一访问模型".to_owned()),
            Some("面向团队的模型 API 工作台。".to_owned()),
            custom_balance_display(),
            defaults.version(),
        ))
        .await?;
    assert_eq!(saved.site_name(), "AnyFlows Cloud");
    assert_eq!(saved.version(), 2);
    assert_eq!(saved.balance_display(), &custom_balance_display());
    assert_eq!(repository.settings().await?, saved);

    assert_eq!(
        repository
            .update(SiteSettingsWriteRecord::new(
                "陈旧覆盖".to_owned(),
                None,
                None,
                None,
                None,
                custom_balance_display(),
                defaults.version(),
            ))
            .await,
        Err(SiteSettingsRepositoryError::Conflict)
    );

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
                custom_balance_display(),
                1,
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
                custom_balance_display(),
                1,
            ))
            .await,
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );

    pool.close().await?;
    Ok(())
}

#[test]
fn balance_display_policy_rejects_invalid_denominations() {
    assert_eq!(
        BalanceDisplayPolicyRecord::new(
            BalanceDisplayModeRecord::CustomUnit,
            "算力积分".to_owned(),
            "积分".to_owned(),
            0,
            BalanceSymbolPositionRecord::Suffix,
            2,
        ),
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
    assert_eq!(
        BalanceDisplayPolicyRecord::new(
            BalanceDisplayModeRecord::CustomUnit,
            "算力积分".to_owned(),
            "积分".to_owned(),
            10_000,
            BalanceSymbolPositionRecord::Suffix,
            5,
        ),
        Err(SiteSettingsRepositoryError::InvalidSettings)
    );
}
