use sea_orm_migration::prelude::*;

use super::iden::request_outcome_logs;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

const USER_CREATED_INDEX: &str = "idx_request_outcome_user_created";
const ORGANIZATION_CREATED_INDEX: &str = "idx_request_outcome_organization_created";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(request_outcome_logs::Entity)
                    .add_column(ColumnDef::new(request_outcome_logs::Column::UserId).big_integer())
                    .add_column(ColumnDef::new(request_outcome_logs::Column::TokenId).big_integer())
                    .add_column(ColumnDef::new(request_outcome_logs::Column::GroupId).big_integer())
                    .add_column(
                        ColumnDef::new(request_outcome_logs::Column::OrganizationId).big_integer(),
                    )
                    .add_column(
                        ColumnDef::new(request_outcome_logs::Column::OrganizationTeamId)
                            .big_integer(),
                    )
                    .add_column(
                        ColumnDef::new(request_outcome_logs::Column::PublicErrorCode)
                            .string_len(64),
                    )
                    .add_column(
                        ColumnDef::new(request_outcome_logs::Column::PublicErrorMessage)
                            .string_len(255),
                    )
                    .to_owned(),
            )
            .await?;
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
        manager
            .alter_table(
                Table::alter()
                    .table(request_outcome_logs::Entity)
                    .drop_column(request_outcome_logs::Column::PublicErrorMessage)
                    .drop_column(request_outcome_logs::Column::PublicErrorCode)
                    .drop_column(request_outcome_logs::Column::OrganizationTeamId)
                    .drop_column(request_outcome_logs::Column::OrganizationId)
                    .drop_column(request_outcome_logs::Column::GroupId)
                    .drop_column(request_outcome_logs::Column::TokenId)
                    .drop_column(request_outcome_logs::Column::UserId)
                    .to_owned(),
            )
            .await
    }
}
