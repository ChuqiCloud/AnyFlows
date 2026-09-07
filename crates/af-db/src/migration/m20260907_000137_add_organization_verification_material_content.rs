use sea_orm_migration::prelude::*;

use super::iden::organization_verification_materials;

/// 为已有企业认证材料补充可空正文列；旧的元数据记录仍可被审核，但不可下载正文。
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(organization_verification_materials::Entity)
                    .add_column(
                        ColumnDef::new(organization_verification_materials::Column::ContentBytes)
                            .blob()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(organization_verification_materials::Entity)
                    .drop_column(organization_verification_materials::Column::ContentBytes)
                    .to_owned(),
            )
            .await
    }
}
