use sea_orm_migration::prelude::*;

/// Bound merchant ledger report scans by merchant and UTC event timestamp.
pub(in crate::migration) async fn create_merchant_finance_report_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_mfin_entries_owner_created_id")
                .table(Alias::new("merchant_finance_entries"))
                .col(Alias::new("merchant_id"))
                .col(Alias::new("created_at"))
                .col(Alias::new("id"))
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn drop_merchant_finance_report_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_index(
            Index::drop()
                .name("idx_mfin_entries_owner_created_id")
                .table(Alias::new("merchant_finance_entries"))
                .to_owned(),
        )
        .await
}
