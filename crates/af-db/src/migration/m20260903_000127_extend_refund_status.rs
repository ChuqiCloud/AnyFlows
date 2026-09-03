use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

/// 扩展退款请求状态范围，允许人工退款完成事实落在独立终态。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        match manager.get_database_backend() {
            DbBackend::Sqlite => rebuild_sqlite_refund_requests(connection).await?,
            DbBackend::Postgres => {
                connection
                    .execute_unprepared(
                        r#"DO $$
DECLARE constraint_name text;
BEGIN
  FOR constraint_name IN
    SELECT con.conname
      FROM pg_constraint con
      JOIN pg_class rel ON rel.oid = con.conrelid
     WHERE rel.relname = 'refund_requests'
       AND con.contype = 'c'
       AND pg_get_constraintdef(con.oid) ILIKE '%status%BETWEEN%1%5%'
  LOOP
    EXECUTE format('ALTER TABLE refund_requests DROP CONSTRAINT %I', constraint_name);
  END LOOP;
END $$;"#,
                    )
                    .await?;
                connection
                    .execute_unprepared(
                        "ALTER TABLE refund_requests ADD CONSTRAINT ck_refund_requests_status_range CHECK (status BETWEEN 1 AND 7)",
                    )
                    .await?;
            }
            DbBackend::MySql => {
                // 连接池可能为每条语句选择不同连接，先读取约束名称，再在同一迁移连接上逐条执行。
                let rows = connection
                    .query_all(Statement::from_string(
                        DbBackend::MySql,
                        r#"SELECT tc.CONSTRAINT_NAME AS constraint_name
FROM information_schema.TABLE_CONSTRAINTS tc
JOIN information_schema.CHECK_CONSTRAINTS cc
  ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA
 AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
WHERE tc.CONSTRAINT_SCHEMA = DATABASE()
  AND tc.TABLE_NAME = 'refund_requests'
  AND tc.CONSTRAINT_TYPE = 'CHECK'
  AND LOWER(cc.CHECK_CLAUSE) LIKE '%status%'"#,
                    ))
                    .await?;
                for row in rows {
                    let constraint: String = row.try_get("", "constraint_name")?;
                    let statement = format!(
                        "ALTER TABLE `refund_requests` DROP CHECK `{}`",
                        constraint.replace('`', "``")
                    );
                    connection.execute_unprepared(&statement).await?;
                }
                connection
                    .execute_unprepared(
                        "ALTER TABLE `refund_requests` ADD CONSTRAINT `ck_refund_requests_status_range` CHECK (`status` BETWEEN 1 AND 7)",
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        ensure_no_manual_refund_facts(manager).await?;
        match manager.get_database_backend() {
            DbBackend::Sqlite => rebuild_sqlite_refund_requests_with_status(connection, 5).await?,
            DbBackend::Postgres => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE refund_requests DROP CONSTRAINT IF EXISTS ck_refund_requests_status_range",
                    )
                    .await?;
                connection
                    .execute_unprepared(
                        "ALTER TABLE refund_requests ADD CONSTRAINT ck_refund_requests_status_legacy CHECK (status BETWEEN 1 AND 5)",
                    )
                    .await?;
            }
            DbBackend::MySql => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE `refund_requests` DROP CHECK `ck_refund_requests_status_range`",
                    )
                    .await?;
                connection
                    .execute_unprepared(
                        "ALTER TABLE `refund_requests` ADD CONSTRAINT `ck_refund_requests_status_legacy` CHECK (`status` BETWEEN 1 AND 5)",
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// 人工退款完成事实依赖 6/7 状态，降级前必须先拒绝而不是截断状态或删除事实。
async fn ensure_no_manual_refund_facts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let connection = manager.get_connection();
    let manual_status = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT 1 AS marker FROM refund_requests WHERE status IN (6, 7) LIMIT 1",
        ))
        .await?;
    let manual_completion = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT 1 AS marker FROM refund_manual_completions LIMIT 1",
        ))
        .await?;
    if manual_status.is_some() || manual_completion.is_some() {
        return Err(DbErr::Custom(
            "存在人工退款完成事实，无法回退人工退款状态迁移".to_owned(),
        ));
    }
    Ok(())
}

