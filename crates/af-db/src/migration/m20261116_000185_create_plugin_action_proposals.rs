use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("channel_monitor_targets"))
                    .add_column(
                        ColumnDef::new(Alias::new("config_version"))
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new("plugin_action_proposals"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("proposal_key")).string_len(36).not_null())
                    .col(ColumnDef::new(Alias::new("idempotency_key")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("requested_by_user_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("confirmed_by_user_id")).big_integer().null())
                    .col(ColumnDef::new(Alias::new("plugin_id")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("artifact_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("artifact_digest")).string_len(64).not_null())
                    .col(ColumnDef::new(Alias::new("deployment_generation")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("capability_id")).string_len(128).not_null())
                    .col(ColumnDef::new(Alias::new("capability_version")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("action_id")).string_len(96).not_null())
                    .col(ColumnDef::new(Alias::new("action_version")).integer().not_null())
                    // The only registered action input is a bounded JSON object without credentials.
                    .col(ColumnDef::new(Alias::new("input_json")).text().not_null())
                    .col(ColumnDef::new(Alias::new("target_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("expected_target_version")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("expected_enabled")).boolean().not_null())
                    .col(ColumnDef::new(Alias::new("requested_enabled")).boolean().not_null())
                    .col(ColumnDef::new(Alias::new("status")).small_integer().not_null())
                    .col(ColumnDef::new(Alias::new("expires_at")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("created_at")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("confirmed_at")).big_integer().null())
                    .col(ColumnDef::new(Alias::new("confirmation_key")).string_len(128).null())
                    .col(ColumnDef::new(Alias::new("outcome_json")).text().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_action_proposals_requester")
                            .from(Alias::new("plugin_action_proposals"), Alias::new("requested_by_user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_action_proposals_confirmer")
                            .from(Alias::new("plugin_action_proposals"), Alias::new("confirmed_by_user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, columns) in [
            ("uq_plugin_action_proposals_key", vec!["proposal_key"]),
            (
                "uq_plugin_action_proposals_idempotency",
                vec!["requested_by_user_id", "idempotency_key"],
            ),
            (
                "idx_plugin_action_proposals_expiry",
                vec!["status", "expires_at", "id"],
            ),
        ] {
            let mut index = Index::create();
            index
                .name(name)
                .table(Alias::new("plugin_action_proposals"));
            for column in columns {
                index.col(Alias::new(column));
            }
            if name.starts_with("uq_") {
                index.unique();
            }
            manager.create_index(index.to_owned()).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("plugin_action_proposals"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("channel_monitor_targets"))
                    .drop_column(Alias::new("config_version"))
                    .to_owned(),
            )
            .await
    }
}
