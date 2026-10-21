use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

/// 路由可绑定整个渠道；保留旧候选的标识、统计与指定凭据外键。
#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        match manager.get_database_backend() {
            DbBackend::Sqlite => rebuild_sqlite(manager, true).await?,
            DbBackend::Postgres => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE route_channels ALTER COLUMN credential_id DROP NOT NULL",
                    )
                    .await?;
                connection.execute_unprepared("ALTER TABLE route_channels ADD COLUMN binding_key BIGINT GENERATED ALWAYS AS (COALESCE(credential_id, 0)) STORED").await?;
            }
            DbBackend::MySql => {
                connection
                    .execute_unprepared(
                        "ALTER TABLE route_channels MODIFY credential_id BIGINT NULL",
                    )
                    .await?;
                // MySQL 不允许存储生成列的基础列参与级联外键；虚拟索引保留既有级联语义。
                connection.execute_unprepared("ALTER TABLE route_channels ADD COLUMN binding_key BIGINT GENERATED ALWAYS AS (COALESCE(credential_id, 0)) VIRTUAL").await?;
            }
        }
        connection.execute_unprepared("CREATE UNIQUE INDEX uq_route_channels_binding ON route_channels (route_id, channel_id, binding_key)").await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        let query = sea_orm::Statement::from_string(
            manager.get_database_backend(),
            "SELECT id FROM route_channels WHERE credential_id IS NULL LIMIT 1",
        );
        if connection.query_one(query).await?.is_some() {
            return Err(DbErr::Custom("存在渠道级路由绑定，无法回退".to_owned()));
        }
        match manager.get_database_backend() {
            DbBackend::Sqlite => rebuild_sqlite(manager, false).await?,
            DbBackend::Postgres => {
                connection
                    .execute_unprepared("DROP INDEX uq_route_channels_binding")
                    .await?;
                connection
                    .execute_unprepared("ALTER TABLE route_channels DROP COLUMN binding_key")
                    .await?;
                connection
                    .execute_unprepared(
                        "ALTER TABLE route_channels ALTER COLUMN credential_id SET NOT NULL",
                    )
                    .await?;
            }
            DbBackend::MySql => {
                connection
                    .execute_unprepared("DROP INDEX uq_route_channels_binding ON route_channels")
                    .await?;
                connection
                    .execute_unprepared("ALTER TABLE route_channels DROP COLUMN binding_key")
                    .await?;
                connection
                    .execute_unprepared(
                        "ALTER TABLE route_channels MODIFY credential_id BIGINT NOT NULL",
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

async fn rebuild_sqlite(manager: &SchemaManager<'_>, nullable: bool) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let nullability = if nullable { "" } else { "NOT NULL" };
    let generated = if nullable {
        ", binding_key BIGINT GENERATED ALWAYS AS (COALESCE(credential_id, 0)) STORED"
    } else {
        ""
    };
    connection.execute_unprepared(&format!("CREATE TABLE route_channels_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT, route_id BIGINT NOT NULL, channel_id BIGINT NOT NULL,
        credential_id BIGINT {nullability}, priority INTEGER NOT NULL DEFAULT 0 CHECK (priority >= 0),
        weight INTEGER NOT NULL DEFAULT 10 CHECK (weight >= 0), enabled BOOLEAN NOT NULL DEFAULT TRUE,
        success_count BIGINT NOT NULL DEFAULT 0 CHECK (success_count >= 0),
        fail_count BIGINT NOT NULL DEFAULT 0 CHECK (fail_count >= 0),
        total_latency BIGINT NOT NULL DEFAULT 0 CHECK (total_latency >= 0),
        cooldown_level SMALLINT NOT NULL DEFAULT 0 CHECK (cooldown_level BETWEEN 0 AND 3),
        cooldown_until DATETIME, last_selected_at DATETIME, last_failure_at DATETIME,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        {generated},
        FOREIGN KEY (route_id) REFERENCES routes(id) ON UPDATE CASCADE ON DELETE CASCADE,
        FOREIGN KEY (channel_id) REFERENCES channels(id) ON UPDATE CASCADE ON DELETE CASCADE,
        FOREIGN KEY (credential_id, channel_id) REFERENCES credentials(id, channel_id) ON UPDATE CASCADE ON DELETE CASCADE
    )")).await?;
    connection.execute_unprepared("INSERT INTO route_channels_new (id, route_id, channel_id, credential_id, priority, weight, enabled, success_count, fail_count, total_latency, cooldown_level, cooldown_until, last_selected_at, last_failure_at, created_at, updated_at) SELECT id, route_id, channel_id, credential_id, priority, weight, enabled, success_count, fail_count, total_latency, cooldown_level, cooldown_until, last_selected_at, last_failure_at, created_at, updated_at FROM route_channels").await?;
    connection
        .execute_unprepared("DROP TABLE route_channels")
        .await?;
    connection
        .execute_unprepared("ALTER TABLE route_channels_new RENAME TO route_channels")
        .await?;
    connection.execute_unprepared("CREATE INDEX idx_route_channels_route_enabled ON route_channels (route_id, enabled, priority, id)").await?;
    connection.execute_unprepared("CREATE UNIQUE INDEX uq_route_channels_route_channel_credential ON route_channels (route_id, channel_id, credential_id)").await?;
    Ok(())
}