async fn rebuild_sqlite_refund_requests(
    connection: &SchemaManagerConnection<'_>,
) -> Result<(), DbErr> {
    rebuild_sqlite_refund_requests_with_status(connection, 7).await
}

async fn rebuild_sqlite_refund_requests_with_status(
    connection: &SchemaManagerConnection<'_>,
    max_status: i16,
) -> Result<(), DbErr> {
    connection
        .execute_unprepared("PRAGMA foreign_keys=OFF")
        .await?;
    let result = async {
        connection
            .execute_unprepared(
            &format!(
                "CREATE TABLE refund_requests_new (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT,\
                    request_key VARCHAR(32) NOT NULL,\
                    idempotency_key VARCHAR(32) NOT NULL,\
                    user_id BIGINT NOT NULL,\
                    order_kind SMALLINT NOT NULL,\
                    order_key VARCHAR(32) NOT NULL,\
                    provider VARCHAR(64) NOT NULL,\
                    payment_reference VARCHAR(128),\
                    currency VARCHAR(3) NOT NULL,\
                    original_amount_minor BIGINT NOT NULL,\
                    refund_amount_minor BIGINT NOT NULL,\
                    provider_refund_id VARCHAR(128),\
                    status SMALLINT NOT NULL,\
                    approval_status SMALLINT NOT NULL DEFAULT 1,\
                    approval_actor_id BIGINT,\
                    approval_reason VARCHAR(512),\
                    version BIGINT NOT NULL,\
                    created_at DATETIME NOT NULL,\
                    updated_at DATETIME NOT NULL,\
                    CONSTRAINT fk_refund_requests_user FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE CASCADE ON DELETE RESTRICT,\
                    CONSTRAINT ck_refund_requests_order_kind CHECK (order_kind BETWEEN 1 AND 2),\
                    CONSTRAINT ck_refund_requests_original_amount CHECK (original_amount_minor > 0),\
                    CONSTRAINT ck_refund_requests_refund_amount_positive CHECK (refund_amount_minor > 0),\
                    CONSTRAINT ck_refund_requests_refund_amount_limit CHECK (refund_amount_minor <= original_amount_minor),\
                    CONSTRAINT ck_refund_requests_status_range CHECK (status BETWEEN 1 AND {max_status}),\
                    CONSTRAINT ck_refund_requests_version CHECK (version > 0)\
                )"
            ),
            )
            .await?;
        connection
            .execute_unprepared(
            "INSERT INTO refund_requests_new (id, request_key, idempotency_key, user_id, order_kind, order_key, provider, payment_reference, currency, original_amount_minor, refund_amount_minor, provider_refund_id, status, approval_status, approval_actor_id, approval_reason, version, created_at, updated_at) SELECT id, request_key, idempotency_key, user_id, order_kind, order_key, provider, payment_reference, currency, original_amount_minor, refund_amount_minor, provider_refund_id, status, approval_status, approval_actor_id, approval_reason, version, created_at, updated_at FROM refund_requests",
            )
            .await?;
        connection
            .execute_unprepared("DROP TABLE refund_requests")
            .await?;
        connection
            .execute_unprepared("ALTER TABLE refund_requests_new RENAME TO refund_requests")
            .await?;
        for sql in [
            "CREATE UNIQUE INDEX uq_refund_requests_request_key ON refund_requests(request_key)",
            "CREATE UNIQUE INDEX uq_refund_requests_user_idempotency_key ON refund_requests(user_id, idempotency_key)",
            "CREATE UNIQUE INDEX uq_refund_requests_order ON refund_requests(order_kind, order_key)",
            "CREATE INDEX idx_refund_requests_approval_status_id ON refund_requests(approval_status, id)",
        ] {
            connection.execute_unprepared(sql).await?;
        }
        Ok::<(), DbErr>(())
    }
    .await;
    let restore = connection
        .execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .map(|_| ());
    match (result, restore) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
