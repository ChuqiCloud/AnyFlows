use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("plugin_event_deliveries"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("plugin_id")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("event_key")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("event_kind")).string_len(64).not_null())
                    .col(ColumnDef::new(Alias::new("artifact_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("artifact_digest")).string_len(64).not_null())
                    .col(ColumnDef::new(Alias::new("deployment_generation")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("capability_id")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("capability_version")).integer().not_null())
                    // 32 KiB is below MySQL's smallest TEXT ceiling and is checked before insert.
                    .col(ColumnDef::new(Alias::new("payload_json")).text().not_null())
                    .col(ColumnDef::new(Alias::new("status")).small_integer().not_null())
                    .col(ColumnDef::new(Alias::new("attempt_count")).small_integer().not_null())
                    .col(ColumnDef::new(Alias::new("max_attempts")).small_integer().not_null())
                    .col(ColumnDef::new(Alias::new("next_attempt_at")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("lease_expires_at")).big_integer().null())
                    .col(ColumnDef::new(Alias::new("completed_at")).big_integer().null())
                    .col(ColumnDef::new(Alias::new("expires_at")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("version")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("created_at")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("updated_at")).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_event_deliveries_artifact")
                            .from(Alias::new("plugin_event_deliveries"), Alias::new("artifact_id"))
                            .to(Alias::new("plugin_artifacts"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_plugin_event_deliveries_idempotency")
                    .table(Alias::new("plugin_event_deliveries"))
                    .col(Alias::new("plugin_id"))
                    .col(Alias::new("event_key"))
                    .col(Alias::new("deployment_generation"))
                    .col(Alias::new("capability_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_plugin_event_deliveries_due")
                    .table(Alias::new("plugin_event_deliveries"))
                    .col(Alias::new("status"))
                    .col(Alias::new("next_attempt_at"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("plugin_event_deliveries"))
                    .to_owned(),
            )
            .await
    }
}
