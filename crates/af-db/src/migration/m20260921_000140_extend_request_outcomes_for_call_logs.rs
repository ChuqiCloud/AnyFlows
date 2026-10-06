use sea_orm_migration::prelude::*;

use super::iden::request_outcome_logs;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

const USER_CREATED_INDEX: &str = "idx_request_outcome_user_created";
const ORGANIZATION_CREATED_INDEX: &str = "idx_request_outcome_organization_created";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite accepts only one column change per ALTER TABLE statement.
        for column in [
            request_outcome_logs::Column::UserId,
            request_outcome_logs::Column::TokenId,
            request_outcome_logs::Column::GroupId,
            request_outcome_logs::Column::OrganizationId,
            request_outcome_logs::Column::OrganizationTeamId,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(request_outcome_logs::Entity)
                        .add_column(ColumnDef::new(column).big_integer())
                        .to_owned(),
                )
                .await?;
        }
        for (column, length) in [
            (request_outcome_logs::Column::PublicErrorCode, 64),
            (request_outcome_logs::Column::PublicErrorMessage, 255),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(request_outcome_logs::Entity)
                        .add_column(ColumnDef::new(column).string_len(length))
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name(USER_CREATED_INDEX)
                    .table(request_outcome_logs::Entity)
                    .col(request_outcome_logs::Column::UserId)
                    .col(request_outcome_logs::Column::CreatedAt)
                    .col(request_outcome_logs::Column::Id)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(ORGANIZATION_CREATED_INDEX)
                    .table(request_outcome_logs::Entity)
                    .col(request_outcome_logs::Column::OrganizationId)
                    .col(request_outcome_logs::Column::CreatedAt)
                    .col(request_outcome_logs::Column::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(ORGANIZATION_CREATED_INDEX)
                    .table(request_outcome_logs::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name(USER_CREATED_INDEX)
                    .table(request_outcome_logs::Entity)
                    .to_owned(),
            )
            .await?;
        for column in [
            request_outcome_logs::Column::PublicErrorMessage,
            request_outcome_logs::Column::PublicErrorCode,
            request_outcome_logs::Column::OrganizationTeamId,
            request_outcome_logs::Column::OrganizationId,
            request_outcome_logs::Column::GroupId,
            request_outcome_logs::Column::TokenId,
            request_outcome_logs::Column::UserId,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(request_outcome_logs::Entity)
                        .drop_column(column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
