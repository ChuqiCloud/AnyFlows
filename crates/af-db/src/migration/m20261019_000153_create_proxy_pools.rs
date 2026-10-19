use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("proxy_pools"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("settings_secret"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("subscribed"))
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("interval_secs"))
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("revision"))
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(Alias::new("last_refresh_at")).big_integer())
                    .col(ColumnDef::new(Alias::new("last_check_at")).big_integer())
                    .col(
                        ColumnDef::new(Alias::new("status"))
                            .string_len(32)
                            .not_null()
                            .default("pending"),
                    )
                    .col(
                        ColumnDef::new(Alias::new("work_until"))
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_proxy_pools_proxy")
                            .from(Alias::new("proxy_pools"), Alias::new("id"))
                            .to(Alias::new("proxies"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("proxy_pool_nodes"))
                    .col(
                        ColumnDef::new(Alias::new("pool_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("proxy_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("fingerprint"))
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("remark"))
                            .string_len(128)
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Alias::new("present"))
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Alias::new("healthy"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Alias::new("latency_ms")).big_integer())
                    .col(ColumnDef::new(Alias::new("checked_at")).big_integer())
                    .primary_key(
                        Index::create()
                            .col(Alias::new("pool_id"))
                            .col(Alias::new("proxy_id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_proxy_pool_nodes_pool")
                            .from(Alias::new("proxy_pool_nodes"), Alias::new("pool_id"))
                            .to(Alias::new("proxy_pools"), Alias::new("id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_proxy_pool_nodes_proxy")
                            .from(Alias::new("proxy_pool_nodes"), Alias::new("proxy_id"))
                            .to(Alias::new("proxies"), Alias::new("id")),
                    )
                    .to_owned(),
            )
            .await?;
        manager.create_table(Table::create().table(Alias::new("proxy_pool_bindings"))
            // 0 为全局绑定，其余值为渠道 ID。
            .col(ColumnDef::new(Alias::new("channel_id")).big_integer().not_null().primary_key())
            .col(ColumnDef::new(Alias::new("pool_id")).big_integer().not_null())
            .foreign_key(ForeignKey::create().name("fk_proxy_pool_bindings_pool").from(Alias::new("proxy_pool_bindings"), Alias::new("pool_id")).to(Alias::new("proxy_pools"), Alias::new("id")))
            .to_owned()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let used = Query::select()
            .column(Alias::new("id"))
            .from(Alias::new("proxy_pools"))
            .limit(1)
            .to_owned();
        if manager
            .get_connection()
            .query_one(manager.get_database_backend().build(&used))
            .await?
            .is_some()
        {
            return Err(DbErr::Custom("代理池仍存在，不能回退代理池迁移".to_owned()));
        }
        for table in ["proxy_pool_bindings", "proxy_pool_nodes", "proxy_pools"] {
            manager
                .drop_table(Table::drop().table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}
