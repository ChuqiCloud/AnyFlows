use sea_orm_migration::prelude::*;

use super::schema::table;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut policies = table(manager, Alias::new("group_key_policies"));
        policies
            .col(
                ColumnDef::new(Alias::new("group_id"))
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Alias::new("selection_mode"))
                    .string_len(32)
                    .not_null(),
            )
            .check(Expr::col(Alias::new("selection_mode")).is_in([
                "default_only",
                "all_users",
                "selected_users",
            ]))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_key_policy_group")
                    .from(Alias::new("group_key_policies"), Alias::new("group_id"))
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            );
        manager.create_table(policies).await?;
        let mut grants = table(manager, Alias::new("user_group_grants"));
        grants
            .col(
                ColumnDef::new(Alias::new("group_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("user_id"))
                    .big_integer()
                    .not_null(),
            )
            .primary_key(
                Index::create()
                    .col(Alias::new("group_id"))
                    .col(Alias::new("user_id")),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_user_group_grant_group")
                    .from(Alias::new("user_group_grants"), Alias::new("group_id"))
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_user_group_grant_user")
                    .from(Alias::new("user_group_grants"), Alias::new("user_id"))
                    .to(Alias::new("users"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            );
        manager.create_table(grants).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_user_group_grants_user_group")
                    .table(Alias::new("user_group_grants"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("group_id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("user_group_grants"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("group_key_policies"))
                    .to_owned(),
            )
            .await
    }
}
