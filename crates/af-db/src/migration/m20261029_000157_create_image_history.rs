use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::schema::{table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[derive(DeriveIden)]
enum ImageHistory {
    #[sea_orm(iden = "image_history")]
    Table,
    ImageId,
    OwnerUserId,
    Model,
    Prompt,
    RevisedPrompt,
    RequestId,
    Format,
    ByteSize,
    ThumbnailFormat,
    ThumbnailByteSize,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut images = table(manager, ImageHistory::Table);
        images
            .col(
                ColumnDef::new(ImageHistory::ImageId)
                    .char_len(32)
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(ImageHistory::OwnerUserId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(ImageHistory::Model)
                    .string_len(256)
                    .not_null(),
            )
            .col(ColumnDef::new(ImageHistory::Prompt).text().not_null())
            .col(ColumnDef::new(ImageHistory::RevisedPrompt).text())
            .col(ColumnDef::new(ImageHistory::RequestId).string_len(128))
            .col(
                ColumnDef::new(ImageHistory::Format)
                    .string_len(8)
                    .not_null(),
            )
            .col(
                ColumnDef::new(ImageHistory::ByteSize)
                    .big_integer()
                    .not_null()
                    .check(Expr::col(ImageHistory::ByteSize).between(1_i64, 52_428_800_i64)),
            )
            .col(ColumnDef::new(ImageHistory::ThumbnailFormat).string_len(8))
            .col(
                ColumnDef::new(ImageHistory::ThumbnailByteSize)
                    .big_integer()
                    .check(Expr::col(ImageHistory::ThumbnailByteSize).is_null().or(
                        Expr::col(ImageHistory::ThumbnailByteSize).between(1_i64, 524_288_i64),
                    )),
            )
            .col(timestamp(manager, ImageHistory::CreatedAt))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_image_history_owner")
                    .from(ImageHistory::Table, ImageHistory::OwnerUserId)
                    .to(Alias::new("users"), Alias::new("id"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(image_id_format_check(manager.get_database_backend()))
            .check(Expr::col(ImageHistory::Model).ne(""))
            .check(Expr::col(ImageHistory::Prompt).ne(""))
            .check(Expr::col(ImageHistory::Format).is_in(["png", "jpeg", "webp"]))
            .check(
                Expr::col(ImageHistory::ThumbnailFormat)
                    .is_null()
                    .or(Expr::col(ImageHistory::ThumbnailFormat).is_in(["png", "jpeg", "webp"])),
            )
            .check(thumbnail_metadata_check(manager.get_database_backend()));
        manager.create_table(images).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_image_history_owner_created")
                    .table(ImageHistory::Table)
                    .col(ImageHistory::OwnerUserId)
                    .col(ImageHistory::CreatedAt)
                    .col(ImageHistory::ImageId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ImageHistory::Table).to_owned())
            .await
    }
}

fn image_id_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""image_id" ~ '^[0-9a-f]{32}$' AND "image_id" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`image_id`) = 32 AND `image_id` REGEXP '^[0-9a-f]{32}$' \
             AND `image_id` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("image_id") = 32 AND "image_id" NOT GLOB '*[^0-9a-f]*' AND "image_id" <> '00000000000000000000000000000000'"#,
        ),
    }
}

fn thumbnail_metadata_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => {
            Expr::cust(r#"("thumbnail_format" IS NULL) = ("thumbnail_byte_size" IS NULL)"#)
        }
        DbBackend::MySql => {
            Expr::cust("(`thumbnail_format` IS NULL) = (`thumbnail_byte_size` IS NULL)")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"("thumbnail_format" IS NULL) = ("thumbnail_byte_size" IS NULL)"#)
        }
    }
}
