use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("external_account_connections"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("user_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("connector_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("issuer_hash"))
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("external_subject"))
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("base_url"))
                            .string_len(2048)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("display_name"))
                            .string_len(256)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("secret_key_id"))
                            .string_len(128)
                            .null(),
                    )
                    .col(ColumnDef::new(Alias::new("secret_nonce")).binary().null())
                    .col(
                        ColumnDef::new(Alias::new("secret_ciphertext"))
                            .blob()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("active"))
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Alias::new("revoked_at"))
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("created_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("updated_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_external_account_connections_user")
                            .from(
                                Alias::new("external_account_connections"),
                                Alias::new("user_id"),
                            )
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        for (name, columns) in [
            (
                "idx_external_account_connections_user_connector",
                vec!["user_id", "connector_id"],
            ),
            (
                "idx_external_account_connections_external_subject",
                vec!["connector_id", "external_subject"],
            ),
            (
                "idx_external_account_connections_user_created",
                vec!["user_id", "created_at", "id"],
            ),
        ] {
            let mut index = Index::create();
            index
                .name(name)
                .table(Alias::new("external_account_connections"));
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.to_owned()).await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(Alias::new("external_account_subject_claims"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("connector_id"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("issuer_hash"))
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("external_subject"))
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("connection_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_external_account_subject_claims_connection")
                            .from(
                                Alias::new("external_account_subject_claims"),
                                Alias::new("connection_id"),
                            )
                            .to(Alias::new("external_account_connections"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, columns) in [
            (
                "uq_external_account_subject_claims_subject",
                vec!["connector_id", "issuer_hash", "external_subject"],
            ),
            (
                "uq_external_account_subject_claims_connection",
                vec!["connection_id"],
            ),
        ] {
            let mut index = Index::create();
            index
                .name(name)
                .table(Alias::new("external_account_subject_claims"))
                .unique();
            for column in columns {
                index.col(Alias::new(column));
            }
            manager.create_index(index.to_owned()).await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(Alias::new("external_account_balance_snapshots"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("connection_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("amount")).string_len(128).null())
                    .col(ColumnDef::new(Alias::new("unit")).string_len(64).null())
                    .col(
                        ColumnDef::new(Alias::new("status"))
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("observed_at"))
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_external_account_balance_snapshots_connection")
                            .from(
                                Alias::new("external_account_balance_snapshots"),
                                Alias::new("connection_id"),
                            )
                            .to(Alias::new("external_account_connections"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_external_account_balance_snapshots_connection_cursor")
                    .table(Alias::new("external_account_balance_snapshots"))
                    .col(Alias::new("connection_id"))
                    .col(Alias::new("observed_at"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("external_account_balance_snapshots"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("external_account_subject_claims"))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("external_account_connections"))
                    .to_owned(),
            )
            .await
    }
}
