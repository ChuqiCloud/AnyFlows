use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use super::iden::credentials;

const SHAPE_CONSTRAINT: &str = "ck_credentials_spark_shadow_shape";
const ACTIVE_SHADOW_INDEX: &str = "uq_credentials_active_spark_parent";
const SQLITE_INSERT_TRIGGER: &str = "trg_credentials_spark_shape_insert";
const SQLITE_UPDATE_TRIGGER: &str = "trg_credentials_spark_shape_update";
const MYSQL_INSERT_TRIGGER: &str = "trg_credentials_spark_shape_insert";
const MYSQL_UPDATE_TRIGGER: &str = "trg_credentials_spark_shape_update";

/// 固化普通凭据与 Spark 影子的二选一形状，并限制每个母凭据只有一个活动影子。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::MySql {
            // MySQL 触发器负责同步唯一投影，因此必须先创建投影列和索引。
            create_active_shadow_index(manager).await?;
            return create_shape_guard(manager).await;
        }
        create_shape_guard(manager).await?;
        create_active_shadow_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::MySql {
            drop_shape_guard(manager).await?;
            return drop_active_shadow_index(manager).await;
        }
        drop_active_shadow_index(manager).await?;
        drop_shape_guard(manager).await
    }
}

async fn create_shape_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Postgres => manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE credentials ADD CONSTRAINT ck_credentials_spark_shadow_shape \
                 CHECK (deleted_at IS NOT NULL OR \
                 (parent_id IS NULL AND quota_dimension = 'global') OR \
                 (parent_id IS NOT NULL AND quota_dimension = 'spark' AND kind = 'oauth' AND \
                 concurrency IS NULL AND proxy_id IS NULL AND oauth_provider IS NULL AND oauth_account_key IS NULL AND \
                 oauth_project_id IS NULL AND oauth_token_pending = FALSE)) NOT VALID",
            )
            .await
            .map(|_| ()),
        DbBackend::Sqlite => {
            for statement in [
                "CREATE TRIGGER trg_credentials_spark_shape_insert BEFORE INSERT ON credentials \
                 WHEN NEW.deleted_at IS NULL AND NOT (\
                 (NEW.parent_id IS NULL AND NEW.quota_dimension = 'global') OR \
                 (NEW.parent_id IS NOT NULL AND NEW.quota_dimension = 'spark' AND NEW.kind = 'oauth' AND \
                 NEW.concurrency IS NULL AND NEW.proxy_id IS NULL AND NEW.oauth_provider IS NULL AND NEW.oauth_account_key IS NULL AND \
                 NEW.oauth_project_id IS NULL AND NEW.oauth_token_pending = 0)) \
                 BEGIN SELECT RAISE(ABORT, 'invalid Spark shadow shape'); END",
                "CREATE TRIGGER trg_credentials_spark_shape_update BEFORE UPDATE ON credentials \
                 WHEN NEW.deleted_at IS NULL AND NOT (\
                 (NEW.parent_id IS NULL AND NEW.quota_dimension = 'global') OR \
                 (NEW.parent_id IS NOT NULL AND NEW.quota_dimension = 'spark' AND NEW.kind = 'oauth' AND \
                 NEW.concurrency IS NULL AND NEW.proxy_id IS NULL AND NEW.oauth_provider IS NULL AND NEW.oauth_account_key IS NULL AND \
                 NEW.oauth_project_id IS NULL AND NEW.oauth_token_pending = 0)) \
                 BEGIN SELECT RAISE(ABORT, 'invalid Spark shadow shape'); END",
            ] {
                manager
                    .get_connection()
                    .execute_unprepared(statement)
                    .await?;
            }
            Ok(())
        }
        DbBackend::MySql => {
            for statement in [
                "CREATE TRIGGER trg_credentials_spark_shape_insert BEFORE INSERT ON credentials FOR EACH ROW \
                 BEGIN SET NEW._active_spark_parent = CASE \
                 WHEN NEW.deleted_at IS NULL AND NEW.quota_dimension = 'spark' THEN NEW.parent_id ELSE NULL END; \
                 IF NEW.deleted_at IS NULL AND NOT (\
                 (NEW.parent_id IS NULL AND NEW.quota_dimension = 'global') OR \
                 (NEW.parent_id IS NOT NULL AND NEW.quota_dimension = 'spark' AND NEW.kind = 'oauth' AND \
                 NEW.concurrency IS NULL AND NEW.proxy_id IS NULL AND NEW.oauth_provider IS NULL AND NEW.oauth_account_key IS NULL AND \
                 NEW.oauth_project_id IS NULL AND NEW.oauth_token_pending = FALSE)) \
                 THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'invalid Spark shadow shape'; END IF; END",
                "CREATE TRIGGER trg_credentials_spark_shape_update BEFORE UPDATE ON credentials FOR EACH ROW \
                 BEGIN SET NEW._active_spark_parent = CASE \
                 WHEN NEW.deleted_at IS NULL AND NEW.quota_dimension = 'spark' THEN NEW.parent_id ELSE NULL END; \
                 IF NEW.deleted_at IS NULL AND NOT (\
                 (NEW.parent_id IS NULL AND NEW.quota_dimension = 'global') OR \
                 (NEW.parent_id IS NOT NULL AND NEW.quota_dimension = 'spark' AND NEW.kind = 'oauth' AND \
                 NEW.concurrency IS NULL AND NEW.proxy_id IS NULL AND NEW.oauth_provider IS NULL AND NEW.oauth_account_key IS NULL AND \
                 NEW.oauth_project_id IS NULL AND NEW.oauth_token_pending = FALSE)) \
                 THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'invalid Spark shadow shape'; END IF; END",
            ] {
                manager
                    .get_connection()
                    .execute_unprepared(statement)
                    .await?;
            }
            Ok(())
        }
    }
}

