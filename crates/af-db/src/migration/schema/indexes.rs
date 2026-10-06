use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    abilities, billing_reservations, channel_groups, channel_models, channels, credentials, groups,
    tokens, users,
};

pub(in crate::migration) async fn create_billing_reservations_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_indexes(
        manager,
        [
            Index::create()
                .name("idx_billing_reservations_status_expires")
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::Status)
                .col(billing_reservations::Column::ExpiresAt)
                .to_owned(),
            Index::create()
                .name("idx_billing_reservations_user_created")
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::UserId)
                .col(billing_reservations::Column::CreatedAt)
                .to_owned(),
            Index::create()
                .name("idx_billing_reservations_token_created")
                .table(billing_reservations::Entity)
                .col(billing_reservations::Column::TokenId)
                .col(billing_reservations::Column::CreatedAt)
                .to_owned(),
        ],
    )
    .await
}

pub(in crate::migration) async fn create_groups_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_groups_active_name_unique(manager).await?;
    if manager.get_database_backend() != DbBackend::MySql {
        manager
            .create_index(
                Index::create()
                    .name("idx_groups_fallback_group")
                    .table(groups::Entity)
                    .col(groups::Column::FallbackGroupId)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

pub(in crate::migration) async fn create_users_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("uq_users_aff_code")
                .table(users::Entity)
                .col(users::Column::AffCode)
                .unique()
                .to_owned(),
        )
        .await?;
    create_users_active_identity_unique(manager).await?;

    let mut indexes = vec![
        Index::create()
            .name("idx_users_status")
            .table(users::Entity)
            .col(users::Column::Status)
            .to_owned(),
    ];
    if manager.get_database_backend() != DbBackend::MySql {
        indexes.extend([
            Index::create()
                .name("idx_users_default_group")
                .table(users::Entity)
                .col(users::Column::DefaultGroupId)
                .to_owned(),
            Index::create()
                .name("idx_users_inviter")
                .table(users::Entity)
                .col(users::Column::InviterId)
                .to_owned(),
        ]);
    }
    create_indexes(manager, indexes).await
}

pub(in crate::migration) async fn create_tokens_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut indexes = vec![
        Index::create()
            .name("uq_tokens_key_hash")
            .table(tokens::Entity)
            .col(tokens::Column::KeyHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_tokens_user_status")
            .table(tokens::Entity)
            .col(tokens::Column::UserId)
            .col(tokens::Column::Status)
            .to_owned(),
        Index::create()
            .name("idx_tokens_expired_at")
            .table(tokens::Entity)
            .col(tokens::Column::ExpiredAt)
            .to_owned(),
    ];
    if manager.get_database_backend() != DbBackend::MySql {
        indexes.push(
            Index::create()
                .name("idx_tokens_group")
                .table(tokens::Entity)
                .col(tokens::Column::GroupId)
                .to_owned(),
        );
    }
    create_indexes(manager, indexes).await
}

pub(in crate::migration) async fn create_channels_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_indexes(
        manager,
        [
            Index::create()
                .name("idx_channels_status_priority")
                .table(channels::Entity)
                .col(channels::Column::Status)
                .col(channels::Column::Priority)
                .to_owned(),
            Index::create()
                .name("idx_channels_tag")
                .table(channels::Entity)
                .col(channels::Column::Tag)
                .to_owned(),
        ],
    )
    .await
}

pub(in crate::migration) async fn create_credentials_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut indexes = vec![
        Index::create()
            .name("idx_credentials_channel_status_schedulable")
            .table(credentials::Entity)
            .col(credentials::Column::ChannelId)
            .col(credentials::Column::Status)
            .col(credentials::Column::Schedulable)
            .to_owned(),
    ];
    if manager.get_database_backend() != DbBackend::MySql {
        indexes.push(
            Index::create()
                .name("idx_credentials_parent")
                .table(credentials::Entity)
                .col(credentials::Column::ParentId)
                .to_owned(),
        );
    }
    create_indexes(manager, indexes).await
}

pub(in crate::migration) async fn create_channel_models_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_channel_models_model_channel")
                .table(channel_models::Entity)
                .col(channel_models::Column::Model)
                .col(channel_models::Column::ChannelId)
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_channel_groups_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_channel_groups_group_channel")
                .table(channel_groups::Entity)
                .col(channel_groups::Column::GroupId)
                .col(channel_groups::Column::ChannelId)
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_abilities_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut indexes = vec![
        Index::create()
            .name("idx_abilities_group_model_enabled")
            .table(abilities::Entity)
            .col(abilities::Column::GroupId)
            .col(abilities::Column::Model)
            .col(abilities::Column::Enabled)
            .to_owned(),
    ];
    if manager.get_database_backend() != DbBackend::MySql {
        indexes.push(
            Index::create()
                .name("idx_abilities_channel")
                .table(abilities::Entity)
                .col(abilities::Column::ChannelId)
                .to_owned(),
        );
    }
    create_indexes(manager, indexes).await
}

async fn create_groups_active_name_unique(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE `groups`
ADD COLUMN `_active_name` VARCHAR(64)
    GENERATED ALWAYS AS (CASE WHEN `deleted_at` IS NULL THEN `name` ELSE NULL END) STORED,
ADD UNIQUE INDEX `uq_groups_active_name` (`_active_name`)"#,
            )
            .await?;
        return Ok(());
    }

    manager
        .create_index(
            Index::create()
                .name("uq_groups_active_name")
                .table(groups::Entity)
                .col(groups::Column::Name)
                .unique()
                .and_where(Expr::col(groups::Column::DeletedAt).is_null())
                .to_owned(),
        )
        .await
}

async fn create_users_active_identity_unique(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE `users`
ADD COLUMN `_active_username` VARCHAR(64)
    GENERATED ALWAYS AS (CASE WHEN `deleted_at` IS NULL THEN `username` ELSE NULL END) STORED,
ADD COLUMN `_active_email` VARCHAR(320)
    GENERATED ALWAYS AS (CASE WHEN `deleted_at` IS NULL THEN `email` ELSE NULL END) STORED,
ADD UNIQUE INDEX `uq_users_active_username` (`_active_username`),
ADD UNIQUE INDEX `uq_users_active_email` (`_active_email`)"#,
            )
            .await?;
        return Ok(());
    }

    create_indexes(
        manager,
        [
            Index::create()
                .name("uq_users_active_username")
                .table(users::Entity)
                .col(users::Column::Username)
                .unique()
                .and_where(Expr::col(users::Column::DeletedAt).is_null())
                .to_owned(),
            Index::create()
                .name("uq_users_active_email")
                .table(users::Entity)
                .col(users::Column::Email)
                .unique()
                .and_where(Expr::col(users::Column::DeletedAt).is_null())
                .to_owned(),
        ],
    )
    .await
}

async fn create_indexes<I>(manager: &SchemaManager<'_>, indexes: I) -> Result<(), DbErr>
where
    I: IntoIterator<Item = IndexCreateStatement>,
{
    for index in indexes {
        manager.create_index(index).await?;
    }
    Ok(())
}
