use sea_orm_migration::prelude::*;

use super::schema::{table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut overrides = table(manager, Alias::new("group_model_ratio_overrides"));
        overrides
            .col(
                ColumnDef::new(Alias::new("source_group_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("target_group_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("model"))
                    .string_len(256)
                    .not_null()
                    .check(Expr::col(Alias::new("model")).ne("")),
            )
            .col(
                ColumnDef::new(Alias::new("ratio_micros"))
                    .big_integer()
                    .not_null()
                    .check(Expr::col(Alias::new("ratio_micros")).gte(0_i64)),
            )
            .col(timestamp(manager, Alias::new("created_at")))
            .col(timestamp(manager, Alias::new("updated_at")))
            .primary_key(
                Index::create()
                    .name("pk_group_model_ratio_overrides")
                    .col(Alias::new("source_group_id"))
                    .col(Alias::new("target_group_id"))
                    .col(Alias::new("model")),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_model_ratio_overrides_source")
                    .from(
                        Alias::new("group_model_ratio_overrides"),
                        Alias::new("source_group_id"),
                    )
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_model_ratio_overrides_target")
                    .from(
                        Alias::new("group_model_ratio_overrides"),
                        Alias::new("target_group_id"),
                    )
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            );
        manager.create_table(overrides.to_owned()).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_group_model_ratio_overrides_target_source_model")
                    .table(Alias::new("group_model_ratio_overrides"))
                    .col(Alias::new("target_group_id"))
                    .col(Alias::new("source_group_id"))
                    .col(Alias::new("model"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("group_model_ratio_overrides"))
                    .to_owned(),
            )
            .await
    }
}