async fn create_active_shadow_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        // 组合 ALTER 会让 InnoDB 重建带自引用外键的表并报 1215；拆分后新增列可走即时 DDL。
        for statement in [
            "ALTER TABLE `credentials` ADD COLUMN `_active_spark_parent` BIGINT NULL",
            "UPDATE `credentials` SET `_active_spark_parent` = CASE \
             WHEN `deleted_at` IS NULL AND `quota_dimension` = 'spark' THEN `parent_id` ELSE NULL END",
            "CREATE UNIQUE INDEX `uq_credentials_active_spark_parent` \
             ON `credentials` (`_active_spark_parent`)",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        return Ok(());
    }
    manager
        .create_index(
            Index::create()
                .name(ACTIVE_SHADOW_INDEX)
                .table(credentials::Entity)
                .col(credentials::Column::ParentId)
                .unique()
                .and_where(Expr::col(credentials::Column::ParentId).is_not_null())
                .and_where(Expr::col(credentials::Column::QuotaDimension).eq("spark"))
                .and_where(Expr::col(credentials::Column::DeletedAt).is_null())
                .to_owned(),
        )
        .await
}

async fn drop_active_shadow_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        for statement in [
            "DROP INDEX `uq_credentials_active_spark_parent` ON `credentials`",
            "ALTER TABLE `credentials` DROP COLUMN `_active_spark_parent`",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        return Ok(());
    }
    manager
        .drop_index(
            Index::drop()
                .name(ACTIVE_SHADOW_INDEX)
                .table(credentials::Entity)
                .to_owned(),
        )
        .await
}

async fn drop_shape_guard(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Postgres => manager
            .get_connection()
            .execute_unprepared(&format!(
                "ALTER TABLE credentials DROP CONSTRAINT IF EXISTS {SHAPE_CONSTRAINT}"
            ))
            .await
            .map(|_| ()),
        DbBackend::Sqlite => {
            for trigger in [SQLITE_INSERT_TRIGGER, SQLITE_UPDATE_TRIGGER] {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .await?;
            }
            Ok(())
        }
        DbBackend::MySql => {
            for trigger in [MYSQL_INSERT_TRIGGER, MYSQL_UPDATE_TRIGGER] {
                manager
                    .get_connection()
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS `{trigger}`"))
                    .await?;
            }
            Ok(())
        }
    }
}
