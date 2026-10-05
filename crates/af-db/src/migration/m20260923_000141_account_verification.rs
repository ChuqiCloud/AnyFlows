use super::schema::{auto_id, table, timestamp};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("account_verifications"))
                    .col(auto_id(Alias::new("id")))
                    .col(
                        ColumnDef::new(Alias::new("user_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("kind")).string_len(16).not_null())
                    .col(
                        ColumnDef::new(Alias::new("subject_name"))
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("summary"))
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("status"))
                            .small_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("version"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("reviewer_user_id")).big_integer())
                    .col(ColumnDef::new(Alias::new("review_reason")).string_len(512))
                    .col(timestamp(manager, Alias::new("created_at")))
                    .col(timestamp(manager, Alias::new("updated_at")))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_account_verification_user")
                            .from(Alias::new("account_verifications"), Alias::new("user_id"))
                            .to(Alias::new("users"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_account_verification_user_kind")
                    .table(Alias::new("account_verifications"))
                    .col(Alias::new("user_id"))
                    .col(Alias::new("kind"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_account_verification_status")
                    .table(Alias::new("account_verifications"))
                    .col(Alias::new("status"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("account_verification_materials"))
                    .col(auto_id(Alias::new("id")))
                    .col(
                        ColumnDef::new(Alias::new("case_id"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("kind")).string_len(64).not_null())
                    .col(
                        ColumnDef::new(Alias::new("file_name"))
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("content_type"))
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("size_bytes"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("content_bytes"))
                            .blob()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_account_verification_material_case")
                            .from(
                                Alias::new("account_verification_materials"),
                                Alias::new("case_id"),
                            )
                            .to(Alias::new("account_verifications"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_account_verification_material_case")
                    .table(Alias::new("account_verification_materials"))
                    .col(Alias::new("case_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        // MySQL BLOB cannot hold the file sizes accepted by the upload APIs.
        // Widen the public account material storage. Enterprise material tables
        // are owned by the enterprise migration set and are not part of this schema.
        if manager.get_database_backend() == sea_orm::DbBackend::MySql {
            let mut content = ColumnDef::new(Alias::new("content_bytes"));
            content.custom(Alias::new("LONGBLOB")).not_null();
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("account_verification_materials"))
                        .modify_column(content)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Keep the widened legacy columns on rollback; shrinking them could lose files.
        for name in ["account_verification_materials", "account_verifications"] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
