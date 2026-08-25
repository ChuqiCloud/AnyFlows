use sea_orm_migration::prelude::*;

use super::iden::organization_sso_login_transactions;

/// 为已验证 OIDC ticket 增加企业隔离邀请邮箱指纹；不保存邮箱原文或 ID Token。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(organization_sso_login_transactions::Entity)
                    .add_column(
                        ColumnDef::new(
                            organization_sso_login_transactions::Column::InvitationEmailFingerprint,
                        )
                        .string_len(64),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(organization_sso_login_transactions::Entity)
                    .drop_column(
                        organization_sso_login_transactions::Column::InvitationEmailFingerprint,
                    )
                    .to_owned(),
            )
            .await
    }
}
