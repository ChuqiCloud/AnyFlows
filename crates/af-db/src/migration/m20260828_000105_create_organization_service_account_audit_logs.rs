use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_service_account_audit_logs as audit,
        organization_service_account_keys as keys, organization_service_accounts as accounts,
        organizations, usage_logs,
    },
    schema::{auto_id, table, timestamp},
};

/// 创建服务账号调用身份到已落库用量事件的只追加投影。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, audit::Entity)
                    .if_not_exists()
                    .col(auto_id(audit::Column::Id))
                    .col(
                        ColumnDef::new(audit::Column::EventId)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit::Column::OrganizationId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit::Column::ServiceAccountId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit::Column::ServiceAccountKey)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(audit::Column::CredentialKey)
                            .char_len(32)
                            .not_null(),
                    )
                    .col(timestamp(manager, audit::Column::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_audit_organization")
                            .from(audit::Entity, audit::Column::OrganizationId)
                            .to(organizations::Entity, organizations::Column::Id)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_audit_account")
                            .from(
                                audit::Entity,
                                (
                                    audit::Column::OrganizationId,
                                    audit::Column::ServiceAccountId,
                                ),
                            )
                            .to(
                                accounts::Entity,
                                (accounts::Column::OrganizationId, accounts::Column::Id),
                            )
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_audit_credential")
                            .from(audit::Entity, audit::Column::CredentialKey)
                            .to(keys::Entity, keys::Column::KeyKey)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_service_account_audit_usage_event")
                            .from(audit::Entity, audit::Column::EventId)
                            .to(usage_logs::Entity, usage_logs::Column::EventId)
                            .on_update(ForeignKeyAction::Restrict)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .check(hex_format_check(manager.get_database_backend()))
                    .to_owned(),
            )
            .await?;

        for index in [
            Index::create()
                .name("uq_service_account_audit_event_id")
                .table(audit::Entity)
                .col(audit::Column::EventId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_service_account_audit_organization_account_id")
                .table(audit::Entity)
                .col(audit::Column::OrganizationId)
                .col(audit::Column::ServiceAccountId)
                .col(audit::Column::Id)
                .to_owned(),
            Index::create()
                .name("idx_service_account_audit_organization_credential_id")
                .table(audit::Entity)
                .col(audit::Column::OrganizationId)
                .col(audit::Column::CredentialKey)
                .col(audit::Column::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(audit::Entity).to_owned())
            .await
    }
}

fn hex_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""event_id" ~ '^[0-9a-f]{32}$' AND "service_account_key" ~ '^[0-9a-f]{32}$' AND "credential_key" ~ '^[0-9a-f]{32}$'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`event_id`) = 32 AND `event_id` REGEXP '^[0-9a-f]{32}$' AND CHAR_LENGTH(`service_account_key`) = 32 AND `service_account_key` REGEXP '^[0-9a-f]{32}$' AND CHAR_LENGTH(`credential_key`) = 32 AND `credential_key` REGEXP '^[0-9a-f]{32}$'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("event_id") = 32 AND "event_id" NOT GLOB '*[^0-9a-f]*' AND length("service_account_key") = 32 AND "service_account_key" NOT GLOB '*[^0-9a-f]*' AND length("credential_key") = 32 AND "credential_key" NOT GLOB '*[^0-9a-f]*'"#,
        ),
    }
}
