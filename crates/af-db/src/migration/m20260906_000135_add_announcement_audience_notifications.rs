use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use super::iden::announcements;

/// 扩展公告受众和站内产品更新通知的强类型取值范围。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(announcements::Entity)
                    .add_column(
                        ColumnDef::new(announcements::Column::Audience)
                            .small_integer()
                            .not_null()
                            .default(1_i16)
                            .check(
                                Expr::col(announcements::Column::Audience).is_in([1_i16, 2_i16]),
                            ),
                    )
                    .to_owned(),
            )
            .await?;
        widen_notification_checks(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        ensure_no_product_updates(manager).await?;
        narrow_notification_checks(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(announcements::Entity)
                    .drop_column(announcements::Column::Audience)
                    .to_owned(),
            )
            .await
    }
}

async fn widen_notification_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_notifications(manager.get_connection(), true).await,
        DbBackend::Postgres => {
            drop_postgres_checks(manager.get_connection()).await?;
            add_postgres_checks(manager.get_connection()).await
        }
        DbBackend::MySql => {
            drop_mysql_checks(manager.get_connection()).await?;
            add_mysql_checks(manager.get_connection()).await
        }
    }
}

async fn narrow_notification_checks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_notifications(manager.get_connection(), false).await,
        DbBackend::Postgres => {
            drop_postgres_checks(manager.get_connection()).await?;
            add_postgres_legacy_checks(manager.get_connection()).await
        }
        DbBackend::MySql => {
            drop_mysql_checks(manager.get_connection()).await?;
            add_mysql_legacy_checks(manager.get_connection()).await
        }
    }
}

async fn ensure_no_product_updates(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let row = manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            "SELECT 1 AS marker FROM user_notification_events WHERE kind = 3 OR channel = 2 OR source_kind = 3 LIMIT 1",
        ))
        .await?;
    if row.is_some() {
        return Err(DbErr::Custom(
            "存在站内产品更新通知事实，无法回退公告通知迁移".to_owned(),
        ));
    }
    Ok(())
}

async fn drop_postgres_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    let rows = connection
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            "SELECT con.conname AS constraint_name FROM pg_constraint con JOIN pg_class rel ON rel.oid = con.conrelid WHERE rel.relname = 'user_notification_events' AND con.contype = 'c'",
        ))
        .await?;
    for row in rows {
        let name: String = row.try_get("", "constraint_name")?;
        connection
            .execute_unprepared(&format!(
                "ALTER TABLE user_notification_events DROP CONSTRAINT \"{}\"",
                name.replace('"', "\"\"")
            ))
            .await?;
    }
    Ok(())
}

async fn add_postgres_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_kind CHECK (kind IN (1, 2, 3))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_channel CHECK (channel IN (1, 2))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_delivery_state CHECK (delivery_state IN (1, 2, 3, 4, 5))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_source_kind CHECK (source_kind IN (1, 2, 3))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_attempts CHECK (delivery_attempts BETWEEN 0 AND 5)")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_threshold CHECK (threshold_percent IS NULL OR threshold_percent BETWEEN 1 AND 99)")
        .await
        .map(|_| ())
}

async fn add_postgres_legacy_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_kind CHECK (kind IN (1, 2))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_channel CHECK (channel = 1)")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_delivery_state CHECK (delivery_state IN (1, 2, 3, 4))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_source_kind CHECK (source_kind IN (1, 2))")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_attempts CHECK (delivery_attempts BETWEEN 0 AND 5)")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE user_notification_events ADD CONSTRAINT ck_user_notification_events_threshold CHECK (threshold_percent IS NULL OR threshold_percent BETWEEN 1 AND 99)")
        .await
        .map(|_| ())
}

async fn drop_mysql_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    let rows = connection
        .query_all(Statement::from_string(
            DbBackend::MySql,
            "SELECT tc.CONSTRAINT_NAME AS constraint_name FROM information_schema.TABLE_CONSTRAINTS tc JOIN information_schema.CHECK_CONSTRAINTS cc ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE tc.CONSTRAINT_SCHEMA = DATABASE() AND tc.TABLE_NAME = 'user_notification_events' AND tc.CONSTRAINT_TYPE = 'CHECK'",
        ))
        .await?;
    for row in rows {
        let name: String = row.try_get("", "constraint_name")?;
        connection
            .execute_unprepared(&format!(
                "ALTER TABLE `user_notification_events` DROP CHECK `{}`",
                name.replace('`', "``")
            ))
            .await?;
    }
    Ok(())
}

async fn add_mysql_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    for sql in [
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_kind` CHECK (`kind` IN (1, 2, 3))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_channel` CHECK (`channel` IN (1, 2))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_delivery_state` CHECK (`delivery_state` IN (1, 2, 3, 4, 5))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_source_kind` CHECK (`source_kind` IN (1, 2, 3))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_attempts` CHECK (`delivery_attempts` BETWEEN 0 AND 5)",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_threshold` CHECK (`threshold_percent` IS NULL OR `threshold_percent` BETWEEN 1 AND 99)",
    ] {
        connection.execute_unprepared(sql).await?;
    }
    Ok(())
}

