use super::schema::{auto_id, table, timestamp};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

fn text(name: &str, length: u32) -> ColumnDef {
    let mut column = ColumnDef::new(Alias::new(name));
    column.string_len(length).not_null();
    column
}
fn number(name: &str) -> ColumnDef {
    let mut column = ColumnDef::new(Alias::new(name));
    column.big_integer().not_null();
    column
}
fn foreign(
    from_table: &str,
    column: &str,
    to_table: &str,
    name: &str,
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(Alias::new(from_table), Alias::new(column))
        .to(Alias::new(to_table), Alias::new("id"))
        .on_delete(ForeignKeyAction::Restrict)
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("merchant_profiles"))
                    .col(auto_id(Alias::new("id")))
                    .col(number("owner_user_id"))
                    .col(text("name", 128))
                    .col(text("description", 2048))
                    .col(text("status", 32))
                    .col(ColumnDef::new(Alias::new("review_reason")).string_len(2048))
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("status")).is_in([
                        "pending",
                        "approved",
                        "rejected",
                        "suspended",
                    ]))
                    .foreign_key(&mut foreign(
                        "merchant_profiles",
                        "owner_user_id",
                        "users",
                        "fk_merchant_owner",
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_owner_unique")
                    .table(Alias::new("merchant_profiles"))
                    .col(Alias::new("owner_user_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_pools"))
                    .col(auto_id(Alias::new("id")))
                    .col(number("group_id"))
                    .col(text("name", 128))
                    .col(text("description", 2048))
                    .col(
                        ColumnDef::new(Alias::new("enabled"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Alias::new("commission_bps"))
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("commission_inherit_global"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(number("min_ratio_micros"))
                    .col(number("max_ratio_micros"))
                    .col(number("settlement_delay_secs"))
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("commission_bps")).between(0, 10_000))
                    .check(Expr::col(Alias::new("min_ratio_micros")).gt(0_i64))
                    .check(
                        Expr::col(Alias::new("max_ratio_micros")).between(1_i64, 100_000_000_i64),
                    )
                    .check(
                        Expr::col(Alias::new("settlement_delay_secs"))
                            .between(0_i64, 7_776_000_i64),
                    )
                    .foreign_key(&mut foreign(
                        "merchant_pools",
                        "group_id",
                        "groups",
                        "fk_merchant_pool_group",
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_pool_group_unique")
                    .table(Alias::new("merchant_pools"))
                    .col(Alias::new("group_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_services"))
                    .col(auto_id(Alias::new("id")))
                    .col(number("merchant_id"))
                    .col(number("pool_id"))
                    .col(text("name", 128))
                    .col(text("description", 2048))
                    .col(text("status", 32))
                    .col(ColumnDef::new(Alias::new("published_revision_id")).big_integer())
                    .col(
                        ColumnDef::new(Alias::new("merchant_paused"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Alias::new("platform_suspended"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Alias::new("pending_revision_id")).big_integer())
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("status")).is_in([
                        "draft",
                        "pending",
                        "published",
                        "paused",
                        "suspended",
                        "rejected",
                    ]))
                    .foreign_key(&mut foreign(
                        "merchant_services",
                        "merchant_id",
                        "merchant_profiles",
                        "fk_merchant_service_owner",
                    ))
                    .foreign_key(&mut foreign(
                        "merchant_services",
                        "pool_id",
                        "merchant_pools",
                        "fk_merchant_service_pool",
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_services_pool_state")
                    .table(Alias::new("merchant_services"))
                    .col(Alias::new("pool_id"))
                    .col(Alias::new("status"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_service_revisions"))
                    .col(auto_id(Alias::new("id")))
                    .col(number("service_id"))
                    .col(number("revision"))
                    .col(text("status", 32))
                    .col(text("name", 128))
                    .col(text("description", 2048))
                    .col(text("base_url", 2048))
                    .col(text("protocol", 32))
                    .col(number("default_ratio_micros"))
                    .col(
                        ColumnDef::new(Alias::new("models"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(number("channel_id"))
                    .col(number("credential_id"))
                    .col(ColumnDef::new(Alias::new("concurrency")).integer())
                    .col(
                        ColumnDef::new(Alias::new("timeout_secs"))
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("review_reason")).string_len(2048))
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("status")).is_in([
                        "draft",
                        "pending",
                        "published",
                        "rejected",
                        "superseded",
                    ]))
                    .check(Expr::col(Alias::new("revision")).gt(0_i64))
                    .check(
                        Expr::col(Alias::new("default_ratio_micros"))
                            .between(1_i64, 100_000_000_i64),
                    )
                    .check(Expr::col(Alias::new("timeout_secs")).between(1, 300))
                    .foreign_key(&mut foreign(
                        "merchant_service_revisions",
                        "service_id",
                        "merchant_services",
                        "fk_merchant_revision_service",
                    ))
                    .foreign_key(&mut foreign(
                        "merchant_service_revisions",
                        "channel_id",
                        "channels",
                        "fk_merchant_revision_channel",
                    ))
                    .foreign_key(&mut foreign(
                        "merchant_service_revisions",
                        "credential_id",
                        "credentials",
                        "fk_merchant_revision_credential",
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_revision_unique")
                    .table(Alias::new("merchant_service_revisions"))
                    .col(Alias::new("service_id"))
                    .col(Alias::new("revision"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_revision_channel_unique")
                    .table(Alias::new("merchant_service_revisions"))
                    .col(Alias::new("channel_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_pool_settings"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("commission_bps"))
                            .integer()
                            .not_null(),
                    )
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .check(Expr::col(Alias::new("id")).eq(1_i64))
                    .check(Expr::col(Alias::new("commission_bps")).between(0, 10_000))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_pool_terms"))
                    .col(number("pool_id"))
                    .col(number("merchant_id"))
                    .col(ColumnDef::new(Alias::new("commission_bps")).integer())
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .primary_key(
                        Index::create()
                            .col(Alias::new("pool_id"))
                            .col(Alias::new("merchant_id")),
                    )
                    .check(Expr::col(Alias::new("commission_bps")).between(0, 10_000))
                    .foreign_key(&mut foreign(
                        "merchant_pool_terms",
                        "pool_id",
                        "merchant_pools",
                        "fk_merchant_terms_pool",
                    ))
                    .foreign_key(&mut foreign(
                        "merchant_pool_terms",
                        "merchant_id",
                        "merchant_profiles",
                        "fk_merchant_terms_merchant",
                    ))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Downgrade must not turn approved merchant channels into ordinary unaccounted supply.
        manager.get_connection().execute_unprepared("UPDATE channels SET status = 2 WHERE id IN (SELECT channel_id FROM merchant_service_revisions)").await?;
        manager.get_connection().execute_unprepared("DELETE FROM abilities WHERE channel_id IN (SELECT channel_id FROM merchant_service_revisions)").await?;
        for name in [
            "merchant_pool_terms",
            "merchant_pool_settings",
            "merchant_service_revisions",
            "merchant_services",
            "merchant_pools",
            "merchant_profiles",
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
