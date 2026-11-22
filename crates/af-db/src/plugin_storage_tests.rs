use std::error::Error;

use crate::{DatabaseOptions, MigrationOptions, PluginStorageRepository};

#[tokio::test]
async fn expired_rows_do_not_hide_live_rows_from_limited_lists() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PluginStorageRepository::new(pool.clone());

    repository
        .put(
            "org.example.storage",
            "a-expired",
            serde_json::json!(1),
            Some(20),
            10,
        )
        .await?;
    repository
        .put(
            "org.example.storage",
            "z-live",
            serde_json::json!(2),
            None,
            10,
        )
        .await?;

    let rows = repository.list("org.example.storage", None, 1, 21).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key, "z-live");
    assert!(
        repository
            .get("org.example.storage", "a-expired", 21)
            .await?
            .is_none()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repeated_puts_upsert_the_same_key() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PluginStorageRepository::new(pool.clone());

    repository
        .put(
            "org.example.storage",
            "cursor",
            serde_json::json!(1),
            None,
            10,
        )
        .await?;
    repository
        .put(
            "org.example.storage",
            "cursor",
            serde_json::json!(2),
            None,
            11,
        )
        .await?;

    let record = repository
        .get("org.example.storage", "cursor", 10)
        .await?
        .expect("one upsert should remain visible");
    assert_eq!(record.value, serde_json::json!(2));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn list_prefix_treats_underscores_as_literal_key_characters() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PluginStorageRepository::new(pool.clone());

    repository
        .put(
            "org.example.storage",
            "m_value",
            serde_json::json!(1),
            None,
            10,
        )
        .await?;
    repository
        .put(
            "org.example.storage",
            "mXvalue",
            serde_json::json!(2),
            None,
            10,
        )
        .await?;
    repository
        .put(
            "org.example.storage",
            "M_value",
            serde_json::json!(3),
            None,
            10,
        )
        .await?;

    let rows = repository
        .list("org.example.storage", Some("m_"), 10, 10)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key, "m_value");

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expiration_purge_removes_rows_from_all_plugin_namespaces() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PluginStorageRepository::new(pool.clone());

    repository
        .put(
            "org.example.first",
            "cache",
            serde_json::json!(1),
            Some(20),
            10,
        )
        .await?;
    repository
        .put(
            "org.example.second",
            "cache",
            serde_json::json!(2),
            Some(20),
            10,
        )
        .await?;

    assert_eq!(repository.purge_expired(21, 100).await?, 2);
    assert!(
        repository
            .get("org.example.first", "cache", 21)
            .await?
            .is_none()
    );
    assert!(
        repository
            .get("org.example.second", "cache", 21)
            .await?
            .is_none()
    );

    pool.close().await?;
    Ok(())
}
