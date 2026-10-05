use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    refund_manual_completions, refund_reconciliation_entries, refund_requests, users,
};

use super::schema;

const MANUAL_UPDATE_TRIGGER: &str = "trg_refund_manual_completion_append_only_update";
const MANUAL_DELETE_TRIGGER: &str = "trg_refund_manual_completion_append_only_delete";
const POSTGRES_MANUAL_UPDATE_FUNCTION: &str = "reject_refund_manual_completion_update";
const POSTGRES_MANUAL_DELETE_FUNCTION: &str = "reject_refund_manual_completion_delete";

/// 为易支付人工退款建立不可变完成事实，并允许对账记录引用人工或 Provider 来源。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut table = schema::table(manager, refund_manual_completions::Entity);
        table
            .col(schema::auto_id(refund_manual_completions::Column::Id))
            .col(
                ColumnDef::new(refund_manual_completions::Column::CompletionKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_manual_completions::Column::RequestKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_manual_completions::Column::ExpectedVersion)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_manual_completions::Column::ActorUserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_manual_completions::Column::Result)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_manual_completions::Column::ReferenceSha256)
                    .char_len(64)
                    .not_null(),
            )
            .col(schema::timestamp(
                manager,
                refund_manual_completions::Column::CompletedAt,
            ))
            .col(schema::timestamp(
                manager,
                refund_manual_completions::Column::CreatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_manual_completion_request")
                    .from(
                        refund_manual_completions::Entity,
                        refund_manual_completions::Column::RequestKey,
                    )
                    .to(refund_requests::Entity, refund_requests::Column::RequestKey)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_manual_completion_actor")
                    .from(
                        refund_manual_completions::Entity,
                        refund_manual_completions::Column::ActorUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(Expr::col(refund_manual_completions::Column::Result).between(1_i16, 2_i16))
            .check(Expr::col(refund_manual_completions::Column::ExpectedVersion).gt(0_i64));
        manager.create_table(table).await?;
        for index in [
            Index::create()
                .name("uq_refund_manual_completion_key")
                .table(refund_manual_completions::Entity)
                .col(refund_manual_completions::Column::CompletionKey)
                .unique()
                .to_owned(),
            Index::create()
                .name("uq_refund_manual_completion_request")
                .table(refund_manual_completions::Entity)
                .col(refund_manual_completions::Column::RequestKey)
                .unique()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        alter_reconciliation(manager).await?;
        create_manual_append_only_guards(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_manual_append_only_guards(manager).await?;
        revert_reconciliation(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(refund_manual_completions::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn alter_reconciliation(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    if manager.get_database_backend() == DbBackend::Sqlite {
        connection
            .execute_unprepared("PRAGMA foreign_keys=OFF")
            .await?;
        let result = async {
            connection
                .execute_unprepared(
                    "ALTER TABLE refund_reconciliation_entries RENAME TO refund_reconciliation_entries_old",
                )
                .await?;
            connection
                .execute_unprepared(
                    "CREATE TABLE refund_reconciliation_entries (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT,\
                    request_key VARCHAR(32) NOT NULL,\
                    provider_event_id BIGINT,\
                    manual_completion_id BIGINT,\
                    user_id BIGINT NOT NULL,\
                    organization_id BIGINT,\
                    approval_actor_id BIGINT NOT NULL,\
                    order_kind SMALLINT NOT NULL,\
                    order_key VARCHAR(32) NOT NULL,\
                    provider VARCHAR(64) NOT NULL,\
                    amount_delta_minor BIGINT NOT NULL,\
                    currency VARCHAR(3) NOT NULL,\
                    created_at DATETIME NOT NULL,\
                    CONSTRAINT fk_refund_reconciliation_request FOREIGN KEY (request_key) REFERENCES refund_requests(request_key) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_provider_event FOREIGN KEY (provider_event_id) REFERENCES refund_provider_events(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_manual_completion FOREIGN KEY (manual_completion_id) REFERENCES refund_manual_completions(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_user FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_approval_actor FOREIGN KEY (approval_actor_id) REFERENCES users(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT ck_refund_reconciliation_source CHECK ((provider_event_id IS NOT NULL AND manual_completion_id IS NULL) OR (provider_event_id IS NULL AND manual_completion_id IS NOT NULL)),\
                    CONSTRAINT ck_refund_reconciliation_order_kind CHECK (order_kind BETWEEN 1 AND 2),\
                    CONSTRAINT ck_refund_reconciliation_amount CHECK (amount_delta_minor < 0 AND amount_delta_minor != -9223372036854775808),\
                    CONSTRAINT ck_refund_reconciliation_org_scope CHECK (order_kind = 1 OR organization_id IS NULL)\
                    )",
                )
                .await?;
            connection
                .execute_unprepared(
                    "INSERT INTO refund_reconciliation_entries (id, request_key, provider_event_id, manual_completion_id, user_id, organization_id, approval_actor_id, order_kind, order_key, provider, amount_delta_minor, currency, created_at) SELECT id, request_key, provider_event_id, NULL, user_id, organization_id, approval_actor_id, order_kind, order_key, provider, amount_delta_minor, currency, created_at FROM refund_reconciliation_entries_old",
                )
                .await?;
            connection
                .execute_unprepared("DROP TABLE refund_reconciliation_entries_old")
                .await?;
            for sql in [
                "CREATE UNIQUE INDEX uq_refund_reconciliation_request ON refund_reconciliation_entries(request_key)",
                "CREATE UNIQUE INDEX uq_refund_reconciliation_provider_event ON refund_reconciliation_entries(provider_event_id)",
                "CREATE UNIQUE INDEX uq_refund_reconciliation_manual_completion ON refund_reconciliation_entries(manual_completion_id)",
                "CREATE INDEX idx_refund_reconciliation_user_id ON refund_reconciliation_entries(user_id, id)",
                "CREATE INDEX idx_refund_reconciliation_organization_id ON refund_reconciliation_entries(organization_id, id)",
                "CREATE INDEX idx_refund_reconciliation_approval_actor_id ON refund_reconciliation_entries(approval_actor_id, id)",
            ] {
                connection.execute_unprepared(sql).await?;
            }
            connection
                .execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_update BEFORE UPDATE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END")
                .await?;
            connection
                .execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_delete BEFORE DELETE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END")
                .await
                .map(|_| ())
        }
        .await;
        let restore = connection
            .execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .map(|_| ());
        return match (result, restore) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        };
    }

    manager
        .alter_table(
            Table::alter()
                .table(refund_reconciliation_entries::Entity)
                .add_column(
                    ColumnDef::new(refund_reconciliation_entries::Column::ManualCompletionId)
                        .big_integer(),
                )
                .modify_column(
                    ColumnDef::new(refund_reconciliation_entries::Column::ProviderEventId)
                        .big_integer(),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_foreign_key(
            ForeignKey::create()
                .name("fk_refund_reconciliation_manual_completion")
                .from(
                    refund_reconciliation_entries::Entity,
                    refund_reconciliation_entries::Column::ManualCompletionId,
                )
                .to(
                    refund_manual_completions::Entity,
                    refund_manual_completions::Column::Id,
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict)
                .to_owned(),
        )
        .await?;
    for index in [Index::create()
        .name("uq_refund_reconciliation_manual_completion")
        .table(refund_reconciliation_entries::Entity)
        .col(refund_reconciliation_entries::Column::ManualCompletionId)
        .unique()
        .to_owned()]
    {
        manager.create_index(index).await?;
    }
    let check = match manager.get_database_backend() {
        DbBackend::Postgres => {
            "ALTER TABLE refund_reconciliation_entries ADD CONSTRAINT ck_refund_reconciliation_source CHECK ((provider_event_id IS NOT NULL AND manual_completion_id IS NULL) OR (provider_event_id IS NULL AND manual_completion_id IS NOT NULL))"
        }
        DbBackend::MySql => {
            "ALTER TABLE `refund_reconciliation_entries` ADD CONSTRAINT `ck_refund_reconciliation_source` CHECK ((`provider_event_id` IS NOT NULL AND `manual_completion_id` IS NULL) OR (`provider_event_id` IS NULL AND `manual_completion_id` IS NOT NULL))"
        }
        DbBackend::Sqlite => unreachable!(),
    };
    connection.execute_unprepared(check).await.map(|_| ())
}

/// 回滚时恢复原迁移的单一 Provider 回执来源结构。
async fn revert_reconciliation(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    ensure_no_manual_refund_facts(manager).await?;
    if manager.get_database_backend() == DbBackend::Sqlite {
        connection
            .execute_unprepared("PRAGMA foreign_keys=OFF")
            .await?;
        let result = async {
            for sql in [
                "DROP TRIGGER IF EXISTS trg_refund_reconciliation_append_only_update",
                "DROP TRIGGER IF EXISTS trg_refund_reconciliation_append_only_delete",
                "DROP INDEX IF EXISTS uq_refund_reconciliation_manual_completion",
                "ALTER TABLE refund_reconciliation_entries RENAME TO refund_reconciliation_entries_new",
                "CREATE TABLE refund_reconciliation_entries (\
                    id INTEGER PRIMARY KEY AUTOINCREMENT,\
                    request_key VARCHAR(32) NOT NULL,\
                    provider_event_id BIGINT NOT NULL,\
                    user_id BIGINT NOT NULL,\
                    organization_id BIGINT,\
                    approval_actor_id BIGINT NOT NULL,\
                    order_kind SMALLINT NOT NULL,\
                    order_key VARCHAR(32) NOT NULL,\
                    provider VARCHAR(64) NOT NULL,\
                    amount_delta_minor BIGINT NOT NULL,\
                    currency VARCHAR(3) NOT NULL,\
                    created_at DATETIME NOT NULL,\
                    CONSTRAINT fk_refund_reconciliation_request FOREIGN KEY (request_key) REFERENCES refund_requests(request_key) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_provider_event FOREIGN KEY (provider_event_id) REFERENCES refund_provider_events(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_user FOREIGN KEY (user_id) REFERENCES users(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT fk_refund_reconciliation_approval_actor FOREIGN KEY (approval_actor_id) REFERENCES users(id) ON UPDATE RESTRICT ON DELETE RESTRICT,\
                    CONSTRAINT ck_refund_reconciliation_order_kind CHECK (order_kind BETWEEN 1 AND 2),\
                    CONSTRAINT ck_refund_reconciliation_amount CHECK (amount_delta_minor < 0 AND amount_delta_minor != -9223372036854775808),\
                    CONSTRAINT ck_refund_reconciliation_org_scope CHECK (order_kind = 1 OR organization_id IS NULL)\
                )",
                "INSERT INTO refund_reconciliation_entries (id, request_key, provider_event_id, user_id, organization_id, approval_actor_id, order_kind, order_key, provider, amount_delta_minor, currency, created_at) SELECT id, request_key, provider_event_id, user_id, organization_id, approval_actor_id, order_kind, order_key, provider, amount_delta_minor, currency, created_at FROM refund_reconciliation_entries_new WHERE provider_event_id IS NOT NULL",
                "DROP TABLE refund_reconciliation_entries_new",
                "CREATE UNIQUE INDEX uq_refund_reconciliation_request ON refund_reconciliation_entries(request_key)",
                "CREATE UNIQUE INDEX uq_refund_reconciliation_provider_event ON refund_reconciliation_entries(provider_event_id)",
                "CREATE INDEX idx_refund_reconciliation_user_id ON refund_reconciliation_entries(user_id, id)",
                "CREATE INDEX idx_refund_reconciliation_organization_id ON refund_reconciliation_entries(organization_id, id)",
                "CREATE INDEX idx_refund_reconciliation_approval_actor_id ON refund_reconciliation_entries(approval_actor_id, id)",
            ] {
                connection.execute_unprepared(sql).await?;
            }
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_update BEFORE UPDATE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END").await?;
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_delete BEFORE DELETE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END").await.map(|_| ())
        }.await;
        let restore = connection
            .execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .map(|_| ());
        return match (result, restore) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        };
    }
    let drop_check = match manager.get_database_backend() {
        DbBackend::Postgres => {
            "ALTER TABLE refund_reconciliation_entries DROP CONSTRAINT IF EXISTS ck_refund_reconciliation_source"
        }
        DbBackend::MySql => {
            "ALTER TABLE `refund_reconciliation_entries` DROP CHECK `ck_refund_reconciliation_source`"
        }
        DbBackend::Sqlite => unreachable!(),
    };
    connection.execute_unprepared(drop_check).await?;
    // MySQL 要求先解除外键，再删除承载外键所需的唯一索引。
    manager
        .drop_foreign_key(
            ForeignKey::drop()
                .name("fk_refund_reconciliation_manual_completion")
                .table(refund_reconciliation_entries::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .drop_index(
            Index::drop()
                .name("uq_refund_reconciliation_manual_completion")
                .table(refund_reconciliation_entries::Entity)
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(refund_reconciliation_entries::Entity)
                .drop_column(refund_reconciliation_entries::Column::ManualCompletionId)
                .modify_column(
                    ColumnDef::new(refund_reconciliation_entries::Column::ProviderEventId)
                        .big_integer()
                        .not_null(),
                )
                .to_owned(),
        )
        .await
}

/// 人工退款完成后禁止降级迁移，避免删除完成事实或把状态截断为旧范围。
async fn ensure_no_manual_refund_facts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let connection = manager.get_connection();
    let manual_fact = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT 1 AS marker FROM refund_reconciliation_entries WHERE manual_completion_id IS NOT NULL LIMIT 1",
        ))
        .await?;
    let manual_completion = connection
        .query_one(Statement::from_string(
            backend,
            "SELECT 1 AS marker FROM refund_manual_completions LIMIT 1",
        ))
        .await?;
    if manual_fact.is_some() || manual_completion.is_some() {
        return Err(DbErr::Custom(
            "存在人工退款完成事实，无法回退人工退款迁移".to_owned(),
        ));
    }
    Ok(())
}

async fn create_manual_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => {
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_UPDATE_TRIGGER} BEFORE UPDATE ON refund_manual_completions BEGIN SELECT RAISE(ABORT, 'refund manual completions are append only'); END")).await?;
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_DELETE_TRIGGER} BEFORE DELETE ON refund_manual_completions BEGIN SELECT RAISE(ABORT, 'refund manual completions are append only'); END")).await.map(|_| ())
        }
        DbBackend::Postgres => {
            connection.execute_unprepared(&format!("CREATE OR REPLACE FUNCTION {POSTGRES_MANUAL_UPDATE_FUNCTION}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refund manual completions are append only'; END; $$")).await?;
            connection.execute_unprepared(&format!("CREATE OR REPLACE FUNCTION {POSTGRES_MANUAL_DELETE_FUNCTION}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refund manual completions are append only'; END; $$")).await?;
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_UPDATE_TRIGGER} BEFORE UPDATE ON refund_manual_completions FOR EACH ROW EXECUTE FUNCTION reject_refund_manual_completion_update()")).await?;
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_DELETE_TRIGGER} BEFORE DELETE ON refund_manual_completions FOR EACH ROW EXECUTE FUNCTION reject_refund_manual_completion_delete()")).await.map(|_| ())
        }
        DbBackend::MySql => {
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_UPDATE_TRIGGER} BEFORE UPDATE ON refund_manual_completions FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'refund manual completions are append only'")).await?;
            connection.execute_unprepared(&format!("CREATE TRIGGER {MANUAL_DELETE_TRIGGER} BEFORE DELETE ON refund_manual_completions FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'refund manual completions are append only'")).await.map(|_| ())
        }
    }
}

async fn drop_manual_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::MySql => {
            connection
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {MANUAL_UPDATE_TRIGGER}"))
                .await?;
            connection
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {MANUAL_DELETE_TRIGGER}"))
                .await
                .map(|_| ())
        }
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {MANUAL_UPDATE_TRIGGER} ON refund_manual_completions"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {MANUAL_DELETE_TRIGGER} ON refund_manual_completions"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_MANUAL_UPDATE_FUNCTION}()"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_MANUAL_DELETE_FUNCTION}()"
                ))
                .await
                .map(|_| ())
        }
    }
}
