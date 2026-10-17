use std::{error::Error, path::PathBuf, time::SystemTime};

use af_db::{
    DatabaseError, DatabaseOptions, MigrationHistoryAdoption, MigrationOptions, MigratorExtension,
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
    for extension in [
        MigratorExtension::<ExtensionMigrator>::new()
            .with_legacy_history(MigrationHistoryAdoption::from_versions(["extension_v1"])),
        MigratorExtension::<ExtensionMigrator>::new(),
    ] {
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

struct ConflictingMigration;
struct ConflictingMigrator;

impl MigrationName for ConflictingMigration {
    fn name(&self) -> &str {
        "m20260718_000001_create_groups"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ConflictingMigration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "conflicting migration must not run".to_owned(),
        ))
    }
}

impl MigratorTrait for ConflictingMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(ConflictingMigration)]
    }

    fn migration_table_name() -> sea_orm::DynIden {
        Alias::new("extension_migrations").into_iden()
    }
}

#[tokio::test]
async fn extension_cannot_claim_a_public_migration_version() -> TestResult {
    let options = DatabaseOptions::new("sqlite::memory:")?;
    let extension = MigratorExtension::<ConflictingMigrator>::new();
    let error =
        connect_and_migrate_with_extension(&options, MigrationOptions::default(), Some(&extension))
            .await
            .expect_err("public and extension registries must be disjoint");
    assert!(
        matches!(&error, DatabaseError::Migration(source) if source.to_string().contains("overlap the public registry")),
        "{error:?}"
    );
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
    assert!(
        matches!(&error, DatabaseError::Migration(source) if source.to_string().contains("unknown_version")),
        "{error:?}"
    );
    let db = Database::connect(file.url()).await?;
    assert_eq!(records(&db, "seaql_migrations").await?, original);
    assert!(!SchemaManager::new(&db).has_table("extension_probe").await?);
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_enterprise_history_is_preserved_during_community_startup() -> TestResult {
    let file = TemporaryDatabase::new()?;
    let db = Database::connect(file.url()).await?;
    for sql in [
        "CREATE TABLE seaql_migrations (version VARCHAR(255) NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL)",
        "CREATE TABLE enterprise_preserved_fixture (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
        "INSERT INTO enterprise_preserved_fixture VALUES (1, 'retained after downgrade')",
    ] {
        db.execute_unprepared(sql).await?;
    }
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO seaql_migrations (version, applied_at) VALUES (?, ?)",
        [
            "m20260815_000061_create_organization_foundation".into(),
            42_i64.into(),
        ],
    ))
    .await?;
    db.close().await?;

    connect_and_migrate(
        &DatabaseOptions::new(file.url())?,
        MigrationOptions::default(),
    )
    .await?
    .close()
    .await?;

    let db = Database::connect(file.url()).await?;
    let history = records(&db, "seaql_migrations").await?;
    assert!(history.contains(&(
        "m20260815_000061_create_organization_foundation".to_owned(),
        42
    )));
    assert!(
        history
            .iter()
            .any(|(version, _)| { version == "m20260906_000134_create_announcements" })
    );
    let retained = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, value FROM enterprise_preserved_fixture ORDER BY id",
        ))
        .await?;
    assert_eq!(retained.len(), 1);
    assert_eq!(
        retained[0].try_get::<String>("", "value")?,
        "retained after downgrade"
    );
    assert!(SchemaManager::new(&db).has_table("groups").await?);
    assert!(SchemaManager::new(&db).has_table("announcements").await?);
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn community_startup_still_rejects_unknown_migration_history() -> TestResult {
    let file = TemporaryDatabase::new()?;
    let db = Database::connect(file.url()).await?;
    db.execute_unprepared(
        "CREATE TABLE seaql_migrations (version VARCHAR(255) NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL)",
    )
    .await?;
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO seaql_migrations (version, applied_at) VALUES (?, ?)",
        ["unrecognized_migration".into(), 42_i64.into()],
    ))
    .await?;
    db.close().await?;

    let error = connect_and_migrate(
        &DatabaseOptions::new(file.url())?,
        MigrationOptions::default(),
    )
    .await
    .expect_err("unknown migration history must stop community startup");
    assert!(
        matches!(&error, DatabaseError::Migration(source) if source.to_string().contains("unrecognized_migration")),
        "{error:?}"
    );

    let db = Database::connect(file.url()).await?;
    assert_eq!(
        records(&db, "seaql_migrations").await?,
        vec![("unrecognized_migration".to_owned(), 42)]
    );
    assert!(!SchemaManager::new(&db).has_table("groups").await?);
    db.close().await?;
    Ok(())
}
