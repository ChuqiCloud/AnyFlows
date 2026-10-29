use sea_orm_migration::prelude::*;

use super::schema::table;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut policies = table(manager, Alias::new("group_model_policies"));
        policies
            .col(
                ColumnDef::new(Alias::new("group_id"))
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Alias::new("allowed_models"))
                    .json_binary()
                    .not_null(),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_group_model_policy_group")
                    .from(Alias::new("group_model_policies"), Alias::new("group_id"))
                    .to(Alias::new("groups"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            );
        manager.create_table(policies).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("group_model_policies"))
                    .to_owned(),
            )
            .await
    }
}
