use sea_orm_migration::prelude::*;

use super::{
    iden::{
        custom_oauth2_identities, custom_oauth2_login_transactions, custom_oauth2_providers, users,
    },
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

/// 创建自定义 OAuth2 独立身份绑定和配置版本锁定的单次登录事务。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_identities(manager).await?;
        create_transactions(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(custom_oauth2_login_transactions::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(custom_oauth2_identities::Entity)
                    .to_owned(),
            )
            .await
    }
}

async fn create_identities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, custom_oauth2_identities::Entity);
    statement
        .col(auto_id(custom_oauth2_identities::Column::Id))
        .col(
            ColumnDef::new(custom_oauth2_identities::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(custom_oauth2_identities::Column::ProviderKey)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(custom_oauth2_identities::Column::Subject)
                .string_len(255)
                .not_null(),
        )
        .col(timestamp(
            manager,
            custom_oauth2_identities::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            custom_oauth2_identities::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_custom_oauth2_identities_user")
                .from(
                    custom_oauth2_identities::Entity,
                    custom_oauth2_identities::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_custom_oauth2_identities_provider")
                .from(
                    custom_oauth2_identities::Entity,
                    custom_oauth2_identities::Column::ProviderKey,
                )
                .to(
                    custom_oauth2_providers::Entity,
                    custom_oauth2_providers::Column::ProviderKey,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    for (name, columns) in [
        (
            "uq_custom_oauth2_identities_subject",
            vec![
                custom_oauth2_identities::Column::ProviderKey,
                custom_oauth2_identities::Column::Subject,
            ],
        ),
        (
            "uq_custom_oauth2_identities_user_provider",
            vec![
                custom_oauth2_identities::Column::UserId,
                custom_oauth2_identities::Column::ProviderKey,
            ],
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(custom_oauth2_identities::Entity)
            .unique();
        for column in columns {
            index.col(column);
        }
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_transactions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, custom_oauth2_login_transactions::Entity);
    statement
        .col(auto_id(custom_oauth2_login_transactions::Column::Id))
        .col(
            ColumnDef::new(custom_oauth2_login_transactions::Column::ProviderKey)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(custom_oauth2_login_transactions::Column::ConfigurationVersion)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(custom_oauth2_login_transactions::Column::ConfigurationVersion)
                        .gte(1_i64),
                ),
        )
        .col(
            ColumnDef::new(custom_oauth2_login_transactions::Column::StateDigest)
                .string_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            custom_oauth2_login_transactions::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            custom_oauth2_login_transactions::Column::ClaimedAt,
        ))
        .col(ColumnDef::new(custom_oauth2_login_transactions::Column::UserId).big_integer())
        .col(ColumnDef::new(custom_oauth2_login_transactions::Column::TicketDigest).string_len(64))
        .col(nullable_timestamp(
            manager,
            custom_oauth2_login_transactions::Column::TicketExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            custom_oauth2_login_transactions::Column::ExchangedAt,
        ))
        .col(timestamp(
            manager,
            custom_oauth2_login_transactions::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            custom_oauth2_login_transactions::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_custom_oauth2_login_transactions_provider")
                .from(
                    custom_oauth2_login_transactions::Entity,
                    custom_oauth2_login_transactions::Column::ProviderKey,
                )
                .to(
                    custom_oauth2_providers::Entity,
                    custom_oauth2_providers::Column::ProviderKey,
                )
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_custom_oauth2_login_transactions_user")
                .from(
                    custom_oauth2_login_transactions::Entity,
                    custom_oauth2_login_transactions::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );
    manager.create_table(statement).await?;
    for (name, column, unique) in [
        (
            "uq_custom_oauth2_login_transactions_state",
            custom_oauth2_login_transactions::Column::StateDigest,
            true,
        ),
        (
            "uq_custom_oauth2_login_transactions_ticket",
            custom_oauth2_login_transactions::Column::TicketDigest,
            true,
        ),
        (
            "idx_custom_oauth2_login_transactions_expires",
            custom_oauth2_login_transactions::Column::ExpiresAt,
            false,
        ),
        (
            "idx_custom_oauth2_login_transactions_ticket_expires",
            custom_oauth2_login_transactions::Column::TicketExpiresAt,
            false,
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(custom_oauth2_login_transactions::Entity)
            .col(column);
        if unique {
            index.unique();
        }
        manager.create_index(index).await?;
    }
    Ok(())
}
