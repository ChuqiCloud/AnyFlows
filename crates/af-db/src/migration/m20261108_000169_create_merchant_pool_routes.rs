use super::schema::table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("merchant_pool_routes"))
                    .col(
                        ColumnDef::new(Alias::new("route_id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("pool_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("members"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("enrolled_merchants"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("excluded_merchant_ids"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("allow_pool_fallback"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Alias::new("accept_new_merchants"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at_millis"))
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_merchant_pool_routes_route")
                            .from(Alias::new("merchant_pool_routes"), Alias::new("route_id"))
                            .to(Alias::new("user_routes"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_merchant_pool_routes_pool")
                            .from(Alias::new("merchant_pool_routes"), Alias::new("pool_id"))
                            .to(Alias::new("merchant_pools"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_pool_attempts"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .string_len(36)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("pool_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("route_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("request_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("merchant_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("service_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("model"))
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("protocol"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("streaming")).boolean().not_null())
                    .col(
                        ColumnDef::new(Alias::new("outcome"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("latency_ms"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("ttft_ms")).big_integer())
                    .col(
                        ColumnDef::new(Alias::new("completed_at_millis"))
                            .big_integer()
                            .not_null(),
                    )
                    .check(
                        Expr::col(Alias::new("outcome")).is_in(["success", "failure", "ignored"]),
                    )
                    .check(Expr::col(Alias::new("latency_ms")).gte(0_i64))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_pool_attempts_window")
                    .table(Alias::new("merchant_pool_attempts"))
                    .col(Alias::new("pool_id"))
                    .col(Alias::new("completed_at_millis"))
                    .col(Alias::new("model"))
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in ["merchant_pool_attempts", "merchant_pool_routes"] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
