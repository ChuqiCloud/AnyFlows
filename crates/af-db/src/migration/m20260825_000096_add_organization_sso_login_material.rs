use sea_orm_migration::prelude::*;

use super::iden::organization_sso_login_transactions;

/// 为企业 OIDC 单次登录事务增加短期加密材料封套；禁止保存 PKCE verifier 或 nonce 明文。
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
                            organization_sso_login_transactions::Column::LoginMaterialEnvelope,
                        )
                        .json_binary(),
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
                    .drop_column(organization_sso_login_transactions::Column::LoginMaterialEnvelope)
                    .to_owned(),
            )
            .await
    }
}
