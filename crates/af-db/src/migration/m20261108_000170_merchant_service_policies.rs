use super::schema::{auto_id, table, timestamp};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("merchant_service_prices"))
                    .col(auto_id(Alias::new("id")))
                    .col(
                        ColumnDef::new(Alias::new("service_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("revision_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("effective_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("default_ratio_micros"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("models"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("status"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("reason")).string_len(128))
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("status")).is_in([
                        "scheduled",
                        "active",
                        "superseded",
                        "cancelled",
                        "invalidated",
                    ]))
                    .check(
                        Expr::col(Alias::new("default_ratio_micros"))
                            .between(1_i64, 100_000_000_i64),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_merchant_price_service")
                            .from(
                                Alias::new("merchant_service_prices"),
                                Alias::new("service_id"),
                            )
                            .to(Alias::new("merchant_services"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_merchant_price_revision")
                            .from(
                                Alias::new("merchant_service_prices"),
                                Alias::new("revision_id"),
                            )
                            .to(Alias::new("merchant_service_revisions"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_price_due")
                    .table(Alias::new("merchant_service_prices"))
                    .col(Alias::new("status"))
                    .col(Alias::new("effective_at"))
                    .col(Alias::new("service_id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_price_history")
                    .table(Alias::new("merchant_service_prices"))
                    .col(Alias::new("service_id"))
                    .col(Alias::new("revision_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager.get_connection().execute_unprepared("INSERT INTO merchant_service_prices (service_id,revision_id,effective_at,default_ratio_micros,models,status,created_at,updated_at) SELECT s.id,r.id,0,r.default_ratio_micros,r.models,'active',r.created_at,r.updated_at FROM merchant_services s INNER JOIN merchant_service_revisions r ON r.id=s.published_revision_id WHERE r.status='published'").await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_maintenance"))
                    .col(auto_id(Alias::new("id")))
                    .col(
                        ColumnDef::new(Alias::new("service_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("starts_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("ends_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("message"))
                            .string_len(2048)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("blocks_requests"))
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("cancelled"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("starts_at")).gte(0_i64))
                    .check(Expr::col(Alias::new("ends_at")).gt(Expr::col(Alias::new("starts_at"))))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_merchant_maintenance_service")
                            .from(Alias::new("merchant_maintenance"), Alias::new("service_id"))
                            .to(Alias::new("merchant_services"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_maintenance_window")
                    .table(Alias::new("merchant_maintenance"))
                    .col(Alias::new("service_id"))
                    .col(Alias::new("cancelled"))
                    .col(Alias::new("ends_at"))
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Preserve consumer pricing safety if policy history is removed on downgrade.
        manager.get_connection().execute_unprepared("UPDATE channels SET status=2 WHERE id IN (SELECT channel_id FROM merchant_service_revisions)").await?;
        manager.get_connection().execute_unprepared("DELETE FROM abilities WHERE channel_id IN (SELECT channel_id FROM merchant_service_revisions)").await?;
        for name in ["merchant_maintenance", "merchant_service_prices"] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
