use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    refund_provider_events, refund_reconciliation_entries, refund_requests, users,
};

use super::schema;

const UPDATE_TRIGGER: &str = "trg_refund_reconciliation_append_only_update";
const DELETE_TRIGGER: &str = "trg_refund_reconciliation_append_only_delete";
const POSTGRES_UPDATE_FUNCTION: &str = "reject_refund_reconciliation_update";
const POSTGRES_DELETE_FUNCTION: &str = "reject_refund_reconciliation_delete";

/// 创建退款成功后的负向现金对账账本，并在数据库边界禁止改写或删除。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = schema::table(manager, refund_reconciliation_entries::Entity);
        statement
            .col(schema::auto_id(refund_reconciliation_entries::Column::Id))
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::RequestKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::ProviderEventId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::UserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::OrganizationId)
                    .big_integer(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::ApprovalActorId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::OrderKind)
                    .small_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::OrderKey)
                    .char_len(32)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::Provider)
                    .string_len(64)
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::AmountDeltaMinor)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(refund_reconciliation_entries::Column::Currency)
                    .char_len(3)
                    .not_null(),
            )
            .col(schema::timestamp(
                manager,
                refund_reconciliation_entries::Column::CreatedAt,
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_reconciliation_request")
                    .from(
                        refund_reconciliation_entries::Entity,
                        refund_reconciliation_entries::Column::RequestKey,
                    )
                    .to(refund_requests::Entity, refund_requests::Column::RequestKey)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_reconciliation_provider_event")
                    .from(
                        refund_reconciliation_entries::Entity,
                        refund_reconciliation_entries::Column::ProviderEventId,
                    )
                    .to(refund_provider_events::Entity, refund_provider_events::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_reconciliation_user")
                    .from(
                        refund_reconciliation_entries::Entity,
                        refund_reconciliation_entries::Column::UserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_refund_reconciliation_approval_actor")
                    .from(
                        refund_reconciliation_entries::Entity,
                        refund_reconciliation_entries::Column::ApprovalActorId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .check(
                Expr::col(refund_reconciliation_entries::Column::OrderKind)
                    .between(1_i16, 2_i16),
            )
            .check(
                Expr::col(refund_reconciliation_entries::Column::AmountDeltaMinor)
                    .lt(0_i64),
            )
            .check(
                Expr::col(refund_reconciliation_entries::Column::AmountDeltaMinor)
                    .ne(i64::MIN),
            )
            // 订阅订单当前只有个人资金主体；企业作用域只能来自企业充值订单。
            .check(
                Expr::col(refund_reconciliation_entries::Column::OrderKind)
                    .eq(1_i16)
                    .or(Expr::col(refund_reconciliation_entries::Column::OrganizationId).is_null()),
            );
        manager.create_table(statement).await?;

        for index in [
            Index::create()
                .name("uq_refund_reconciliation_request")
                .table(refund_reconciliation_entries::Entity)
                .col(refund_reconciliation_entries::Column::RequestKey)
                .unique()
                .to_owned(),
            Index::create()
                .name("uq_refund_reconciliation_provider_event")
                .table(refund_reconciliation_entries::Entity)
                .col(refund_reconciliation_entries::Column::ProviderEventId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_refund_reconciliation_user_id")
                .table(refund_reconciliation_entries::Entity)
                .col(refund_reconciliation_entries::Column::UserId)
                .col(refund_reconciliation_entries::Column::Id)
                .to_owned(),
            Index::create()
                .name("idx_refund_reconciliation_organization_id")
                .table(refund_reconciliation_entries::Entity)
                .col(refund_reconciliation_entries::Column::OrganizationId)
                .col(refund_reconciliation_entries::Column::Id)
                .to_owned(),
            Index::create()
                .name("idx_refund_reconciliation_approval_actor_id")
                .table(refund_reconciliation_entries::Entity)
                .col(refund_reconciliation_entries::Column::ApprovalActorId)
                .col(refund_reconciliation_entries::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        create_append_only_guards(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_append_only_guards(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(refund_reconciliation_entries::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn create_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => {
            connection
                .execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_update BEFORE UPDATE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END")
                .await?;
            connection
                .execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_delete BEFORE DELETE ON refund_reconciliation_entries BEGIN SELECT RAISE(ABORT, 'refund reconciliations are append only'); END")
                .await
                .map(|_| ())
        }
        DbBackend::Postgres => {
            connection.execute_unprepared("CREATE FUNCTION reject_refund_reconciliation_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refund reconciliations are append only'; RETURN OLD; END $$").await?;
            connection.execute_unprepared("CREATE FUNCTION reject_refund_reconciliation_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'refund reconciliations are append only'; RETURN OLD; END $$").await?;
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_update BEFORE UPDATE ON refund_reconciliation_entries FOR EACH ROW EXECUTE FUNCTION reject_refund_reconciliation_update()") .await?;
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_delete BEFORE DELETE ON refund_reconciliation_entries FOR EACH ROW EXECUTE FUNCTION reject_refund_reconciliation_delete()") .await.map(|_| ())
        }
        DbBackend::MySql => {
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_update BEFORE UPDATE ON refund_reconciliation_entries FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'refund reconciliations are append only'").await?;
            connection.execute_unprepared("CREATE TRIGGER trg_refund_reconciliation_append_only_delete BEFORE DELETE ON refund_reconciliation_entries FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'refund reconciliations are append only'").await.map(|_| ())
        }
    }
}

async fn drop_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::MySql => {
            connection
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {UPDATE_TRIGGER}"))
                .await?;
            connection
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {DELETE_TRIGGER}"))
                .await
                .map(|_| ())
        }
        DbBackend::Postgres => {
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {UPDATE_TRIGGER} ON refund_reconciliation_entries"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP TRIGGER IF EXISTS {DELETE_TRIGGER} ON refund_reconciliation_entries"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_UPDATE_FUNCTION}()"
                ))
                .await?;
            connection
                .execute_unprepared(&format!(
                    "DROP FUNCTION IF EXISTS {POSTGRES_DELETE_FUNCTION}()"
                ))
                .await
                .map(|_| ())
        }
    }
}
