use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ARTIFACTS: &str = "plugin_artifacts";
const APPROVALS: &str = "plugin_artifact_approvals";
const REVOCATIONS: &str = "plugin_artifact_revocations";
const DEPLOYMENTS: &str = "plugin_deployments";
const REVISIONS: &str = "plugin_deployment_revisions";
const IMMUTABLE_TABLES: [&str; 3] = [ARTIFACTS, APPROVALS, REVISIONS];
const IMMUTABLE_ACTIONS: [&str; 2] = ["update", "delete"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new(ARTIFACTS))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("plugin_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("plugin_version"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("digest"))
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("manifest"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("source_kind"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("source_reference"))
                            .string_len(2048)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("signer_key_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("signature"))
                            .string_len(256)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("created_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, columns) in [
            (
                "uq_plugin_artifacts_plugin_digest",
                vec!["plugin_id", "digest"],
            ),
            (
                "uq_plugin_artifacts_plugin_version",
                vec!["plugin_id", "plugin_version"],
            ),
        ] {
            let mut index = Index::create();
            index.name(name).table(Alias::new(ARTIFACTS)).unique();
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.to_owned()).await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(Alias::new(APPROVALS))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("artifact_id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("approved_by"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("approved_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("request_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_artifact_approvals_artifact")
                            .from(Alias::new(APPROVALS), Alias::new("artifact_id"))
                            .to(Alias::new(ARTIFACTS), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_artifact_approvals_operator")
                            .from(Alias::new(APPROVALS), Alias::new("approved_by"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_plugin_artifact_approvals_request")
                    .table(Alias::new(APPROVALS))
                    .col(Alias::new("request_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new(REVOCATIONS))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("artifact_id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("revoked_by"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("revoked_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("request_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("reason"))
                            .string_len(512)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_artifact_revocations_artifact")
                            .from(Alias::new(REVOCATIONS), Alias::new("artifact_id"))
                            .to(Alias::new(ARTIFACTS), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_artifact_revocations_operator")
                            .from(Alias::new(REVOCATIONS), Alias::new("revoked_by"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_plugin_artifact_revocations_request")
                    .table(Alias::new(REVOCATIONS))
                    .col(Alias::new("request_id"))
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new(DEPLOYMENTS))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("plugin_id"))
                            .string_len(128)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("generation"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("artifact_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("enabled")).boolean().not_null())
                    .col(
                        ColumnDef::new(Alias::new("capability_grants"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("config"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_by"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("request_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_deployments_artifact")
                            .from(Alias::new(DEPLOYMENTS), Alias::new("artifact_id"))
                            .to(Alias::new(ARTIFACTS), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_deployments_operator")
                            .from(Alias::new(DEPLOYMENTS), Alias::new("updated_by"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Alias::new(REVISIONS))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("plugin_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("generation"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("artifact_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("enabled")).boolean().not_null())
                    .col(
                        ColumnDef::new(Alias::new("capability_grants"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("config"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_by"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("request_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_deployment_revisions_artifact")
                            .from(Alias::new(REVISIONS), Alias::new("artifact_id"))
                            .to(Alias::new(ARTIFACTS), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_plugin_deployment_revisions_operator")
                            .from(Alias::new(REVISIONS), Alias::new("updated_by"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, columns) in [
            (
                "uq_plugin_deployment_revision_generation",
                vec!["plugin_id", "generation"],
            ),
            ("uq_plugin_deployment_revision_request", vec!["request_id"]),
            (
                "idx_plugin_deployment_revisions_plugin",
                vec!["plugin_id", "id"],
            ),
        ] {
            let mut index = Index::create();
            index.name(name).table(Alias::new(REVISIONS));
            if name.starts_with("uq_") {
                index.unique();
            }
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.to_owned()).await?;
        }
        create_append_only_guards(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_append_only_guards(manager).await?;
        manager
            .drop_table(Table::drop().table(Alias::new(REVISIONS)).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new(DEPLOYMENTS)).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new(REVOCATIONS)).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new(APPROVALS)).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new(ARTIFACTS)).to_owned())
            .await
    }
}

async fn create_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    match manager.get_database_backend() {
        DbBackend::Sqlite => {
            for table in IMMUTABLE_TABLES {
                for action in IMMUTABLE_ACTIONS {
                    let trigger = format!("trg_{table}_append_only_{action}");
                    connection
                        .execute_unprepared(&format!(
                            "CREATE TRIGGER {trigger} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT, 'plugin catalog records are append only'); END"
                        ))
                        .await?;
                }
            }
            for action in IMMUTABLE_ACTIONS {
                let trigger = format!("trg_{REVOCATIONS}_append_only_{action}");
                connection
                    .execute_unprepared(&format!(
                        "CREATE TRIGGER {trigger} BEFORE {action} ON {REVOCATIONS} BEGIN SELECT RAISE(ABORT, 'plugin catalog revocations are append only'); END"
                    ))
                    .await?;
            }
            Ok(())
        }
        DbBackend::Postgres => {
            connection
                .execute_unprepared(
                    "CREATE FUNCTION reject_plugin_catalog_mutation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'plugin catalog records are append only'; RETURN OLD; END $$",
                )
                .await?;
            for table in IMMUTABLE_TABLES.into_iter().chain([REVOCATIONS]) {
                for action in IMMUTABLE_ACTIONS {
                    let trigger = format!("trg_{table}_append_only_{action}");
                    connection
                        .execute_unprepared(&format!(
                            "CREATE TRIGGER {trigger} BEFORE {action} ON {table} FOR EACH ROW EXECUTE FUNCTION reject_plugin_catalog_mutation()"
                        ))
                        .await?;
                }
            }
            Ok(())
        }
        DbBackend::MySql => {
            for table in IMMUTABLE_TABLES.into_iter().chain([REVOCATIONS]) {
                for action in IMMUTABLE_ACTIONS {
                    let trigger = format!("trg_{table}_append_only_{action}");
                    connection
                        .execute_unprepared(&format!(
                            "CREATE TRIGGER {trigger} BEFORE {action} ON {table} FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'plugin catalog records are append only'"
                        ))
                        .await?;
                }
            }
            Ok(())
        }
    }
}

async fn drop_append_only_guards(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    for table in IMMUTABLE_TABLES.into_iter().chain([REVOCATIONS]) {
        for action in IMMUTABLE_ACTIONS {
            let trigger = format!("trg_{table}_append_only_{action}");
            if manager.get_database_backend() == DbBackend::Postgres {
                connection
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger} ON {table}"))
                    .await?;
            } else {
                connection
                    .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger}"))
                    .await?;
            }
        }
    }
    if manager.get_database_backend() == DbBackend::Postgres {
        connection
            .execute_unprepared("DROP FUNCTION IF EXISTS reject_plugin_catalog_mutation()")
            .await?;
    }
    Ok(())
}