async fn add_mysql_legacy_checks(connection: &SchemaManagerConnection<'_>) -> Result<(), DbErr> {
    for sql in [
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_kind` CHECK (`kind` IN (1, 2))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_channel` CHECK (`channel` = 1)",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_delivery_state` CHECK (`delivery_state` IN (1, 2, 3, 4))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_source_kind` CHECK (`source_kind` IN (1, 2))",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_attempts` CHECK (`delivery_attempts` BETWEEN 0 AND 5)",
        "ALTER TABLE `user_notification_events` ADD CONSTRAINT `ck_user_notification_events_threshold` CHECK (`threshold_percent` IS NULL OR `threshold_percent` BETWEEN 1 AND 99)",
    ] {
        connection.execute_unprepared(sql).await?;
    }
    Ok(())
}

async fn rebuild_sqlite_notifications(
    connection: &SchemaManagerConnection<'_>,
    widened: bool,
) -> Result<(), DbErr> {
    connection
        .execute_unprepared("PRAGMA foreign_keys=OFF")
        .await?;
    let result = async {
        let checks = if widened {
            "CONSTRAINT ck_user_notification_events_kind CHECK (kind IN (1, 2, 3)), CONSTRAINT ck_user_notification_events_channel CHECK (channel IN (1, 2)), CONSTRAINT ck_user_notification_events_delivery_state CHECK (delivery_state IN (1, 2, 3, 4, 5)), CONSTRAINT ck_user_notification_events_source_kind CHECK (source_kind IN (1, 2, 3)), CONSTRAINT ck_user_notification_events_attempts CHECK (delivery_attempts BETWEEN 0 AND 5), CONSTRAINT ck_user_notification_events_threshold CHECK (threshold_percent IS NULL OR threshold_percent BETWEEN 1 AND 99)"
        } else {
            "CONSTRAINT ck_user_notification_events_kind CHECK (kind IN (1, 2)), CONSTRAINT ck_user_notification_events_channel CHECK (channel = 1), CONSTRAINT ck_user_notification_events_delivery_state CHECK (delivery_state IN (1, 2, 3, 4)), CONSTRAINT ck_user_notification_events_source_kind CHECK (source_kind IN (1, 2)), CONSTRAINT ck_user_notification_events_attempts CHECK (delivery_attempts BETWEEN 0 AND 5), CONSTRAINT ck_user_notification_events_threshold CHECK (threshold_percent IS NULL OR threshold_percent BETWEEN 1 AND 99)"
        };
        connection
            .execute_unprepared(&format!(
                "CREATE TABLE user_notification_events_new (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id BIGINT NOT NULL, kind SMALLINT NOT NULL, channel SMALLINT NOT NULL, template_version VARCHAR(32) NOT NULL, occurred_at DATETIME NOT NULL, delivery_state SMALLINT NOT NULL DEFAULT 1, delivery_attempts SMALLINT NOT NULL DEFAULT 0, source_kind SMALLINT NOT NULL, source_key VARCHAR(128) NOT NULL, observed_quota BIGINT, threshold_quota BIGINT, subscription_id VARCHAR(32), window_ends_at DATETIME, quota_amount BIGINT, quota_used BIGINT, threshold_percent SMALLINT, updated_at DATETIME NOT NULL, {checks}, FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE CASCADE ON DELETE RESTRICT)"
            ))
            .await?;
        connection
            .execute_unprepared("INSERT INTO user_notification_events_new (id, user_id, kind, channel, template_version, occurred_at, delivery_state, delivery_attempts, source_kind, source_key, observed_quota, threshold_quota, subscription_id, window_ends_at, quota_amount, quota_used, threshold_percent, updated_at) SELECT id, user_id, kind, channel, template_version, occurred_at, delivery_state, delivery_attempts, source_kind, source_key, observed_quota, threshold_quota, subscription_id, window_ends_at, quota_amount, quota_used, threshold_percent, updated_at FROM user_notification_events")
            .await?;
        connection
            .execute_unprepared("DROP TABLE user_notification_events")
            .await?;
        connection
            .execute_unprepared("ALTER TABLE user_notification_events_new RENAME TO user_notification_events")
            .await?;
        for sql in [
            "CREATE UNIQUE INDEX uq_user_notification_events_source ON user_notification_events(source_kind, source_key)",
            "CREATE INDEX idx_user_notification_events_user_cursor ON user_notification_events(user_id, occurred_at, id)",
            "CREATE INDEX idx_user_notification_events_delivery ON user_notification_events(delivery_state, occurred_at)",
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
