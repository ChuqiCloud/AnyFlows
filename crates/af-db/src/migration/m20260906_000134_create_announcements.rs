use sea_orm_migration::prelude::*;

use crate::migration::iden::{announcements, users};

use super::schema::{auto_id, nullable_timestamp, table, timestamp};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut statement = table(manager, announcements::Entity);
        statement
            .col(auto_id(announcements::Column::Id))
            .col(
                ColumnDef::new(announcements::Column::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64)
                    .check(Expr::col(announcements::Column::Version).gt(0_i64)),
            )
            .col(
                ColumnDef::new(announcements::Column::Status)
                    .small_integer()
                    .not_null()
                    .default(1_i16)
                    .check(Expr::col(announcements::Column::Status).is_in([1_i16, 2, 3])),
            )
            .col(
                ColumnDef::new(announcements::Column::TitleZh)
                    .string_len(160)
                    .not_null(),
            )
            .col(
                ColumnDef::new(announcements::Column::TitleEn)
                    .string_len(160)
                    .not_null(),
            )
            .col(
                ColumnDef::new(announcements::Column::BodyZh)
                    // MySQL utf8mb4 下长 VARCHAR 会把整行推过 65535 字节；长度由仓储边界校验。
                    .text()
                    .not_null(),
            )
            .col(
                ColumnDef::new(announcements::Column::BodyEn)
                    .text()
                    .not_null(),
            )
            .col(nullable_timestamp(
                manager,
                announcements::Column::VisibleFrom,
            ))
            .col(nullable_timestamp(
                manager,
                announcements::Column::VisibleUntil,
            ))
            .col(
                ColumnDef::new(announcements::Column::CreatedBy)
                    .big_integer()
                    .not_null(),
            )
            .col(nullable_timestamp(
                manager,
                announcements::Column::PublishedAt,
            ))
            .col(nullable_timestamp(
                manager,
                announcements::Column::RevokedAt,
            ))
            .col(timestamp(manager, announcements::Column::CreatedAt))
            .col(timestamp(manager, announcements::Column::UpdatedAt))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_announcements_created_by")
                    .from(announcements::Entity, announcements::Column::CreatedBy)
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::Restrict),
            );
        manager.create_table(statement).await?;
        for index in [
            Index::create()
                .name("idx_announcements_admin_order")
                .table(announcements::Entity)
                .col(announcements::Column::UpdatedAt)
                .col(announcements::Column::Id)
                .to_owned(),
            Index::create()
                .name("idx_announcements_public_window")
                .table(announcements::Entity)
                .col(announcements::Column::Status)
                .col(announcements::Column::VisibleFrom)
                .col(announcements::Column::VisibleUntil)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(announcements::Entity).to_owned())
            .await
    }
}
