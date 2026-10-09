use std::{error::Error, path::PathBuf, time::SystemTime};

use af_db::{
    DatabaseOptions, MigrationHistoryAdoption, MigrationOptions, MigratorExtension,
    connect_and_migrate, connect_and_migrate_with_extension,
};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};
use sea_orm_migration::prelude::*;

type TestResult = Result<(), Box<dyn Error>>;

struct LegacyMigration;
struct PendingMigration;
struct ExtensionMigrator;

impl MigrationName for LegacyMigration {
    fn name(&self) -> &str {
        "extension_v1"
    }
}

impl MigrationName for PendingMigration {
    fn name(&self) -> &str {
        "extension_v2"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for LegacyMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE TABLE extension_probe (id INTEGER PRIMARY KEY, value TEXT)")
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MigrationTrait for PendingMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("INSERT INTO extension_probe VALUES (2, 'new extension migration')")
            .await?;
        Ok(())
    }
}

impl MigratorTrait for ExtensionMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(LegacyMigration), Box::new(PendingMigration)]
    }

    fn migration_table_name() -> sea_orm::DynIden {
        Alias::new("extension_migrations").into_iden()
    }
}

struct TemporaryDatabase(PathBuf);

impl TemporaryDatabase {
    fn new() -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos();
        Ok(Self(std::env::temp_dir().join(format!(
            "anyflows-extension-history-{}-{nonce}.db",
            std::process::id()
        ))))
    }

    fn url(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.0.display())
    }
}

impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn records(db: &DatabaseConnection, table: &str) -> Result<Vec<(String, i64)>, DbErr> {
    db.query_all(Statement::from_string(
        DbBackend::Sqlite,
        format!("SELECT version, applied_at FROM {table} ORDER BY version"),
    ))
    .await?
    .into_iter()
    .map(|row| Ok((row.try_get("", "version")?, row.try_get("", "applied_at")?)))
    .collect()
}

#[tokio::test]
async fn registered_extension_history_is_preserved_and_pending_migrations_run_once() -> TestResult {
    let file = TemporaryDatabase::new()?;
    let options = DatabaseOptions::new(file.url())?;
    connect_and_migrate(&options, MigrationOptions::default())
        .await?
        .close()
        .await?;
    let db = Database::connect(file.url()).await?;
    for sql in [
        "CREATE TABLE extension_probe (id INTEGER PRIMARY KEY, value TEXT)",
        "INSERT INTO extension_probe VALUES (1, 'existing data')",
        "INSERT INTO seaql_migrations VALUES ('extension_v1', 42)",
    ] {
        db.execute_unprepared(sql).await?;
    }
    let original = records(&db, "seaql_migrations").await?;
    db.close().await?;
    let extension = MigratorExtension::<ExtensionMigrator>::new()
        .with_legacy_history(MigrationHistoryAdoption::from_versions(["extension_v1"]));
    for _ in 0..2 {
        connect_and_migrate_with_extension(&options, MigrationOptions::default(), Some(&extension))
            .await?
            .close()
            .await?;
        let db = Database::connect(file.url()).await?;
        assert_eq!(records(&db, "seaql_migrations").await?, original);
        let private = records(&db, "extension_migrations").await?;
        assert_eq!(private.len(), 2);
        assert_eq!(private[0], ("extension_v1".to_owned(), 42));
        let data = db
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id, value FROM extension_probe ORDER BY id",
            ))
            .await?;
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].try_get::<String>("", "value")?, "existing data");
        db.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn undeclared_history_is_rejected_without_running_extension_migrations() -> TestResult {
    let file = TemporaryDatabase::new()?;
    let options = DatabaseOptions::new(file.url())?;
    connect_and_migrate(&options, MigrationOptions::default())
        .await?
        .close()
        .await?;
    let db = Database::connect(file.url()).await?;
    db.execute_unprepared("INSERT INTO seaql_migrations VALUES ('unknown_version', 42)")
        .await?;
    let original = records(&db, "seaql_migrations").await?;
    db.close().await?;
    let extension = MigratorExtension::<ExtensionMigrator>::new();
    let error =
        connect_and_migrate_with_extension(&options, MigrationOptions::default(), Some(&extension))
            .await
            .expect_err("undeclared history must stop startup");
    assert!(error.to_string().contains("unknown_version"));
    let db = Database::connect(file.url()).await?;
    assert_eq!(records(&db, "seaql_migrations").await?, original);
    assert!(!SchemaManager::new(&db).has_table("extension_probe").await?);
    db.close().await?;
    Ok(())
}
