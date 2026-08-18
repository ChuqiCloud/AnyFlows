use sea_orm_migration::prelude::*;

use super::{
    iden::{passkey_registration_challenges, passkeys, users},
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

/// 创建 Passkey 凭证目录和一次性注册挑战。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_passkeys(manager).await?;
        create_challenges(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(passkey_registration_challenges::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(passkeys::Entity).to_owned())
            .await
    }
}

async fn create_passkeys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, passkeys::Entity)
                .col(auto_id(passkeys::Column::Id))
                .col(
                    ColumnDef::new(passkeys::Column::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkeys::Column::CredentialId)
                        .string_len(2048)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkeys::Column::CredentialIdDigest)
                        .char_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkeys::Column::Passkey)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkeys::Column::DisplayName)
                        .string_len(128)
                        .not_null(),
                )
                .col(timestamp(manager, passkeys::Column::CreatedAt))
                .col(nullable_timestamp(manager, passkeys::Column::LastUsedAt))
                .col(nullable_timestamp(manager, passkeys::Column::RevokedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_passkeys_user")
                        .from(passkeys::Entity, passkeys::Column::UserId)
                        .to(users::Entity, users::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("uq_passkeys_credential_id_digest")
                .table(passkeys::Entity)
                .col(passkeys::Column::CredentialIdDigest)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_passkeys_user_active")
                .table(passkeys::Entity)
                .col(passkeys::Column::UserId)
                .col(passkeys::Column::RevokedAt)
                .to_owned(),
        )
        .await
}

async fn create_challenges(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, passkey_registration_challenges::Entity)
                .col(auto_id(passkey_registration_challenges::Column::Id))
                .col(
                    ColumnDef::new(passkey_registration_challenges::Column::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkey_registration_challenges::Column::ChallengeDigest)
                        .char_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(passkey_registration_challenges::Column::RegistrationState)
                        .json_binary()
                        .not_null(),
                )
                .col(timestamp(
                    manager,
                    passkey_registration_challenges::Column::ExpiresAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    passkey_registration_challenges::Column::ConsumedAt,
                ))
                .col(timestamp(
                    manager,
                    passkey_registration_challenges::Column::CreatedAt,
                ))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_passkey_registration_challenges_user")
                        .from(
                            passkey_registration_challenges::Entity,
                            passkey_registration_challenges::Column::UserId,
                        )
                        .to(users::Entity, users::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_passkey_registration_challenges_user_state")
                .table(passkey_registration_challenges::Entity)
                .col(passkey_registration_challenges::Column::UserId)
                .col(passkey_registration_challenges::Column::ConsumedAt)
                .col(passkey_registration_challenges::Column::ExpiresAt)
                .to_owned(),
        )
        .await
}
