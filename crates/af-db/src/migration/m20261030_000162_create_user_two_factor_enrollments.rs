use sea_orm_migration::prelude::*;

use crate::migration::iden::users;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[derive(Iden)]
enum UserTwoFactorEnrollments {
    Table,
    UserId,
    PendingConfiguration,
    PendingCreatedAt,
    PendingExpiresAt,
    BoundAt,
    LastVerifiedAt,
    PendingSessionVersion,
    PendingAttempts,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserTwoFactorEnrollments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::UserId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::PendingConfiguration)
                            .json_binary()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::PendingCreatedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::PendingExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::BoundAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::LastVerifiedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::PendingSessionVersion)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(UserTwoFactorEnrollments::PendingAttempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_two_factor_enrollments_user")
                            .from(
                                UserTwoFactorEnrollments::Table,
                                UserTwoFactorEnrollments::UserId,
                            )
                            .to(users::Entity, users::Column::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UserTwoFactorEnrollments::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
