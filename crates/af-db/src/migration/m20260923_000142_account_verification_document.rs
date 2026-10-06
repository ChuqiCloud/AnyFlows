use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (name, max) in [
            ("provider", 32),
            ("document_country", 2),
            ("document_type", 32),
            ("document_number_masked", 32),
        ] {
            let mut column = ColumnDef::new(Alias::new(name));
            column.string_len(max);
            if name == "provider" {
                column.not_null().default("manual");
            } else if name == "document_country" {
                column.not_null().default("CN");
            } else if name == "document_type" {
                column.not_null().default("identity");
            }
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("account_verifications"))
                        .add_column(&mut column)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in [
            "document_number_masked",
            "document_type",
            "document_country",
            "provider",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("account_verifications"))
                        .drop_column(Alias::new(name))
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}
