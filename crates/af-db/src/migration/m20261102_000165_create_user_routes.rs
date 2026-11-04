use sea_orm_migration::prelude::*;

use super::schema::{auto_id, nullable_timestamp, table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("user_routes"))
                    .col(auto_id(Alias::new("id")))
                    .col(
                        ColumnDef::new(Alias::new("owner_user_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("name"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("strategy"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("enabled"))
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Alias::new("max_group_attempts"))
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("max_price_ratio_micros")).big_integer())
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .col(nullable_timestamp(manager, Alias::new("deleted_at")))
                    .check(Expr::col(Alias::new("strategy")).is_in([
                        "price_first",
                        "stable_first",
                        "ordered",
                        "round_robin",
                        "weighted",
                    ]))
                    .check(Expr::col(Alias::new("max_group_attempts")).between(1, 16))
                    .check(Expr::col(Alias::new("max_price_ratio_micros")).gte(0_i64))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_routes_owner")
                            .from(Alias::new("user_routes"), Alias::new("owner_user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_user_routes_owner")
                    .table(Alias::new("user_routes"))
                    .col(Alias::new("owner_user_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("user_route_members"))
                    .col(
                        ColumnDef::new(Alias::new("route_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("group_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("priority")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("weight")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("last_selected_at_millis")).big_integer())
                    .primary_key(
                        Index::create()
                            .col(Alias::new("route_id"))
                            .col(Alias::new("group_id")),
                    )
                    .check(Expr::col(Alias::new("priority")).gte(0_i32))
                    .check(Expr::col(Alias::new("weight")).between(1, 10_000))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_route_members_route")
                            .from(Alias::new("user_route_members"), Alias::new("route_id"))
                            .to(Alias::new("user_routes"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_route_members_group")
                            .from(Alias::new("user_route_members"), Alias::new("group_id"))
                            .to(Alias::new("groups"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("user_route_health"))
                    .col(
                        ColumnDef::new(Alias::new("route_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("group_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("model"))
                            .string_len(256)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("success_count"))
                            .big_integer()
                            .not_null()
                            .default(0_i64),
                    )
                    .col(
                        ColumnDef::new(Alias::new("fail_count"))
                            .big_integer()
                            .not_null()
                            .default(0_i64),
                    )
                    .col(
                        ColumnDef::new(Alias::new("total_latency_ms"))
                            .big_integer()
                            .not_null()
                            .default(0_i64),
                    )
                    .col(
                        ColumnDef::new(Alias::new("consecutive_failures"))
                            .integer()
                            .not_null()
                            .default(0_i32),
                    )
                    .col(ColumnDef::new(Alias::new("cooldown_until_millis")).big_integer())
                    .col(
                        ColumnDef::new(Alias::new("window_started_at_millis"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at_millis"))
                            .big_integer()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(Alias::new("route_id"))
                            .col(Alias::new("group_id"))
                            .col(Alias::new("model")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_route_health_member")
                            .from(
                                Alias::new("user_route_health"),
                                (Alias::new("route_id"), Alias::new("group_id")),
                            )
                            .to(
                                Alias::new("user_route_members"),
                                (Alias::new("route_id"), Alias::new("group_id")),
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // 保留墓碑关联，停用或删除路由后不得把已有 Key 退回默认组。
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("tokens"))
                    .add_column(ColumnDef::new(Alias::new("user_route_id")).big_integer())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_tokens_user_route")
                    .table(Alias::new("tokens"))
                    .col(Alias::new("user_route_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_tokens_user_route")
                    .table(Alias::new("tokens"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("tokens"))
                    .drop_column(Alias::new("user_route_id"))
                    .to_owned(),
            )
            .await?;
        for name in ["user_route_health", "user_route_members", "user_routes"] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
