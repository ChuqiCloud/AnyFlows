use sea_orm_migration::prelude::*;

use super::{
    iden::{oauth_login_providers, oauth_login_transactions, user_oauth_identities, users},
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

const GITHUB_PROVIDER: &str = "github";

/// 创建用户登录 OAuth Provider、外部身份与单次事务持久化边界。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_providers(manager).await?;
        seed_github_provider(manager).await?;
        create_identities(manager).await?;
        create_transactions(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(oauth_login_transactions::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(user_oauth_identities::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(oauth_login_providers::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn create_providers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, oauth_login_providers::Entity);
    statement
        .col(
            ColumnDef::new(oauth_login_providers::Column::Provider)
                .string_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(oauth_login_providers::Column::Enabled)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(ColumnDef::new(oauth_login_providers::Column::ClientId).string_len(255))
        .col(ColumnDef::new(oauth_login_providers::Column::ClientSecret).json_binary())
        .col(
            ColumnDef::new(oauth_login_providers::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(oauth_login_providers::Column::Version).gte(1_i64)),
        )
        .col(timestamp(manager, oauth_login_providers::Column::CreatedAt))
        .col(timestamp(manager, oauth_login_providers::Column::UpdatedAt));
    manager.create_table(statement.to_owned()).await
}

async fn seed_github_provider(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .exec_stmt(
            Query::insert()
                .into_table(oauth_login_providers::Entity)
                .columns([oauth_login_providers::Column::Provider])
                .values_panic([GITHUB_PROVIDER.into()])
                .to_owned(),
        )
        .await
}

async fn create_identities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, user_oauth_identities::Entity);
    statement
        .col(auto_id(user_oauth_identities::Column::Id))
        .col(
            ColumnDef::new(user_oauth_identities::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_oauth_identities::Column::Provider)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(user_oauth_identities::Column::Subject)
                .string_len(255)
                .not_null(),
        )
        .col(timestamp(manager, user_oauth_identities::Column::CreatedAt))
        .col(timestamp(manager, user_oauth_identities::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_user_oauth_identities_user")
                .from(
                    user_oauth_identities::Entity,
                    user_oauth_identities::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_user_oauth_identities_provider")
                .from(
                    user_oauth_identities::Entity,
                    user_oauth_identities::Column::Provider,
                )
                .to(
                    oauth_login_providers::Entity,
                    oauth_login_providers::Column::Provider,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement.to_owned()).await?;
    for (name, columns) in [
        (
            "uq_user_oauth_identities_subject",
            vec![
                user_oauth_identities::Column::Provider,
                user_oauth_identities::Column::Subject,
            ],
        ),
        (
            "uq_user_oauth_identities_user_provider",
            vec![
                user_oauth_identities::Column::UserId,
                user_oauth_identities::Column::Provider,
            ],
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(user_oauth_identities::Entity)
            .unique();
        for column in columns {
            index.col(column);
        }
        manager.create_index(index.to_owned()).await?;
    }
    Ok(())
}

async fn create_transactions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, oauth_login_transactions::Entity);
    statement
        .col(auto_id(oauth_login_transactions::Column::Id))
        .col(
            ColumnDef::new(oauth_login_transactions::Column::Provider)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(oauth_login_transactions::Column::StateDigest)
                .string_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            oauth_login_transactions::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            oauth_login_transactions::Column::ClaimedAt,
        ))
        .col(ColumnDef::new(oauth_login_transactions::Column::UserId).big_integer())
        .col(ColumnDef::new(oauth_login_transactions::Column::TicketDigest).string_len(64))
        .col(nullable_timestamp(
            manager,
            oauth_login_transactions::Column::TicketExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            oauth_login_transactions::Column::ExchangedAt,
        ))
        .col(timestamp(
            manager,
            oauth_login_transactions::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            oauth_login_transactions::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_oauth_login_transactions_provider")
                .from(
                    oauth_login_transactions::Entity,
                    oauth_login_transactions::Column::Provider,
                )
                .to(
                    oauth_login_providers::Entity,
                    oauth_login_providers::Column::Provider,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_oauth_login_transactions_user")
                .from(
                    oauth_login_transactions::Entity,
                    oauth_login_transactions::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement.to_owned()).await?;
    for (name, column, unique) in [
        (
            "uq_oauth_login_transactions_state",
            oauth_login_transactions::Column::StateDigest,
            true,
        ),
        (
            "uq_oauth_login_transactions_ticket",
            oauth_login_transactions::Column::TicketDigest,
            true,
        ),
        (
            "idx_oauth_login_transactions_expires",
            oauth_login_transactions::Column::ExpiresAt,
            false,
        ),
        (
            "idx_oauth_login_transactions_ticket_expires",
            oauth_login_transactions::Column::TicketExpiresAt,
            false,
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(oauth_login_transactions::Entity)
            .col(column);
        if unique {
            index.unique();
        }
        manager.create_index(index.to_owned()).await?;
    }
    Ok(())
}
