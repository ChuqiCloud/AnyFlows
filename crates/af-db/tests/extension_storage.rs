use std::error::Error;

use af_db::{DatabaseOptions, MigrationOptions, connect_and_migrate};
use sea_orm::{ConnectionTrait, Statement, TransactionTrait};

#[tokio::test]
async fn extension_queries_and_transactions_share_the_core_pool() -> Result<(), Box<dyn Error>> {
    let options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = connect_and_migrate(&options, MigrationOptions::default()).await?;
    let connection = pool.extension_connection();
    connection
        .execute_unprepared("CREATE TABLE extension_storage_probe (id INTEGER PRIMARY KEY)")
        .await?;

    let transaction = connection.begin().await?;
    transaction
        .execute_unprepared("INSERT INTO extension_storage_probe (id) VALUES (1)")
        .await?;
    transaction.rollback().await?;

    let records = connection
        .query_all(Statement::from_string(
            connection.get_database_backend(),
            "SELECT id FROM extension_storage_probe",
        ))
        .await?;
    assert!(records.is_empty());
    connection
        .execute_unprepared("INSERT INTO extension_storage_probe (id) VALUES (2)")
        .await?;

    let transaction = connection.begin().await?;
    let row = transaction
        .query_one(Statement::from_string(
            transaction.get_database_backend(),
            "SELECT id FROM extension_storage_probe",
        ))
        .await?
        .expect("extension transaction must see the shared connection data");
    assert_eq!(row.try_get::<i64>("", "id")?, 2);
    transaction.commit().await?;
    pool.ping().await?;
    pool.close().await?;
    Ok(())
}
